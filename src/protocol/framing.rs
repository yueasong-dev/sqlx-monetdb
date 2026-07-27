//! MAPI block framing: encoding/decoding the length-prefixed block format
//! that wraps every logical MAPI message.
//!
//! See `docs/DEVELOPMENT.md` §4.2. Key facts (cross-verified against
//! pymonetdb, the official C client, and `MonetDB/monetdb-rust`):
//!
//! - Max payload per block is 8190 bytes.
//! - The 2-byte header is **always little-endian**, regardless of host or
//!   negotiated server byte order: `header = (payload_len << 1) | last_bit`.
//! - A message with zero-byte payload is still terminated by an explicit
//!   `header = 1` (length 0, last bit set) block — this doubles as a
//!   "flush marker".
//! - Block boundaries are independent of UTF-8 character boundaries: never
//!   validate/decode text until a full message has been reassembled.

// Not yet wired to a real socket: that lands in docs/DEVELOPMENT.md step 11
// (read_message/write_message built on top of these pure functions).
#![allow(dead_code)]

/// Maximum payload bytes carried by a single MAPI block.
pub(crate) const MAX_BLOCK_PAYLOAD: usize = 8 * 1024 - 2;

/// Encode a single block header: `(payload_len << 1) | last_bit`, as raw
/// little-endian bytes.
fn encode_header(payload_len: usize, is_last: bool) -> [u8; 2] {
    debug_assert!(
        payload_len <= MAX_BLOCK_PAYLOAD,
        "block payload exceeds MAX_BLOCK_PAYLOAD"
    );
    let header = ((payload_len as u16) << 1) | u16::from(is_last);
    header.to_le_bytes()
}

/// Decode a single block header into `(payload_len, is_last)`.
///
/// `pub(crate)` (rather than private): [`super::read_message`] reads one
/// block at a time from the socket and needs this same primitive — it
/// can't use [`decode_message`], which assumes the full message is
/// already buffered in memory.
pub(crate) fn decode_header(bytes: [u8; 2]) -> (usize, bool) {
    let header = u16::from_le_bytes(bytes);
    ((header >> 1) as usize, header & 1 == 1)
}

/// Encode `payload` as one or more MAPI blocks, appending the raw wire
/// bytes (2-byte header + payload, repeated) to `out`.
///
/// An empty `payload` still produces a single zero-length, last-bit-set
/// block (the MAPI "flush marker").
pub(crate) fn encode_message(payload: &[u8], out: &mut Vec<u8>) {
    if payload.is_empty() {
        out.extend_from_slice(&encode_header(0, true));
        return;
    }

    let mut chunks = payload.chunks(MAX_BLOCK_PAYLOAD).peekable();
    while let Some(chunk) = chunks.next() {
        let is_last = chunks.peek().is_none();
        out.extend_from_slice(&encode_header(chunk.len(), is_last));
        out.extend_from_slice(chunk);
    }
}

