//! Values and references to values from MonetDB result sets.
//!
//! MAPI is a text protocol (`docs/DEVELOPMENT.md` §4.3-4.5): every field
//! arrives as a parsed `Option<String>` (`None` = SQL NULL) courtesy of
//! `protocol::response`. Decoding into Rust types (`src/types/`) works
//! from that text representation directly rather than raw bytes.

use std::borrow::Cow;

use sqlx_core::error::{BoxDynError, UnexpectedNullError};

use crate::database::Monet;
use crate::type_info::MonetTypeInfo;

/// An owned value from a MonetDB result set.
#[derive(Debug, Clone)]
pub struct MonetValue {
    data: Option<String>,
    type_info: MonetTypeInfo,
}

impl sqlx_core::value::Value for MonetValue {
    type Database = Monet;

    fn as_ref(&self) -> <Monet as sqlx_core::database::Database>::ValueRef<'_> {
        MonetValueRef {
            data: self.data.as_deref(),
            type_info: &self.type_info,
        }
    }

    fn type_info(&self) -> Cow<'_, <Monet as sqlx_core::database::Database>::TypeInfo> {
        Cow::Borrowed(&self.type_info)
    }

    fn is_null(&self) -> bool {
        self.data.is_none()
    }
}

/// A reference to a value from a MonetDB result set.
#[derive(Debug)]
pub struct MonetValueRef<'r> {
    data: Option<&'r str>,
    type_info: &'r MonetTypeInfo,
}

impl<'r> MonetValueRef<'r> {
    pub(crate) fn null(type_info: &'r MonetTypeInfo) -> Self {
        Self {
            data: None,
            type_info,
        }
    }

    pub(crate) fn new(data: &'r str, type_info: &'r MonetTypeInfo) -> Self {
        Self {
            data: Some(data),
            type_info,
        }
    }

    /// The raw MAPI text representation of this value. Errors (rather
    /// than silently mis-decoding) if the value is NULL — used by the
    /// `Decode` impls in `src/types/`, none of which can represent NULL
    /// directly (sqlx's blanket `Decode` for `Option<T>` checks
    /// `is_null()` beforehand and only calls `T::decode` when non-null).
    pub(crate) fn text(&self) -> Result<&'r str, BoxDynError> {
        self.data
            .ok_or_else(|| Box::new(UnexpectedNullError) as BoxDynError)
    }
}

impl<'r> sqlx_core::value::ValueRef<'r> for MonetValueRef<'r> {
    type Database = Monet;

    fn to_owned(&self) -> <Monet as sqlx_core::database::Database>::Value {
        MonetValue {
            data: self.data.map(str::to_string),
            type_info: self.type_info.clone(),
        }
    }

    fn type_info(&self) -> Cow<'_, <Monet as sqlx_core::database::Database>::TypeInfo> {
        Cow::Borrowed(self.type_info)
    }

    fn is_null(&self) -> bool {
        self.data.is_none()
    }
}
