pub mod types {
    use std::time::Duration;

    use hiarc::{Hiarc, hiarc_safer_rc_refcell};
    use serde::{Deserialize, Serialize};

    use crate::config::config::{ConfigGameType, ConfigVanilla};

    #[derive(Debug, Hiarc, Clone, Copy, Default, Serialize, Deserialize)]
    pub enum GameType {
        #[default]
        Solo,
        Sided,
    }

    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc)]
    pub struct GameOptions {
        ty: GameType,
        config: ConfigVanilla,
    }

    #[hiarc_safer_rc_refcell]
    impl GameOptions {
        pub fn new(ty: GameType, config: ConfigVanilla) -> Self {
            Self { ty, config }
        }

        pub fn ty(&self) -> GameType {
            self.ty
        }
        pub fn game_ty(&self) -> ConfigGameType {
            self.config.game_type
        }
        pub fn allow_stages(&self) -> bool {
            self.config.allow_stages
        }
        pub fn score_limit(&self) -> u64 {
            self.config.score_limit
        }
        pub fn time_limit(&self) -> Option<Duration> {
            if self.config.time_limit_secs > 0 {
                Some(Duration::from_secs(self.config.time_limit_secs))
            } else {
                None
            }
        }
        pub fn sided_balance_time(&self) -> Option<Duration> {
            if self.config.auto_side_balance_secs > 0 {
                Some(Duration::from_secs(self.config.auto_side_balance_secs))
            } else {
                None
            }
        }
        pub fn friendly_fire(&self) -> bool {
            self.config.friendly_fire
        }
        pub fn laser_hit_self(&self) -> bool {
            self.config.laser_hit_self
        }
        pub fn max_ingame_players(&self) -> u32 {
            self.config.max_ingame_players
        }
        pub fn tournament_mode(&self) -> bool {
            self.config.tournament_mode
        }

        pub fn config_clone(&self) -> ConfigVanilla {
            self.config.clone()
        }
        pub fn replace_conf(&mut self, config: ConfigVanilla) {
            self.config = config;
        }
    }
}
