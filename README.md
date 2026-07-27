# sqlx-monetdb

Async, pure-Rust [SQLx](https://github.com/transact-rs/sqlx) driver for [MonetDB](https://www.monetdb.org/), implemented directly over the native MAPI wire protocol.

## What is this

MonetDB is a column-store analytical database widely used for OLAP/data-warehouse workloads. `sqlx-monetdb` lets Rust applications talk to MonetDB the same way they already talk to Postgres/MySQL/SQLite through SQLx — async, connection-pooled, with compile-time-friendly ergonomics — without shelling out to the official C client library or any FFI bindings.

## Problem it solves

MonetDB currently has no production-grade async Rust driver:

- The only existing Rust client (`monetdb`, maintained under the official `MonetDB` GitHub org) is early-stage, synchronous-leaning, and has seen no updates since October 2024 — large parts of the protocol (types, prepared statements, transactions) are unimplemented.
- There is no MonetDB driver for SQLx at all, meaning MonetDB is invisible to the large ecosystem of Rust services already standardized on SQLx's `Pool`/`Row`/`query!` API.
- Every alternative path today means dropping down to the official C client via FFI (`libmonetdb5`/`mapi.c`), which drags in a C dependency, blocks the async runtime, and loses Rust's memory-safety guarantees at the FFI boundary.

`sqlx-monetdb` closes that gap by implementing the MAPI protocol (handshake/auth, block framing, simple query protocol, result decoding) natively in Rust, and wiring it into SQLx's `Database` trait family so it behaves like any other first-class SQLx driver.

## Advantages

- **Pure Rust, no FFI** — no C client library to install, cross-compile, or audit; the entire MAPI protocol (challenge/response auth, block-based framing, result parsing) is implemented in safe Rust.
- **Async and runtime-agnostic** — built on `sqlx-core`'s runtime abstraction, so it works under `tokio`, `async-std`, or `smol` without code changes, the same way `sqlx-postgres`/`sqlx-mysql` do.
- **Drop-in SQLx ecosystem citizen** — reuses SQLx's generic `Pool`, `Transaction`, and `Row`/`Column`/`Arguments` machinery instead of inventing a parallel API surface; if you already use SQLx for another database, MonetDB feels the same.
- **Typed, documented protocol implementation** — built from a from-scratch protocol study cross-referencing the official C client (`mapi.c`), the official Python client (`pymonetdb`), and the existing Rust implementation, rather than guessing wire formats.
- **Actively targeting real gaps** — first-class support for MonetDB's type system (decimal precision/scale via `typesizes`, `hugeint`, temporal types, etc.), not just a thin int/string-only proof of concept.

## Status

Early development. See `docs/` (development and acceptance notes, not tracked in this repo) for the current implementation roadmap.

## License

Licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE)).
