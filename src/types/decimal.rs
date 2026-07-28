use rust_decimal::Decimal;
use sqlx_core::decode::Decode;
use sqlx_core::error::BoxDynError;
use sqlx_core::types::Type;

use crate::database::Monet;
use crate::type_info::MonetTypeInfo;
use crate::types::compatible_with_names;
use crate::value::MonetValueRef;

impl Type<Monet> for Decimal {
    fn type_info() -> MonetTypeInfo {
        MonetTypeInfo::new("decimal")
    }

    fn compatible(ty: &MonetTypeInfo) -> bool {
        compatible_with_names(ty, &["decimal", "numeric"])
    }
}

impl<'r> Decode<'r, Monet> for Decimal {
    fn decode(value: MonetValueRef<'r>) -> Result<Self, BoxDynError> {
        // The server's `%...#length`/`typesizes` headers don't reliably
        // carry precision/scale (docs/DEVELOPMENT.md §4.3 correction) —
        // but they don't need to: the text representation itself (e.g.
        // "9.99") already encodes the scale via its decimal point
        // position, which `Decimal::from_str` parses directly.
        Ok(value.text()?.parse::<Decimal>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn decodes_decimal_preserving_scale_from_text() {
        let ty = MonetTypeInfo::new("decimal");
        let decoded = Decimal::decode(MonetValueRef::new("9.99", &ty)).unwrap();
        assert_eq!(decoded, Decimal::from_str("9.99").unwrap());
        assert_eq!(decoded.scale(), 2);
    }

    #[test]
    fn rejects_non_numeric_text() {
        let ty = MonetTypeInfo::new("decimal");
        assert!(Decimal::decode(MonetValueRef::new("nope", &ty)).is_err());
    }
}
