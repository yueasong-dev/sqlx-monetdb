# sqlx-monetdb

[![CI](https://github.com/yueasong-dev/sqlx-monetdb/actions/workflows/ci.yml/badge.svg)](https://github.com/yueasong-dev/sqlx-monetdb/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust edition](https://img.shields.io/badge/edition-2021-orange.svg)](Cargo.toml)

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

**The problem: Rust cannot talk to MonetDB natively.** MonetDB is a widely used open-source column-store database for OLAP/data-warehouse workloads — but until this project, no async Rust application could connect to it without either dropping into unsafe FFI or being stuck on an abandoned, incomplete driver. That's a hard blocker for any Rust service — data pipelines, analytics backends, internal tooling — that needs to query a MonetDB instance.

Concretely, before `sqlx-monetdb`, every Rust project needing MonetDB had exactly two bad options:

- **The existing `monetdb` crate** (under the official `MonetDB` GitHub org) is early-stage, has seen no updates since October 2024, and is missing most of the protocol — types, prepared statements, transactions.
- **FFI into the official C client** (`libmonetdb5`/`mapi.c`) — which means a C toolchain dependency for every build/cross-compile target, a blocked async runtime during I/O, and no memory-safety guarantee at the FFI boundary.

Neither is acceptable for a modern async Rust codebase, and neither integrates with SQLx — the ecosystem's de facto standard for talking to Postgres/MySQL/SQLite — so any team already standardized on `sqlx::Pool`/`Row`/`Executor` had no consistent way to add MonetDB to that mix.

**What this project solves**: `sqlx-monetdb` implements MonetDB's MAPI wire protocol — handshake/auth, block framing, the query protocol, result decoding — natively in safe Rust, with zero C dependencies, and wires it directly into SQLx's `Database` trait family. The result is that MonetDB becomes a first-class SQLx citizen, sitting right next to `sqlx-postgres` and `sqlx-mysql`: same `Pool`, same `Executor`, same async ergonomics, one dependency less to cross-compile.

## Built differently: verified against reality, not assumptions

Most driver protocol implementations stop at "I read the reference source, so it must be right." This one didn't. The protocol reference was cross-checked against three independent sources — the official Python client (`pymonetdb`), the official C client (`mapi.c`), and the existing Rust implementation — and then **every documented behavior was run against a real MonetDB Docker instance**. Reality corrected the documentation more than once:

| What the docs/source said | What a real server actually does |
|---|---|
| String fields are single-quoted (`'like this'`) | Actually **double-quoted** (`"like this"`) |
| `%...#typesizes` gives DECIMAL `(precision, scale)` | That header **never appears**, even with `size_header=1` negotiated — the server sends `%...#length` instead |
| `&1`/`&2` response lines have exactly 4/2 fields | Real servers send several **undocumented extra trailing fields** |
| Declaring `reply_size=-1` ("unlimited") during handshake is enough | **Silently ignored** — a 5000-row query came back with exactly 100 rows and zero error, until a runtime `Xreply_size -1` command was sent after login |

Every one of these was caught by integration tests talking to an actual MonetDB container, not mocks — and every fix ships with the regression test that caught it. Built to withstand contact with the real thing.

## What works today

- Connect — URL parsing, builder API, full challenge/response auth
- `SELECT` / `INSERT` / `UPDATE` / `DELETE` / `CREATE`/`DROP TABLE`
- Parameter binding (`.bind()`, safely escaped, injection-tested)
- Typed decode: `bool`, `i8`/`i16`/`i32`/`i64`, `f32`/`f64`, `String`, `rust_decimal::Decimal`, `chrono` date/time/timestamp, `NULL`
- Connection pooling via `Pool<Monet>` (sqlx-core's generic implementation)
- Large result sets, with no silent truncation
- Runtime-agnostic: `tokio`, `async-std`, or `smol`

## Architecture

- **Pure Rust** — the entire MAPI protocol (challenge/response auth, block-based framing, result parsing) is implemented in safe Rust, no C client to install or cross-compile.
- **Runtime-agnostic** — built on `sqlx-core`'s transport abstraction (`sqlx_core::net`), so it works under `tokio`, `async-std`, or `smol` without code changes.
- **A real SQLx citizen** — implements `Database`/`Connection`/`Row`/`Column`/`Value`/`Arguments`/`Executor` directly, so `Pool`, `Executor::fetch_all`/`fetch_one`, and typed decoding all come from SQLx's own generic machinery rather than a parallel API surface.

## Installation

Not yet published to crates.io — add as a git dependency:

```toml
[dependencies]
sqlx-monetdb = { git = "https://github.com/yueasong-dev/sqlx-monetdb", features = ["runtime-tokio"] }
```

## Testing

Unit tests cover protocol parsing, hashing (validated against standard FIPS/RFC test vectors), and SQL literal escaping. Integration tests run against a real MonetDB instance:

```bash
docker run -d --name monetdb-test -p 50001:50000 -e MDB_DB_ADMIN_PASS=monetdb monetdb/monetdb:latest
cargo test --features runtime-tokio                                       # unit tests
MONETDB_TEST_PORT=50001 cargo test --features runtime-tokio -- --ignored  # + real server
```

CI (`fmt`, `clippy -D warnings`, `build`, `test`) runs on every push and pull request.

## License

Licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE)).
