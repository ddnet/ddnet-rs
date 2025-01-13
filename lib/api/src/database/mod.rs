use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::atomic::AtomicU64,
};

use crate::{read_result_from_host, upload_param};
use anyhow::anyhow;
use base_io::yield_now;
use game_database::{
    statement::{QueryProperties, StatementDriverProps},
    traits::{DbInterface, DbKind, SqlText},
    types::DbType,
};

extern "C" {
    fn api_db_kinds();
    fn api_db_setup();
    fn api_db_prepare_statement();
    fn api_db_drop_statement();
    fn api_db_fetch_optional();
    fn api_db_fetch_one();
    fn api_db_fetch_all();
    fn api_db_execute();
}

#[derive(Debug, Default)]
pub struct GameDbBackend {
    id: AtomicU64,
}

#[async_trait::async_trait]
impl DbInterface for GameDbBackend {
    fn kinds(&self) -> HashSet<DbKind> {
        unsafe { api_db_kinds() };
        read_result_from_host()
    }

    async fn setup(
        &self,
        version_name: &str,
        versioned_stmts: BTreeMap<i64, HashMap<DbKind, Vec<SqlText>>>,
    ) -> anyhow::Result<()> {
        let mut res;
        let id = self.id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        loop {
            upload_param(0, id);
            upload_param(1, version_name);
            upload_param(2, versioned_stmts.clone());
            unsafe {
                api_db_setup();
            }
            res = read_result_from_host::<Option<Result<(), String>>>();
            if res.is_some() {
                break;
            } else {
                yield_now::yield_now().await;
            }
        }
        res.unwrap().map_err(|err| anyhow!(err))
    }

    async fn prepare_statement(
        &self,
        query_props: &QueryProperties,
        kind: &DbKind,
        driver_props: &StatementDriverProps,
    ) -> anyhow::Result<u64> {
        let mut res;
        let id = self.id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        loop {
            upload_param(0, id);
            upload_param(1, query_props);
            upload_param(2, kind);
            upload_param(3, driver_props);
            unsafe {
                api_db_prepare_statement();
            }
            res = read_result_from_host::<Option<Result<u64, String>>>();
            if res.is_some() {
                break;
            } else {
                yield_now::yield_now().await;
            }
        }
        res.unwrap().map_err(|err| anyhow!(err))
    }

    fn drop_statement(&self, unique_id: u64) {
        upload_param(0, unique_id);
        unsafe {
            api_db_drop_statement();
        }
    }

    async fn fetch_optional(
        &self,
        unique_id: u64,
        args: Vec<DbType>,
    ) -> anyhow::Result<Option<HashMap<String, DbType>>> {
        let mut res;
        let id = self.id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        loop {
            upload_param(0, id);
            upload_param(1, unique_id);
            upload_param(2, &args);
            unsafe {
                api_db_fetch_optional();
            }
            res =
                read_result_from_host::<Option<Result<Option<HashMap<String, DbType>>, String>>>();
            if res.is_some() {
                break;
            } else {
                yield_now::yield_now().await;
            }
        }
        res.unwrap().map_err(|err| anyhow!(err))
    }

    async fn fetch_one(
        &self,
        unique_id: u64,
        args: Vec<DbType>,
    ) -> anyhow::Result<HashMap<String, DbType>> {
        let mut res;
        let id = self.id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        loop {
            upload_param(0, id);
            upload_param(1, unique_id);
            upload_param(2, &args);
            unsafe {
                api_db_fetch_one();
            }
            res = read_result_from_host::<Option<Result<HashMap<String, DbType>, String>>>();
            if res.is_some() {
                break;
            } else {
                yield_now::yield_now().await;
            }
        }
        res.unwrap().map_err(|err| anyhow!(err))
    }

    async fn fetch_all(
        &self,
        unique_id: u64,
        args: Vec<DbType>,
    ) -> anyhow::Result<Vec<HashMap<String, DbType>>> {
        let mut res;
        let id = self.id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        loop {
            upload_param(0, id);
            upload_param(1, unique_id);
            upload_param(2, &args);
            unsafe {
                api_db_fetch_all();
            }
            res = read_result_from_host::<Option<Result<Vec<HashMap<String, DbType>>, String>>>();
            if res.is_some() {
                break;
            } else {
                yield_now::yield_now().await;
            }
        }
        res.unwrap().map_err(|err| anyhow!(err))
    }

    async fn execute(&self, unique_id: u64, args: Vec<DbType>) -> anyhow::Result<u64> {
        let mut res;
        let id = self.id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        loop {
            upload_param(0, id);
            upload_param(1, unique_id);
            upload_param(2, &args);
            unsafe {
                api_db_execute();
            }
            res = read_result_from_host::<Option<Result<u64, String>>>();
            if res.is_some() {
                break;
            } else {
                yield_now::yield_now().await;
            }
        }
        res.unwrap().map_err(|err| anyhow!(err))
    }
}
