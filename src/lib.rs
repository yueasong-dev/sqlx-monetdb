//! Async, pure-Rust [SQLx](https://github.com/transact-rs/sqlx) driver for
//! [MonetDB](https://www.monetdb.org/), implemented directly over the native
//! MAPI wire protocol.
//!
//! This crate is under active development; see the project README for
//! current status.

mod error;

pub use error::{MonetDatabaseError, MonetError};
