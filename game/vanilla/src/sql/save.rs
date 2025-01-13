use std::{collections::HashMap, sync::Arc};

use game_database::traits::{DbInterface, DbKind, DbKindExtra, SqlText};

#[derive(Clone)]
pub struct SetupSaves {
    stmts: HashMap<DbKind, Vec<SqlText>>,
}

impl SetupSaves {
    pub async fn new(db: Arc<dyn DbInterface>) -> anyhow::Result<Self> {
        let mut stmts: HashMap<_, Vec<_>> = Default::default();
        let kinds = db.kinds();

        if kinds.contains(&DbKind::MySql(DbKindExtra::Main)) {
            stmts
                .entry(DbKind::MySql(DbKindExtra::Main))
                .or_default()
                .push(include_str!("mysql/save/saves.sql").into());
        }
        if kinds.contains(&DbKind::Sqlite(DbKindExtra::Main)) {
            stmts
                .entry(DbKind::Sqlite(DbKindExtra::Main))
                .or_default()
                .push(include_str!("sqlite/save/saves.sql").into());
        }

        Ok(Self { stmts })
    }
}

pub async fn setup(db: Arc<dyn DbInterface>) -> anyhow::Result<()> {
    let setup_saves = SetupSaves::new(db.clone()).await?;

    db.setup(
        "game-server-vanilla",
        vec![(1, setup_saves.stmts)].into_iter().collect(),
    )
    .await
}
