use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{atomic::AtomicU64, Arc},
};

use anyhow::anyhow;
use ddnet_account_sql::{
    any::{AnyConnection, AnyQuery, AnyRow, AnyStatement},
    version::{get_version, set_version},
};
use game_database::{
    statement::{QueryProperties, StatementDriverProps},
    traits::{DbInterface, DbKind, SqlText},
    types::{DbType, UnixUtcTimestamp},
};
use parking_lot::Mutex;
use sql::database::Database;
use sqlx::Executor;
use sqlx::Row;
use sqlx::Statement;

#[derive(Clone)]
pub struct CachedStatement {
    kind: DbKind,
    stmt: Arc<AnyStatement<'static>>,
    qry_props: QueryProperties,
}

pub struct GameDbBackend {
    db: Arc<Database>,
    statements: Mutex<HashMap<u64, CachedStatement>>,
    id_generator: AtomicU64,
}

impl GameDbBackend {
    pub fn new(db: Arc<Database>) -> anyhow::Result<Self> {
        Ok(Self {
            db,
            statements: Default::default(),
            id_generator: Default::default(),
        })
    }

    fn get_query<'a>(
        &self,
        stmt: &'a CachedStatement,
        args: &'a [DbType],
    ) -> anyhow::Result<AnyQuery<'a>> {
        Ok(match stmt.stmt.as_ref() {
            #[cfg(feature = "mysql")]
            AnyStatement::MySql(stmt) => {
                let mut qry = stmt.query();
                for arg in args.iter() {
                    match arg {
                        DbType::I16(v) => qry = qry.bind(*v),
                        DbType::I32(v) => qry = qry.bind(*v),
                        DbType::I64(v) => qry = qry.bind(*v),
                        DbType::F32(v) => qry = qry.bind(*v),
                        DbType::F64(v) => qry = qry.bind(*v),
                        DbType::Bool(v) => qry = qry.bind(*v),
                        DbType::String(v) => qry = qry.bind(v),
                        DbType::Vec(v) => qry = qry.bind(v),
                        DbType::DateTime(v) => {
                            let time_stamp = <sqlx::types::chrono::DateTime<
                                sqlx::types::chrono::Utc,
                            >>::from_timestamp(
                                v.secs as i64, v.subsec_nanos
                            )
                            .ok_or_else(|| anyhow!("not a valid utc timestamp"))?;
                            qry = qry.bind(time_stamp);
                        }
                    }
                }
                AnyQuery::MySql(qry)
            }
            #[cfg(feature = "sqlite")]
            AnyStatement::Sqlite(stmt) => {
                let mut qry = stmt.query();
                for arg in args.iter() {
                    match arg {
                        DbType::I16(v) => qry = qry.bind(*v),
                        DbType::I32(v) => qry = qry.bind(*v),
                        DbType::I64(v) => qry = qry.bind(*v),
                        DbType::F32(v) => qry = qry.bind(*v),
                        DbType::F64(v) => qry = qry.bind(*v),
                        DbType::Bool(v) => qry = qry.bind(*v),
                        DbType::String(v) => qry = qry.bind(v),
                        DbType::Vec(v) => qry = qry.bind(v),
                        DbType::DateTime(v) => {
                            let time_stamp = <sqlx::types::chrono::DateTime<
                                sqlx::types::chrono::Utc,
                            >>::from_timestamp(
                                v.secs as i64, v.subsec_nanos
                            )
                            .ok_or_else(|| anyhow!("not a valid utc timestamp"))?;
                            qry = qry.bind(time_stamp);
                        }
                    }
                }
                AnyQuery::Sqlite(qry)
            }
        })
    }

    fn get_result(
        stmt: &CachedStatement,
        rows: impl Iterator<Item = AnyRow>,
    ) -> anyhow::Result<Vec<HashMap<String, DbType>>> {
        rows.map(|row| {
            stmt.qry_props
                .result_mapping
                .iter()
                .map(|(name, ty)| {
                    let val = match &row {
                        #[cfg(feature = "mysql")]
                        AnyRow::MySql(row) => match ty {
                            DbType::I16(_) => DbType::I16(row.try_get::<i16, _>(name.as_str())?),
                            DbType::I32(_) => DbType::I32(row.try_get::<i32, _>(name.as_str())?),
                            DbType::I64(_) => DbType::I64(row.try_get::<i64, _>(name.as_str())?),
                            DbType::F32(_) => DbType::F32(row.try_get::<f32, _>(name.as_str())?),
                            DbType::F64(_) => DbType::F64(row.try_get::<f64, _>(name.as_str())?),
                            DbType::Bool(_) => DbType::Bool(row.try_get::<bool, _>(name.as_str())?),
                            DbType::String(_) => {
                                DbType::String(row.try_get::<String, _>(name.as_str())?)
                            }
                            DbType::Vec(_) => {
                                DbType::Vec(row.try_get::<Vec<u8>, _>(name.as_str())?)
                            }
                            DbType::DateTime(_) => {
                                let time_stamp: sqlx::types::chrono::DateTime<
                                    sqlx::types::chrono::Utc,
                                > = row.try_get(name.as_str())?;
                                DbType::DateTime(UnixUtcTimestamp {
                                    secs: time_stamp.timestamp() as u64,
                                    subsec_nanos: time_stamp.timestamp_subsec_nanos(),
                                })
                            }
                        },
                        #[cfg(feature = "sqlite")]
                        AnyRow::Sqlite(row) => match ty {
                            DbType::I16(_) => DbType::I16(row.try_get::<i16, _>(name.as_str())?),
                            DbType::I32(_) => DbType::I32(row.try_get::<i32, _>(name.as_str())?),
                            DbType::I64(_) => DbType::I64(row.try_get::<i64, _>(name.as_str())?),
                            DbType::F32(_) => DbType::F32(row.try_get::<f32, _>(name.as_str())?),
                            DbType::F64(_) => DbType::F64(row.try_get::<f64, _>(name.as_str())?),
                            DbType::Bool(_) => DbType::Bool(row.try_get::<bool, _>(name.as_str())?),
                            DbType::String(_) => {
                                DbType::String(row.try_get::<String, _>(name.as_str())?)
                            }
                            DbType::Vec(_) => {
                                DbType::Vec(row.try_get::<Vec<u8>, _>(name.as_str())?)
                            }
                            DbType::DateTime(_) => {
                                let time_stamp: sqlx::types::chrono::DateTime<
                                    sqlx::types::chrono::Utc,
                                > = row.try_get(name.as_str())?;
                                DbType::DateTime(UnixUtcTimestamp {
                                    secs: time_stamp.timestamp() as u64,
                                    subsec_nanos: time_stamp.timestamp_subsec_nanos(),
                                })
                            }
                        },
                    };
                    anyhow::Ok((name.clone(), val))
                })
                .collect::<anyhow::Result<HashMap<_, _>>>()
        })
        .collect::<anyhow::Result<Vec<_>>>()
    }
}

