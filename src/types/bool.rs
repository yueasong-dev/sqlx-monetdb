use sqlx_core::decode::Decode;
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
}
