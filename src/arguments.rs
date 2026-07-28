//! Arguments for parameterized queries on MonetDB.
//!
//! MAPI's simple query protocol has no wire-level parameter binding
//! (`docs/DEVELOPMENT.md` §4.4) — like pymonetdb, this driver encodes each
//! bound value as a safe SQL literal and substitutes it into the query
//! text client-side before sending, rather than a true prepared-statement
//! bind.

use sqlx_core::encode::Encode;
use sqlx_core::error::{BoxDynError, Error};
use sqlx_core::types::Type;

use crate::database::Monet;

sqlx_core::impl_into_arguments_for_arguments!(MonetArguments);

/// Arguments for a MonetDB query: each bound value is pre-encoded (by
/// `Encode`) into its literal SQL text form (e.g. `42`, `'it''s here'`,
/// `NULL`), ready to substitute for a `?` placeholder.
#[derive(Debug, Default, Clone)]
pub struct MonetArguments {
    literals: Vec<String>,
}

impl MonetArguments {
    pub fn new() -> Self {
        Self::default()
    }

    /// Substitute each `?` placeholder in `sql`, in order, with this
    /// argument list's encoded literals.
    ///
    /// **Known limitation**: this is a naive text substitution — it does
    /// not parse the SQL, so a literal `?` character embedded inside a
    /// quoted string or comment in the *query text itself* (not a bound
    /// value — those are already-encoded literals, not raw `?`) would be
    /// misinterpreted as a placeholder. Acceptable for v1; a real SQL
    /// tokenizer would be needed to close this gap.
    pub(crate) fn substitute_into(&self, sql: &str) -> Result<String, Error> {
        let placeholder_count = sql.matches('?').count();
        let bound = self.literals.len();
        if placeholder_count != bound {
            return Err(Error::Encode(
                format!(
                    "query has {placeholder_count} '?' placeholder(s) but {bound} argument(s) were bound"
                )
                .into(),
            ));
        }

        let mut out = String::with_capacity(sql.len());
        let mut literals = self.literals.iter();
        for part in sql.split('?') {
            out.push_str(part);
            if let Some(literal) = literals.next() {
                out.push_str(literal);
            }
        }
        Ok(out)
    }
}

impl sqlx_core::arguments::Arguments for MonetArguments {
    type Database = Monet;

    fn reserve(&mut self, additional: usize, _size: usize) {
        self.literals.reserve(additional);
    }

    fn add<'t, T>(&mut self, value: T) -> Result<(), BoxDynError>
    where
        T: Encode<'t, Monet> + Type<Monet>,
    {
        let mut buf: Vec<u8> = Vec::new();
        let is_null = value.encode(&mut buf)?;
        let literal = if is_null.is_null() {
            "NULL".to_string()
        } else {
            String::from_utf8(buf)?
        };
        self.literals.push(literal);
        Ok(())
    }

    fn len(&self) -> usize {
        self.literals.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_placeholders_in_order() {
        let mut args = MonetArguments::new();
        args.literals.push("42".to_string());
        args.literals.push("'hi'".to_string());

        assert_eq!(
            args.substitute_into("SELECT * FROM t WHERE a = ? AND b = ?")
                .unwrap(),
            "SELECT * FROM t WHERE a = 42 AND b = 'hi'"
        );
    }

    #[test]
    fn mismatched_placeholder_count_is_an_error() {
        let mut args = MonetArguments::new();
        args.literals.push("42".to_string());

        assert!(args
            .substitute_into("SELECT * FROM t WHERE a = ? AND b = ?")
            .is_err());
        assert!(args.substitute_into("SELECT * FROM t").is_err());
    }

    #[test]
    fn no_placeholders_no_arguments_round_trips() {
        let args = MonetArguments::new();
        assert_eq!(
            args.substitute_into("SELECT 1").unwrap(),
            "SELECT 1".to_string()
        );
    }
}
