//! Async, pure-Rust [SQLx](https://github.com/transact-rs/sqlx) driver for
//! [MonetDB](https://www.monetdb.org/), implemented directly over the native
//! MAPI wire protocol.
//!
//! This crate is under active development; see the project README for
//! current status.

mod error;

mod arguments;
mod column;
mod connection;
mod database;
mod options;
mod protocol;
mod row;
mod statement;
mod transaction;
mod type_info;
mod value;

pub use arguments::MonetArguments;
pub use column::MonetColumn;
pub use connection::MonetConnection;
pub use database::{Monet, MonetQueryResult};
pub use error::{MonetDatabaseError, MonetError};
pub use options::MonetConnectOptions;
pub use row::MonetRow;
pub use statement::MonetStatement;
pub use transaction::MonetTransactionManager;
pub use type_info::MonetTypeInfo;
pub use value::{MonetValue, MonetValueRef};
