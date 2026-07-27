//! MAPI wire protocol implementation.
//!
//! See `docs/DEVELOPMENT.md` §4 for the full protocol reference this module
//! is implemented against.

mod framing;
mod handshake;

use sqlx_core::error::Error;
use sqlx_core::net::{connect_tcp, BufferedSocket, Socket, SocketIntoBox};

use crate::error::MonetError;

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

/// Read a complete MAPI message and decode it as UTF-8 text (the challenge
/// and login-response lines are always plain text, per
/// `docs/DEVELOPMENT.md` §4.1).
async fn read_text_message(stream: &mut MonetStream) -> Result<String, Error> {
    let bytes = read_message(stream).await?;
    String::from_utf8(bytes)
        .map_err(|e| MonetError::Protocol(format!("expected UTF-8 text message: {e}")).into())
}

/// Perform the full MAPI challenge/response handshake against
/// `host:port`, following redirects (`docs/DEVELOPMENT.md` §4.1) up to 10
/// times, and return the authenticated stream ready for query traffic.
///
/// Not yet called by public API — stage D's `MonetConnectOptions::connect`
/// wires this up.
#[allow(dead_code)]
pub(crate) async fn perform_handshake(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    database: &str,
) -> Result<MonetStream, Error> {
    let mut current_host = host.to_string();
    let mut current_port = port;
    let mut reused_stream: Option<MonetStream> = None;

    for _ in 0..10 {
        let mut stream = match reused_stream.take() {
            Some(stream) => stream,
            None => {
                let mut stream = connect(&current_host, current_port).await?;
                send_prime_bytes(&mut stream).await?;
                stream
            }
        };

        let challenge_line = read_text_message(&mut stream).await?;
        let challenge = handshake::parse_challenge(&challenge_line)?;

        let (algo, password_hash) = handshake::compute_password_hash(&challenge, password)?;
        let login_line = handshake::build_login_response(
            &challenge,
            username,
            algo,
            &password_hash,
            database,
            handshake::HandshakeOptions::default(),
        );
        write_message(&mut stream, login_line.as_bytes()).await?;

        let response_line = read_text_message(&mut stream).await?;
        match handshake::parse_login_response(&response_line) {
            handshake::LoginResponse::Ok => return Ok(stream),
            handshake::LoginResponse::Error(message) => {
                return Err(MonetError::Handshake(message).into())
            }
            handshake::LoginResponse::Redirect(target) => {
                match handshake::parse_redirect(&target)? {
                    handshake::Redirect::LocalRetry => {
                        // Same connection, same merger process: just read a
                        // fresh challenge on the next loop iteration.
                        reused_stream = Some(stream);
                    }
                    handshake::Redirect::Reconnect { host, port } => {
                        current_host = host;
                        current_port = port;
                        // reused_stream stays None: next iteration opens a
                        // fresh connection to the new address.
                    }
                }
            }
        }
    }

    Err(MonetError::Handshake("too many redirects (>10) during handshake".to_string()).into())
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

    /// Stage C smoke test: the full challenge/response handshake succeeds
    /// end-to-end against a real MonetDB instance (see the container setup
    /// notes on the sibling test above; same container, database/user/pass
    /// are all `monetdb` per `-e MDB_DB_ADMIN_PASS=monetdb`).
    ///
    /// Run with: `cargo test --features runtime-tokio -- --ignored`
    #[tokio::test]
    #[ignore = "requires a running MonetDB docker instance; see docs/DEVELOPMENT.md stage C step 22"]
    async fn full_handshake_succeeds_against_docker_monetdb() {
        let port: u16 = std::env::var("MONETDB_TEST_PORT")
            .unwrap_or_else(|_| "50001".into())
            .parse()
            .expect("MONETDB_TEST_PORT must be a valid u16 port number");

        perform_handshake("127.0.0.1", port, "monetdb", "monetdb", "monetdb")
            .await
            .expect("handshake against local docker MonetDB instance should succeed");
    }

    /// Wrong password must surface as a handshake error, not silently
    /// succeed or hang.
    #[tokio::test]
    #[ignore = "requires a running MonetDB docker instance; see docs/DEVELOPMENT.md stage C step 22"]
    async fn handshake_with_wrong_password_fails() {
        let port: u16 = std::env::var("MONETDB_TEST_PORT")
            .unwrap_or_else(|_| "50001".into())
            .parse()
            .expect("MONETDB_TEST_PORT must be a valid u16 port number");

        let result = perform_handshake(
            "127.0.0.1",
            port,
            "monetdb",
            "definitely-wrong-password",
            "monetdb",
        )
        .await;

        // `MonetStream` doesn't implement `Debug`, so inspect the error side
        // directly rather than formatting the whole `Result`.
        if result.is_ok() {
            panic!("handshake with a wrong password should have failed");
        }
    }
}
