use std::sync::Arc;

use anyhow::anyhow;
use ddnet_accounts_types::account_id::AccountId;
use game_database::{
    StatementArgs, StatementResult,
    statement::{Statement, StatementBuilder},
    traits::{DbInterface, DbKind},
    types::UnixUtcTimestamp,
};

#[derive(Debug, StatementArgs)]
struct StatementArg {
    account_id: AccountId,
}

#[derive(Debug, StatementResult)]
pub struct StatementResult {
    pub id: i64,
    pub name: String,
    pub create_time: UnixUtcTimestamp,
}

#[derive(Clone)]
pub struct AccountInfo(Arc<Statement<StatementArg, StatementResult>>);

impl AccountInfo {
    pub async fn new(db: Arc<dyn DbInterface>, account_db: Option<DbKind>) -> anyhow::Result<Self> {
        let kind = account_db.ok_or_else(|| anyhow!("No account db specified"))?;
        let builder = StatementBuilder::<_, StatementArg, StatementResult>::new(
            kind,
            include_str!("generic/account_info/account_info.sql"),
            |arg| vec![arg.account_id],
        );

        let stm = Arc::new(Statement::new(db.clone(), builder).await?);

        Ok(Self(stm))
    }

    pub async fn fetch(&self, account_id: AccountId) -> anyhow::Result<StatementResult> {
        self.0.fetch_one(StatementArg { account_id }).await
    }
}
