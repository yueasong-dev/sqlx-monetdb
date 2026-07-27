//! MAPI wire protocol implementation.
//!
//! See `docs/DEVELOPMENT.md` §4 for the full protocol reference this module
//! is implemented against.

mod framing;

use sqlx_core::error::Error;
use sqlx_core::net::{connect_tcp, BufferedSocket, Socket, SocketIntoBox};

/// Open a plain TCP connection to a MonetDB server and wrap it in a
/// [`BufferedSocket`] for buffered reads/writes.
///
/// This performs no MAPI protocol exchange (no prime bytes, no handshake) —
/// see stage C (`docs/DEVELOPMENT.md` step 13 onward) for that.
// Not yet called: will be used by `MonetConnectOptions::connect` once stage C
// wires the handshake on top of this.
#[allow(dead_code)]
pub(crate) async fn connect(
    host: &str,
    port: u16,
) -> Result<BufferedSocket<Box<dyn Socket>>, Error> {
    let socket: Box<dyn Socket> = connect_tcp(host, port, SocketIntoBox).await?;
    Ok(BufferedSocket::new(socket))
}
