//! `sigmaos-signoff` CLI — Arch Linux package signoff tool.
//!
//! Usage mirrors upstream `signoff`:
//!   sigmaos-signoff list [--uninstalled] [--signed-off] [-q]
//!   sigmaos-signoff signoff [--noconfirm] [pkgbase...]
//!   sigmaos-signoff revoke [--noconfirm] [pkgbase...]
//!
//! Credentials come from ARCHWEB_USERNAME / ARCHWEB_PASSWORD.

use std::path::PathBuf;
use std::process::ExitCode;

use sigmaos_signoff::{
    ArchWebBackend, LocalPackage, Options, SignoffBackend, SignoffStatus, filter_signoffs,
    format_signoff, known_bad_warning, list_signoffs, outdated_warning, read_local_packages,
    signoff_status, validate_db_path,
};

struct Cli {
    action: String,
    packages: Vec<String>,
    uninstalled: bool,
    signed_off: bool,
    quiet: bool,
    noconfirm: bool,
    nocolor: bool,
    db_path: PathBuf,
    username: String,
    password: String,
}

fn parse_args() -> Result<Cli, String> {
    let mut action = String::new();
    let mut packages = Vec::new();
    let mut uninstalled = false;
    let mut signed_off = false;
    let mut quiet = false;
    let mut noconfirm = false;
    let mut nocolor = false;
    let mut db_path = PathBuf::from("/var/lib/pacman");
    let mut username = std::env::var("ARCHWEB_USERNAME").unwrap_or_default();
    let mut password = std::env::var("ARCHWEB_PASSWORD").unwrap_or_default();

    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "-l" | "--list" => action = "list".into(),
            "-s" | "--signoff" => action = "signoff".into(),
            "-r" | "--revoke" => action = "revoke".into(),
            "-i" | "--interactive" => action = "interactive".into(),
            "-u" | "--uninstalled" => uninstalled = true,
            "-a" | "--signed-off" => signed_off = true,
            "-q" | "--quiet" => quiet = true,
            "--noconfirm" => noconfirm = true,
            "--nocolor" => nocolor = true,
            "-b" | "--db-path" => {
                db_path = PathBuf::from(args.next().ok_or("--db-path needs a value")?);
            }
            "--username" => {
                username = args.next().ok_or("--username needs a value")?;
            }
            "--password" => {
                password = args.next().ok_or("--password needs a value")?;
            }
            _ if arg.starts_with('-') => return Err(format!("unknown option: {arg}")),
            _ => packages.push(arg),
        }
    }

    if action.is_empty() {
        action = if packages.is_empty() { "list" } else { "signoff" }.into();
    }
    Ok(Cli {
        action,
        packages,
        uninstalled,
        signed_off,
        quiet,
        noconfirm,
        nocolor,
        db_path,
        username,
        password,
    })
}

fn print_usage() {
    eprintln!(
        "sigmaos-signoff — Arch Linux package signoff tool\n\
         \n\
         USAGE:\n    sigmaos-signoff [OPTIONS] [PKGBASE...]\n\
         \n\
         ACTIONS:\n    -l, --list          list packages that can be signed off (default)\n    \
             -s, --signoff       sign off packages\n    -r, --revoke        revoke signed-off packages\n    \
             -i, --interactive   interactively sign off or revoke\n\
         \n\
         OPTIONS:\n    -u, --uninstalled   include uninstalled packages when listing\n    \
             -a, --signed-off    include signed-off packages when listing\n    -q, --quiet         \
             be less verbose when listing\n    -b, --db-path PATH  pacman database path \
             (default /var/lib/pacman)\n        --username USER  ArchWeb username \
             (ARCHWEB_USERNAME)\n        --password PASS  ArchWeb password \
             (ARCHWEB_PASSWORD)\n        --noconfirm       don't ask for confirmation\n        \
             --nocolor         strip ANSI styles from output\n\
         \n\
         CREDENTIALS: set ARCHWEB_USERNAME and ARCHWEB_PASSWORD, e.g.\n    \
         alias sigmaos-signoff='ARCHWEB_PASSWORD=\"$(pass archweb)\" sigmaos-signoff'"
    );
}

fn confirm(prompt: &str) -> bool {
    eprint!("{prompt} [y/N] ");
    use std::io::Write;
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).ok();
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}

