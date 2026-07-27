//! MAPI simple query protocol: request encoding and response parsing.
//!
//! See `docs/DEVELOPMENT.md` §4.3 for the wire format this module is
//! implemented against.

// Wired to a real connection later in this same stage (execute_query in
// protocol/mod.rs), and to sqlx's Row/Column/TypeInfo in stage F.
#![allow(dead_code)]

use crate::error::{MonetDatabaseError, MonetError};

/// Encode a SQL statement as a MAPI query request: `s<SQL>\n;`
/// (`docs/DEVELOPMENT.md` §4.3). The caller passes the returned bytes to
/// `protocol::write_message`.
pub(crate) fn encode_query(sql: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(sql.len() + 3);
    out.push(b's');
    out.extend_from_slice(sql.as_bytes());
    out.extend_from_slice(b"\n;");
    out
}

/// Column metadata assembled from `%...#name` / `%...#type` /
/// `%...#length` / `%...#typesizes` header lines for one result table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ColumnMeta {
    pub name: String,
    pub type_name: String,
    /// From the `%...#length` header (a single integer per column).
    ///
    /// **Correction from docs/DEVELOPMENT.md's original research**: real
    /// testing against a live MonetDB Aug2024 instance (with
    /// `size_header=1` negotiated) showed the server sends `length`, not
    /// `typesizes` — this driver's docs originally claimed `typesizes`
    /// would give `(precision, scale)` for `decimal` columns, but that
    /// line never actually appeared. `length` for a `decimal(10,2)` column
    /// was observed as `12`, which is neither precision nor scale alone;
    /// decoding decimal precision/scale reliably will need a different
    /// source (e.g. the value's own text representation, or a
    /// `sys.columns` lookup) — tracked as a stage F follow-up.
    pub length: Option<usize>,
    /// `(first, second)` from a `%...#typesizes` header, if the server
    /// ever sends one (kept for forward/version compatibility — see the
    /// `length` doc above for why this driver doesn't currently rely on
    /// it existing).
    pub typesizes: Option<(usize, Option<usize>)>,
}

/// A `&1`/`&5` (Q_TABLE/Q_PREPARE) result: a result table plus whatever
/// rows were embedded in this response (up to `embedded_tuples` of
/// `row_count` total — see `docs/DEVELOPMENT.md` §4.3; this driver does not
/// yet implement `Xexport` pagination for the remainder, see step 32).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TableResult {
    pub query_id: i64,
    pub row_count: u64,
    pub declared_columns: usize,
    pub embedded_tuples: usize,
    pub columns: Vec<ColumnMeta>,
    /// One entry per embedded row; `None` per-field means SQL NULL.
    pub rows: Vec<Vec<Option<String>>>,
}

/// The parsed result of one MAPI query response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum QueryResponse {
    /// `&1`/`&5`: a SELECT (or PREPARE) result.
    Table(TableResult),
    /// `&2`: INSERT/UPDATE/DELETE.
    Update {
        affected: u64,
        last_insert_id: Option<i64>,
    },
    /// `&3`: DDL (CREATE/DROP/ALTER...).
    Schema,
    /// `&4`: transaction state change (BEGIN/COMMIT/ROLLBACK/SET
    /// AUTOCOMMIT), carrying the server's authoritative current
    /// autocommit state (`docs/DEVELOPMENT.md` §4.3, §4.8).
    Transaction { auto_commit: bool },
    /// Empty body / `=OK`: command succeeded with no further information.
    Ok,
}

/// Either a driver-internal protocol error, or a genuine server-reported
/// SQL error (`!`-prefixed line) — kept distinct so the latter converts
/// into `sqlx_core::Error::Database` (via `MonetDatabaseError`) rather
/// than `Error::Protocol`.
#[derive(Debug)]
pub(crate) enum ResponseError {
    Protocol(MonetError),
    Database(MonetDatabaseError),
}

impl From<MonetError> for ResponseError {
    fn from(err: MonetError) -> Self {
        ResponseError::Protocol(err)
    }
}

impl From<ResponseError> for sqlx_core::error::Error {
    fn from(err: ResponseError) -> Self {
        match err {
            ResponseError::Protocol(e) => e.into(),
            ResponseError::Database(e) => e.into(),
        }
    }
}

