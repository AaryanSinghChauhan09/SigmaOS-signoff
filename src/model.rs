//! Signoff data model — mirrors the ArchWeb `/packages/signoffs/json/` v2 API.

use serde::{Deserialize, Serialize};

/// A single user's signoff record on a package.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Signoff {
    /// ArchWeb username of the signer.
    pub user: String,
    /// ISO-8601 timestamp of the signoff.
    pub created: String,
    /// Whether this signoff has been revoked.
    #[serde(default)]
    pub revoked: bool,
}

/// A package up for testing in the signoff repos.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SignoffPackage {
    /// Package base name.
    pub pkgbase: String,
    /// Binary package names in this base.
    pub pkgnames: Vec<String>,
    /// Repository name (e.g. `testing`, `multilib`).
    pub repo: String,
    /// Architecture (e.g. `x86_64`).
    pub arch: String,
    /// Version string awaiting signoff.
    pub version: String,
    /// ISO-8601 timestamp of the last package update.
    pub last_update: String,
    /// Packager (TU) who pushed the package.
    pub packager: String,
    /// Optional package comments.
    #[serde(default)]
    pub comments: Option<String>,
    /// Whether the package is flagged known-bad.
    #[serde(default)]
    pub known_bad: bool,
    /// All signoff/revocation records for this package.
    #[serde(default)]
    pub signoffs: Vec<Signoff>,
}

/// A locally installed pacman package, as matched against signoff entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalPackage {
    /// Binary package name.
    pub name: String,
    /// Package base name (`base` in alpm, may equal `name`).
    pub base: Option<String>,
    /// Installed version string.
    pub version: String,
    /// Install date as a unix timestamp.
    pub installdate: i64,
}

impl LocalPackage {
    /// The package base, falling back to the binary name.
    pub fn pkgbase(&self) -> &str {
        self.base.as_deref().unwrap_or(&self.name)
    }
}

/// The current signoff state of `user` on `pkg`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignoffStatus {
    /// The user's latest record is an active signoff.
    SignedOff,
    /// The user's latest record is a revocation.
    Revoked,
    /// The user has no records on this package.
    None,
}

impl std::fmt::Display for SignoffStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignoffStatus::SignedOff => write!(f, "signed-off"),
            SignoffStatus::Revoked => write!(f, "revoked"),
            SignoffStatus::None => write!(f, "none"),
        }
    }
}

/// A signoff group as returned by the ArchWeb JSON API.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SignoffGroup {
    /// API format version; must be 2.
    pub version: u32,
    /// Package entries awaiting signoff.
    pub signoff_groups: Vec<SignoffPackage>,
}

impl SignoffGroup {
    /// Parse and validate a signoffs JSON document.
    pub fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        let group: Self = serde_json::from_slice(bytes)?;
        if group.version != 2 {
            return Err(serde::de::Error::custom(format!(
                "unsupported signoffs API version {}",
                group.version
            )));
        }
        Ok(group)
    }
}
