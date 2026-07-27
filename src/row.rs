//! Row representation for MonetDB result sets.

use sqlx_core::column::{Column, ColumnIndex};
use sqlx_core::error::Error;

use crate::column::MonetColumn;
use crate::database::Monet;
use crate::value::MonetValueRef;

/// A row from a MonetDB result set.
#[derive(Debug, Clone)]
pub struct MonetRow {
    /// Columns in this row.
    columns: Vec<MonetColumn>,
    /// Values in this row, indexed by column position.
    #[allow(dead_code)]
    values: Vec<Option<Vec<u8>>>,
}

impl MonetRow {
    /// Create a new row with the given columns and values.
    #[allow(dead_code)]
    pub(crate) fn new(columns: Vec<MonetColumn>, values: Vec<Option<Vec<u8>>>) -> Self {
        Self { columns, values }
    }
}

impl sqlx_core::row::Row for MonetRow {
    type Database = Monet;

    fn columns(&self) -> &[<Monet as sqlx_core::database::Database>::Column] {
        &self.columns
    }

    fn try_get_raw<I>(&self, index: I) -> Result<MonetValueRef<'_>, Error>
    where
        I: ColumnIndex<Self>,
    {
        let col_index = index.index(self)?;
        let column = &self.columns[col_index];

        match &self.values[col_index] {
            Some(data) => Ok(MonetValueRef::new(data, column.type_info())),
            None => Ok(MonetValueRef::null(column.type_info())),
        }
    }
}
