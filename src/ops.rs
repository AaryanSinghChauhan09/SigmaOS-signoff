//! Core signoff logic: status resolution, filtering, and display formatting.
//! Ported from `archlinux/arch-signoff` `signoff/__init__.py`.

use crate::model::{LocalPackage, SignoffPackage, SignoffStatus};

/// Resolve the current signoff status of `user` on `pkg`.
///
/// Signoffs are considered newest-first by `created` timestamp; the user's
/// most recent record decides the status (revoked records yield `Revoked`).
pub fn signoff_status(pkg: &SignoffPackage, user: &str) -> SignoffStatus {
    let mut records: Vec<&crate::model::Signoff> =
        pkg.signoffs.iter().filter(|s| s.user == user).collect();
    // ISO-8601 timestamps sort correctly lexicographically.
    records.sort_by(|a, b| b.created.cmp(&a.created));
    match records.first() {
        Some(s) if s.revoked => SignoffStatus::Revoked,
        Some(_) => SignoffStatus::SignedOff,
        None => SignoffStatus::None,
    }
}

/// Command-line style options controlling listing and filtering.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Explicit package bases supplied on the command line.
    pub packages: Vec<String>,
    /// Include packages the user already signed off.
    pub show_signed_off: bool,
    /// Include packages not installed locally.
    pub show_uninstalled: bool,
    /// Compact one-line output.
    pub quiet: bool,
    /// ArchWeb username used for status lookups.
    pub username: String,
    /// Disable ANSI styling.
    pub nocolor: bool,
}

/// Filter `(signoff_package, local_package)` pairs per the given options.
///
/// Command-line package names override the status/install filters, matching
/// upstream behaviour.
pub fn filter_signoffs<'a>(
    signoffs: impl Iterator<Item = (&'a SignoffPackage, Option<&'a LocalPackage>)>,
    options: &Options,
) -> Vec<(&'a SignoffPackage, Option<&'a LocalPackage>)> {
    let wanted: std::collections::HashSet<&str> = options
        .packages
        .iter()
        .map(String::as_str)
        .collect();
    let mut out = Vec::new();
    for (pkg, local) in signoffs {
        if !wanted.is_empty() {
            if wanted.contains(pkg.pkgbase.as_str()) {
                out.push((pkg, local));
            }
            continue;
        }
        let status = signoff_status(pkg, &options.username);
        if status == SignoffStatus::SignedOff && !options.show_signed_off {
            continue;
        }
        if local.is_none() && !options.show_uninstalled {
            continue;
        }
        out.push((pkg, local));
    }
    out
}

/// Format a single signoff record: `user` or `user (revoked)`.
pub fn format_signoff_user(signoff: &crate::model::Signoff) -> String {
    if signoff.revoked {
        format!("{} (revoked)", signoff.user)
    } else {
        signoff.user.clone()
    }
}

/// Format a package for display; `color` enables minimal ANSI styling.
pub fn format_signoff(
    pkg: &SignoffPackage,
    local: Option<&LocalPackage>,
    options: &Options,
    color: bool,
) -> String {
    if options.quiet {
        format_signoff_short(pkg, local, options, color)
    } else {
        format_signoff_long(pkg, local, options)
    }
}

fn paint(color: bool, code: &str, text: &str) -> String {
    if color {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

/// Format a package in the compact one-line style.
pub fn format_signoff_short(
    pkg: &SignoffPackage,
    local: Option<&LocalPackage>,
    options: &Options,
    color: bool,
) -> String {
    let mut out = format!(
        "{} {}",
        paint(color, "1", &pkg.pkgbase),
        paint(color, "32", &pkg.version)
    );
    if let Some(local) = local {
        if local.version != pkg.version {
            out.push_str(&paint(color, "1;31", " (outdated)"));
        } else if options.show_uninstalled {
            out.push_str(&paint(color, "1;34", " (installed)"));
        }
    }
    if pkg.known_bad {
        out.push_str(&paint(color, "1;31", " (known bad)"));
    }
    match signoff_status(pkg, &options.username) {
        SignoffStatus::SignedOff if options.show_signed_off => {
            out.push_str(&paint(color, "1;36", " [signed off]"));
        }
        SignoffStatus::Revoked => out.push_str(&paint(color, "1;31", " [revoked]")),
        _ => {}
    }
    out
}

fn attr(name: &str, value: impl std::fmt::Display) -> String {
    format!("{name:<13} : {value}")
}

/// Format a package in pacman -Qi style.
pub fn format_signoff_long(pkg: &SignoffPackage, local: Option<&LocalPackage>, options: &Options) -> String {
    let mut attrs = Vec::new();
    if pkg.pkgnames.len() > 1 {
        attrs.push(attr("Package base", &pkg.pkgbase));
        attrs.push(attr("Packages", pkg.pkgnames.join(", ")));
    } else {
        attrs.push(attr("Package", &pkg.pkgbase));
    }
    attrs.push(attr("Version", &pkg.version));
    if let Some(local) = local {
        if local.version != pkg.version {
            attrs.push(attr("Local version", &local.version));
        }
        attrs.push(attr("Install date", local.installdate));
    }
    attrs.push(attr("Last updated", &pkg.last_update));
    attrs.push(attr("Packager", &pkg.packager));
    attrs.push(attr(
        "Comments",
        pkg.comments.as_deref().unwrap_or("None"),
    ));
    let signoffs: Vec<String> = pkg.signoffs.iter().map(format_signoff_user).collect();
    attrs.push(attr("Signoffs", signoffs.join(", ")));
    let signed_off = match signoff_status(pkg, &options.username) {
        SignoffStatus::None => "No",
        SignoffStatus::SignedOff => "Yes",
        SignoffStatus::Revoked => "Revoked",
    };
    attrs.push(attr("Signed off", signed_off));
    if pkg.known_bad {
        attrs.push(attr("Known bad", "Yes"));
    }
    attrs.join("\n")
}

/// Warning text when the local version differs from the signoff version.
pub fn outdated_warning(pkg: &SignoffPackage, local: &LocalPackage) -> Option<String> {
    (local.version != pkg.version).then(|| {
        format!(
            "warning: local {} ({}) is not the same as sign-off version ({})",
            pkg.pkgbase, local.version, pkg.version
        )
    })
}

/// Warning text when the package is flagged known-bad.
pub fn known_bad_warning(pkg: &SignoffPackage) -> Option<String> {
    pkg.known_bad
        .then(|| format!("warning: {} is known to be bad", pkg.pkgbase))
}
