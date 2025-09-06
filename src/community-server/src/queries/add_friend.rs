use std::sync::Arc;

use ddnet_accounts_types::account_id::AccountId;
use game_database::{
    StatementArgs,
    statement::{Statement, StatementBuilder},
    traits::{DbInterface, DbKind, DbKindExtra},
};

#[derive(Debug, StatementArgs)]
struct StatementArg {
    account_id: AccountId,
    other_account_id: AccountId,
}

#[derive(Clone)]
pub struct AddFriend(Arc<Statement<StatementArg, ()>>);

impl AddFriend {
    pub async fn new(db: Arc<dyn DbInterface>) -> anyhow::Result<Self> {
        let builder = StatementBuilder::<_, StatementArg, ()>::new(
            DbKind::MySql(DbKindExtra::Main),
            include_str!("mysql/add_friend.sql"),
            |arg| vec![arg.account_id],
        );

        let stm = Arc::new(Statement::new(db.clone(), builder).await?);

        Ok(Self(stm))
    }

    pub async fn execute(
        &self,
        account_id: AccountId,
        other_account_id: AccountId,
    ) -> anyhow::Result<u64> {
        self.0
            .execute(StatementArg {
                account_id,
                other_account_id,
            })
            .await
    }
}