/// Decode a complete MAPI message (one or more concatenated blocks) from
/// the front of `data`.
///
/// Returns `Some((message, bytes_consumed))` on success, or `None` if
/// `data` does not yet contain a complete message (the caller should read
/// more bytes and retry). This is a pure, allocation-only helper used both
/// by tests and — via [`super::read_message`] — by the real socket-backed
/// reader.
pub(crate) fn decode_message(data: &[u8]) -> Option<(Vec<u8>, usize)> {
    let mut offset = 0;
    let mut message = Vec::new();

    loop {
        if data.len() < offset + 2 {
            return None;
        }
        let (len, is_last) = decode_header([data[offset], data[offset + 1]]);
        offset += 2;

        if data.len() < offset + len {
            return None;
        }
        message.extend_from_slice(&data[offset..offset + len]);
        offset += len;

        if is_last {
            return Some((message, offset));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trip() {
        for (len, last) in [(0, false), (0, true), (1, false), (8190, true)] {
            let bytes = encode_header(len, last);
            assert_eq!(decode_header(bytes), (len, last));
        }
    }

    #[test]
    fn header_is_little_endian_regardless_of_host_order() {
        // length=1, last=1 -> header value 3 -> bytes [3, 0] in LE.
        assert_eq!(encode_header(1, true), [0x03, 0x00]);
        // length=8190, last=0 -> header value 16380 = 0x3FFC -> LE bytes.
        assert_eq!(encode_header(8190, false), [0xFC, 0x3F]);
    }

    #[test]
    fn empty_message_is_a_single_flush_marker_block() {
        let mut out = Vec::new();
        encode_message(&[], &mut out);
        assert_eq!(out, vec![0x01, 0x00]);

        let (message, consumed) = decode_message(&out).expect("complete message");
        assert!(message.is_empty());
        assert_eq!(consumed, out.len());
    }

    #[test]
    fn message_within_a_single_block_round_trips() {
        let payload = vec![7u8; 42];
        let mut out = Vec::new();
        encode_message(&payload, &mut out);

        // 2-byte header + 42-byte payload, single block.
        assert_eq!(out.len(), 2 + 42);

        let (message, consumed) = decode_message(&out).expect("complete message");
        assert_eq!(message, payload);
        assert_eq!(consumed, out.len());
    }

    #[test]
    fn message_exactly_at_block_boundary_is_a_single_block() {
        let payload = vec![9u8; MAX_BLOCK_PAYLOAD];
        let mut out = Vec::new();
        encode_message(&payload, &mut out);

        // Exactly one block: no second header should have been written.
        assert_eq!(out.len(), 2 + MAX_BLOCK_PAYLOAD);

        let (message, consumed) = decode_message(&out).expect("complete message");
        assert_eq!(message, payload);
        assert_eq!(consumed, out.len());
    }

    #[test]
    fn message_one_byte_over_boundary_splits_into_two_blocks() {
        let payload = vec![3u8; MAX_BLOCK_PAYLOAD + 1];
        let mut out = Vec::new();
        encode_message(&payload, &mut out);

        // First block: header + MAX_BLOCK_PAYLOAD bytes, last_bit unset.
        // Second block: header + 1 byte, last_bit set.
        let expected_len = (2 + MAX_BLOCK_PAYLOAD) + (2 + 1);
        assert_eq!(out.len(), expected_len);
        assert_eq!(decode_header([out[0], out[1]]), (MAX_BLOCK_PAYLOAD, false));
        assert_eq!(
            decode_header([out[2 + MAX_BLOCK_PAYLOAD], out[2 + MAX_BLOCK_PAYLOAD + 1]]),
            (1, true)
        );

        let (message, consumed) = decode_message(&out).expect("complete message");
        assert_eq!(message, payload);
        assert_eq!(consumed, out.len());
    }

    #[test]
    fn multi_block_message_spanning_several_full_blocks() {
        let payload = vec![5u8; MAX_BLOCK_PAYLOAD * 2 + 100];
        let mut out = Vec::new();
        encode_message(&payload, &mut out);

        let (message, consumed) = decode_message(&out).expect("complete message");
        assert_eq!(message, payload);
        assert_eq!(consumed, out.len());
    }

    #[test]
    fn incomplete_header_returns_none() {
        assert_eq!(decode_message(&[0x01]), None);
        assert_eq!(decode_message(&[]), None);
    }

    #[test]
    fn incomplete_payload_returns_none() {
        // Header claims 10 bytes of payload, last-bit set, but only 3 are present.
        let header = encode_header(10, true);
        let mut data = header.to_vec();
        data.extend_from_slice(&[1, 2, 3]);
        assert_eq!(decode_message(&data), None);
    }

    #[test]
    fn trailing_bytes_after_message_are_not_consumed() {
        let mut out = Vec::new();
        encode_message(b"hello", &mut out);
        out.extend_from_slice(b"trailing garbage");

        let (message, consumed) = decode_message(&out).expect("complete message");
        assert_eq!(message, b"hello");
        assert_eq!(consumed, 2 + 5);
        assert!(consumed < out.len());
    }
}
