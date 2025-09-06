use std::{
    collections::{BTreeMap, HashMap},
    num::NonZeroU64,
    time::Duration,
};

use base::network_string::{NetworkReducedAsciiString, NetworkString};
use game_base::network::messages::{
    MsgClAddLocalPlayer, MsgClChatMsg, MsgClInputs, MsgClLoadVotes, MsgClReady, MsgClReadyResponse,
    MsgClSnapshotAck, MsgSvAddLocalPlayerResponse, MsgSvChatMsg, MsgSvServerInfo,
};
use game_interface::{
    account_info::{AccountInfo, MAX_ACCOUNT_NAME_LEN},
    client_commands::{ClientCameraMode, JoinStage},
    events::GameEvents,
    rcon_entries::RconEntries,
    types::{
        character_info::NetworkCharacterInfo,
        emoticons::EmoticonType,
        game::GameTickType,
        id_types::PlayerId,
        player_info::PlayerUniqueId,
        render::{character::TeeEye, game::game_match::MatchSide},
    },
    votes::{
        MAX_CATEGORY_NAME_LEN, MapVote, MapVoteKey, MiscVote, MiscVoteKey, VoteIdentifierType,
        VoteState, Voted,
    },
};
use pool::mt_datatypes::PoolCow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MsgSvInputAck {
    pub id: u64,
    /// Logic overhead here means that the server does not
    /// directly ack an input and how ever long it took
    /// for the input packet from arriving to ack'ing, that
    /// is the overhead time here.
    pub logic_overhead: Duration,
}

/// List of votes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MsgSvLoadVotes {
    Map {
        categories: BTreeMap<NetworkString<MAX_CATEGORY_NAME_LEN>, BTreeMap<MapVoteKey, MapVote>>,
        has_unfinished_map_votes: bool,
    },
    Misc {
        votes: BTreeMap<NetworkString<MAX_CATEGORY_NAME_LEN>, BTreeMap<MiscVoteKey, MiscVote>>,
    },
}

/// Type of votes to reset.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MsgSvResetVotes {
    Map,
    Misc,
}

/// Vote result of vote started by a client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MsgSvStartVoteResult {
    Success,
    AnotherVoteAlreadyActive,
    MapVoteDoesNotExist,
    CantVoteSelf,
    CantSameClient,
    CantSameNetwork,
    CantVoteAdminOrModerator,
    CantVoteFromOtherStage,
    PlayerDoesNotExist,
    TooFewPlayersToVote,
    MiscVoteDoesNotExist,
    CantVoteAsSpectator,
    RandomUnfinishedMapUnsupported,
}

/// List of votes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsgSvSpatialChatOfEntitity {
    pub latest_opus_frames: BTreeMap<u64, Vec<Vec<u8>>>,
    pub player_unique_id: PlayerUniqueId,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerToClientMessage<'a> {
    Custom(PoolCow<'a, [u8]>),
    QueueInfo(NetworkString<1024>),
    /// Server requires a password before detailed server info is sent
    RequiresPassword,
    ServerInfo {
        info: MsgSvServerInfo,
        /// To make the first ping estimation better the server adds
        /// the overhead from network to when this msg is sent.
        overhead: Duration,
    },
    /// Reponse to a [`ClientToServerMessage::Ready`] packet
    ReadyResponse(MsgClReadyResponse),
    Snapshot {
        /// overhead time: (e.g. if the tick was calculated too late relative to the tick time) + the overhead from the simulation itself etc.
        overhead_time: Duration,
        snapshot: PoolCow<'a, [u8]>,
        /// diff_id: optional snapshot id to which to apply a binary diff against
        diff_id: Option<u64>,
        /// id of this snapshot
        /// if `diff_id` is `Some`, this value must be added to the diff id
        /// to get the real `snap_id`
        snap_id_diffed: u64,
        /// a strict monotonic tick that is used client side to
        /// make synchronization with the server easier
        /// (for example for sending inputs) and/or
        /// to know the difference between two snapshots, e.g.
        /// for demo replay.
        /// if `diff_id` is `Some`, this value must be added to the
        /// monotonic tick of the related diff
        /// to get the real `game_monotonic_tick`.
        game_monotonic_tick_diff: GameTickType,
        /// the client should _try_ to store this snap
        /// for snapshot differences.
        as_diff: bool,
        /// An input is ack'd by the server,
        /// Note that the server doesn't care if the input packet
        /// actually contained player inputs.
        input_ack: PoolCow<'a, [MsgSvInputAck]>,
    },
    Events {
        /// see Snapshot variant
        game_monotonic_tick: GameTickType,
        events: GameEvents,
    },
    // a load event, e.g. because of a map change
    Load(MsgSvServerInfo),
    Chat(MsgSvChatMsg),
    /// A value of `None` must be interpreted as no vote active.
    StartVoteRes(MsgSvStartVoteResult),
    Vote(Option<VoteState>),
    LoadVotes(MsgSvLoadVotes),
    ResetVotes(MsgSvResetVotes),
    RconEntries(RconEntries),
    RconExecResult {
        /// Since multiple commands or confi vars could have been executed/changed,
        /// this returns a list of strings.
        results: Vec<Result<NetworkString<65536>, NetworkString<65536>>>,
    },
    /// If `Ok` returns the new name.
    AccountRenameRes(Result<NetworkReducedAsciiString<32>, NetworkString<1024>>),
    AccountDetails(Result<AccountInfo, NetworkString<1024>>),
    SpatialChat {
        entities: HashMap<PlayerId, MsgSvSpatialChatOfEntitity>,
    },
    AddLocalPlayerResponse(MsgSvAddLocalPlayerResponse),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientToServerPlayerMessage<'a> {
    Custom(PoolCow<'a, [u8]>),
    RemLocalPlayer,
    Chat(MsgClChatMsg),
    Kill,
    JoinSpectator,
    SwitchToCamera(ClientCameraMode),
    StartVote(VoteIdentifierType),
    Voted(Voted),
    Emoticon(EmoticonType),
    ChangeEyes {
        eye: TeeEye,
        duration: Duration,
    },
    JoinStage(JoinStage),
    JoinVanillaSide(MatchSide),
    UpdateCharacterInfo {
        version: NonZeroU64,
        info: Box<NetworkCharacterInfo>,
    },
    RconExec {
        /// The raw ident text (with all modifiers as is)
        ident_text: NetworkString<65536>,
        /// The input args
        args: NetworkString<65536>,
    },
}

#[derive(Serialize, Deserialize)]
pub enum ClientToServerMessage<'a> {
    Custom(PoolCow<'a, [u8]>),
    PasswordResponse(NetworkString<1024>),
    Ready(MsgClReady),
    AddLocalPlayer(Box<MsgClAddLocalPlayer>),
    PlayerMsg((PlayerId, ClientToServerPlayerMessage<'a>)),
    Inputs {
        /// unique id that identifies this packet (for acks)
        id: u64,
        inputs: MsgClInputs,
        snap_ack: PoolCow<'a, [MsgClSnapshotAck]>,
    },
    LoadVotes(MsgClLoadVotes),
    AccountChangeName {
        new_name: NetworkReducedAsciiString<MAX_ACCOUNT_NAME_LEN>,
    },
    AccountRequestInfo,
    SpatialChat {
        /// One or more opus encoded frames
        opus_frames: Vec<Vec<u8>>,
        /// Ever increasing monotonic id
        id: u64,
    },
    /// Notify the server that the clients wants no
    /// more spatial chat packets.
    SpatialChatDeactivated,
}
