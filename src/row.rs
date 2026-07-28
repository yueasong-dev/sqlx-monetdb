//! Row representation for MonetDB result sets.

use std::sync::Arc;

use sqlx_core::column::{Column, ColumnIndex};
use sqlx_core::error::Error;

use crate::column::MonetColumn;
use crate::database::Monet;
use crate::protocol::response::TableResult;
use crate::type_info::MonetTypeInfo;
use crate::value::MonetValueRef;

/// A row from a MonetDB result set.
///
/// Columns are shared (`Arc`) across every row of the same result table
/// rather than cloned per row.
#[derive(Debug, Clone)]
pub struct MonetRow {
    columns: Arc<[MonetColumn]>,
    values: Vec<Option<String>>,
}

impl MonetRow {
    /// Convert a fully-parsed [`TableResult`] (`protocol::response`) into
    /// the `MonetRow`s sqlx expects, one per embedded tuple.
    // Not yet called: wired up by stage G's Executor.
    #[allow(dead_code)]
    pub(crate) fn from_table_result(table: TableResult) -> Vec<Self> {
        let columns: Arc<[MonetColumn]> = table
            .columns
            .iter()
            .enumerate()
            .map(|(ordinal, meta)| MonetColumn::from_meta(ordinal, meta))
            .collect();

        table
            .rows
            .into_iter()
            .map(|values| Self {
                columns: Arc::clone(&columns),
                values,
            })
            .collect()
    }
}

impl ColumnIndex<MonetRow> for usize {
    fn index(&self, row: &MonetRow) -> Result<usize, Error> {
        let len = row.columns.len();
        if *self >= len {
            return Err(Error::ColumnIndexOutOfBounds { index: *self, len });
        }
        Ok(*self)
    }
}

impl ColumnIndex<MonetRow> for str {
    fn index(&self, row: &MonetRow) -> Result<usize, Error> {
        row.columns
            .iter()
            .find(|c| c.name() == self)
            .map(Column::ordinal)
            .ok_or_else(|| Error::ColumnNotFound(self.to_string()))
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
        let type_info: &MonetTypeInfo = column.type_info();

        match &self.values[col_index] {
            Some(data) => Ok(MonetValueRef::new(data, type_info)),
            None => Ok(MonetValueRef::null(type_info)),
        }
    }
}

#[cfg(all(test, feature = "runtime-tokio"))]
mod docker_tests {
    use rust_decimal::Decimal;
    use sqlx_core::row::Row;
    use std::str::FromStr;

    use crate::protocol::response::QueryResponse;
    use crate::protocol::{execute_query, perform_handshake};

    use super::*;

    /// Stage F smoke test: the whole chain — real MAPI query response ->
    /// `MonetRow::from_table_result` -> `Row::try_get` -> `Decode` — against
    /// real data from a live MonetDB instance, including the trickier
    /// cases (double-quoted strings, decimal scale from text).
    #[tokio::test]
    #[ignore = "requires a running MonetDB docker instance; see docs/DEVELOPMENT.md stage F"]
    async fn decodes_real_rows_into_rust_types() {
        let port: u16 = std::env::var("MONETDB_TEST_PORT")
            .unwrap_or_else(|_| "50001".into())
            .parse()
            .expect("MONETDB_TEST_PORT must be a valid u16 port number");

        let mut stream = perform_handshake("127.0.0.1", port, "monetdb", "monetdb", "monetdb")
            .await
            .expect("handshake should succeed");

        let _ = execute_query(&mut stream, "DROP TABLE sqlx_monetdb_row_decode_test").await;
        execute_query(
            &mut stream,
            "CREATE TABLE sqlx_monetdb_row_decode_test (id INT, name VARCHAR(50), price DECIMAL(10,2), active BOOLEAN)",
        )
        .await
        .expect("CREATE TABLE should succeed");
        execute_query(
            &mut stream,
            "INSERT INTO sqlx_monetdb_row_decode_test VALUES (1, 'widget', 9.99, true), (2, NULL, 100.50, false)",
        )
        .await
        .expect("INSERT should succeed");

        let response = execute_query(
            &mut stream,
            "SELECT id, name, price, active FROM sqlx_monetdb_row_decode_test ORDER BY id",
        )
        .await
        .expect("SELECT should succeed");
        let QueryResponse::Table(table) = response else {
            panic!("expected a Table response");
        };

        let rows = MonetRow::from_table_result(table);
        assert_eq!(rows.len(), 2);

        assert_eq!(rows[0].try_get::<i32, _>(0).unwrap(), 1);
        assert_eq!(rows[0].try_get::<String, _>(1).unwrap(), "widget");
        assert_eq!(
            rows[0].try_get::<Decimal, _>(2).unwrap(),
            Decimal::from_str("9.99").unwrap()
        );
        assert!(rows[0].try_get::<bool, _>(3).unwrap());

        assert_eq!(rows[1].try_get::<i32, _>(0).unwrap(), 2);
        assert_eq!(rows[1].try_get::<Option<String>, _>(1).unwrap(), None);
        assert_eq!(
            rows[1].try_get::<Decimal, _>(2).unwrap(),
            Decimal::from_str("100.50").unwrap()
        );
        assert!(!rows[1].try_get::<bool, _>(3).unwrap());

        execute_query(&mut stream, "DROP TABLE sqlx_monetdb_row_decode_test")
            .await
            .expect("cleanup DROP TABLE should succeed");
    }
}
