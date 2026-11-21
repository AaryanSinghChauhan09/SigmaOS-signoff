//! Integration tests for the SigmaOS signoff component.

use sigmaos_signoff::{
    LocalPackage, Options, Signoff, SignoffBackend, SignoffError, SignoffGroup, SignoffPackage,
    SignoffStatus, filter_signoffs, find_by_pkgbase, format_signoff, format_signoff_user,
    known_bad_warning, list_signoffs, outdated_warning, read_local_packages, signoff_status,
};

fn pkgbase(name: &str) -> SignoffPackage {
    SignoffPackage {
        pkgbase: name.into(),
        pkgnames: vec![name.into()],
        repo: "testing".into(),
        arch: "x86_64".into(),
        version: "1.0-1".into(),
        last_update: "2026-11-01T00:00:00".into(),
        packager: "tu".into(),
        comments: None,
        known_bad: false,
        signoffs: vec![],
    }
}

fn signoff(user: &str, created: &str, revoked: bool) -> Signoff {
    Signoff {
        user: user.into(),
        created: created.into(),
        revoked,
    }
}

fn local(name: &str, version: &str) -> LocalPackage {
    LocalPackage {
        name: name.into(),
        base: None,
        version: version.into(),
        installdate: 1_700_000_000,
    }
}

#[test]
fn test_signoff_status_none_without_records() {
    let p = pkgbase("foo");
    assert_eq!(signoff_status(&p, "alice"), SignoffStatus::None);
}

#[test]
fn test_signoff_status_signed_off() {
    let mut p = pkgbase("foo");
    p.signoffs.push(signoff("alice", "2026-01-01T00:00:00", false));
    assert_eq!(signoff_status(&p, "alice"), SignoffStatus::SignedOff);
}

#[test]
fn test_signoff_status_revoked_wins_when_latest() {
    let mut p = pkgbase("foo");
    p.signoffs.push(signoff("alice", "2026-01-01T00:00:00", false));
    p.signoffs.push(signoff("alice", "2026-02-01T00:00:00", true));
    assert_eq!(signoff_status(&p, "alice"), SignoffStatus::Revoked);
}

#[test]
fn test_signoff_status_ignores_other_users() {
    let mut p = pkgbase("foo");
    p.signoffs.push(signoff("bob", "2026-01-01T00:00:00", false));
    assert_eq!(signoff_status(&p, "alice"), SignoffStatus::None);
}

#[test]
fn test_filter_excludes_signed_off_by_default() {
    let mut p = pkgbase("foo");
    p.signoffs.push(signoff("alice", "2026-01-01T00:00:00", false));
    let l = local("foo", "1.0-1");
    let opts = Options {
        username: "alice".into(),
        ..Default::default()
    };
    let out = filter_signoffs(
        vec![(&p, Some(&l))].into_iter(),
        &opts,
    );
    assert!(out.is_empty());
}

#[test]
fn test_filter_includes_signed_off_when_flagged() {
    let mut p = pkgbase("foo");
    p.signoffs.push(signoff("alice", "2026-01-01T00:00:00", false));
    let l = local("foo", "1.0-1");
    let opts = Options {
        username: "alice".into(),
        show_signed_off: true,
        ..Default::default()
    };
    let out = filter_signoffs(vec![(&p, Some(&l))].into_iter(), &opts);
    assert_eq!(out.len(), 1);
}

#[test]
fn test_filter_excludes_uninstalled_by_default() {
    let p = pkgbase("foo");
    let opts = Options::default();
    let out = filter_signoffs(vec![(&p, None)].into_iter(), &opts);
    assert!(out.is_empty());
}

#[test]
fn test_explicit_packages_override_filters() {
    let mut p = pkgbase("foo");
    p.signoffs.push(signoff("alice", "2026-01-01T00:00:00", false));
    let opts = Options {
        username: "alice".into(),
        packages: vec!["foo".into()],
        ..Default::default()
    };
    let out = filter_signoffs(vec![(&p, None)].into_iter(), &opts);
    assert_eq!(out.len(), 1);
}

#[test]
fn test_format_signoff_short_shows_outdated() {
    let p = pkgbase("foo");
    let l = local("foo", "0.9-1");
    let opts = Options {
        username: "alice".into(),
        quiet: true,
        ..Default::default()
    };
    let out = format_signoff(&p, Some(&l), &opts, false);
    assert!(out.contains("foo"));
    assert!(out.contains("outdated"));
}

#[test]
fn test_format_signoff_user_revoked_suffix() {
    assert_eq!(
        format_signoff_user(&signoff("bob", "t", true)),
        "bob (revoked)"
    );
    assert_eq!(format_signoff_user(&signoff("bob", "t", false)), "bob");
}

