//! SigmaOS signoff component.
//!
//! Rust implementation of the [Arch Linux signoff tool]
//! (`archlinux/arch-signoff`), used by the Arch Testing Team to sign off and
//! revoke testing packages via ArchWeb.
//!
//! [Arch Linux signoff tool]: https://github.com/archlinux/arch-signoff

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod localdb;
pub mod model;
pub mod ops;
pub mod session;

pub use error::SignoffError;
pub use localdb::{find_by_pkgbase, list_signoffs, read_local_packages, validate_db_path};
pub use model::{
    LocalPackage, Signoff, SignoffGroup, SignoffPackage, SignoffStatus,
};
pub use ops::{
    Options, filter_signoffs, format_signoff, format_signoff_long, format_signoff_short,
    format_signoff_user, known_bad_warning, outdated_warning, signoff_status,
};
pub use session::{ArchWebBackend, SignoffBackend};
