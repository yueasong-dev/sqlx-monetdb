//! MAPI wire protocol implementation.
//!
//! See `docs/DEVELOPMENT.md` §4 for the full protocol reference this module
//! is implemented against.

mod framing;
mod handshake;

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

/// Send the MAPI "prime" bytes: 8 null bytes written before the server's
/// challenge line on a plain (non-TLS) connection.
///
/// This is a historical technique (present in both pymonetdb and the
/// official C client) to avoid hanging if the client accidentally connects
/// to a TLS-only endpoint; the server ignores these bytes. See
/// `docs/DEVELOPMENT.md` §4.1.
// Not yet called: wired up once stage C's handshake sequencing lands.
#[allow(dead_code)]
pub(crate) async fn send_prime_bytes(stream: &mut MonetStream) -> Result<(), Error> {
    stream.write_buffer_mut().put_slice(&[0u8; 8]);
    stream.flush().await?;
    Ok(())
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

#[cfg(all(test, feature = "runtime-tokio"))]
mod docker_tests {
    use super::*;

    /// Smoke test for stage B: plain TCP connect + prime bytes + reading
    /// the server's raw (unparsed) challenge message end-to-end, against a
    /// real MonetDB instance. Parsing the challenge itself is stage C's job
    /// (`docs/DEVELOPMENT.md` step 15) — this only proves the transport and
    /// block-framing read path work against the real wire format.
    ///
    /// Requires a running MonetDB instance (see `docs/ACCEPTANCE.md` — the
    /// image requires `MDB_DB_ADMIN_PASS` or it exits immediately):
    /// ```sh
    /// docker run -d --name monetdb-test -p 50001:50000 \
    ///     -e MDB_DB_ADMIN_PASS=monetdb monetdb/monetdb:latest
    /// ```
    /// Override the port with `MONETDB_TEST_PORT` if 50001 is taken.
    ///
    /// Run with: `cargo test --features runtime-tokio -- --ignored`
    #[tokio::test]
    #[ignore = "requires a running MonetDB docker instance; see docs/DEVELOPMENT.md stage B step 14"]
    async fn connects_and_reads_raw_challenge_from_docker_monetdb() {
        let port: u16 = std::env::var("MONETDB_TEST_PORT")
            .unwrap_or_else(|_| "50001".into())
            .parse()
            .expect("MONETDB_TEST_PORT must be a valid u16 port number");

        let mut stream = connect("127.0.0.1", port)
            .await
            .expect("TCP connect to local MonetDB docker instance failed");

        send_prime_bytes(&mut stream)
            .await
            .expect("failed to send MAPI prime bytes");

        let challenge = read_message(&mut stream)
            .await
            .expect("failed to read raw challenge message");

        assert!(!challenge.is_empty(), "expected a non-empty challenge line");
        let challenge_text = String::from_utf8(challenge).expect("challenge should be valid UTF-8");

        // Challenge format: salt:servertype:protover:hashes:endian:serverhash:...
        // (docs/DEVELOPMENT.md §4.1). protover must be 9.
        assert!(
            challenge_text.contains(":mserver:9:") || challenge_text.contains(":merovingian:9:"),
            "unexpected challenge format: {challenge_text:?}"
        );
    }
}
