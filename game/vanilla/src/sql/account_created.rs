use std::sync::Arc;

use anyhow::anyhow;
use ddnet_accounts_types::account_id::AccountId;
use game_database::{
    StatementArgs,
    statement::{Statement, StatementBuilder},
    traits::{DbInterface, DbKind},
};
use game_interface::types::player_info::Hash;

#[derive(Debug, StatementArgs)]
struct StatementArg {
    account_id: AccountId,
    cert_hash: Vec<u8>,
}

type StatementResult = ();

#[derive(Debug, Clone, Copy)]
pub struct StatementAffected {
    pub rewrite_saves: u64,
}

#[derive(Clone)]
pub struct AccountCreated {
    rewrite_saves: Arc<Statement<StatementArg, StatementResult>>,
}

impl AccountCreated {
    pub async fn new(db: Arc<dyn DbInterface>, account_db: Option<DbKind>) -> anyhow::Result<Self> {
        let kind = account_db.ok_or_else(|| anyhow!("No account db specified"))?;
        let builder = StatementBuilder::<_, StatementArg, StatementResult>::new(
            kind,
            if matches!(kind, DbKind::MySql(_)) {
                include_str!("mysql/account_created/rewrite_saves.sql")
            } else {
                include_str!("sqlite/account_created/rewrite_saves.sql")
            },
            |arg| vec![arg.account_id, arg.cert_hash],
        );

        let rewrite_saves = Arc::new(Statement::new(db.clone(), builder).await?);

        Ok(Self { rewrite_saves })
    }

    /// On success returns the amount of saves rewritten.
    /// On error it returns the error and the amount of saves rewritten.
    pub async fn execute(
        &self,
        account_id: AccountId,
        cert_hash: Hash,
    ) -> anyhow::Result<StatementAffected, (anyhow::Error, StatementAffected)> {
        let rewrite_saves = self
            .rewrite_saves
            .execute(StatementArg {
                account_id,
                cert_hash: cert_hash.to_vec(),
            })
            .await
            .map_err(|err| (err, StatementAffected { rewrite_saves: 0 }))?;

        Ok(StatementAffected { rewrite_saves })
    }
}
