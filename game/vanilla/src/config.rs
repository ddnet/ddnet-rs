pub mod config {
    use config::{config_default, ConfigInterface};
    use serde::{Deserialize, Serialize};

    #[derive(
        Debug,
        Default,
        Clone,
        Copy,
        Serialize,
        Deserialize,
        ConfigInterface,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
    )]
    pub enum ConfigGameType {
        #[default]
        Dm,
        Ctf,
    }

    #[config_default]
    #[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
    pub struct ConfigVanilla {
        pub game_type: ConfigGameType,
        #[default = 100]
        pub score_limit: u64,
        /// A value of `0` means no time limit.
        ///
        /// Time unit is seconds.
        #[default = 0]
        pub time_limit_secs: u64,
        /// A value of `0` means no balancing will happen.
        ///
        /// Time unit is seconds.
        #[default = 60]
        pub auto_side_balance_secs: u64,
        pub allow_stages: bool,
        pub friendly_fire: bool,
        pub laser_hit_self: bool,
        /// The maximum allowed players that are allowed to join the game.
        /// All other connected clients will instead be spectators.
        #[default = 16]
        #[conf_valid(range(min = 1, max = 1000000))]
        pub max_ingame_players: u32,
        pub tournament_mode: bool,
        /// This will allow the game to follow the current voted player
        /// even if not in range. Since this potentially allows cheating
        /// this is false for vanilla
        pub allow_player_vote_cam: bool,
    }

    /// Wraps vanilla config for the console chain
    #[config_default]
    #[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
    pub struct ConfigVanillaWrapper {
        pub vanilla: ConfigVanilla,
    }
}
