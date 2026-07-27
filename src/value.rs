//! Values and references to values from MonetDB result sets.

use std::borrow::Cow;

use crate::database::Monet;
use crate::type_info::MonetTypeInfo;

/// An owned value from a MonetDB result set.
///
/// This is a placeholder that stores the raw bytes and type information.
/// Stage F will implement actual decoding logic.
#[derive(Debug, Clone)]
pub struct MonetValue {
    /// The raw data bytes from the server.
    #[allow(dead_code)]
    data: Option<Vec<u8>>,
    /// Type information for this value; will be populated when stage F decodes the row.
    type_info: MonetTypeInfo,
}

impl MonetValue {
    /// Create a new NULL value with the given type.
    #[allow(dead_code)]
    pub(crate) fn null(type_info: MonetTypeInfo) -> Self {
        Self {
            data: None,
            type_info,
        }
    }

    /// Create a new value with the given data and type.
    #[allow(dead_code)]
    pub(crate) fn new(data: Vec<u8>, type_info: MonetTypeInfo) -> Self {
        Self {
            data: Some(data),
            type_info,
        }
    }
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
    /// A reference to the raw data bytes, or None if NULL.
    #[allow(dead_code)]
    data: Option<&'r [u8]>,
    /// Type information for this value.
    type_info: &'r MonetTypeInfo,
}

impl<'r> MonetValueRef<'r> {
    /// Create a reference to a NULL value.
    #[allow(dead_code)]
    pub(crate) fn null(type_info: &'r MonetTypeInfo) -> Self {
        Self {
            data: None,
            type_info,
        }
    }

    /// Create a reference to a value with data.
    #[allow(dead_code)]
    pub(crate) fn new(data: &'r [u8], type_info: &'r MonetTypeInfo) -> Self {
        Self {
            data: Some(data),
            type_info,
        }
    }
}

impl<'r> sqlx_core::value::ValueRef<'r> for MonetValueRef<'r> {
    type Database = Monet;

    fn to_owned(&self) -> <Monet as sqlx_core::database::Database>::Value {
        MonetValue {
            data: self.data.map(|d| d.to_vec()),
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
