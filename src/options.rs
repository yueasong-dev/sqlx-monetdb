//! Connection options for MonetDB.

#![allow(clippy::manual_async_fn)]

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use log::LevelFilter;
use sqlx_core::error::Error;
use sqlx_core::Url;

use crate::connection::MonetConnection;

/// Options for establishing a connection to MonetDB.
///
/// This is a placeholder that will be expanded in stage B to parse
/// `monetdb://` URLs and handle authentication/TLS options.
#[derive(Debug, Clone)]
pub struct MonetConnectOptions {
    /// Connection URL (placeholder for now).
    #[allow(dead_code)]
    url: String,
}

impl MonetConnectOptions {
    /// Create a new `MonetConnectOptions` from a connection string.
    pub fn new() -> Self {
        Self { url: String::new() }
    }
}

impl Default for MonetConnectOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MonetConnectOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "monetdb://{}", self.url)
    }
}

impl FromStr for MonetConnectOptions {
    type Err = Error;

    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        unimplemented!("stage B: parse monetdb:// URL into connection options")
    }
}

impl sqlx_core::connection::ConnectOptions for MonetConnectOptions {
    type Connection = MonetConnection;

    fn from_url(_url: &Url) -> Result<Self, Error> {
        unimplemented!("stage B: parse Url into MonetConnectOptions")
    }

    fn connect(
        &self,
    ) -> impl std::future::Future<Output = Result<Self::Connection, Error>> + Send + '_
    where
        Self::Connection: Sized,
    {
        async { unimplemented!("stage B: establish TCP connection and perform MAPI handshake") }
    }

    fn log_statements(self, _level: LevelFilter) -> Self {
        unimplemented!("stage B: configure statement logging level")
    }

    fn log_slow_statements(self, _level: LevelFilter, _duration: Duration) -> Self {
        unimplemented!("stage B: configure slow statement logging")
    }
}
