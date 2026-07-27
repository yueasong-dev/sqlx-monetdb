//! Connection options for MonetDB.

#![allow(clippy::manual_async_fn)]

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use log::LevelFilter;
use sqlx_core::connection::LogSettings;
use sqlx_core::error::Error;
use sqlx_core::Url;

use crate::connection::MonetConnection;
use crate::error::MonetError;

/// Default MAPI port (`docs/DEVELOPMENT.md` §4).
const DEFAULT_PORT: u16 = 50000;

/// Options for establishing a connection to MonetDB, parsed from a
/// `monetdb://user:pass@host:port/database` URL or built up with the
/// builder methods below.
#[derive(Debug, Clone)]
pub struct MonetConnectOptions {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) database: String,
    log_settings: LogSettings,
}

impl MonetConnectOptions {
    /// Create options with the same defaults as [`Default`]: connects to
    /// `localhost:50000` as user `monetdb` against database `monetdb` (the
    /// database the official Docker image creates when given
    /// `MDB_DB_ADMIN_PASS`, see `docs/ACCEPTANCE.md`).
    pub fn new() -> Self {
        Self::default()
    }

    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn username(mut self, username: impl Into<String>) -> Self {
        self.username = username.into();
        self
    }

    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = password.into();
        self
    }

    pub fn database(mut self, database: impl Into<String>) -> Self {
        self.database = database.into();
        self
    }
}

impl Default for MonetConnectOptions {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: DEFAULT_PORT,
            username: "monetdb".to_string(),
            password: "monetdb".to_string(),
            database: "monetdb".to_string(),
            log_settings: LogSettings::default(),
        }
    }
}

impl fmt::Display for MonetConnectOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "monetdb://{}@{}:{}/{}",
            self.username, self.host, self.port, self.database
        )
    }
}

impl FromStr for MonetConnectOptions {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let url = Url::parse(s)
            .map_err(|e| MonetError::Handshake(format!("invalid connection URL: {e}")))?;
        <Self as sqlx_core::connection::ConnectOptions>::from_url(&url)
    }
}

impl sqlx_core::connection::ConnectOptions for MonetConnectOptions {
    type Connection = MonetConnection;

    fn from_url(url: &Url) -> Result<Self, Error> {
        let mut options = Self::default();

        if let Some(host) = url.host_str() {
            options.host = host.to_string();
        }
        if let Some(port) = url.port() {
            options.port = port;
        }
        if !url.username().is_empty() {
            options.username = url.username().to_string();
        }
        if let Some(password) = url.password() {
            options.password = password.to_string();
        }
        let database = url.path().trim_start_matches('/');
        if !database.is_empty() {
            options.database = database.to_string();
        }

        Ok(options)
    }

    fn connect(
        &self,
    ) -> impl std::future::Future<Output = Result<Self::Connection, Error>> + Send + '_
    where
        Self::Connection: Sized,
    {
        async move {
            let stream = crate::protocol::perform_handshake(
                &self.host,
                self.port,
                &self.username,
                &self.password,
                &self.database,
            )
            .await?;
            Ok(MonetConnection::new(stream))
        }
    }

    fn log_statements(mut self, level: LevelFilter) -> Self {
        self.log_settings.log_statements(level);
        self
    }

    fn log_slow_statements(mut self, level: LevelFilter, duration: Duration) -> Self {
        self.log_settings.log_slow_statements(level, duration);
        self
    }
}

#[cfg(all(test, feature = "runtime-tokio"))]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_connection_url() {
        let options: MonetConnectOptions = "monetdb://alice:secret@example.com:12345/mydb"
            .parse()
            .expect("valid URL");

        assert_eq!(options.host, "example.com");
        assert_eq!(options.port, 12345);
        assert_eq!(options.username, "alice");
        assert_eq!(options.password, "secret");
        assert_eq!(options.database, "mydb");
    }

    #[test]
    fn missing_url_parts_fall_back_to_defaults() {
        let options: MonetConnectOptions = "monetdb://example.com".parse().expect("valid URL");

        let defaults = MonetConnectOptions::default();
        assert_eq!(options.host, "example.com");
        assert_eq!(options.port, defaults.port);
        assert_eq!(options.username, defaults.username);
        assert_eq!(options.password, defaults.password);
        assert_eq!(options.database, defaults.database);
    }

    #[test]
    fn builder_methods_override_defaults() {
        let options = MonetConnectOptions::new()
            .host("db.internal")
            .port(50042)
            .username("bob")
            .password("hunter2")
            .database("analytics");

        assert_eq!(options.host, "db.internal");
        assert_eq!(options.port, 50042);
        assert_eq!(options.username, "bob");
        assert_eq!(options.password, "hunter2");
        assert_eq!(options.database, "analytics");
    }

    /// Stage D smoke test: the public `ConnectOptions::connect` /
    /// `Connection::close` path works end-to-end against a real MonetDB
    /// instance (same container as the stage B/C tests — see
    /// `docs/ACCEPTANCE.md`).
    ///
    /// Run with: `cargo test --features runtime-tokio -- --ignored`
    #[tokio::test]
    #[ignore = "requires a running MonetDB docker instance; see docs/DEVELOPMENT.md stage D step 27"]
    async fn connect_and_close_against_docker_monetdb() {
        use sqlx_core::connection::{ConnectOptions, Connection};

        let port: u16 = std::env::var("MONETDB_TEST_PORT")
            .unwrap_or_else(|_| "50001".into())
            .parse()
            .expect("MONETDB_TEST_PORT must be a valid u16 port number");

        let options = MonetConnectOptions::new()
            .host("127.0.0.1")
            .port(port)
            .username("monetdb")
            .password("monetdb")
            .database("monetdb");

        let connection = options
            .connect()
            .await
            .expect("connect() should succeed against local docker MonetDB instance");

        connection.close().await.expect("close() should succeed");
    }
}
