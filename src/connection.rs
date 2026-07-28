//! Database connection to MonetDB.

#![allow(clippy::manual_async_fn)]

use futures_core::future::BoxFuture;
use futures_core::stream::BoxStream;
use futures_util::StreamExt;
use sqlx_core::error::{BoxDynError, Error};
use sqlx_core::executor::{Execute, Executor};
use sqlx_core::sql_str::SqlStr;
use sqlx_core::transaction::Transaction;
use sqlx_core::Either;

use crate::arguments::MonetArguments;
use crate::database::{Monet, MonetQueryResult};
use crate::options::MonetConnectOptions;
use crate::protocol::response::QueryResponse;
use crate::protocol::MonetStream;
use crate::row::MonetRow;
use crate::statement::MonetStatement;
use crate::type_info::MonetTypeInfo;

/// A connection to a MonetDB server.
///
/// Holds the buffered MAPI transport; stage C adds the handshake/auth
/// state needed to actually establish one (see
/// `MonetConnectOptions::connect`), and stage D wires up `ping`/`close`.
pub struct MonetConnection {
    pub(crate) stream: MonetStream,
}

impl std::fmt::Debug for MonetConnection {
    // `BufferedSocket`/`Box<dyn Socket>` don't implement `Debug`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MonetConnection").finish_non_exhaustive()
    }
}

impl MonetConnection {
    #[allow(dead_code)] // constructed by stage C once the handshake completes
    pub(crate) fn new(stream: MonetStream) -> Self {
        Self { stream }
    }
}

impl sqlx_core::connection::Connection for MonetConnection {
    type Database = Monet;
    type Options = MonetConnectOptions;

    fn close(mut self) -> impl std::future::Future<Output = Result<(), Error>> + Send + 'static {
        // The MAPI reference implementations (pymonetdb, the official C
        // client) don't send a dedicated logout/bye message on close —
        // closing the TCP connection is sufficient. Flush first so any
        // still-buffered writes aren't silently dropped.
        async move {
            self.stream.flush().await?;
            self.stream.shutdown().await.map_err(Error::from)
        }
    }

    fn close_hard(
        mut self,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send + 'static {
        async move { self.stream.shutdown().await.map_err(Error::from) }
    }

    fn ping(&mut self) -> impl std::future::Future<Output = Result<(), Error>> + Send + '_ {
        // MAPI has no dedicated ping/heartbeat message
        // (docs/DEVELOPMENT.md §4.8) — a lightweight query is the
        // recommended connection-health check.
        async {
            crate::protocol::execute_query(&mut self.stream, "SELECT 1").await?;
            Ok(())
        }
    }

    fn begin(
        &mut self,
    ) -> impl std::future::Future<Output = Result<Transaction<'_, Monet>, Error>> + Send + '_ {
        async { unimplemented!("stage H: begin transaction via TransactionManager") }
    }

    fn shrink_buffers(&mut self) {
        self.stream.shrink_buffers();
    }

    fn flush(&mut self) -> impl std::future::Future<Output = Result<(), Error>> + Send + '_ {
        async { self.stream.flush().await.map_err(Error::from) }
    }

    fn should_flush(&self) -> bool {
        // Every write path in this driver (protocol::write_message) flushes
        // before returning, so there's never unflushed data sitting in the
        // buffer between driver-visible operations.
        false
    }
}

