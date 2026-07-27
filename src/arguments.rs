//! Arguments for parameterized queries on MonetDB.

use sqlx_core::encode::Encode;
use sqlx_core::error::BoxDynError;
use sqlx_core::types::Type;

use crate::database::Monet;

/// Arguments for a MonetDB query.
///
/// Stage G will implement the actual parameter binding and encoding logic.
#[derive(Debug, Default, Clone)]
pub struct MonetArguments {
    /// Encoded argument bytes; will be populated when stage G implements encoding.
    #[allow(dead_code)]
    data: Vec<u8>,
    /// Number of arguments added.
    count: usize,
}

impl MonetArguments {
    /// Create new empty arguments.
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            count: 0,
        }
    }
}

impl sqlx_core::arguments::Arguments for MonetArguments {
    type Database = Monet;

    fn reserve(&mut self, _additional: usize, _size: usize) {
        unimplemented!("stage G: reserve capacity for arguments")
    }

    fn add<'t, T>(&mut self, _value: T) -> Result<(), BoxDynError>
    where
        T: Encode<'t, Monet> + Type<Monet>,
    {
        unimplemented!("stage G: encode and add argument to query")
    }

    fn len(&self) -> usize {
        self.count
    }
}
