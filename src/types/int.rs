use sqlx_core::decode::Decode;
use sqlx_core::error::BoxDynError;
use sqlx_core::types::Type;

use crate::database::Monet;
use crate::type_info::MonetTypeInfo;
use crate::types::compatible_with_names;
use crate::value::MonetValueRef;

macro_rules! impl_integer_type {
    ($rust_ty:ty, $canonical_name:literal, [$($alias:literal),+ $(,)?]) => {
        impl Type<Monet> for $rust_ty {
            fn type_info() -> MonetTypeInfo {
                MonetTypeInfo::new($canonical_name)
            }

            fn compatible(ty: &MonetTypeInfo) -> bool {
                compatible_with_names(ty, &[$($alias),+])
            }
        }

        impl<'r> Decode<'r, Monet> for $rust_ty {
            fn decode(value: MonetValueRef<'r>) -> Result<Self, BoxDynError> {
                Ok(value.text()?.parse::<$rust_ty>()?)
            }
        }
    };
}

// docs/DEVELOPMENT.md §4.5 type name list.
impl_integer_type!(i8, "tinyint", ["tinyint"]);
impl_integer_type!(i16, "smallint", ["smallint", "shortint"]);
impl_integer_type!(i32, "int", ["int", "integer", "mediumint", "serial"]);
impl_integer_type!(i64, "bigint", ["bigint", "longint"]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_each_integer_width() {
        let ty = MonetTypeInfo::new("int");
        assert_eq!(i8::decode(MonetValueRef::new("-12", &ty)).unwrap(), -12);
        assert_eq!(i16::decode(MonetValueRef::new("1234", &ty)).unwrap(), 1234);
        assert_eq!(
            i32::decode(MonetValueRef::new("123456", &ty)).unwrap(),
            123456
        );
        assert_eq!(
            i64::decode(MonetValueRef::new("123456789012", &ty)).unwrap(),
            123456789012
        );
    }

    #[test]
    fn rejects_non_numeric_text() {
        let ty = MonetTypeInfo::new("int");
        assert!(i32::decode(MonetValueRef::new("not a number", &ty)).is_err());
    }

    #[test]
    fn serial_is_compatible_with_i32() {
        let ty = MonetTypeInfo::new("serial");
        assert!(i32::compatible(&ty));
    }
}
