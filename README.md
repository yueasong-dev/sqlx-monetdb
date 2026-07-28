# sqlx-monetdb

**A pure-Rust, async [SQLx](https://github.com/transact-rs/sqlx) driver for [MonetDB](https://www.monetdb.org/) — the MAPI wire protocol implemented from scratch and verified against a real running server, not just against documentation.**

No C client library. No FFI. No guessing at byte formats and hoping they're right.

```rust
use sqlx_core::{connection::ConnectOptions, executor::Executor};
use sqlx_monetdb::MonetConnectOptions;

let mut conn = MonetConnectOptions::new()
    .host("localhost")
    .username("monetdb")
    .password("monetdb")
    .database("monetdb")
    .connect()
    .await?;

let rows = Executor::fetch_all(&mut conn, "SELECT id, name FROM users").await?;
```

## Why this exists

MonetDB — a column-store analytical database used for OLAP/data-warehouse workloads — currently has no production-grade async Rust driver:

- The only existing Rust client (`monetdb`, under the official `MonetDB` GitHub org) is early-stage, has seen no updates since October 2024, and is missing most of the protocol (types, prepared statements, transactions).
- There's no MonetDB driver for SQLx at all — MonetDB is invisible to the large ecosystem of Rust services already standardized on SQLx's `Pool`/`Row`/`Executor` API.
- The only other path is FFI into the official C client (`libmonetdb5`/`mapi.c`), which means a C dependency, a blocked async runtime, and no memory-safety guarantee at the FFI boundary.

`sqlx-monetdb` implements the MAPI protocol — handshake/auth, block framing, the simple query protocol, result decoding — natively in Rust, wired into SQLx's `Database` trait family so it's a first-class citizen next to `sqlx-postgres` and `sqlx-mysql`.

## Built differently: verified against reality, not assumptions

Most of this driver's protocol reference material was cross-checked against three independent sources — the official Python client (`pymonetdb`), the official C client (`mapi.c`), and the existing (incomplete) Rust implementation. Reading source code is necessary but not sufficient: **every documented protocol behavior was then run against a real MonetDB Docker instance**, and reality corrected the documentation more than once:

| What the docs/source said | What a real server actually does |
|---|---|
| String fields are single-quoted (`'like this'`) | Actually **double-quoted** (`"like this"`) |
| `%...#typesizes` gives DECIMAL `(precision, scale)` | That header **never appears**, even with `size_header=1` negotiated — server sends `%...#length` instead |
| `&1`/`&2` response lines have exactly 4/2 fields | Real servers send several **undocumented extra trailing fields** |
| Declaring `reply_size=-1` ("unlimited") during handshake is enough | **It's silently ignored** — a 5000-row query came back with exactly 100 rows and zero error, until a runtime `Xreply_size -1` command was sent after login |

Every one of these was caught by integration tests that talk to an actual MonetDB container, not mocks — and every fix has a regression test backing it. This is a driver built to withstand contact with the real thing, and the corrections are documented so nobody has to rediscover them.

## What works today

| Capability | Status |
|---|---|
| Connect (URL parsing, builder API, challenge/response auth) | ✅ |
| `SELECT` / `INSERT` / `UPDATE` / `DELETE` / `CREATE`/`DROP TABLE` | ✅ |
| Parameter binding (`.bind()`, safely escaped, injection-tested) | ✅ |
| Typed decode: `bool`, `i8`/`i16`/`i32`/`i64`, `f32`/`f64`, `String`, `rust_decimal::Decimal`, `chrono` date/time/timestamp, `NULL` | ✅ |
| Connection pooling (`Pool<Monet>`, sqlx-core's generic implementation) | ✅ |
| Large result sets (no silent truncation) | ✅ |
| Runtime-agnostic (`tokio` / `async-std` / `smol`) | ✅ |
| Transactions (`BEGIN`/`COMMIT`/`ROLLBACK`) | ❌ not yet |
| TLS (`monetdbs://`) | ❌ not yet |
| Server-side prepared statements, `query!` compile-time macro | ❌ not yet |
| `hugeint`, `uuid`, `json`, `inet`, `interval` types | ❌ not yet |

**Known limitation**: bound-parameter placeholder substitution is a naive text split on `?` (no SQL tokenizer), so a literal `?` character inside the *query text itself* would be misread as a placeholder.

## Architecture

- **Pure Rust** — the entire MAPI protocol (challenge/response auth, block-based framing, result parsing) is implemented in safe Rust, no C client to install or cross-compile.
- **Runtime-agnostic** — built on `sqlx-core`'s transport abstraction (`sqlx_core::net`), so it works under `tokio`, `async-std`, or `smol` without code changes.
- **A real SQLx citizen** — implements `Database`/`Connection`/`Row`/`Column`/`Value`/`Arguments`/`Executor` etc. directly, so `Pool`, `Executor::fetch_all`/`fetch_one`, and typed decoding all come from SQLx's own generic machinery rather than a parallel API surface.

## Testing

69 unit tests (protocol parsing, hashing against standard FIPS/RFC test vectors, SQL literal escaping) plus a suite of integration tests that run against a real MonetDB instance:

```bash
docker run -d --name monetdb-test -p 50001:50000 -e MDB_DB_ADMIN_PASS=monetdb monetdb/monetdb:latest
cargo test --features runtime-tokio                       # unit tests
MONETDB_TEST_PORT=50001 cargo test --features runtime-tokio -- --ignored  # + real server
```

CI (`fmt`, `clippy -D warnings`, `build`, `test`) runs on every push/PR.

## Status

Early but functional — the "connect + CRUD" path is real and tested, not aspirational. Not yet published to crates.io; add as a git or path dependency.

## License

Licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE)).