/// Run one query to completion and collect its results.
///
/// MAPI's simple query protocol (`docs/DEVELOPMENT.md` §4.3) returns a
/// whole response in one round trip rather than a true server-side cursor
/// stream, so "streaming" here just means wrapping an already-collected
/// `Vec` in a `Stream` — there is nothing to page in from the server.
async fn run_query(
    conn: &mut MonetConnection,
    sql: SqlStr,
    arguments: Result<Option<MonetArguments>, BoxDynError>,
) -> Vec<Result<Either<MonetQueryResult, MonetRow>, Error>> {
    let arguments = match arguments {
        Ok(arguments) => arguments,
        Err(err) => return vec![Err(Error::Encode(err))],
    };

    let sql_text = match &arguments {
        Some(arguments) => match arguments.substitute_into(sql.as_str()) {
            Ok(sql_text) => sql_text,
            Err(err) => return vec![Err(err)],
        },
        // No arguments bound (e.g. `sqlx::raw_sql()` or a plain `&str`
        // passed straight to an `Executor` method): send the SQL as-is.
        None => sql.as_str().to_string(),
    };

    let response = match crate::protocol::execute_query(&mut conn.stream, &sql_text).await {
        Ok(response) => response,
        Err(err) => return vec![Err(err)],
    };

    match response {
        QueryResponse::Table(table) => {
            let rows_affected = table.row_count;
            let mut items: Vec<Result<Either<MonetQueryResult, MonetRow>, Error>> =
                MonetRow::from_table_result(table)
                    .into_iter()
                    .map(|row| Ok(Either::Right(row)))
                    .collect();
            items.push(Ok(Either::Left(MonetQueryResult { rows_affected })));
            items
        }
        QueryResponse::Update { affected, .. } => {
            vec![Ok(Either::Left(MonetQueryResult {
                rows_affected: affected,
            }))]
        }
        QueryResponse::Schema | QueryResponse::Transaction { .. } | QueryResponse::Ok => {
            vec![Ok(Either::Left(MonetQueryResult::default()))]
        }
    }
}

impl<'c> Executor<'c> for &'c mut MonetConnection {
    type Database = Monet;

    fn fetch_many<'e, 'q: 'e, E>(
        self,
        mut query: E,
    ) -> BoxStream<'e, Result<Either<MonetQueryResult, MonetRow>, Error>>
    where
        'c: 'e,
        E: 'q + Execute<'q, Monet>,
    {
        let arguments = query.take_arguments();
        let sql = query.sql();

        Box::pin(
            futures_util::stream::once(async move {
                futures_util::stream::iter(run_query(self, sql, arguments).await)
            })
            .flatten(),
        )
    }

    fn fetch_optional<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxFuture<'e, Result<Option<MonetRow>, Error>>
    where
        'c: 'e,
        E: 'q + Execute<'q, Monet>,
    {
        Box::pin(async move {
            let mut stream = self.fetch_many(query);
            while let Some(item) = stream.next().await {
                if let Either::Right(row) = item? {
                    return Ok(Some(row));
                }
            }
            Ok(None)
        })
    }

    fn prepare_with<'e>(
        self,
        _sql: SqlStr,
        _parameters: &'e [MonetTypeInfo],
    ) -> BoxFuture<'e, Result<MonetStatement<'static>, Error>>
    where
        'c: 'e,
    {
        // Real server-side PREPARE isn't implemented in v1
        // (docs/DEVELOPMENT.md §4.4) — and isn't needed for it: the
        // `sqlx::query()`/`query_as()`/`query_scalar()` free functions
        // that this driver targets call `Executor::fetch_many`/
        // `fetch_optional` directly, never `prepare_with`. This is only
        // reachable via the explicit `conn.prepare()` API.
        Box::pin(async {
            unimplemented!(
                "stage J: explicit conn.prepare() is not supported in v1; use sqlx::query() instead"
            )
        })
    }
}

#[cfg(all(test, feature = "runtime-tokio"))]
mod docker_tests {
    use rust_decimal::Decimal;
    use sqlx_core::connection::{ConnectOptions, Connection as _};
    use sqlx_core::row::Row as _;
    use std::str::FromStr;

    use crate::options::MonetConnectOptions;

    use super::*;

    async fn connect(port: u16) -> MonetConnection {
        MonetConnectOptions::new()
            .host("127.0.0.1")
            .port(port)
            .username("monetdb")
            .password("monetdb")
            .database("monetdb")
            .connect()
            .await
            .expect("connect() should succeed against local docker MonetDB instance")
    }

