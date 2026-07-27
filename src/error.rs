//! Driver-specific error types and conversions into [`sqlx_core::Error`].
//!
//! MonetDB's MAPI protocol distinguishes two kinds of failure (see
//! `docs/DEVELOPMENT.md` §4.7):
//!
//! - a `!`-prefixed line returned by the server while executing a query —
//!   this is a genuine database error and is represented by
//!   [`MonetDatabaseError`], which implements sqlx's [`DatabaseError`] trait
//!   so it surfaces through `sqlx_core::Error::Database`.
//! - everything else that can go wrong in the driver itself (handshake
//!   failure, a malformed wire message, I/O failure) — represented by
//!   [`MonetError`].

use std::fmt;

use sqlx_core::error::{DatabaseError, Error as SqlxError, ErrorKind};

/// A `!`-prefixed error line returned by the MonetDB server.
///
/// The driver does not attempt fine-grained SQLSTATE-based classification:
/// the pymonetdb reference implementation's prefix-parsing for this is not
/// backed by a documented protocol specification (see
/// `docs/DEVELOPMENT.md` §4.7), so the full server message is preserved
/// as-is rather than risking a wrong classification.
#[derive(Debug)]
pub struct MonetDatabaseError {
    message: String,
}

impl MonetDatabaseError {
    // Not yet called: will be constructed by the `!`-line parser added in
    // docs/DEVELOPMENT.md stage E (step 35).
    #[allow(dead_code)]
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MonetDatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MonetDatabaseError {}

impl DatabaseError for MonetDatabaseError {
    fn message(&self) -> &str {
        &self.message
    }

    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }

    fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
        self
    }
}

/// Driver-internal errors that are not a server-reported SQL error.
#[derive(Debug)]
pub enum MonetError {
    /// I/O failure on the underlying socket.
    Io(std::io::Error),
    /// The MAPI challenge/response handshake failed (bad credentials,
    /// unsupported protocol version, unexpected challenge format).
    Handshake(String),
    /// The server sent a message the driver could not parse as valid MAPI
    /// wire protocol (see `docs/DEVELOPMENT.md` §4 for the expected format).
    Protocol(String),
}

impl fmt::Display for MonetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MonetError::Io(err) => write!(f, "I/O error: {err}"),
            MonetError::Handshake(msg) => write!(f, "MAPI handshake failed: {msg}"),
            MonetError::Protocol(msg) => write!(f, "MAPI protocol error: {msg}"),
        }
    }
}

impl std::error::Error for MonetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MonetError::Io(err) => Some(err),
            MonetError::Handshake(_) | MonetError::Protocol(_) => None,
        }
    }
}

impl From<std::io::Error> for MonetError {
    fn from(err: std::io::Error) -> Self {
        MonetError::Io(err)
    }
}

impl From<MonetError> for SqlxError {
    fn from(err: MonetError) -> Self {
        match err {
            MonetError::Io(io_err) => SqlxError::Io(io_err),
            MonetError::Handshake(msg) => SqlxError::Protocol(format!("handshake failed: {msg}")),
            MonetError::Protocol(msg) => SqlxError::Protocol(msg),
        }
    }
}

// Note: `sqlx_core` provides a blanket `impl<E: DatabaseError> From<E> for
// Error`, so `MonetDatabaseError` converts into `sqlx_core::Error::Database`
// automatically — no manual `From` impl needed (and one would conflict).
