use sqlx_core::decode::Decode;
use sqlx_core::encode::{Encode, IsNull};
use sqlx_core::error::BoxDynError;
use sqlx_core::types::Type;

use crate::database::Monet;
use crate::type_info::MonetTypeInfo;
use crate::types::compatible_with_names;
use crate::value::MonetValueRef;

impl Type<Monet> for bool {
    fn type_info() -> MonetTypeInfo {
        MonetTypeInfo::new("boolean")
    }

    fn compatible(ty: &MonetTypeInfo) -> bool {
        compatible_with_names(ty, &["boolean"])
    }
}

impl<'r> Decode<'r, Monet> for bool {
    fn decode(value: MonetValueRef<'r>) -> Result<Self, BoxDynError> {
        // docs/DEVELOPMENT.md §4.5: bare `true`/`false` literals.
        match value.text()? {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(format!("invalid boolean literal: {other:?}").into()),
        }
    }
}

impl<'q> Encode<'q, Monet> for bool {
    fn encode_by_ref(&self, buf: &mut Vec<u8>) -> Result<IsNull, BoxDynError> {
        buf.extend_from_slice(if *self { b"true" } else { b"false" });
        Ok(IsNull::No)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_info::MonetTypeInfo;

    #[test]
    fn decodes_true_and_false() {
        let ty = MonetTypeInfo::new("boolean");
        assert!(bool::decode(MonetValueRef::new("true", &ty)).unwrap());
        assert!(!bool::decode(MonetValueRef::new("false", &ty)).unwrap());
    }

    #[test]
    fn rejects_garbage() {
        let ty = MonetTypeInfo::new("boolean");
        assert!(bool::decode(MonetValueRef::new("nope", &ty)).is_err());
    }

    #[test]
    fn encode_round_trips_through_decode() {
        let ty = MonetTypeInfo::new("boolean");
        let mut buf = Vec::new();
        let _ = true.encode_by_ref(&mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(bool::decode(MonetValueRef::new(&text, &ty)).unwrap());
    }
}
