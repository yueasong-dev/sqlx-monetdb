//! Prepared statement representation for MonetDB.

use sqlx_core::sql_str::SqlStr;
use sqlx_core::Either;

use crate::column::MonetColumn;
use crate::database::Monet;
use crate::type_info::MonetTypeInfo;

/// A prepared statement from MonetDB.
///
/// Stage G will implement statement preparation and caching.
#[derive(Debug, Clone)]
pub struct MonetStatement<'q> {
    /// The SQL text for this statement.
    sql: SqlStr,
    /// Expected columns in the result set (empty until statement is prepared).
    #[allow(dead_code)]
    columns: Vec<MonetColumn>,
    /// Expected parameter types (empty if not available).
    #[allow(dead_code)]
    parameters: Option<Vec<MonetTypeInfo>>,
    /// Lifetime marker for borrowed SQL.
    _marker: std::marker::PhantomData<&'q ()>,
}

impl<'q> MonetStatement<'q> {
    /// Create a new statement with the given SQL.
    #[allow(dead_code)]
    pub(crate) fn new(sql: SqlStr) -> Self {
        Self {
            sql,
            columns: Vec::new(),
            parameters: None,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'q> sqlx_core::statement::Statement for MonetStatement<'q> {
    type Database = Monet;

    fn into_sql(self) -> SqlStr {
        self.sql
    }

    fn sql(&self) -> &SqlStr {
        &self.sql
    }

    fn parameters(
        &self,
    ) -> Option<Either<&[<Monet as sqlx_core::database::Database>::TypeInfo], usize>> {
        self.parameters
            .as_ref()
            .map(|params| Either::Left(params.as_slice()))
    }

    fn columns(&self) -> &[<Monet as sqlx_core::database::Database>::Column] {
        &self.columns
    }

    fn query(
        &self,
    ) -> sqlx_core::query::Query<'_, Monet, <Monet as sqlx_core::database::Database>::Arguments>
    {
        unimplemented!("stage G: implement query_statement helper")
    }

    fn query_with<A>(&self, _arguments: A) -> sqlx_core::query::Query<'_, Monet, A>
    where
        A: sqlx_core::arguments::IntoArguments<Monet>,
    {
        unimplemented!("stage G: implement query_statement_with helper")
    }

    fn query_as<O>(
        &self,
    ) -> sqlx_core::query_as::QueryAs<
        '_,
        Monet,
        O,
        <Monet as sqlx_core::database::Database>::Arguments,
    >
    where
        O: for<'r> sqlx_core::from_row::FromRow<'r, <Monet as sqlx_core::database::Database>::Row>,
    {
        unimplemented!("stage G: implement query_statement_as helper")
    }

    fn query_as_with<'s, O, A>(
        &'s self,
        _arguments: A,
    ) -> sqlx_core::query_as::QueryAs<'s, Monet, O, A>
    where
        O: for<'r> sqlx_core::from_row::FromRow<'r, <Monet as sqlx_core::database::Database>::Row>,
        A: sqlx_core::arguments::IntoArguments<Monet>,
    {
        unimplemented!("stage G: implement query_statement_as_with helper")
    }

    fn query_scalar<O>(
        &self,
    ) -> sqlx_core::query_scalar::QueryScalar<
        '_,
        Monet,
        O,
        <Monet as sqlx_core::database::Database>::Arguments,
    >
    where
        (O,):
            for<'r> sqlx_core::from_row::FromRow<'r, <Monet as sqlx_core::database::Database>::Row>,
    {
        unimplemented!("stage G: implement query_statement_scalar helper")
    }

    fn query_scalar_with<'s, O, A>(
        &'s self,
        _arguments: A,
    ) -> sqlx_core::query_scalar::QueryScalar<'s, Monet, O, A>
    where
        (O,):
            for<'r> sqlx_core::from_row::FromRow<'r, <Monet as sqlx_core::database::Database>::Row>,
        A: sqlx_core::arguments::IntoArguments<Monet>,
    {
        unimplemented!("stage G: implement query_statement_scalar_with helper")
    }
}
