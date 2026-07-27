//! MAPI challenge/response handshake (protocol version 9).
//!
//! See `docs/DEVELOPMENT.md` §4.1 for the full reference this module is
//! implemented against (cross-verified against pymonetdb, the official C
//! client, and `MonetDB/monetdb-rust`).

// Not yet wired to a real connection: that lands in docs/DEVELOPMENT.md
// step 22 (perform_handshake, orchestrating these pure functions over a
// real MonetStream).
#![allow(dead_code)]

use hex::encode as hex_encode;
use ripemd::Ripemd160;
use sha1::Sha1;
use sha2::{Digest, Sha224, Sha256, Sha384, Sha512};

use crate::error::MonetError;

/// Byte order the server declared it will use for any binary data it sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Endian {
    Big,
    Little,
}

/// The server's challenge line (protocol version 9), colon-separated:
/// `salt:servertype:protover:hashes:endian:serverhash:[sql=N]:[BINARY=N]:[OOBINTR=1]:[CLIENTINFO]:`
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Challenge {
    pub salt: String,
    pub server_type: String,
    pub protocol_version: String,
    /// Candidate hash algorithms for the *response* hash, in the order the
    /// server listed them (client picks the first one it supports, by its
    /// own priority list — see [`select_hash_algorithm`]).
    pub hashes: Vec<String>,
    pub server_endian: Endian,
    /// Algorithm used to pre-hash the raw password (see
    /// [`compute_password_hash`]).
    pub server_hash: String,
    /// The `sql=<N>` handshake option negotiation level, if the server sent
    /// one. Determines which options the client may include in its login
    /// response (`docs/DEVELOPMENT.md` §4.1 table).
    pub handshake_option_level: Option<u8>,
}

/// Parse a raw MAPI challenge line into its fields.
pub(crate) fn parse_challenge(line: &str) -> Result<Challenge, MonetError> {
    let line = line.trim_end_matches(':');
    let fields: Vec<&str> = line.split(':').collect();

    if fields.len() < 6 {
        return Err(MonetError::Handshake(format!(
            "challenge line has {} fields, expected at least 6: {line:?}",
            fields.len()
        )));
    }

    let protocol_version = fields[2].to_string();
    if protocol_version != "9" {
        return Err(MonetError::Handshake(format!(
            "unsupported MAPI protocol version {protocol_version:?}; only \"9\" is supported"
        )));
    }

    let server_endian = match fields[4] {
        "BIG" => Endian::Big,
        "LIT" => Endian::Little,
        other => {
            return Err(MonetError::Handshake(format!(
                "unexpected endian field {other:?} in challenge"
            )))
        }
    };

    let mut handshake_option_level = None;
    for extra in &fields[6..] {
        if let Some(level) = extra.strip_prefix("sql=") {
            handshake_option_level = level.parse().ok();
        }
    }

    Ok(Challenge {
        salt: fields[0].to_string(),
        server_type: fields[1].to_string(),
        protocol_version,
        hashes: fields[3].split(',').map(str::to_string).collect(),
        server_endian,
        server_hash: fields[5].to_string(),
        handshake_option_level,
    })
}

/// Hash algorithms the driver supports for the response hash, in the
/// client's preferred order (strongest first). This exact set and order
/// matches both the official C client and pymonetdb.
const HASH_PRIORITY: &[&str] = &["RIPEMD160", "SHA512", "SHA384", "SHA256", "SHA224", "SHA1"];

/// Pick the strongest algorithm (by [`HASH_PRIORITY`]) that both the driver
/// and the server (per its advertised `hashes` list) support.
pub(crate) fn select_hash_algorithm(server_hashes: &[String]) -> Option<&'static str> {
    HASH_PRIORITY
        .iter()
        .find(|&&candidate| server_hashes.iter().any(|h| h == candidate))
        .copied()
}

/// Hex-digest `data` with the named algorithm (one of [`HASH_PRIORITY`]).
/// Returns `None` for any other name.
fn hash_hex(algorithm: &str, data: &[u8]) -> Option<String> {
    let hex = match algorithm {
        "RIPEMD160" => hex_encode(Ripemd160::digest(data)),
        "SHA512" => hex_encode(Sha512::digest(data)),
        "SHA384" => hex_encode(Sha384::digest(data)),
        "SHA256" => hex_encode(Sha256::digest(data)),
        "SHA224" => hex_encode(Sha224::digest(data)),
        "SHA1" => hex_encode(Sha1::digest(data)),
        _ => return None,
    };
    Some(hex)
}

