#![allow(clippy::too_many_arguments)]

#[cfg(feature = "recorder")]
pub mod recorder;

pub mod utils;

use std::{collections::BTreeMap, time::Duration};

use base::{
    hash::Hash,
    network_string::{NetworkReducedAsciiString, NetworkString},
};
use game_base::{
    network::{
        messages::{GameModification, RenderModification, RequiredResources},
        types::chat::NetChatMsg,
    },
    types::ClientLocalInfos,
};
use game_interface::{
    events::GameEvents,
    interface::{GameStateCreateOptions, MAX_MAP_NAME_LEN, MAX_PHYSICS_GROUP_NAME_LEN},
    types::game::NonZeroGameTickType,
};
use serde::{Deserialize, Serialize};

pub type DemoGameModification = GameModification;
pub type DemoRenderModification = RenderModification;

/// The demo header, of const size.
/// A broken demo can be detected if [`DemoHeader::len`] or
/// [`DemoHeader::size_chunks`] is zero.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct DemoHeader {
    /// Length of the full demo
    pub len: Duration,
    /// Size to read for the whole [`DemoHeaderExt`] struct.
    pub size_ext: u64,
    /// Size to read for all chunks.
    pub size_chunks: u64,
}

/// The tail of the demo is written last,
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DemoTail {
    /// the key is the monotonic tick, while the value is the
    /// file offset relative to the beginning of the chunk.
    pub snapshots_index: BTreeMap<u64, u64>,
    /// the key is the monotonic tick, while the value is the
    /// file offset relative to the beginning of the chunk.
    pub events_index: BTreeMap<u64, u64>,
}

/// A more flexible header, that can contain dynamic sized elements.
/// Here header simply means, never changing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoHeaderExt {
    /// optional server name, address or whatever - can be left empty
    pub server: NetworkString<32>,
    pub physics_mod: DemoGameModification,
    pub render_mod: DemoRenderModification,
    /// resources the game **has** to load before
    /// the game/demo starts (e.g. because the game mod requires
    /// them for gameplay).
    pub required_resources: RequiredResources,
    /// Resources the client most likely sees first
    /// (e.g. as part of local player information).
    pub client_local_infos: ClientLocalInfos,
    pub map: NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
    pub map_hash: Hash,
    pub ticks_per_second: NonZeroGameTickType,
    pub game_options: GameStateCreateOptions,
    pub physics_group_name: NetworkReducedAsciiString<MAX_PHYSICS_GROUP_NAME_LEN>,
}

/// When a chunk of snapshots or events ([`DemoRecorderChunk`]) is serialized, this header
/// is written.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkHeader {
    pub monotonic_tick: u64,
    pub size: u64,
}

pub type DemoSnapshot = Vec<u8>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DemoEvent {
    Game(GameEvents),
    Chat(Box<NetChatMsg>),
    /// A demo marker that marks a specific time point.
    Marker,
}

pub type DemoEvents = Vec<DemoEvent>;