/// Parse a complete MAPI query response (one or more `\n`-separated lines,
/// as returned by `protocol::read_message` + UTF-8 decode) into a single
/// [`QueryResponse`].
///
/// Lines are processed in whatever order the server sent them: `&1`/`&5`
/// opens a [`TableResult`] accumulator, `%...#`/`[...]` lines fill it in,
/// and a bare `!` line short-circuits with a database error regardless of
/// where it appears (`docs/DEVELOPMENT.md` §4.7).
pub(crate) fn parse_response(text: &str) -> Result<QueryResponse, ResponseError> {
    let mut table: Option<TableResult> = None;
    let mut result: Option<QueryResponse> = None;

    for line in text.split('\n') {
        if line.is_empty() || line == "=OK" {
            continue;
        }

        if let Some(message) = line.strip_prefix('!') {
            return Err(ResponseError::Database(MonetDatabaseError::new(message)));
        }

        if line.starts_with('#') {
            continue; // informational line, not an error
        }

        if let Some(rest) = line.strip_prefix("&1").or_else(|| line.strip_prefix("&5")) {
            table = Some(parse_table_header(rest)?);
            continue;
        }

        if let Some(rest) = line.strip_prefix("&2") {
            result = Some(parse_update_line(rest)?);
            continue;
        }

        if line.starts_with("&3") {
            result = Some(QueryResponse::Schema);
            continue;
        }

        if line.starts_with("&4") {
            result = Some(parse_transaction_line(line)?);
            continue;
        }

        if let Some(rest) = line.strip_prefix('%') {
            let (identity, values) = parse_header_values(rest)?;
            apply_header_values(
                table.as_mut().ok_or_else(|| {
                    MonetError::Protocol(format!("'%' line before '&1': {line:?}"))
                })?,
                &identity,
                values,
            )?;
            continue;
        }

        if let Some(rest) = line.strip_prefix('[') {
            let fields = parse_tuple_fields(rest)?;
            table
                .as_mut()
                .ok_or_else(|| MonetError::Protocol(format!("'[' line before '&1': {line:?}")))?
                .rows
                .push(fields);
            continue;
        }

        // Unrecognized line: rather than silently ignore (which could mask
        // a real protocol change), surface it as a protocol error.
        return Err(MonetError::Protocol(format!("unrecognized response line: {line:?}")).into());
    }

    if let Some(table) = table {
        return Ok(QueryResponse::Table(table));
    }
    if let Some(result) = result {
        return Ok(result);
    }
    Ok(QueryResponse::Ok)
}

fn parse_table_header(rest: &str) -> Result<TableResult, MonetError> {
    // Documented as exactly 4 fields, but real servers send more (observed:
    // `0 2 3 2 6 6764 372 1565` from a live SELECT) — undocumented
    // trailing fields are read but intentionally ignored, same rationale
    // as parse_update_line.
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 4 {
        return Err(MonetError::Protocol(format!(
            "'&1'/'&5' line expects at least 4 fields (query_id rowcount columns tuples), got {parts:?}"
        )));
    }
    Ok(TableResult {
        query_id: parse_field(parts[0], "query_id")?,
        row_count: parse_field(parts[1], "rowcount")?,
        declared_columns: parse_field(parts[2], "columns")?,
        embedded_tuples: parse_field(parts[3], "tuples")?,
        columns: Vec::new(),
        rows: Vec::new(),
    })
}

fn parse_field<T: std::str::FromStr>(s: &str, field: &str) -> Result<T, MonetError> {
    s.parse()
        .map_err(|_| MonetError::Protocol(format!("invalid {field} in '&1'/'&5' line: {s:?}")))
}

fn parse_update_line(rest: &str) -> Result<QueryResponse, MonetError> {
    // Documented as "affected identity" (2 fields), but real servers send
    // more (observed: `affected -1 2 12443 2035 108` from a live INSERT) —
    // undocumented trailing fields are read but intentionally ignored
    // rather than rejected, since docs/DEVELOPMENT.md §4.3 already flags
    // this part of the wire format as not formally specified.
    let parts: Vec<&str> = rest.split_whitespace().collect();
    let affected: u64 = parts
        .first()
        .ok_or_else(|| MonetError::Protocol(format!("'&2' line missing affected count: {rest:?}")))?
        .parse()
        .map_err(|_| MonetError::Protocol(format!("invalid affected count in: {rest:?}")))?;
    let last_insert_id = match parts.get(1) {
        Some(&"-1") | None => None,
        Some(s) => Some(
            s.parse()
                .map_err(|_| MonetError::Protocol(format!("invalid identity value: {s:?}")))?,
        ),
    };
    Ok(QueryResponse::Update {
        affected,
        last_insert_id,
    })
}

