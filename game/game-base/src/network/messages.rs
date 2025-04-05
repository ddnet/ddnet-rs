use std::collections::HashMap;

use base::{
    hash::Hash,
    network_string::{NetworkReducedAsciiString, NetworkString},
};
use game_interface::{
    interface::{GameStateServerOptions, MAX_MAP_NAME_LEN},
    types::{
        character_info::{NetworkCharacterInfo, MAX_ASSET_NAME_LEN},
        game::GameTickType,
        id_types::PlayerId,
    },
};
use math::math::vector::vec2;
use pool::mt_datatypes::{PoolFxLinkedHashMap, PoolVec};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::player_input::PlayerInput;

use super::types::chat::NetChatMsg;

pub const MAX_PHYSICS_MOD_NAME_LEN: usize = 32;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameModification {
    Native,
    Ddnet,
    Wasm {
        /// Name of the game mod to play
        name: NetworkReducedAsciiString<MAX_PHYSICS_MOD_NAME_LEN>,
        /// Since this variant can be downloaded over network,
        /// it must also add the hash to it.
        hash: Hash,
    },
}

pub const MAX_RENDER_MOD_NAME_LEN: usize = 32;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderModification {
    Native,
    /// Try to load this wasm module, falling back to
    /// [`RenderModification::Native`] on error.
    TryWasm {
        /// Name of the game mod to play
        name: NetworkReducedAsciiString<MAX_RENDER_MOD_NAME_LEN>,
        /// Since this variant potentially is downloaded over network,
        /// it must also add the hash to it.
        hash: Hash,
    },
    RequiresWasm {
        /// Name of the game mod to play
        name: NetworkReducedAsciiString<MAX_RENDER_MOD_NAME_LEN>,
        /// Since this variant potentially is downloaded over network,
        /// it must also add the hash to it.
        hash: Hash,
    },
}

/// Resource type used as key.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ResourceType {
    Custom(u64),
    Skin,
    Weapon,
    Hook,
    Entities,
    Freeze,
    Emoticons,
    Particles,
    Ninja,
    Game,
    Hud,
    Ctf,
}

/// Resource properties to load it from disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceProps {
    /// File name of the resource
    pub name: NetworkReducedAsciiString<MAX_ASSET_NAME_LEN>,
    /// The blake 3 hash
    pub hash: Hash,
}

pub type RequiredResources = HashMap<ResourceType, Vec<ResourceProps>>;

