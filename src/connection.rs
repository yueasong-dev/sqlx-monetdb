//! Database connection to MonetDB.

#![allow(clippy::manual_async_fn)]

use sqlx_core::error::Error;
use sqlx_core::transaction::Transaction;

use crate::database::Monet;
use crate::options::MonetConnectOptions;

/// A connection to a MonetDB server.
///
/// Stage B implements the underlying TCP/socket connection and MAPI
/// protocol initialization (handshake, authentication).
#[derive(Debug)]
pub struct MonetConnection {
    /// Placeholder for the underlying socket/stream; will be replaced
    /// with actual I/O structures in stage B.
    #[allow(dead_code)]
    _inner: (),
}

impl MonetConnection {
    /// Create a new connection (placeholder).
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self { _inner: () }
    }
}

impl sqlx_core::connection::Connection for MonetConnection {
    type Database = Monet;
    type Options = MonetConnectOptions;

    fn close(self) -> impl std::future::Future<Output = Result<(), Error>> + Send + 'static {
        async { unimplemented!("stage B: close socket and send MAPI bye message") }
    }

    fn close_hard(self) -> impl std::future::Future<Output = Result<(), Error>> + Send + 'static {
        async { unimplemented!("stage B: hard close socket without graceful shutdown") }
    }

    fn ping(&mut self) -> impl std::future::Future<Output = Result<(), Error>> + Send + '_ {
        async { unimplemented!("stage B: send MAPI ping or SELECT 1 to verify connection") }
    }

    fn begin(
        &mut self,
    ) -> impl std::future::Future<Output = Result<Transaction<'_, Monet>, Error>> + Send + '_ {
        async { unimplemented!("stage H: begin transaction via TransactionManager") }
    }

    fn shrink_buffers(&mut self) {
        // Placeholder: no-op for now; stage B will manage actual buffers
        unimplemented!("stage B: shrink internal connection buffers")
    }

    fn flush(&mut self) -> impl std::future::Future<Output = Result<(), Error>> + Send + '_ {
        async { unimplemented!("stage B: flush any buffered outgoing data") }
    }

    fn should_flush(&self) -> bool {
        unimplemented!("stage B: check if connection has buffered data to flush")
    }
}