fn parse_transaction_line(line: &str) -> Result<QueryResponse, MonetError> {
    // docs/DEVELOPMENT.md §4.3: the 4th character (0-indexed 3) is 't'/'f'.
    let flag = line.chars().nth(3).ok_or_else(|| {
        MonetError::Protocol(format!(
            "'&4' line too short to contain autocommit flag: {line:?}"
        ))
    })?;
    let auto_commit = match flag {
        't' => true,
        'f' => false,
        other => {
            return Err(MonetError::Protocol(format!(
                "unexpected autocommit flag {other:?} in '&4' line: {line:?}"
            )))
        }
    };
    Ok(QueryResponse::Transaction { auto_commit })
}

/// Parse a `%<comma-separated values>#<identity>` header line (with the
/// leading `%` already stripped) into `(identity, values)`.
fn parse_header_values(rest: &str) -> Result<(String, Vec<String>), MonetError> {
    let (values, identity) = rest.rsplit_once('#').ok_or_else(|| {
        MonetError::Protocol(format!("'%' line missing '#identity' suffix: {rest:?}"))
    })?;
    let values = values
        .trim()
        .split(',')
        .map(|v| v.trim().to_string())
        .collect();
    Ok((identity.trim().to_string(), values))
}

fn apply_header_values(
    table: &mut TableResult,
    identity: &str,
    values: Vec<String>,
) -> Result<(), MonetError> {
    if table.columns.len() < values.len() {
        table.columns.resize(values.len(), ColumnMeta::default());
    }

    match identity {
        "name" => {
            for (col, value) in table.columns.iter_mut().zip(values) {
                col.name = value;
            }
        }
        "type" => {
            for (col, value) in table.columns.iter_mut().zip(values) {
                col.type_name = value;
            }
        }
        "typesizes" => {
            for (col, value) in table.columns.iter_mut().zip(values) {
                col.typesizes = parse_typesizes_value(&value)?;
            }
        }
        "length" => {
            for (col, value) in table.columns.iter_mut().zip(values) {
                let value = value.trim();
                col.length = if value.is_empty() {
                    None
                } else {
                    Some(parse_field(value, "length")?)
                };
            }
        }
        // "table_name" is part of the protocol but unused by this driver
        // (docs/DEVELOPMENT.md §4.4) — accepted and ignored rather than
        // rejected, since a server sending it is not an error.
        "table_name" => {}
        other => {
            return Err(MonetError::Protocol(format!(
                "unknown header identity {other:?}"
            )))
        }
    }
    Ok(())
}

fn parse_typesizes_value(value: &str) -> Result<Option<(usize, Option<usize>)>, MonetError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let mut parts = value.split_whitespace();
    let first: usize = parts
        .next()
        .ok_or_else(|| MonetError::Protocol(format!("empty typesizes value: {value:?}")))?
        .parse()
        .map_err(|_| MonetError::Protocol(format!("invalid typesizes value: {value:?}")))?;
    let second = parts
        .next()
        .map(|s| {
            s.parse()
                .map_err(|_| MonetError::Protocol(format!("invalid typesizes value: {value:?}")))
        })
        .transpose()?;
    Ok(Some((first, second)))
}

/// Parse a `[val1,\tval2,\t...]` tuple line (with the leading `[` already
/// stripped — the trailing `]` is stripped here) into per-field values,
/// with `NULL` mapped to `None` and quoted-string fields unescaped
/// (`docs/DEVELOPMENT.md` §4.3).
fn parse_tuple_fields(rest: &str) -> Result<Vec<Option<String>>, MonetError> {
    let inner = rest
        .strip_suffix(']')
        .ok_or_else(|| MonetError::Protocol(format!("tuple line missing ']': {rest:?}")))?
        .trim();

    if inner.is_empty() {
        return Ok(Vec::new());
    }

    inner
        .split(",\t")
        .map(|field| unescape_field(field.trim()))
        .collect()
}

