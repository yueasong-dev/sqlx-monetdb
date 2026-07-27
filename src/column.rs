//! Column information for MonetDB result sets.

use crate::database::Monet;
use crate::type_info::MonetTypeInfo;

/// Column information from a MonetDB result set.
#[derive(Debug, Clone)]
pub struct MonetColumn {
    /// The ordinal position of this column in the result set.
    ordinal: usize,
    /// The column name.
    name: String,
    /// The type information for this column.
    type_info: MonetTypeInfo,
}

impl MonetColumn {
    /// Create a new column with the given name and type info.
    #[allow(dead_code)]
    pub(crate) fn new(ordinal: usize, name: impl Into<String>, type_info: MonetTypeInfo) -> Self {
        Self {
            ordinal,
            name: name.into(),
            type_info,
        }
    }
}

impl sqlx_core::column::Column for MonetColumn {
    type Database = Monet;

    fn ordinal(&self) -> usize {
        self.ordinal
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn type_info(&self) -> &<Monet as sqlx_core::database::Database>::TypeInfo {
        &self.type_info
    }
}
