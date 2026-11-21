//! Error types for the signoff component.

/// Errors that can occur while talking to ArchWeb or processing signoffs.
#[derive(Debug, thiserror::Error)]
pub enum SignoffError {
    /// Authentication against ArchWeb failed.
    #[error("authentication failed: {0}")]
    Auth(String),

    /// An HTTP transport error occurred.
    #[error("http error: {0}")]
    Http(String),

    /// A response could not be parsed.
    #[error("parse error: {0}")]
    Parse(String),

    /// The pacman local database could not be read.
    #[error("alpm error: {0}")]
    Alpm(String),
}