#[test]
fn test_format_signoff_long_contains_attrs() {
    let mut p = pkgbase("foo");
    p.pkgnames = vec!["foo".into(), "foo-lib".into()];
    p.signoffs.push(signoff("alice", "2026-01-01T00:00:00", false));
    let l = local("foo", "1.0-1");
    let opts = Options {
        username: "alice".into(),
        ..Default::default()
    };
    let out = format_signoff(&p, Some(&l), &opts, false);
    assert!(out.contains("Package base"));
    assert!(out.contains("Signed off"));
    assert!(out.contains(": Yes"));
    assert!(out.contains("alice"));
}

#[test]
fn test_outdated_and_bad_warnings() {
    let mut p = pkgbase("foo");
    let l = local("foo", "0.9-1");
    assert!(outdated_warning(&p, &l).unwrap().contains("not the same"));
    p.known_bad = true;
    assert!(known_bad_warning(&p).unwrap().contains("known to be bad"));
}

#[test]
fn test_list_signoffs_sorted_by_pkgbase() {
    let pkgs = vec![pkgbase("zzz"), pkgbase("aaa")];
    let locals = vec![local("aaa", "1.0-1"), local("zzz", "2.0-1")];
    let pairs = list_signoffs(&pkgs, &locals);
    assert_eq!(pairs[0].0.pkgbase, "aaa");
    assert_eq!(pairs[1].0.pkgbase, "zzz");
    assert!(pairs[0].1.is_some());
}

#[test]
fn test_find_by_pkgbase_matches_base() {
    let mut l = local("foo-bin", "1.0-1");
    l.base = Some("foo".into());
    assert_eq!(find_by_pkgbase(&[l], "foo").unwrap().name, "foo-bin");
}

#[test]
fn test_signoff_group_json_parse_and_version_check() {
    let json = br#"{"version":2,"signoff_groups":[{"pkgbase":"foo","pkgnames":["foo"],"repo":"testing","arch":"x86_64","version":"1.0-1","last_update":"2026-01-01T00:00:00","packager":"tu","comments":null,"known_bad":false,"signoffs":[]}]}"#;
    let group = SignoffGroup::from_json(json).unwrap();
    assert_eq!(group.signoff_groups.len(), 1);
    assert_eq!(group.signoff_groups[0].pkgbase, "foo");
}

#[test]
fn test_signoff_group_rejects_wrong_version() {
    let json = br#"{"version":1,"signoff_groups":[]}"#;
    assert!(SignoffGroup::from_json(json).is_err());
}

#[test]
fn test_read_local_packages_from_pacman_desc() {
    // Arrange: scratch pacman db with one package desc file.
    let dir = tempfile::tempdir().unwrap();
    let pkg_dir = dir.path().join("local/foo-1.0-1");
    std::fs::create_dir_all(&pkg_dir).unwrap();
    std::fs::write(
        pkg_dir.join("desc"),
        "%NAME%\nfoo\n%BASE%\nfoo\n%VERSION%\n1.0-1\n%INSTALLDATE%\n1700000000\n",
    )
    .unwrap();

    // Act
    let pkgs = read_local_packages(dir.path()).unwrap();

    // Assert
    assert_eq!(pkgs.len(), 1);
    assert_eq!(pkgs[0].name, "foo");
    assert_eq!(pkgs[0].version, "1.0-1");
    assert_eq!(pkgs[0].installdate, 1_700_000_000);
}

#[test]
fn test_read_local_packages_missing_db_errors() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read_local_packages(dir.path()).is_err());
}

/// Mock backend for exercising the SignoffBackend contract.
struct MockBackend {
    packages: Vec<SignoffPackage>,
    signed: std::cell::RefCell<Vec<String>>,
    revoked: std::cell::RefCell<Vec<String>>,
}

impl SignoffBackend for MockBackend {
    fn get_signoffs(&self) -> Result<Vec<SignoffPackage>, SignoffError> {
        Ok(self.packages.clone())
    }
    fn signoff_package(&self, pkg: &SignoffPackage) -> Result<(), SignoffError> {
        self.signed.borrow_mut().push(pkg.pkgbase.clone());
        Ok(())
    }
    fn revoke_package(&self, pkg: &SignoffPackage) -> Result<(), SignoffError> {
        self.revoked.borrow_mut().push(pkg.pkgbase.clone());
        Ok(())
    }
}

#[test]
fn test_mock_backend_signoff_and_revoke() {
    let backend = MockBackend {
        packages: vec![pkgbase("foo")],
        signed: Default::default(),
        revoked: Default::default(),
    };
    let pkg = &backend.packages[0];
    backend.signoff_package(pkg).unwrap();
    backend.revoke_package(pkg).unwrap();
    assert_eq!(backend.signed.borrow().as_slice(), ["foo"]);
    assert_eq!(backend.revoked.borrow().as_slice(), ["foo"]);
}
