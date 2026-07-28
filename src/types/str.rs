use sqlx_core::decode::Decode;
use sqlx_core::encode::{Encode, IsNull};
use sqlx_core::error::BoxDynError;
use sqlx_core::types::Type;

use crate::database::Monet;
use crate::type_info::MonetTypeInfo;
use crate::types::compatible_with_names;
use crate::value::MonetValueRef;

const STRING_TYPE_NAMES: &[&str] = &["char", "varchar", "clob", "str", "url", "json", "xml"];

/// Encode `s` as a single-quoted SQL string literal, doubling any embedded
/// single quotes per standard SQL escaping (`it's` -> `'it''s'`).
fn encode_string_literal(s: &str, buf: &mut Vec<u8>) {
    buf.push(b'\'');
    for chunk in s.split('\'') {
        buf.extend_from_slice(chunk.as_bytes());
        buf.extend_from_slice(b"''");
    }
    buf.truncate(buf.len() - 1); // drop the trailing extra `'` from the loop
}

impl Type<Monet> for String {
    fn type_info() -> MonetTypeInfo {
        MonetTypeInfo::new("varchar")
    }

    fn compatible(ty: &MonetTypeInfo) -> bool {
        compatible_with_names(ty, STRING_TYPE_NAMES)
    }
}

impl<'r> Decode<'r, Monet> for String {
    fn decode(value: MonetValueRef<'r>) -> Result<Self, BoxDynError> {
        Ok(value.text()?.to_string())
    }
}

impl<'q> Encode<'q, Monet> for String {
    fn encode_by_ref(&self, buf: &mut Vec<u8>) -> Result<IsNull, BoxDynError> {
        encode_string_literal(self, buf);
        Ok(IsNull::No)
    }
}

impl Type<Monet> for &str {
    fn type_info() -> MonetTypeInfo {
        <String as Type<Monet>>::type_info()
    }

    fn compatible(ty: &MonetTypeInfo) -> bool {
        <String as Type<Monet>>::compatible(ty)
    }
}

impl<'r> Decode<'r, Monet> for &'r str {
    fn decode(value: MonetValueRef<'r>) -> Result<Self, BoxDynError> {
        value.text()
    }
}

impl<'q> Encode<'q, Monet> for &'q str {
    fn encode_by_ref(&self, buf: &mut Vec<u8>) -> Result<IsNull, BoxDynError> {
        encode_string_literal(self, buf);
        Ok(IsNull::No)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_owned_and_borrowed_strings() {
        let ty = MonetTypeInfo::new("varchar");
        let value = MonetValueRef::new("hello", &ty);
        assert_eq!(
            String::decode(MonetValueRef::new("hello", &ty)).unwrap(),
            "hello"
        );
        assert_eq!(<&str>::decode(value).unwrap(), "hello");
    }

    #[test]
    fn escapes_embedded_single_quotes() {
        let mut buf = Vec::new();
        encode_string_literal("it's here", &mut buf);
        assert_eq!(String::from_utf8(buf).unwrap(), "'it''s here'");
    }

    #[test]
    fn escapes_a_lone_single_quote() {
        let mut buf = Vec::new();
        encode_string_literal("'", &mut buf);
        assert_eq!(String::from_utf8(buf).unwrap(), "''''");
    }

    #[test]
    fn escapes_empty_string() {
        let mut buf = Vec::new();
        encode_string_literal("", &mut buf);
        assert_eq!(String::from_utf8(buf).unwrap(), "''");
    }

    #[test]
    fn plain_string_has_no_escaping() {
        let mut buf = Vec::new();
        encode_string_literal("hello", &mut buf);
        assert_eq!(String::from_utf8(buf).unwrap(), "'hello'");
    }
}
