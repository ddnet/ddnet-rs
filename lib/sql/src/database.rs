use std::collections::HashMap;

use ddnet_account_sql::any::AnyPool;
use game_database::traits::DbKind;
use sqlx::{
    Any, FromRow,
    any::{AnyArguments, AnyRow},
    query::QueryAs,
};

#[derive(Debug, Clone)]
pub struct DatabaseDetails {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub ca_cert_path: String,
    pub connection_count: usize,
}

#[derive(Debug)]
pub struct Database {
    pub pools: HashMap<DbKind, AnyPool>,
}

impl Database {
    pub async fn new(connection_details: HashMap<DbKind, DatabaseDetails>) -> anyhow::Result<Self> {
        let mut pools: HashMap<DbKind, AnyPool> = Default::default();
        for (ty, connection_details) in connection_details {
            let is_localhost = connection_details.host == "localhost"
                || connection_details.host == "127.0.0.1"
                || connection_details.host == "::1";
            let pool = match ty {
                #[cfg(feature = "mysql")]
                DbKind::MySql(_) => AnyPool::MySql(
                    sqlx::mysql::MySqlPoolOptions::new()
                        .max_connections(connection_details.connection_count as u32)
                        .connect_with(
                            sqlx::mysql::MySqlConnectOptions::new()
                                .charset("utf8mb4")
                                .host(&connection_details.host)
                                .port(connection_details.port)
                                .database(&connection_details.database)
                                .username(&connection_details.username)
                                .password(&connection_details.password)
                                .ssl_mode(if !is_localhost {
                                    sqlx::mysql::MySqlSslMode::Required
                                } else {
                                    sqlx::mysql::MySqlSslMode::Preferred
                                })
                                .ssl_ca(&connection_details.ca_cert_path),
                        )
                        .await?,
                ),
                #[cfg(feature = "sqlite")]
                DbKind::Sqlite(_) => AnyPool::Sqlite(
                    sqlx::sqlite::SqlitePoolOptions::new()
                        .max_connections(connection_details.connection_count as u32)
                        .connect_with(
                            sqlx::sqlite::SqliteConnectOptions::new()
                                .filename(connection_details.database)
                                .create_if_missing(true),
                        )
                        .await?,
                ),
            };
            pools.insert(ty, pool);
        }

        Ok(Self { pools })
    }

    pub fn get_query<'a, F>(str: &'a str) -> QueryAs<'a, Any, F, AnyArguments<'a>>
    where
        F: for<'r> FromRow<'r, AnyRow>,
    {
        sqlx::query_as::<_, F>(str)
    }
}
