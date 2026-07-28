//! `Type<Monet>` / `Decode<'_, Monet>` / `Encode<'_, Monet>` implementations
//! mapping MonetDB's text wire representation (`docs/DEVELOPMENT.md` §4.5)
//! onto Rust types. `Encode` produces safe SQL literal text for
//! `arguments::MonetArguments` to substitute into query placeholders
//! (`docs/DEVELOPMENT.md` §4.4 — MAPI has no wire-level parameter binding).

mod bool;
mod decimal;
mod float;
mod int;
mod str;
mod temporal;

use sqlx_core::type_info::TypeInfo;

use crate::database::Monet;
use crate::type_info::MonetTypeInfo;

sqlx_core::impl_encode_for_option!(Monet);

/// Case-insensitive match against any of `names` — used to implement
/// `Type::compatible` so that SQL type aliases (e.g. `serial` for `int`)
/// are accepted, not just the single canonical name `Type::type_info()`
/// returns.
pub(crate) fn compatible_with_names(ty: &MonetTypeInfo, names: &[&str]) -> bool {
    names.iter().any(|n| ty.name().eq_ignore_ascii_case(n))
}