fn run(cli: &Cli) -> Result<(), String> {
    validate_db_path(&cli.db_path).map_err(|e| e.to_string())?;
    let local: Vec<LocalPackage> = read_local_packages(&cli.db_path).map_err(|e| e.to_string())?;
    let backend = ArchWebBackend::new("https://archlinux.org/", &cli.username, &cli.password)
        .map_err(|e| e.to_string())?;

    let signoffs = backend.get_signoffs().map_err(|e| e.to_string())?;
    let pairs = list_signoffs(&signoffs, &local);
    let options = Options {
        packages: cli.packages.clone(),
        show_signed_off: cli.signed_off,
        show_uninstalled: cli.uninstalled,
        quiet: cli.quiet,
        username: cli.username.clone(),
        nocolor: cli.nocolor,
    };
    let color = !cli.nocolor && std::env::var("TERM").map(|t| t != "dumb").unwrap_or(false);
    let filtered = filter_signoffs(pairs.iter().copied(), &options);

    // Validate explicit package names exist in signoffs.
    for pkg in &options.packages {
        if !signoffs.iter().any(|p| p.pkgbase == *pkg) {
            return Err(format!("package base {pkg} not found in signoffs"));
        }
    }

    match cli.action.as_str() {
        "list" => {
            for (pkg, local_pkg) in filtered {
                println!("{}", format_signoff(pkg, local_pkg, &options, color));
                if !cli.quiet {
                    println!();
                }
            }
        }
        "signoff" | "revoke" | "interactive" => {
            let revoke_mode = cli.action == "revoke";
            for (pkg, local_pkg) in &filtered {
                let local_pkg = (*local_pkg)
                    .ok_or_else(|| format!("{} package not installed", pkg.pkgbase))?;
                if let Some(w) = outdated_warning(pkg, local_pkg) {
                    eprintln!("{w}");
                }
                if let Some(w) = known_bad_warning(pkg) {
                    eprintln!("{w}");
                }
            }

            let pkgbases: Vec<&str> = filtered.iter().map(|(p, _)| p.pkgbase.as_str()).collect();
            if pkgbases.is_empty() {
                return Ok(());
            }
            if cli.action == "interactive" {
                for (pkg, local_pkg) in &filtered {
                    let local_pkg = *local_pkg;
                    println!("{}", format_signoff(pkg, local_pkg, &options, color));
                    let status = signoff_status(pkg, &cli.username);
                    let prompt = if status == SignoffStatus::SignedOff {
                        format!("Revoke sign-off for {}?", pkg.pkgbase)
                    } else {
                        format!("Sign off {}?", pkg.pkgbase)
                    };
                    if !cli.noconfirm && !confirm(&prompt) {
                        continue;
                    }
                    let res = if status == SignoffStatus::SignedOff {
                        backend.revoke_package(pkg)
                    } else {
                        backend.signoff_package(pkg)
                    };
                    match res {
                        Ok(()) => println!(
                            "{} {}.",
                            if status == SignoffStatus::SignedOff {
                                "Revoked sign-off for"
                            } else {
                                "Signed off"
                            },
                            pkg.pkgbase
                        ),
                        Err(e) => eprintln!("error for {}: {e}", pkg.pkgbase),
                    }
                    println!();
                }
            } else {
                let verb = if revoke_mode { "Revoke sign-off for" } else { "Sign off" };
                if !cli.noconfirm
                    && !confirm(&format!("{} {}?", verb, pkgbases.join(" ")))
                {
                    return Ok(());
                }
                for (pkg, _) in &filtered {
                    let res = if revoke_mode {
                        backend.revoke_package(pkg)
                    } else {
                        backend.signoff_package(pkg)
                    };
                    match res {
                        Ok(()) if revoke_mode => {
                            println!("Revoked sign-off for {}.", pkg.pkgbase)
                        }
                        Ok(()) => println!("Signed off {}.", pkg.pkgbase),
                        Err(e) => eprintln!("Could not sign off {} ({e})", pkg.pkgbase),
                    }
                }
            }
        }
        other => return Err(format!("unknown action: {other}")),
    }

    let _ = backend.logout();
    Ok(())
}

fn main() -> ExitCode {
    let cli = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            print_usage();
            return ExitCode::from(2);
        }
    };
    if cli.username.is_empty() || cli.password.is_empty() {
        eprintln!("error: ARCHWEB_USERNAME and ARCHWEB_PASSWORD must be set");
        return ExitCode::from(2);
    }
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