#[async_trait::async_trait]
impl DbInterface for GameDbBackend {
    fn kinds(&self) -> HashSet<DbKind> {
        let mut res: HashSet<DbKind> = Default::default();
        res.extend(self.db.pools.keys());
        res
    }

    async fn setup(
        &self,
        version_name: &str,
        versioned_stmts: BTreeMap<i64, HashMap<DbKind, Vec<SqlText>>>,
    ) -> anyhow::Result<()> {
        let mut stmts_per_kind: HashMap<DbKind, BTreeMap<_, Vec<_>>> = Default::default();
        versioned_stmts.into_iter().for_each(|(version, stmts)| {
            for (kind, mut stmt) in stmts {
                stmts_per_kind
                    .entry(kind)
                    .or_default()
                    .entry(version)
                    .or_default()
                    .append(&mut stmt);
            }
        });

        for (kind, versioned_stmts) in stmts_per_kind {
            if let Some(pool) = self.db.pools.get(&kind) {
                let mut connection = pool.acquire().await?;
                let mut connection = connection.acquire().await?;

                let version_name = version_name.to_string();
                connection
                    .transaction(|mut con| {
                        Box::pin(async move {
                            let mut version = get_version(&mut con.con(), &version_name).await?;
                            for (stmts_version, stmts) in versioned_stmts {
                                if version < stmts_version {
                                    for s in stmts {
                                        let qry = match kind {
                                            #[cfg(feature = "mysql")]
                                            DbKind::MySql(_) => AnyQuery::MySql(sqlx::query(&s)),
                                            #[cfg(feature = "sqlite")]
                                            DbKind::Sqlite(_) => AnyQuery::Sqlite(sqlx::query(&s)),
                                        };
                                        qry.execute(&mut con.con()).await?;
                                    }

                                    set_version(&mut con.con(), &version_name, stmts_version)
                                        .await?;
                                    version = stmts_version;
                                }
                            }
                            anyhow::Ok(())
                        })
                    })
                    .await?;
            }
        }

        Ok(())
    }

