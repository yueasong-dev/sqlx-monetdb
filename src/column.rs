//! Column information for MonetDB result sets.

use crate::database::Monet;
use crate::protocol::response::ColumnMeta;
use crate::type_info::MonetTypeInfo;

/// Column information from a MonetDB result set.
#[derive(Debug, Clone)]
pub struct MonetColumn {
    ordinal: usize,
    name: String,
    type_info: MonetTypeInfo,
}

impl MonetColumn {
    /// Build column metadata from a parsed `%`-header entry
    /// (`docs/DEVELOPMENT.md` §4.3-4.4).
    // Not yet called: wired up by stage G's Executor.
    #[allow(dead_code)]
    pub(crate) fn from_meta(ordinal: usize, meta: &ColumnMeta) -> Self {
        Self {
            ordinal,
            name: meta.name.clone(),
            type_info: MonetTypeInfo::new(meta.type_name.clone()),
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