/// Compute the MAPI two-level password hash (`docs/DEVELOPMENT.md` §4.1):
///
/// 1. Hash the raw password with the server's advertised `server_hash`
///    algorithm, producing a hex string.
/// 2. Hash `(that hex string) || salt` (plain concatenation, no
///    separator) with the strongest mutually-supported algorithm from
///    `hashes`, producing another hex string.
///
/// Returns `(algorithm_name, hex_digest)` — the caller wraps this as
/// `{algorithm_name}hex_digest` in the login response.
pub(crate) fn compute_password_hash(
    challenge: &Challenge,
    password: &str,
) -> Result<(&'static str, String), MonetError> {
    let prehashed = hash_hex(&challenge.server_hash, password.as_bytes()).ok_or_else(|| {
        MonetError::Handshake(format!(
            "unsupported server_hash algorithm {:?}",
            challenge.server_hash
        ))
    })?;

    let algo = select_hash_algorithm(&challenge.hashes).ok_or_else(|| {
        MonetError::Handshake(format!(
            "no mutually supported hash algorithm in server list {:?}",
            challenge.hashes
        ))
    })?;

    let mut combined = prehashed.into_bytes();
    combined.extend_from_slice(challenge.salt.as_bytes());
    let response_hash =
        hash_hex(algo, &combined).expect("algo came from HASH_PRIORITY, hash_hex must support it");

    Ok((algo, response_hash))
}

/// Handshake options the client may negotiate (`docs/DEVELOPMENT.md` §4.1
/// levels 1-5). v1 only sends levels 1 (`auto_commit`) and 3
/// (`size_header`, always on so the driver can read column
/// precision/scale/length from `%...#typesizes`) — levels 2, 4, 5
/// (`reply_size`, `columnar_protocol`, `time_zone`) are left at the
/// server's defaults for now.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HandshakeOptions {
    pub auto_commit: bool,
}

impl Default for HandshakeOptions {
    fn default() -> Self {
        Self { auto_commit: true }
    }
}

/// Build the client's login response line
/// (`docs/DEVELOPMENT.md` §4.1: `endian:user:{algo}hash:language:database:FILETRANS:[options]:`).
pub(crate) fn build_login_response(
    challenge: &Challenge,
    username: &str,
    password_algo: &str,
    password_hash_hex: &str,
    database: &str,
    options: HandshakeOptions,
) -> String {
    // The server does not care which byte order the client declares here
    // (it only affects how the client would interpret binary data *it*
    // sends, which this driver's v1 text-only protocol path never does).
    // pymonetdb hardcodes "BIG" and monetdb-rust uses the host's native
    // order — both work in practice, so this uses a fixed value rather
    // than `cfg!(target_endian)` to keep this function's output (and its
    // tests) portable across host architectures.
    let mut parts = vec![
        "BIG".to_string(),
        username.to_string(),
        format!("{{{password_algo}}}{password_hash_hex}"),
        "sql".to_string(),
        database.to_string(),
        "FILETRANS".to_string(),
    ];

    let level = challenge.handshake_option_level.unwrap_or(0);
    if level >= 1 {
        parts.push(format!("auto_commit={}", i32::from(options.auto_commit)));
    }
    if level >= 3 {
        parts.push("size_header=1".to_string());
    }

    format!("{}:", parts.join(":"))
}

/// The server's response to the client's login line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoginResponse {
    /// Empty body, `=OK`, or a `#`-prefixed welcome message.
    Ok,
    /// `!`-prefixed: authentication failed or the server rejected the
    /// login for another reason. Holds the message with the `!` stripped.
    Error(String),
    /// `^`-prefixed: redirect to another server/proxy. Holds the target
    /// with the `^` stripped (see [`parse_redirect`]).
    Redirect(String),
}

/// Parse the server's response to the client's login line.
pub(crate) fn parse_login_response(line: &str) -> LoginResponse {
    if line.is_empty() || line == "=OK" || line.starts_with('#') {
        LoginResponse::Ok
    } else if let Some(rest) = line.strip_prefix('!') {
        LoginResponse::Error(rest.to_string())
    } else if let Some(rest) = line.strip_prefix('^') {
        LoginResponse::Redirect(rest.to_string())
    } else {
        // Not a documented case, but treating unknown formats as success
        // would risk silently proceeding on a garbled/unexpected response.
        LoginResponse::Error(format!("unexpected login response: {line:?}"))
    }
}