fn unescape_field(field: &str) -> Result<Option<String>, MonetError> {
    if field == "NULL" {
        return Ok(None);
    }

    // **Correction from docs/DEVELOPMENT.md's original research**: it
    // (based on pymonetdb source reading) claimed string fields are
    // single-quoted. Real testing against a live MonetDB Aug2024 instance
    // showed VARCHAR fields wrapped in *double* quotes instead (e.g.
    // `"widget"`). Both are handled here defensively, matching whichever
    // quote character actually wraps the field.
    let quote = match field.chars().next() {
        Some(c @ ('\'' | '"')) => c,
        _ => {
            // Unquoted literal: numbers, booleans, dates, etc. — used as-is.
            return Ok(Some(field.to_string()));
        }
    };
    let Some(quoted) = field
        .strip_prefix(quote)
        .and_then(|s| s.strip_suffix(quote))
    else {
        return Ok(Some(field.to_string()));
    };

    let mut unescaped = String::with_capacity(quoted.len());
    let mut chars = quoted.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            unescaped.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => unescaped.push('\n'),
            Some('t') => unescaped.push('\t'),
            Some('r') => unescaped.push('\r'),
            Some('\\') => unescaped.push('\\'),
            Some(c @ ('\'' | '"')) => unescaped.push(c),
            Some(other) => {
                unescaped.push('\\');
                unescaped.push(other);
            }
            None => unescaped.push('\\'),
        }
    }
    Ok(Some(unescaped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_query_request() {
        assert_eq!(encode_query("SELECT 1"), b"sSELECT 1\n;");
    }

    #[test]
    fn parses_a_synthetic_select_response_with_typesizes() {
        // Hypothetical shape exercising the `typesizes` parsing path in
        // case some server configuration ever sends it (see ColumnMeta's
        // doc comment — real testing showed `length` instead, covered by
        // the sibling test below using an actual captured response).
        let text = concat!(
            "&1 0 3 4 2\n",
            "% sys.t,sys.t,sys.t # table_name\n",
            "% a,b,c # name\n",
            "% int,varchar,decimal # type\n",
            "% 0,0,20 2 # typesizes\n",
            "[ 1,\t'hello',\t3.50\t]\n",
            "[ 2,\tNULL,\t9.99\t]",
        );

        let response = parse_response(text).expect("valid response");
        let QueryResponse::Table(table) = response else {
            panic!("expected a Table response, got {response:?}");
        };

        assert_eq!(table.query_id, 0);
        assert_eq!(table.row_count, 3);
        assert_eq!(table.declared_columns, 4);
        assert_eq!(table.embedded_tuples, 2);

        assert_eq!(table.columns.len(), 3);
        assert_eq!(table.columns[0].name, "a");
        assert_eq!(table.columns[0].type_name, "int");
        assert_eq!(table.columns[1].name, "b");
        assert_eq!(table.columns[1].type_name, "varchar");
        assert_eq!(table.columns[2].name, "c");
        assert_eq!(table.columns[2].type_name, "decimal");
        assert_eq!(table.columns[2].typesizes, Some((20, Some(2))));

        assert_eq!(
            table.rows,
            vec![
                vec![
                    Some("1".to_string()),
                    Some("hello".to_string()),
                    Some("3.50".to_string())
                ],
                vec![Some("2".to_string()), None, Some("9.99".to_string())],
            ]
        );
    }

    #[test]
    fn parses_a_real_captured_select_response() {
        // Captured verbatim from a live MonetDB Aug2024 docker instance
        // (docker_tests::full_crud_cycle_against_docker_monetdb) — note
        // the double-quoted strings and the "length" (not "typesizes")
        // header, both corrections to the original protocol research; see
        // ColumnMeta's and unescape_field's doc comments.
        let text = concat!(
            "&1 0 2 3 2 14 1706 104 125\n",
            "% .sqlx_monetdb_crud_test,\tsys.sqlx_monetdb_crud_test,\tsys.sqlx_monetdb_crud_test # table_name\n",
            "% id,\tname,\tprice # name\n",
            "% int,\tvarchar,\tdecimal # type\n",
            "% 1,\t6,\t12 # length\n",
            "[ 1,\t\"widget\",\t9.99\t]\n",
            "[ 2,\t\"gadget\",\t19.50\t]",
        );

        let response = parse_response(text).expect("valid response");
        let QueryResponse::Table(table) = response else {
            panic!("expected a Table response, got {response:?}");
        };

        assert_eq!(table.row_count, 2);
        assert_eq!(table.columns[0].name, "id");
        assert_eq!(table.columns[1].name, "name");
        assert_eq!(table.columns[2].name, "price");
        assert_eq!(table.columns[2].type_name, "decimal");
        assert_eq!(table.columns[2].length, Some(12));
        assert_eq!(table.columns[2].typesizes, None);
        assert_eq!(
            table.rows,
            vec![
                vec![
                    Some("1".to_string()),
                    Some("widget".to_string()),
                    Some("9.99".to_string())
                ],
                vec![
                    Some("2".to_string()),
                    Some("gadget".to_string()),
                    Some("19.50".to_string())
                ],
            ]
        );
    }

    #[test]
    fn table_header_with_trailing_undocumented_fields_is_accepted() {
        // Real MonetDB sends more than 4 fields on a live SELECT (observed:
        // `0 2 3 2 6 6764 372 1565`); the extras must be ignored, not
        // rejected.
        let table = parse_table_header("0 2 3 2 6 6764 372 1565").unwrap();
        assert_eq!(table.query_id, 0);
        assert_eq!(table.row_count, 2);
        assert_eq!(table.declared_columns, 3);
        assert_eq!(table.embedded_tuples, 2);
    }

    #[test]
    fn parses_an_update_response() {
        assert_eq!(
            parse_response("&2 5 -1").unwrap(),
            QueryResponse::Update {
                affected: 5,
                last_insert_id: None
            }
        );
        assert_eq!(
            parse_response("&2 1 42").unwrap(),
            QueryResponse::Update {
                affected: 1,
                last_insert_id: Some(42)
            }
        );
        // Real MonetDB (Aug2024 release, per docker_tests) sends more than
        // 2 fields on a live INSERT: trailing fields must be ignored, not
        // rejected.
        assert_eq!(
            parse_response("&2 2 -1 2 12443 2035 108").unwrap(),
            QueryResponse::Update {
                affected: 2,
                last_insert_id: None
            }
        );
    }

    #[test]
    fn parses_a_schema_response() {
        assert_eq!(parse_response("&3").unwrap(), QueryResponse::Schema);
    }

    #[test]
    fn parses_a_transaction_response() {
        assert_eq!(
            parse_response("&4 t").unwrap(),
            QueryResponse::Transaction { auto_commit: true }
        );
        assert_eq!(
            parse_response("&4 f").unwrap(),
            QueryResponse::Transaction { auto_commit: false }
        );
    }

    #[test]
    fn empty_or_ok_response_is_ok() {
        assert_eq!(parse_response("").unwrap(), QueryResponse::Ok);
        assert_eq!(parse_response("=OK").unwrap(), QueryResponse::Ok);
    }

    #[test]
    fn error_line_becomes_a_database_error() {
        let err = parse_response("!42S02!no such table 'foo'").unwrap_err();
        match err {
            ResponseError::Database(e) => {
                assert_eq!(e.to_string(), "42S02!no such table 'foo'");
            }
            ResponseError::Protocol(e) => panic!("expected a Database error, got {e:?}"),
        }
    }

    #[test]
    fn error_line_takes_priority_even_mid_table() {
        // A batch/transaction failure can surface an error mid-response
        // (docs/DEVELOPMENT.md §4.7) — must not be swallowed by the table
        // accumulator.
        let text = "&1 0 1 1 1\n% a # name\n!some error occurred";
        let err = parse_response(text).unwrap_err();
        assert!(matches!(err, ResponseError::Database(_)));
    }

    #[test]
    fn info_lines_are_ignored() {
        assert_eq!(
            parse_response("#some notice\n&3").unwrap(),
            QueryResponse::Schema
        );
    }

    #[test]
    fn unrecognized_line_is_a_protocol_error() {
        let err = parse_response("~garbage").unwrap_err();
        assert!(matches!(err, ResponseError::Protocol(_)));
    }

    #[test]
    fn header_before_table_summary_is_a_protocol_error() {
        let err = parse_response("% a # name").unwrap_err();
        assert!(matches!(err, ResponseError::Protocol(_)));
    }

    #[test]
    fn unescapes_backslash_sequences_in_string_fields() {
        assert_eq!(
            unescape_field(r"'line1\nline2\ttabbed\\backslash'").unwrap(),
            Some("line1\nline2\ttabbed\\backslash".to_string())
        );
    }

    #[test]
    fn unescapes_double_quoted_string_fields() {
        // Real MonetDB wraps VARCHAR fields in double quotes, not the
        // single quotes docs/DEVELOPMENT.md originally described — see
        // unescape_field's doc comment.
        assert_eq!(
            unescape_field(r#""hello \"world\"""#).unwrap(),
            Some(r#"hello "world""#.to_string())
        );
    }

    #[test]
    fn empty_tuple_line_yields_no_fields() {
        assert_eq!(
            parse_tuple_fields("]").unwrap(),
            Vec::<Option<String>>::new()
        );
    }
}