/// All information about the server
/// so that the client can prepare the game.
/// E.g. current map
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsgSvServerInfo {
    /// the map that is currently played on
    pub map: NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
    pub map_blake3_hash: Hash,
    /// The game mod to play, see the config variable to
    /// read about reserved names
    pub game_mod: GameModification,
    /// The render module that should be used for playing.
    pub render_mod: RenderModification,
    /// The serialized optional config for the mod.
    /// The mod must load this and deal with errors automatically.
    /// This is meant to be similar to [`Self::server_options`] just
    /// more flexable and inside the physics mod.
    pub mod_config: Option<Vec<u8>>,
    /// Options of the server the client should know about
    pub server_options: GameStateServerOptions,
    /// Optional resources that the client loads for the game to make sense.
    /// E.g. a bomb mod might want to make sure that a bomb skin exists.
    ///
    /// A container format like demos can use this information to make
    /// a packed structure that contains all important things to be loaded.
    ///
    /// A server should provide these resource itself.
    pub required_resources: RequiredResources,
    /// - if this is `Some`, it is the port to the fallback resource download server.
    /// - if this is `None`, either resources are downloaded from an official resource
    ///   server or from a resource server stored in the server
    ///   browser information of this server.
    ///
    /// If both cases don't exist, no resources are downloaded, the client might stop connecting.
    /// Note: this is intentionally only a port. If the server contains a resource server in their
    /// server browser info, the client makes sure that the said server relates to this server
    /// (e.g. by a domain + subdomain DNS resolve check)
    pub resource_server_fallback: Option<u16>,
    /// as soon as the client has finished loading it might want to render something to the screen
    /// the server can give a hint what the best camera position is for that
    pub hint_start_camera_pos: vec2,
    /// Whether this server supports spatial chat.
    pub spatial_chat: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MsgSvChatMsg {
    pub msg: NetChatMsg,
}

// # client -> server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsgClAddLocalPlayer {
    pub player_info: NetworkCharacterInfo,

    /// This id is purely for the client to identify the add reponse
    /// message.
    /// The server does not care about the id and just adds it in the response
    /// packet as is.
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsgClReady {
    /// The players the client wants to join at once.
    ///
    /// Only one player is guaranteed to join, the response
    /// packet will contain a list of players joined.
    pub players: Vec<MsgClAddLocalPlayer>,

    /// Optional rcon secret, that should be tried to auth
    /// for rcon access.
    pub rcon_secret: Option<[u8; 32]>,
}

#[derive(Debug, Error, Clone, Serialize, Deserialize)]
pub enum MsgClReadyResponseError {
    #[error("No players were added in the ready request")]
    NoPlayersJoined,
    #[error("The server already received a ready request from this client")]
    ClientAlreadyReady,
    /// Client cannot be made ready, since it's not
    /// a connecting client
    #[error("The client is not connecting to the game of the server")]
    ClientIsNotConnecting,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MsgClReadyResponse {
    Success {
        joined_ids: Vec<(u64, PlayerId)>,
    },
    PartialSuccess {
        joined_ids: Vec<(u64, PlayerId)>,
        non_joined_ids: Vec<u64>,
    },
    Error {
        err: MsgClReadyResponseError,
        non_joined_ids: Vec<u64>,
    },
}

#[derive(Debug, Error, Clone, Serialize, Deserialize)]
pub enum AddLocalPlayerResponseError {
    #[error("{0}")]
    Custom(NetworkString<1024>),
    #[error("The server reached the maximum amount of players.")]
    MaxPlayers,
    #[error("Reached max players per client.")]
    MaxPlayersPerClient,
    #[error("This client already used the given player id for another player.")]
    PlayerIdAlreadyUsedByClient,
    #[error("The client was not yet connected to the server and ready.")]
    ClientWasNotReady,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MsgSvAddLocalPlayerResponse {
    Success {
        /// This id is equal to [`MsgClAddLocalPlayer::id`].
        id: u64,
        /// The player id this local player got
        player_id: PlayerId,
    },
    Err {
        /// This id is equal to [`MsgClAddLocalPlayer::id`].
        id: u64,
        err: AddLocalPlayerResponseError,
    },
}

/// Input that can easily be de-/serialized in a chain, see [`MsgClInputPlayerChain`].
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Default)]
pub struct PlayerInputChainable {
    pub inp: PlayerInput,
    pub for_monotonic_tick: GameTickType,
}

/// The input chain can contain multiple inputs for multiple
/// monotonic ticks.
///
/// # Serialization
/// The first [`MsgClInputPlayer`] uses the player's
/// diff [`MsgClInputPlayer`] (which is the last ack'd input by
/// the server) or [`MsgClInputPlayer::default`] if None such exists.
/// All other inputs in the chain use the previous [`MsgClInputPlayer`].
/// So the second uses the first, the third the second etc.
#[derive(Debug, Serialize, Deserialize)]
pub struct MsgClInputPlayerChain {
    /// The chain of [`PlayerInputChainable`]s (plural)
    pub data: PoolVec<u8>,
    pub diff_id: Option<u64>,
    /// Use this input for this player as diff.
    pub as_diff: bool,
}

pub type MsgClInputs = PoolFxLinkedHashMap<PlayerId, MsgClInputPlayerChain>;

/// Acknowledgement from the client that a snapshot arrived.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MsgClSnapshotAck {
    pub snap_id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MsgClChatMsg {
    Global {
        msg: NetworkString<256>,
    },
    GameTeam {
        msg: NetworkString<256>,
    },
    Whisper {
        receiver_id: PlayerId,
        msg: NetworkString<256>,
    },
}

/// Load a list of votes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MsgClLoadVotes {
    Map {
        /// The blake3 hash as if the votes were serialized (e.g. as json)
        cached_votes: Option<Hash>,
    },
    Misc {
        /// The blake3 hash as if the votes were serialized (e.g. as json)
        cached_votes: Option<Hash>,
    },
}
