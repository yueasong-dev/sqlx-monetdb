use sqlx_core::decode::Decode;
use sqlx_core::encode::{Encode, IsNull};
use sqlx_core::error::BoxDynError;
use sqlx_core::types::Type;

use crate::database::Monet;
use crate::type_info::MonetTypeInfo;
use crate::types::compatible_with_names;
use crate::value::MonetValueRef;

macro_rules! impl_float_type {
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

        impl<'q> Encode<'q, Monet> for $rust_ty {
            fn encode_by_ref(&self, buf: &mut Vec<u8>) -> Result<IsNull, BoxDynError> {
                buf.extend_from_slice(self.to_string().as_bytes());
                Ok(IsNull::No)
            }
        }
    };
}

// Note: MonetDB's `real`/`double` are both stored as 64-bit internally
// (docs/DEVELOPMENT.md §4.5), but that's a server-side implementation
// detail that doesn't affect parsing the text representation into either
// Rust width — `real` maps to f32 and `double` to f64 by SQL naming
// convention, matching what other SQL drivers do.
impl_float_type!(f32, "real", ["real", "float"]);
impl_float_type!(f64, "double", ["double"]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_floats() {
        let ty = MonetTypeInfo::new("double");
        assert_eq!(f64::decode(MonetValueRef::new("3.25", &ty)).unwrap(), 3.25);
        assert_eq!(f32::decode(MonetValueRef::new("2.5", &ty)).unwrap(), 2.5);
    }

    #[test]
    fn rejects_non_numeric_text() {
        let ty = MonetTypeInfo::new("double");
        assert!(f64::decode(MonetValueRef::new("nope", &ty)).is_err());
    }
}