    fn test_port() -> u16 {
        std::env::var("MONETDB_TEST_PORT")
            .unwrap_or_else(|_| "50001".into())
            .parse()
            .expect("MONETDB_TEST_PORT must be a valid u16 port number")
    }

    /// Stage G capstone: the full `sqlx-core` public API this driver
    /// targets — `Executor::fetch_all`/`fetch_one`/`execute`, bound
    /// arguments via `sqlx_core::query::query().bind(..)`, and `ping` —
    /// against a real MonetDB instance. This is the "connect + CRUD"
    /// bar the whole 60-step roadmap was scoped around
    /// (`docs/ACCEPTANCE.md`).
    #[tokio::test]
    #[ignore = "requires a running MonetDB docker instance; see docs/DEVELOPMENT.md stage G"]
    async fn full_sqlx_core_crud_cycle_against_docker_monetdb() {
        let mut conn = connect(test_port()).await;

        conn.ping().await.expect("ping should succeed");

        let _ = Executor::execute(&mut conn, "DROP TABLE sqlx_monetdb_executor_test").await;
        Executor::execute(
            &mut conn,
            "CREATE TABLE sqlx_monetdb_executor_test (id INT, name VARCHAR(50), price DECIMAL(10,2))",
        )
        .await
        .expect("CREATE TABLE should succeed");

        // Plain, unbound raw-SQL execution (Execute blanket impl for
        // SqlSafeStr types; take_arguments() -> None -> simple protocol).
        let insert_result = Executor::execute(
            &mut conn,
            "INSERT INTO sqlx_monetdb_executor_test VALUES (1, 'widget', 9.99)",
        )
        .await
        .expect("plain INSERT should succeed");
        assert_eq!(insert_result.rows_affected, 1);

        // Bound-argument path: sqlx_core::query::query().bind(..), proving
        // MonetArguments/Encode/client-side literal substitution end-to-end.
        let bound_insert = sqlx_core::query::query::<Monet>(
            "INSERT INTO sqlx_monetdb_executor_test VALUES (?, ?, ?)",
        )
        .bind(2_i32)
        .bind("gadget".to_string())
        .bind(Decimal::from_str("19.50").unwrap());
        let bound_result = Executor::execute(&mut conn, bound_insert)
            .await
            .expect("bound INSERT should succeed");
        assert_eq!(bound_result.rows_affected, 1);

        // fetch_all + typed decode.
        let rows = Executor::fetch_all(
            &mut conn,
            "SELECT id, name, price FROM sqlx_monetdb_executor_test ORDER BY id",
        )
        .await
        .expect("SELECT should succeed");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].try_get::<i32, _>(0).unwrap(), 1);
        assert_eq!(rows[0].try_get::<String, _>("name").unwrap(), "widget");
        assert_eq!(
            rows[0].try_get::<Decimal, _>(2).unwrap(),
            Decimal::from_str("9.99").unwrap()
        );
        assert_eq!(rows[1].try_get::<i32, _>(0).unwrap(), 2);
        assert_eq!(rows[1].try_get::<String, _>("name").unwrap(), "gadget");

        // fetch_one via a bound WHERE clause.
        let one = sqlx_core::query::query::<Monet>(
            "SELECT name FROM sqlx_monetdb_executor_test WHERE id = ?",
        )
        .bind(2_i32);
        let row = Executor::fetch_one(&mut conn, one)
            .await
            .expect("fetch_one should find exactly one row");
        assert_eq!(row.try_get::<String, _>(0).unwrap(), "gadget");

        // UPDATE / DELETE via the plain path.
        let update_result = Executor::execute(
            &mut conn,
            "UPDATE sqlx_monetdb_executor_test SET price = 12.00 WHERE id = 1",
        )
        .await
        .expect("UPDATE should succeed");
        assert_eq!(update_result.rows_affected, 1);

        let delete_result = Executor::execute(
            &mut conn,
            "DELETE FROM sqlx_monetdb_executor_test WHERE id = 2",
        )
        .await
        .expect("DELETE should succeed");
        assert_eq!(delete_result.rows_affected, 1);

        // A query against a nonexistent table surfaces as Error::Database,
        // not a hang or a protocol error.
        let error = Executor::fetch_all(&mut conn, "SELECT * FROM no_such_table_at_all")
            .await
            .expect_err("querying a nonexistent table should fail");
        assert!(matches!(error, Error::Database(_)));

        Executor::execute(&mut conn, "DROP TABLE sqlx_monetdb_executor_test")
            .await
            .expect("cleanup DROP TABLE should succeed");

        conn.close().await.expect("close should succeed");
    }

    /// Verifies `Pool<Monet>` — which sqlx-core provides generically, this
    /// driver writes no pool code of its own — actually works: concurrent
    /// `&pool` executor usage across multiple physical connections.
    #[tokio::test]
    #[ignore = "requires a running MonetDB docker instance; see docs/DEVELOPMENT.md stage I"]
    async fn pool_supports_concurrent_queries() {
        use sqlx_core::pool::{Pool, PoolOptions};

        let options = MonetConnectOptions::new()
            .host("127.0.0.1")
            .port(test_port())
            .username("monetdb")
            .password("monetdb")
            .database("monetdb");

        let pool: Pool<Monet> = PoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .expect("pool should connect");

        let tasks = (0..10).map(|i| {
            let pool = pool.clone();
            tokio::spawn(async move {
                let rows = Executor::fetch_all(&pool, "SELECT 1")
                    .await
                    .unwrap_or_else(|e| panic!("query {i} failed: {e}"));
                assert_eq!(rows.len(), 1);
            })
        });

        for task in tasks {
            task.await.expect("task should not panic");
        }

        pool.close().await;
    }

    /// **Important limitation check, not just a happy-path test**: this
    /// driver never negotiates `reply_size` (docs/DEVELOPMENT.md §4.1
    /// handshake option level 2) and doesn't implement `Xexport`
    /// pagination (`docs/DEVELOPMENT.md` step 32's known gap). If the
    /// server's default reply_size is smaller than a result set, rows
    /// would be silently truncated with no error. This test proves (or
    /// disproves) that risk against a concrete row count.
    #[tokio::test]
    #[ignore = "requires a running MonetDB docker instance; see docs/DEVELOPMENT.md known limitations"]
    async fn large_result_set_is_not_silently_truncated() {
        let mut conn = connect(test_port()).await;

        let _ = Executor::execute(&mut conn, "DROP TABLE sqlx_monetdb_truncation_test").await;
        Executor::execute(
            &mut conn,
            "CREATE TABLE sqlx_monetdb_truncation_test (n INT)",
        )
        .await
        .expect("CREATE TABLE should succeed");

        const ROW_COUNT: usize = 5000;
        let values: Vec<String> = (0..ROW_COUNT).map(|n| format!("({n})")).collect();
        let insert_sql = format!(
            "INSERT INTO sqlx_monetdb_truncation_test VALUES {}",
            values.join(", ")
        );
        Executor::execute(&mut conn, sqlx_core::sql_str::AssertSqlSafe(insert_sql))
            .await
            .expect("bulk INSERT should succeed");

        let rows = Executor::fetch_all(&mut conn, "SELECT n FROM sqlx_monetdb_truncation_test")
            .await
            .expect("SELECT should succeed");

        Executor::execute(&mut conn, "DROP TABLE sqlx_monetdb_truncation_test")
            .await
            .expect("cleanup DROP TABLE should succeed");

        assert_eq!(
            rows.len(),
            ROW_COUNT,
            "got {} of {ROW_COUNT} rows back — large result sets ARE being \
             silently truncated by the server's default reply_size; \
             docs/DEVELOPMENT.md must be updated and reply_size=0 (unlimited) \
             negotiated in the handshake before this driver is safe for \
             anything beyond small result sets",
            rows.len()
        );
    }
}
