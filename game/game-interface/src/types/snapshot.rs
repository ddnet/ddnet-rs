use hiarc::Hiarc;
use pool::datatypes::{PoolFxLinkedHashMap, PoolFxLinkedHashSet};
use serde::{Deserialize, Serialize};

use super::id_types::PlayerId;

/// When the server (or client) requests a snapshot it usually requests it for
/// certain players (from the view of these players).
///
/// Additionally it might want to opt-in into snapping everything etc.
/// For server-side demos, it's possible that no player is requested.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum SnapshotClientInfo {
    /// A list of players the client requests the snapshot for.
    /// Usually these are the local players (including the dummy).
    ForPlayerIds(PoolFxLinkedHashSet<PlayerId>),
    /// All stages (a.k.a. ddrace teams) should be snapped
    /// (the client usually renders them with some transparency)
    OtherStagesForPlayerIds(PoolFxLinkedHashSet<PlayerId>),
    /// Everything should be snapped
    Everything,
}

/// Information about the local players from the opaque snapshot
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct SnapshotLocalPlayer {
    /// The _unique_ id given by the client to reidentify the local player.
    pub id: u64,
}

/// A parsed snapshot must return this information, which is usually parsed by the client
pub type SnapshotLocalPlayers = PoolFxLinkedHashMap<PlayerId, SnapshotLocalPlayer>;
