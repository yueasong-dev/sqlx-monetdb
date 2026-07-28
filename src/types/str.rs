use sqlx_core::decode::Decode;
use sqlx_core::error::BoxDynError;
use sqlx_core::types::Type;

use crate::database::Monet;
use crate::type_info::MonetTypeInfo;
use crate::types::compatible_with_names;
use crate::value::MonetValueRef;

const STRING_TYPE_NAMES: &[&str] = &["char", "varchar", "clob", "str", "url", "json", "xml"];

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
}
