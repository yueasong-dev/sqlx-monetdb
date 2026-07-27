//! Database connection to MonetDB.

#![allow(clippy::manual_async_fn)]

use sqlx_core::error::Error;
use sqlx_core::transaction::Transaction;

use crate::database::Monet;
use crate::options::MonetConnectOptions;
use crate::protocol::MonetStream;

/// A connection to a MonetDB server.
///
/// Holds the buffered MAPI transport; stage C adds the handshake/auth
/// state needed to actually establish one (see
/// `MonetConnectOptions::connect`), and stage D wires up `ping`/`close`.
pub struct MonetConnection {
    pub(crate) stream: MonetStream,
}

impl std::fmt::Debug for MonetConnection {
    // `BufferedSocket`/`Box<dyn Socket>` don't implement `Debug`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MonetConnection").finish_non_exhaustive()
    }
}

impl MonetConnection {
    #[allow(dead_code)] // constructed by stage C once the handshake completes
    pub(crate) fn new(stream: MonetStream) -> Self {
        Self { stream }
    }
}

impl sqlx_core::connection::Connection for MonetConnection {
    type Database = Monet;
    type Options = MonetConnectOptions;

    fn close(mut self) -> impl std::future::Future<Output = Result<(), Error>> + Send + 'static {
        // The MAPI reference implementations (pymonetdb, the official C
        // client) don't send a dedicated logout/bye message on close —
        // closing the TCP connection is sufficient. Flush first so any
        // still-buffered writes aren't silently dropped.
        async move {
            self.stream.flush().await?;
            self.stream.shutdown().await.map_err(Error::from)
        }
    }

    fn close_hard(
        mut self,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send + 'static {
        async move { self.stream.shutdown().await.map_err(Error::from) }
    }

    fn ping(&mut self) -> impl std::future::Future<Output = Result<(), Error>> + Send + '_ {
        // A real ping needs the query protocol (send "SELECT 1", read the
        // response) — MAPI has no dedicated ping/heartbeat message
        // (docs/DEVELOPMENT.md §4.8). Wired up once stage E/G lands.
        async { unimplemented!("stage E/G: send a lightweight SELECT 1 to verify the connection") }
    }

    fn begin(
        &mut self,
    ) -> impl std::future::Future<Output = Result<Transaction<'_, Monet>, Error>> + Send + '_ {
        async { unimplemented!("stage H: begin transaction via TransactionManager") }
    }

    fn shrink_buffers(&mut self) {
        self.stream.shrink_buffers();
    }

    fn flush(&mut self) -> impl std::future::Future<Output = Result<(), Error>> + Send + '_ {
        async { self.stream.flush().await.map_err(Error::from) }
    }

    fn should_flush(&self) -> bool {
        // Every write path in this driver (protocol::write_message) flushes
        // before returning, so there's never unflushed data sitting in the
        // buffer between driver-visible operations.
        false
    }
}