    async fn prepare_statement(
        &self,
        query_props: &QueryProperties,
        kind: &DbKind,
        driver_props: &StatementDriverProps,
    ) -> anyhow::Result<u64> {
        let pool = self
            .db
            .pools
            .get(kind)
            .ok_or_else(|| anyhow!("database of kind {kind:?} not active."))?;
        let mut connection = pool.acquire().await?;
        let connection = connection.acquire().await?;

        let stm = match connection {
            #[cfg(feature = "mysql")]
            AnyConnection::MySql(con) => AnyStatement::MySql(sqlx::Statement::to_owned(
                &con.prepare(&driver_props.sql).await?,
            )),
            #[cfg(feature = "sqlite")]
            AnyConnection::Sqlite(con) => AnyStatement::Sqlite(sqlx::Statement::to_owned(
                &con.prepare(&driver_props.sql).await?,
            )),
        };

        let unique_id = self
            .id_generator
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.statements.lock().insert(
            unique_id,
            CachedStatement {
                kind: *kind,
                stmt: Arc::new(stm),
                qry_props: query_props.clone(),
            },
        );
        Ok(unique_id)
    }

    fn drop_statement(&self, unique_id: u64) {
        self.statements.lock().remove(&unique_id);
    }

    async fn fetch_optional(
        &self,
        unique_id: u64,
        args: Vec<DbType>,
    ) -> anyhow::Result<Option<HashMap<String, DbType>>> {
        let stmt = self
            .statements
            .lock()
            .get(&unique_id)
            .cloned()
            .ok_or_else(|| anyhow!("no statement with id \"{}\" found", unique_id))?;

        let pool = self
            .db
            .pools
            .get(&stmt.kind)
            .ok_or_else(|| anyhow!("database of kind {:?} not active.", stmt.kind))?;
        let mut connection = pool.acquire().await?;
        let mut connection = connection.acquire().await?;

        let qry = self.get_query(&stmt, &args)?;

        qry.fetch_optional(&mut connection)
            .await?
            .map(|res| {
                Self::get_result(&stmt, [res].into_iter()).and_then(|res| {
                    res.into_iter()
                        .next()
                        .ok_or_else(|| anyhow!("no result fetched"))
                })
            })
            .transpose()
    }

    async fn fetch_one(
        &self,
        unique_id: u64,
        args: Vec<DbType>,
    ) -> anyhow::Result<HashMap<String, DbType>> {
        let stmt = self
            .statements
            .lock()
            .get(&unique_id)
            .cloned()
            .ok_or_else(|| anyhow!("no statement with id \"{}\" found", unique_id))?;

        let pool = self
            .db
            .pools
            .get(&stmt.kind)
            .ok_or_else(|| anyhow!("database of kind {:?} not active.", stmt.kind))?;
        let mut connection = pool.acquire().await?;
        let mut connection = connection.acquire().await?;

        let qry = self.get_query(&stmt, &args)?;

        Self::get_result(&stmt, [qry.fetch_one(&mut connection).await?].into_iter()).and_then(
            |res| {
                res.into_iter()
                    .next()
                    .ok_or_else(|| anyhow!("no result fetched"))
            },
        )
    }

    async fn fetch_all(
        &self,
        unique_id: u64,
        args: Vec<DbType>,
    ) -> anyhow::Result<Vec<HashMap<String, DbType>>> {
        let stmt = self
            .statements
            .lock()
            .get(&unique_id)
            .cloned()
            .ok_or_else(|| anyhow!("no statement with id \"{}\" found", unique_id))?;

        let pool = self
            .db
            .pools
            .get(&stmt.kind)
            .ok_or_else(|| anyhow!("database of kind {:?} not active.", stmt.kind))?;
        let mut connection = pool.acquire().await?;
        let mut connection = connection.acquire().await?;

        let qry = self.get_query(&stmt, &args)?;

        Self::get_result(&stmt, qry.fetch_all(&mut connection).await?.into_iter())
    }

