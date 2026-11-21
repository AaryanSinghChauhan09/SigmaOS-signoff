//! Local pacman database reader.
//!
//! Upstream uses pyalpm to match signoff pkgbases against installed packages.
//! This implementation reads pacman's local database directly: each installed
//! package is a directory under `<db>/local/<pkgname>/` containing a `desc`
//! file with `%NAME%`, `%BASE%`, `%VERSION%`, and `%INSTALLDATE%` fields.

use std::path::{Path, PathBuf};

use crate::{error::SignoffError, model::LocalPackage};

/// Read all installed packages from a pacman local database directory.
///
/// `db_path` is typically `/var/lib/pacman`; the local db lives at
/// `<db_path>/local`.
pub fn read_local_packages(db_path: &Path) -> Result<Vec<LocalPackage>, SignoffError> {
    let local_dir = db_path.join("local");
    let entries = std::fs::read_dir(&local_dir).map_err(|e| {
        SignoffError::Alpm(format!("cannot read {}: {e}", local_dir.display()))
    })?;

    let mut packages = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| SignoffError::Alpm(e.to_string()))?;
        let desc = entry.path().join("desc");
        if desc.is_file() {
            if let Some(pkg) = parse_desc(&desc) {
                packages.push(pkg);
            }
        }
    }
    Ok(packages)
}

/// Find the installed package whose base or name equals `pkgbase`.
pub fn find_by_pkgbase<'a>(
    packages: &'a [LocalPackage],
    pkgbase: &str,
) -> Option<&'a LocalPackage> {
    packages
        .iter()
        .find(|p| p.pkgbase() == pkgbase || p.name == pkgbase)
}

/// Parse a pacman `desc` file into a [`LocalPackage`].
///
/// The format alternates `%FIELD%` marker lines with value lines, e.g.
/// `%NAME%\nfoo\n%VERSION%\n1.0-1\n`.
fn parse_desc(path: &Path) -> Option<LocalPackage> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut name = None;
    let mut base = None;
    let mut version = None;
    let mut installdate = 0i64;

    let mut lines = text.lines().map(str::trim_end);
    while let Some(line) = lines.next() {
        let field = match line {
            "%NAME%" => Some(&mut name),
            "%BASE%" => Some(&mut base),
            "%VERSION%" => Some(&mut version),
            "%INSTALLDATE%" => {
                if let Some(v) = lines.next() {
                    installdate = v.trim().parse().unwrap_or(0);
                }
                None
            }
            _ => None,
        };
        if let Some(slot) = field {
            if let Some(v) = lines.next() {
                let v = v.trim();
                if !v.is_empty() {
                    *slot = Some(v.to_string());
                }
            }
        }
    }

    Some(LocalPackage {
        name: name?,
        base: base.filter(|b| !b.is_empty()),
        version: version?,
        installdate,
    })
}

/// Collect `(signoff_package, local_package)` pairs sorted by pkgbase,
/// mirroring upstream `list_signoffs`.
pub fn list_signoffs<'a>(
    signoffs: &'a [crate::model::SignoffPackage],
    local: &'a [LocalPackage],
) -> Vec<(&'a crate::model::SignoffPackage, Option<&'a LocalPackage>)> {
    let mut sorted: Vec<&crate::model::SignoffPackage> = signoffs.iter().collect();
    sorted.sort_by(|a, b| a.pkgbase.cmp(&b.pkgbase));
    sorted
        .into_iter()
        .map(|pkg| (pkg, find_by_pkgbase(local, &pkg.pkgbase)))
        .collect()
}

/// Ensure `path` is a plausible pacman db dir (has a `local` subdir).
pub fn validate_db_path(db_path: &Path) -> Result<PathBuf, SignoffError> {
    let local = db_path.join("local");
    if local.is_dir() {
        Ok(db_path.to_path_buf())
    } else {
        Err(SignoffError::Alpm(format!(
            "{} does not look like a pacman database (missing local/)",
            db_path.display()
        )))
    }
}
