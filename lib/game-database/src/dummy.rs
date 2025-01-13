use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::anyhow;

use crate::{
    statement::{QueryProperties, StatementDriverProps},
    traits::{DbInterface, DbKind, SqlText},
    types::DbType,
};

#[derive(Debug)]
pub struct DummyDb;

#[async_trait::async_trait]
impl DbInterface for DummyDb {
    fn kinds(&self) -> HashSet<DbKind> {
        Default::default()
    }

    async fn setup(
        &self,
        _version_name: &str,
        _versioned_stmts: BTreeMap<i64, HashMap<DbKind, Vec<SqlText>>>,
    ) -> anyhow::Result<()> {
        Err(anyhow!("not implemented for the dummy database"))
    }

    async fn prepare_statement(
        &self,
        _query_props: &QueryProperties,
        _kind: &DbKind,
        _driver_props: &StatementDriverProps,
    ) -> anyhow::Result<u64> {
        Err(anyhow!("not implemented for the dummy database"))
    }

    fn drop_statement(&self, _unique_id: u64) {}

    async fn fetch_optional(
        &self,
        _unique_id: u64,
        _args: Vec<DbType>,
    ) -> anyhow::Result<Option<HashMap<String, DbType>>> {
        Err(anyhow!("not implemented for the dummy database"))
    }

    async fn fetch_one(
        &self,
        _unique_id: u64,
        _args: Vec<DbType>,
    ) -> anyhow::Result<HashMap<String, DbType>> {
        Err(anyhow!("not implemented for the dummy database"))
    }

    async fn fetch_all(
        &self,
        _unique_id: u64,
        _args: Vec<DbType>,
    ) -> anyhow::Result<Vec<HashMap<String, DbType>>> {
        Err(anyhow!("not implemented for the dummy database"))
    }

    async fn execute(&self, _unique_id: u64, _args: Vec<DbType>) -> anyhow::Result<u64> {
        Err(anyhow!("not implemented for the dummy database"))
    }
}
