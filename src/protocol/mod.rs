//! MAPI wire protocol implementation.
//!
//! See `docs/DEVELOPMENT.md` §4 for the full protocol reference this module
//! is implemented against.

mod framing;

use sqlx_core::error::Error;
use sqlx_core::net::{connect_tcp, BufferedSocket, Socket, SocketIntoBox};

/// The transport type every MAPI connection is built on: a boxed, buffered
/// socket, kept runtime-agnostic via `sqlx_core::net`.
pub(crate) type MonetStream = BufferedSocket<Box<dyn Socket>>;

/// Open a plain TCP connection to a MonetDB server and wrap it in a
/// [`BufferedSocket`] for buffered reads/writes.
///
/// This performs no MAPI protocol exchange (no prime bytes, no handshake) —
/// see stage C (`docs/DEVELOPMENT.md` step 13 onward) for that.
// Not yet called: will be used by `MonetConnectOptions::connect` once stage C
// wires the handshake on top of this.
#[allow(dead_code)]
pub(crate) async fn connect(host: &str, port: u16) -> Result<MonetStream, Error> {
    let socket: Box<dyn Socket> = connect_tcp(host, port, SocketIntoBox).await?;
    Ok(BufferedSocket::new(socket))
}

/// Send a complete MAPI message, splitting it into blocks per
/// `docs/DEVELOPMENT.md` §4.2 and flushing the underlying socket.
// Not yet called: wired up once stage C sends the handshake response and
// stage E sends query requests.
#[allow(dead_code)]
pub(crate) async fn write_message(stream: &mut MonetStream, payload: &[u8]) -> Result<(), Error> {
    let mut framed = Vec::with_capacity(payload.len() + 2);
    framing::encode_message(payload, &mut framed);
    stream.write_buffer_mut().put_slice(&framed);
    stream.flush().await?;
    Ok(())
}

/// Read a complete MAPI message, reassembling it from one or more blocks
/// per `docs/DEVELOPMENT.md` §4.2.
///
/// Unlike [`framing::decode_message`] (which parses an already-buffered
/// slice, used by unit tests), this reads block-by-block directly off the
/// socket since the full message length isn't known up front.
// Not yet called: wired up once stage C reads the challenge/login response
// and stage E reads query responses.
#[allow(dead_code)]
pub(crate) async fn read_message(stream: &mut MonetStream) -> Result<Vec<u8>, Error> {
    let mut message = Vec::new();
    loop {
        let header = stream.read_buffered(2).await?;
        let (len, is_last) = framing::decode_header([header[0], header[1]]);

        if len > 0 {
            let payload = stream.read_buffered(len).await?;
            message.extend_from_slice(&payload);
        }

        if is_last {
            return Ok(message);
        }
    }
}
