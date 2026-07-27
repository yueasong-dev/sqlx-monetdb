//! Database driver trait implementation for MonetDB.

use crate::arguments::MonetArguments;
use crate::column::MonetColumn;
use crate::connection::MonetConnection;
use crate::row::MonetRow;
use crate::statement::MonetStatement;
use crate::transaction::MonetTransactionManager;
use crate::type_info::MonetTypeInfo;
use crate::value::{MonetValue, MonetValueRef};

/// The MonetDB database driver.
#[derive(Debug)]
pub struct Monet;

/// A query result from executing a statement on MonetDB.
#[derive(Debug, Default, Clone)]
pub struct MonetQueryResult {
    /// Number of rows affected by the query.
    pub rows_affected: u64,
}

impl Extend<MonetQueryResult> for MonetQueryResult {
    fn extend<T: IntoIterator<Item = MonetQueryResult>>(&mut self, iter: T) {
        for result in iter {
            self.rows_affected += result.rows_affected;
        }
    }
}

impl sqlx_core::database::Database for Monet {
    type Connection = MonetConnection;
    type TransactionManager = MonetTransactionManager;
    type Row = MonetRow;
    type QueryResult = MonetQueryResult;
    type Column = MonetColumn;
    type TypeInfo = MonetTypeInfo;
    type Value = MonetValue;
    type ValueRef<'r> = MonetValueRef<'r>;
    type Arguments = MonetArguments;
    type ArgumentBuffer = Vec<u8>;
    type Statement = MonetStatement<'static>;

    const NAME: &'static str = "monetdb";
    const URL_SCHEMES: &'static [&'static str] = &["monetdb"];
}

impl sqlx_core::database::HasStatementCache for Monet {}
