use std::time::Duration;

use base::{
    hash::Hash,
    network_string::{NetworkReducedAsciiString, NetworkString},
};
use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

use crate::{
    interface::MAX_MAP_NAME_LEN,
    types::{
        character_info::{NetworkSkinInfo, MAX_ASSET_NAME_LEN, MAX_CHARACTER_NAME_LEN},
        id_types::PlayerId,
        resource_key::NetworkResourceKey,
    },
};

/// The difficulty of a map from [0-10].
/// Usually the client displays it as stars, where 0 is no stars
/// all uneven numbers are half stars and 10 are 5 stars.
#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub struct MapDifficulty(u8);

impl MapDifficulty {
    pub fn new(difficulty: u8) -> Option<Self> {
        if difficulty <= 10 {
            Some(Self(difficulty))
        } else {
            None
        }
    }

    pub fn get(&self) -> u8 {
        self.0
    }
}

pub const MAX_MAP_AUTHOR_NAME_LEN: usize = 64;
/// Some mod specific details about this map.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum MapVoteDetails {
    None,
    Ddrace {
        /// How many points a player gets as reward
        /// for finishing the map
        points_reward: u64,
        /// See [`MapDifficulty`].
        difficulty: MapDifficulty,
        /// UTC release date of the map
        release_date: chrono::DateTime<chrono::Utc>,
        /// The name of the map authors
        authors: Vec<NetworkString<MAX_MAP_AUTHOR_NAME_LEN>>,
    },
    Vanilla {
        /// Whether the map is suited for sided gameplay.
        /// E.g. red and blue have an equal amount of
        /// Spikes, weapon spawns etc.
        /// In other words if the map is balanced.
        sided_friendly: bool,
    },
}

pub const MAX_CATEGORY_NAME_LEN: usize = 32;
pub const MAX_MISC_NAME_LEN: usize = 64;

/// Information of a map for a vote on the client.
#[derive(Debug, Hiarc, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MapVoteKey {
    pub name: NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
    /// The hash is optional. If the hash is `None`, then
    /// a preview of the map is not possible.
    pub hash: Option<Hash>,
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct MapCategoryVoteKey {
    pub category: NetworkString<MAX_CATEGORY_NAME_LEN>,
    pub map: MapVoteKey,
}

/// Information of a map for a vote on the client.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct MapVote {
    /// Has a thumbnail resource that can be downlaoded
    /// from the game server's resource server.
    ///
    /// If `None` only the text will be displayed.
    pub thumbnail_resource: Option<Hash>,
    /// Details about the map
    pub details: MapVoteDetails,
    /// Usually this should be `true`, except if a map
    /// is used that the default client cannot parse,
    /// which will then disallow any kind of previewing features.
    pub is_default_map: bool,
}

/// Information to identify a misc vote send to a server.
#[derive(Debug, Hiarc, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize)]
pub struct MiscVoteKey {
    /// How the vote is displayed in the vote menu
    pub display_name: NetworkString<MAX_MISC_NAME_LEN>,
}

/// Information to identify a misc vote send to a server.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct MiscVoteCategoryKey {
    /// A category in which this misc vote lies.
    pub category: NetworkString<MAX_CATEGORY_NAME_LEN>,
    pub vote_key: MiscVoteKey,
}

/// Information about a misc vote.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct MiscVote {
    /// The rcon command that is executed
    pub command: NetworkString<{ 65536 * 2 + 1 }>,
}

pub const MAX_VOTE_REASON_LEN: usize = 32;
/// Information to identify a player vote send to a server.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct PlayerVoteKey {
    pub voted_player_id: PlayerId,
    pub reason: NetworkString<MAX_VOTE_REASON_LEN>,
}

/// Information to identify a random unfinished map vote send to a server.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct RandomUnfinishedMapKey {
    /// A category in which this vote was called.
    pub category: NetworkString<MAX_CATEGORY_NAME_LEN>,
    /// An optional difficulty for the random map
    pub difficulty: Option<MapDifficulty>,
}

/// Which kinds of votes are supported.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VoteType {
    /// A map is identified by its name + hash.
    Map {
        map: MapVote,
        key: MapCategoryVoteKey,
    },
    RandomUnfinishedMap {
        key: RandomUnfinishedMapKey,
    },
    VoteKickPlayer {
        key: PlayerVoteKey,
        /// player name
        name: NetworkString<MAX_CHARACTER_NAME_LEN>,
        skin: NetworkResourceKey<MAX_ASSET_NAME_LEN>,
        skin_info: NetworkSkinInfo,
    },
    VoteSpecPlayer {
        key: PlayerVoteKey,
        /// player name
        name: NetworkString<MAX_CHARACTER_NAME_LEN>,
        skin: NetworkResourceKey<MAX_ASSET_NAME_LEN>,
        skin_info: NetworkSkinInfo,
    },
    /// Misc votes are identifies by the display name
    Misc {
        key: MiscVoteCategoryKey,
        vote: MiscVote,
    },
}

/// The vote kinds, but only the parts required to identify the vote.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VoteIdentifierType {
    /// A map is identified by its name + hash.
    Map(MapCategoryVoteKey),
    /// If the server supports it this will try to start
    /// a vote that will load a random unfinished map
    /// of the player (usually only makes sense for race mods)
    RandomUnfinishedMap(RandomUnfinishedMapKey),
    VoteKickPlayer(PlayerVoteKey),
    VoteSpecPlayer(PlayerVoteKey),
    /// Misc votes are identifies by the display name
    Misc(MiscVoteCategoryKey),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Voted {
    Yes,
    No,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteState {
    pub vote: VoteType,
    /// The remaining time of the vote
    /// on the server.
    /// Since there isn't any prediction, this
    /// should simply be subtracted with the avg ping.
    pub remaining_time: Duration,

    pub yes_votes: u64,
    pub no_votes: u64,
    /// Number of clients that are allowed to participate in this vote.
    pub allowed_to_vote_count: u64,
}
