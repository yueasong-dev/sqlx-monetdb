//! Type information for MonetDB columns.

use std::fmt;

/// Type information for a MonetDB column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonetTypeInfo {
    /// The SQL type name (e.g., "int", "varchar").
    name: String,
    is_null: bool,
}

impl MonetTypeInfo {
    /// Create a new type info with the given name.
    #[allow(dead_code)]
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            is_null: false,
        }
    }

    /// Create a type info representing SQL NULL.
    #[allow(dead_code)]
    pub(crate) fn null() -> Self {
        Self {
            name: String::from("null"),
            is_null: true,
        }
    }
}

impl fmt::Display for MonetTypeInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

impl sqlx_core::type_info::TypeInfo for MonetTypeInfo {
    fn is_null(&self) -> bool {
        self.is_null
    }

    fn name(&self) -> &str {
        &self.name
    }
}
