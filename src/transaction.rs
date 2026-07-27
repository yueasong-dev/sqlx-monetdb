//! Transaction management for MonetDB.

#![allow(clippy::manual_async_fn)]

use sqlx_core::error::Error;
use sqlx_core::sql_str::SqlStr;

use crate::database::Monet;

/// Transaction manager for MonetDB.
///
/// Stage H implements transaction state tracking and BEGIN/COMMIT/ROLLBACK
/// statement execution via the connection.
pub struct MonetTransactionManager;

impl sqlx_core::transaction::TransactionManager for MonetTransactionManager {
    type Database = Monet;

    fn begin(
        _conn: &mut <Monet as sqlx_core::database::Database>::Connection,
        _statement: Option<SqlStr>,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send + '_ {
        async { unimplemented!("stage H: send BEGIN or SAVEPOINT command") }
    }

    fn commit(
        _conn: &mut <Monet as sqlx_core::database::Database>::Connection,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send + '_ {
        async { unimplemented!("stage H: send COMMIT command") }
    }

    fn rollback(
        _conn: &mut <Monet as sqlx_core::database::Database>::Connection,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send + '_ {
        async { unimplemented!("stage H: send ROLLBACK or ROLLBACK TO SAVEPOINT command") }
    }

    fn start_rollback(_conn: &mut <Monet as sqlx_core::database::Database>::Connection) {
        unimplemented!("stage H: queue a rollback to be executed on next async operation")
    }

    fn get_transaction_depth(
        _conn: &<Monet as sqlx_core::database::Database>::Connection,
    ) -> usize {
        unimplemented!(
            "stage H: return current transaction depth (0=no txn, 1+=active txn or savepoint)"
        )
    }
}