/// Where a `^`-prefixed redirect points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Redirect {
    /// `mapi:merovingian://proxy...` — retry the handshake on the *same*
    /// connection (the merger process will proxy to the real server).
    LocalRetry,
    /// `mapi:monetdb://host:port/...` — close the current connection and
    /// reconnect to this host/port before retrying the handshake.
    Reconnect { host: String, port: u16 },
}

/// Parse a redirect target (the string inside a [`LoginResponse::Redirect`],
/// i.e. with the leading `^` already stripped).
pub(crate) fn parse_redirect(target: &str) -> Result<Redirect, MonetError> {
    if target.contains("merovingian") {
        return Ok(Redirect::LocalRetry);
    }

    let rest = target.strip_prefix("mapi:monetdb://").ok_or_else(|| {
        MonetError::Handshake(format!("unrecognized redirect target: {target:?}"))
    })?;
    let host_port = rest.split('/').next().unwrap_or(rest);
    let (host, port_str) = host_port.rsplit_once(':').ok_or_else(|| {
        MonetError::Handshake(format!("redirect target missing port: {target:?}"))
    })?;
    let port: u16 = port_str
        .parse()
        .map_err(|_| MonetError::Handshake(format!("invalid port in redirect: {target:?}")))?;

    Ok(Redirect::Reconnect {
        host: host.to_string(),
        port,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_typical_challenge_line() {
        // Example from docs/DEVELOPMENT.md §4.1 (sourced from connect.c).
        let challenge = parse_challenge("rBuCQ9WTn3:mserver:9:RIPEMD160,SHA256,SHA1,MD5:LIT:SHA1:")
            .expect("valid challenge");

        assert_eq!(challenge.salt, "rBuCQ9WTn3");
        assert_eq!(challenge.server_type, "mserver");
        assert_eq!(challenge.protocol_version, "9");
        assert_eq!(challenge.hashes, vec!["RIPEMD160", "SHA256", "SHA1", "MD5"]);
        assert_eq!(challenge.server_endian, Endian::Little);
        assert_eq!(challenge.server_hash, "SHA1");
        assert_eq!(challenge.handshake_option_level, None);
    }

    #[test]
    fn parses_sql_level_option() {
        let challenge = parse_challenge("salt:mserver:9:RIPEMD160,SHA1:BIG:SHA1:sql=6:BINARY=1:")
            .expect("valid challenge");
        assert_eq!(challenge.handshake_option_level, Some(6));
        assert_eq!(challenge.server_endian, Endian::Big);
    }

    #[test]
    fn rejects_unsupported_protocol_version() {
        let err = parse_challenge("salt:mserver:8:RIPEMD160:BIG:SHA1:").unwrap_err();
        assert!(matches!(err, MonetError::Handshake(_)));
    }

    #[test]
    fn rejects_too_few_fields() {
        let err = parse_challenge("salt:mserver:9:").unwrap_err();
        assert!(matches!(err, MonetError::Handshake(_)));
    }

    #[test]
    fn hash_priority_picks_strongest_mutual_algorithm() {
        let server_hashes: Vec<String> = ["SHA1", "MD5", "SHA256"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        // SHA256 outranks SHA1 in HASH_PRIORITY; MD5 isn't a candidate at all.
        assert_eq!(select_hash_algorithm(&server_hashes), Some("SHA256"));
    }

    #[test]
    fn hash_priority_returns_none_when_no_overlap() {
        let server_hashes: Vec<String> = vec!["MD5".to_string()];
        assert_eq!(select_hash_algorithm(&server_hashes), None);
    }

    // Known-answer tests for hash_hex, standard NIST/RFC test vectors for
    // the ASCII input "abc" — confirms the dispatcher wires each algorithm
    // name to the right implementation, not just "compiles".
    #[test]
    fn hash_hex_known_answer_vectors_for_abc() {
        assert_eq!(
            hash_hex("SHA1", b"abc").unwrap(),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hash_hex("SHA256", b"abc").unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hash_hex("SHA224", b"abc").unwrap(),
            "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7"
        );
        assert_eq!(
            hash_hex("SHA384", b"abc").unwrap(),
            "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"
        );
        assert_eq!(
            hash_hex("SHA512", b"abc").unwrap(),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
        assert_eq!(
            hash_hex("RIPEMD160", b"abc").unwrap(),
            "8eb208f7e05d987a9b044a8e98c6b087f15a0bfc"
        );
        assert_eq!(hash_hex("MD5", b"abc"), None);
    }

    #[test]
    fn compute_password_hash_matches_manual_double_hash() {
        let challenge = Challenge {
            salt: "s0m3salt".to_string(),
            server_type: "mserver".to_string(),
            protocol_version: "9".to_string(),
            hashes: vec!["SHA256".to_string(), "SHA1".to_string()],
            server_endian: Endian::Little,
            server_hash: "SHA1".to_string(),
            handshake_option_level: None,
        };

        let (algo, digest) = compute_password_hash(&challenge, "hunter2").unwrap();
        assert_eq!(algo, "SHA256"); // outranks SHA1 in HASH_PRIORITY

        let prehashed = hash_hex("SHA1", b"hunter2").unwrap();
        let mut combined = prehashed.into_bytes();
        combined.extend_from_slice(b"s0m3salt");
        let expected = hash_hex("SHA256", &combined).unwrap();

        assert_eq!(digest, expected);
    }

    #[test]
    fn build_login_response_includes_options_gated_by_sql_level() {
        let challenge = Challenge {
            salt: "salt".to_string(),
            server_type: "mserver".to_string(),
            protocol_version: "9".to_string(),
            hashes: vec!["SHA1".to_string()],
            server_endian: Endian::Little,
            server_hash: "SHA1".to_string(),
            handshake_option_level: Some(3),
        };

        let response = build_login_response(
            &challenge,
            "monetdb",
            "SHA1",
            "deadbeef",
            "monetdb",
            HandshakeOptions { auto_commit: true },
        );

        assert!(response.starts_with("BIG:monetdb:{SHA1}deadbeef:sql:monetdb:FILETRANS:"));
        assert!(response.contains("auto_commit=1"));
        assert!(response.contains("size_header=1"));
        assert!(response.ends_with(':'));
    }

    #[test]
    fn build_login_response_omits_size_header_below_level_3() {
        let challenge = Challenge {
            salt: "salt".to_string(),
            server_type: "mserver".to_string(),
            protocol_version: "9".to_string(),
            hashes: vec!["SHA1".to_string()],
            server_endian: Endian::Little,
            server_hash: "SHA1".to_string(),
            handshake_option_level: Some(1),
        };

        let response = build_login_response(
            &challenge,
            "monetdb",
            "SHA1",
            "deadbeef",
            "monetdb",
            HandshakeOptions { auto_commit: false },
        );

        assert!(response.contains("auto_commit=0"));
        assert!(!response.contains("size_header"));
    }

    #[test]
    fn parses_login_ok_variants() {
        assert_eq!(parse_login_response(""), LoginResponse::Ok);
        assert_eq!(parse_login_response("=OK"), LoginResponse::Ok);
        assert_eq!(
            parse_login_response("#some welcome message"),
            LoginResponse::Ok
        );
    }

    #[test]
    fn parses_login_error() {
        assert_eq!(
            parse_login_response("!InvalidCredentialsException:bad login"),
            LoginResponse::Error("InvalidCredentialsException:bad login".to_string())
        );
    }

    #[test]
    fn parses_login_redirect() {
        assert_eq!(
            parse_login_response("^mapi:monetdb://otherhost:12345/db"),
            LoginResponse::Redirect("mapi:monetdb://otherhost:12345/db".to_string())
        );
    }

    #[test]
    fn unexpected_login_response_is_treated_as_error_not_silently_ok() {
        assert!(matches!(
            parse_login_response("garbage"),
            LoginResponse::Error(_)
        ));
    }

    #[test]
    fn parses_redirect_to_another_host() {
        let redirect = parse_redirect("mapi:monetdb://otherhost:12345/mydb").unwrap();
        assert_eq!(
            redirect,
            Redirect::Reconnect {
                host: "otherhost".to_string(),
                port: 12345
            }
        );
    }

    #[test]
    fn parses_local_merovingian_redirect() {
        let redirect = parse_redirect("mapi:merovingian://proxy?").unwrap();
        assert_eq!(redirect, Redirect::LocalRetry);
    }

    #[test]
    fn rejects_unrecognized_redirect_format() {
        assert!(parse_redirect("not-a-redirect").is_err());
    }
}
