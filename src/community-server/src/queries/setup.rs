use std::{collections::HashMap, sync::Arc};

use game_database::traits::{DbInterface, DbKind, DbKindExtra, SqlText};

#[derive(Clone)]
pub struct SetupFriendList {
    stmts: HashMap<DbKind, Vec<SqlText>>,
}

impl SetupFriendList {
    pub async fn new(_db: Arc<dyn DbInterface>) -> anyhow::Result<Self> {
        let mut stmts: HashMap<DbKind, Vec<SqlText>> = Default::default();

        stmts
            .entry(DbKind::MySql(DbKindExtra::Main))
            .or_default()
            .push(include_str!("mysql/friend_list.sql").into());

        Ok(Self { stmts })
    }
}

pub async fn setup(db: Arc<dyn DbInterface>) -> anyhow::Result<()> {
    let setup_friend_list = SetupFriendList::new(db.clone()).await?;

    db.setup(
        "friend-list",
        vec![(1, setup_friend_list.stmts)].into_iter().collect(),
    )
    .await
}
