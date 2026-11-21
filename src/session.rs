//! ArchWeb session handling: authentication and signoff API calls.
//!
//! Ported from `SignoffSession` in `archlinux/arch-signoff`. The session
//! performs a CSRF-token login against ArchWeb and exposes signoff, revoke,
//! and signoff-list operations over HTTPS.

use crate::{
    error::SignoffError,
    model::{SignoffGroup, SignoffPackage},
};

/// Backend abstraction for the ArchWeb signoff API.
///
/// Implementations talk to a real ArchWeb instance ([`ArchWebBackend`]) or a
/// test double.
pub trait SignoffBackend {
    /// Fetch all packages awaiting signoff.
    fn get_signoffs(&self) -> Result<Vec<SignoffPackage>, SignoffError>;
    /// Sign off `pkg` for the authenticated user.
    fn signoff_package(&self, pkg: &SignoffPackage) -> Result<(), SignoffError>;
    /// Revoke the authenticated user's signoff on `pkg`.
    fn revoke_package(&self, pkg: &SignoffPackage) -> Result<(), SignoffError>;
}

/// Live ArchWeb backend using reqwest with cookie-backed CSRF login.
pub struct ArchWebBackend {
    client: reqwest::blocking::Client,
    base_url: String,
    password: String,
}

impl ArchWebBackend {
    /// Create a backend for `base_url` (e.g. `https://archlinux.org/`).
    ///
    /// Performs the CSRF login immediately; returns an error on bad
    /// credentials, mirroring upstream behaviour.
    pub fn new(
        base_url: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<Self, SignoffError> {
        let base_url = base_url.into();
        let username = username.into();
        let password = password.into();
        let client = reqwest::blocking::Client::builder()
            .cookie_store(true)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| SignoffError::Http(e.to_string()))?;

        let backend = Self {
            client,
            base_url,
            password,
        };
        backend.login(&username)?;
        Ok(backend)
    }

    fn login_url(&self) -> String {
        join_url(&self.base_url, "/login/")
    }

    fn logout_url(&self) -> String {
        join_url(&self.base_url, "/logout/")
    }

    fn signoffs_url(&self) -> String {
        join_url(&self.base_url, "/packages/signoffs/json/")
    }

    fn signoff_url(&self, pkg: &SignoffPackage) -> String {
        join_url(
            &self.base_url,
            &format!(
                "/packages/{}/{}/{}/signoff/",
                pkg.repo, pkg.arch, pkg.pkgbase
            ),
        )
    }

    fn revoke_url(&self, pkg: &SignoffPackage) -> String {
        format!("{}/revoke/", self.signoff_url(pkg))
    }

    /// CSRF login: GET /login/ for the `csrftoken` cookie, POST credentials
    /// with the matching `csrfmiddlewaretoken` field, expect a 3xx redirect.
    fn login(&self, username: &str) -> Result<(), SignoffError> {
        let login_url = self.login_url();
        let get = self
            .client
            .get(&login_url)
            .send()
            .map_err(|e| SignoffError::Http(e.to_string()))?;
        let csrftoken = extract_cookie(get.headers(), "csrftoken")
            .ok_or_else(|| SignoffError::Auth("no csrftoken cookie from ArchWeb".into()))?;

        let response = self
            .client
            .post(&login_url)
            .header("referer", &login_url)
            .header("x-csrftoken", &csrftoken)
            .form(&[
                ("username", username),
                ("password", self.password.as_str()),
                ("csrfmiddlewaretoken", csrftoken.as_str()),
            ])
            .send()
            .map_err(|e| SignoffError::Http(e.to_string()))?;

        let status = response.status().as_u16();
        if !(300..400).contains(&status) {
            return Err(SignoffError::Auth(format!(
                "could not log in -- check credentials (HTTP {status})"
            )));
        }
        Ok(())
    }

    /// Log out of ArchWeb.
    pub fn logout(&self) -> Result<(), SignoffError> {
        self.client
            .get(self.logout_url())
            .send()
            .map_err(|e| SignoffError::Http(e.to_string()))?;
        Ok(())
    }
}

impl SignoffBackend for ArchWebBackend {
    fn get_signoffs(&self) -> Result<Vec<SignoffPackage>, SignoffError> {
        let body = self
            .client
            .get(self.signoffs_url())
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| SignoffError::Http(e.to_string()))?
            .bytes()
            .map_err(|e| SignoffError::Http(e.to_string()))?;
        let group = SignoffGroup::from_json(&body).map_err(|e| {
            SignoffError::Parse(format!("invalid signoffs JSON: {e}"))
        })?;
        Ok(group.signoff_groups)
    }

    fn signoff_package(&self, pkg: &SignoffPackage) -> Result<(), SignoffError> {
        self.client
            .get(self.signoff_url(pkg))
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| SignoffError::Http(e.to_string()))?;
        Ok(())
    }

    fn revoke_package(&self, pkg: &SignoffPackage) -> Result<(), SignoffError> {
        self.client
            .get(self.revoke_url(pkg))
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| SignoffError::Http(e.to_string()))?;
        Ok(())
    }
}

fn join_url(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

/// Extract `name=value` for `name` from Set-Cookie headers.
fn extract_cookie(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    for value in headers.get_all(reqwest::header::SET_COOKIE) {
        let s = value.to_str().ok()?;
        for part in s.split(';') {
            let part = part.trim();
            if let Some(rest) = part.strip_prefix(name) {
                if let Some(v) = rest.strip_prefix('=') {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}