    async fn execute(&self, unique_id: u64, args: Vec<DbType>) -> anyhow::Result<u64> {
        let stmt = self
            .statements
            .lock()
            .get(&unique_id)
            .cloned()
            .ok_or_else(|| anyhow!("no statement with id \"{}\" found", unique_id))?;

        let pool = self
            .db
            .pools
            .get(&stmt.kind)
            .ok_or_else(|| anyhow!("database of kind {:?} not active.", stmt.kind))?;
        let mut connection = pool.acquire().await?;
        let mut connection = connection.acquire().await?;

        let qry = self.get_query(&stmt, &args)?;

        Ok(qry.execute(&mut connection).await?.rows_affected())
    }
}

// TODO: these tests make no sense without having ddnet database
#[cfg(test)]
mod test {
    use std::sync::Arc;

    use game_database::{
        statement::{Statement, StatementBuilder},
        traits::{DbKind, DbKindExtra},
        StatementArgs, StatementResult,
    };
    use sql::database::{Database, DatabaseDetails};

    use crate::GameDbBackend;

    #[tokio::test]
    async fn builder() -> anyhow::Result<()> {
        #[derive(StatementArgs)]
        struct StatementArg {
            map: String,
            server: String,
            offset: i64,
            count: i64,
        }

        #[allow(unused)]
        #[derive(Debug, StatementResult)]
        struct StatementResult {
            name: String,
            time: f32,
            ranking: i64,
        }

        let builder = StatementBuilder::<_, StatementArg, StatementResult>::new(
            DbKind::MySql(DbKindExtra::Main),
            "
            SELECT 
                name, time, ranking 
            FROM 
                ( 
                    SELECT 
                        RANK() OVER w AS ranking, 
                        MIN(Time) AS time, 
                        Name as name 
                    FROM 
                        record_race 
                    WHERE 
                        Map = ? AND 
                        Server LIKE ? 
                    GROUP BY Name 
                    WINDOW w AS (ORDER BY MIN(Time)) 
                ) as a 
            ORDER BY Ranking DESC 
            LIMIT ? OFFSET ?
            ;",
            |arg| vec![arg.map, arg.server, arg.count, arg.offset],
        );

        let db = Arc::new(GameDbBackend::new(Arc::new(
            Database::new(
                [(
                    DbKind::MySql(DbKindExtra::Main),
                    DatabaseDetails {
                        host: "localhost".into(),
                        port: 3306,
                        database: "teeworlds".into(),
                        username: "ddnet-account-test".into(),
                        password: "test".into(),
                        ca_cert_path: "/etc/mysql/ssl/ca-cert.pem".into(),
                        connection_count: 3,
                    },
                )]
                .into(),
            )
            .await?,
        ))?);

        let b = base::benchmark::Benchmark::new(true);
        let stm = Statement::new(db.clone(), builder).await?;
        b.bench("statement");

        let res = stm
            .fetch_all(StatementArg {
                map: "Multeasymap".into(),
                server: "%".into(),
                offset: 5,
                count: 10,
            })
            .await?;
        b.bench("result");

        dbg!(res);

        let builder = StatementBuilder::<_, (), StatementRankResult>::new(
            DbKind::MySql(DbKindExtra::Main),
            "
            SELECT Ranking as ranking, Time as time, PercentRank as rank_percent 
            FROM (  
                SELECT 
                    RANK() OVER w AS Ranking, PERCENT_RANK() OVER w as PercentRank, MIN(Time) AS Time, Name 
                FROM 
                    record_race 
                WHERE 
                    Map = 'Multeasymap' AND Server LIKE '%' 
                GROUP BY 
                    Name 
                WINDOW w AS (ORDER BY MIN(Time))
            ) as a 
            WHERE Name = 'deen';
        ",|_| vec![]);
        let stmt = Statement::new(db, builder).await?;
        b.bench("statement");

        #[allow(unused)]
        #[derive(Debug, StatementResult)]
        struct StatementRankResult {
            ranking: i64,
            time: f32,
            rank_percent: f32,
        }

        let res = stmt.fetch_all(()).await?;
        b.bench("result");

        dbg!(res);

        Ok(())
    }
}
