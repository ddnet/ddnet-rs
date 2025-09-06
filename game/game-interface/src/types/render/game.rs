pub mod game_match;

use base::network_string::PoolNetworkString;
use game_match::{MatchSide, MatchStandings};
use hiarc::Hiarc;
use pool::datatypes::PoolVec;
use serde::{Deserialize, Serialize};

use crate::{
    client_commands::MAX_TEAM_NAME_LEN,
    types::{
        character_info::{MAX_ASSET_NAME_LEN, MAX_CHARACTER_NAME_LEN, NetworkSkinInfo},
        game::GameTickType,
        resource_key::PoolNetworkResourceKey,
    },
};

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct MatchRoundGameOverWinnerCharacter {
    pub name: PoolNetworkString<MAX_CHARACTER_NAME_LEN>,
    pub skin: PoolNetworkResourceKey<MAX_ASSET_NAME_LEN>,
    pub skin_info: NetworkSkinInfo,
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum MatchRoundGameOverWinner {
    Characters(PoolVec<MatchRoundGameOverWinnerCharacter>),
    Side(MatchSide),
    /// A side won and the game provided a custom name for it,
    /// e.g. the clan name if all player of a side were in one clan
    SideNamed(PoolNetworkString<MAX_TEAM_NAME_LEN>),
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum MatchRoundGameOverWinBy {
    TimeLimit,
    ScoreLimit,
    Other,
}

/// If the game round has a game round countdown for this character,
/// this should be set to [`MatchRoundTimeType::TimeLimit`].
/// Else it should be set to [`MatchRoundTimeType::Normal`].
/// If the round is over, but a winner must be decided [`MatchRoundTimeType::SuddenDeath`].
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum MatchRoundTimeType {
    Normal,
    TimeLimit {
        ticks_left: GameTickType,
    },
    SuddenDeath,
    GameOver {
        winner: MatchRoundGameOverWinner,
        by: MatchRoundGameOverWinBy,
    },
}

/// The game information for a single game in a stage.
/// The type of game depends on the game mode (race for ddrace, match for vanilla, etc.)
#[derive(Debug, Hiarc, Serialize, Deserialize)]
pub enum GameRenderInfo {
    Race {},
    Match {
        standings: MatchStandings,
        /// This is usually a round timer e.g. for competitive games.
        /// See [`MatchRoundTimeType`] for more information.
        round_time_type: MatchRoundTimeType,
        /// Whether to show a warning that the current sides have
        /// an unequal amount of players
        unbalanced: bool,
    },
}
