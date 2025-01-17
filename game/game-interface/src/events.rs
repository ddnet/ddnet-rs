use std::time::Duration;

use base::network_string::MtPoolNetworkString;
use bitflags::bitflags;
use hiarc::Hiarc;
use math::math::vector::vec2;
use pool::{
    datatypes::PoolFxLinkedHashSet,
    mt_datatypes::{PoolFxLinkedHashMap, PoolVec},
};
use serde::{Deserialize, Serialize};

use crate::{
    client_commands::MAX_TEAM_NAME_LEN,
    types::{
        character_info::{NetworkSkinInfo, MAX_ASSET_NAME_LEN, MAX_CHARACTER_NAME_LEN},
        flag::FlagType,
        id_gen::{IdGenerator, IdGeneratorIdType},
        id_types::{CharacterId, PlayerId, StageId},
        player_info::PlayerDropReason,
        resource_key::MtPoolNetworkResourceKey,
        weapons::WeaponType,
    },
};

/// The id of an event
#[derive(
    Debug, Hiarc, Serialize, Deserialize, PartialEq, Eq, Copy, Clone, Hash, PartialOrd, Ord,
)]
pub struct EventId(IdGeneratorIdType);

impl From<IdGeneratorIdType> for EventId {
    fn from(value: IdGeneratorIdType) -> Self {
        Self(value)
    }
}

pub type EventIdGenerator = IdGenerator;

/// Sounds that a ninja spawns
#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameBuffNinjaEventSound {
    /// a pickup spawned
    Spawn,
    /// a pickup was collected by a character
    Collect,
    /// user used attack
    Attack,
    /// hits an object/character
    Hit,
}

/// Effects that a ninja spawns
#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameBuffNinjaEventEffect {}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameBuffSoundEvent {
    Ninja(GameBuffNinjaEventSound),
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameBuffEffectEvent {
    Ninja(GameBuffNinjaEventEffect),
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameDebuffFrozenEventSound {
    /// user (tried to) used attack
    Attack,
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameDebuffFrozenEventEffect {}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameDebuffSoundEvent {
    Frozen(GameDebuffFrozenEventSound),
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameDebuffEffectEvent {
    Frozen(GameDebuffFrozenEventEffect),
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameCharacterEventSound {
    WeaponSwitch {
        new_weapon: WeaponType,
    },
    NoAmmo {
        weapon: WeaponType,
    },
    HammerFire,
    GunFire,
    GrenadeFire,
    LaserFire,
    ShotgunFire,
    GroundJump,
    AirJump,
    HookHitPlayer {
        /// Where the hook was when it hit the player.
        hook_pos: Option<vec2>,
    },
    HookHitHookable {
        /// Where the hook was when it hit the player.
        hook_pos: Option<vec2>,
    },
    HookHitUnhookable {
        /// Where the hook was when it hit the player.
        hook_pos: Option<vec2>,
    },
    Spawn,
    Death,
    Pain {
        long: bool,
    },
    Hit {
        strong: bool,
    },
    HammerHit,
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameCharacterEventEffect {
    Spawn,
    Death,
    AirJump,
    DamageIndicator { vel: vec2 },
    HammerHit,
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameCharacterSoundEvent {
    Sound(GameCharacterEventSound),
    Buff(GameBuffSoundEvent),
    Debuff(GameDebuffSoundEvent),
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameCharacterEffectEvent {
    Effect(GameCharacterEventEffect),
    Buff(GameBuffEffectEvent),
    Debuff(GameDebuffEffectEvent),
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameGrenadeEventSound {
    /// pickup spawned
    Spawn,
    /// a pickup was collected by a character
    Collect,
    Explosion,
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameGrenadeEventEffect {
    Explosion,
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameLaserEventSound {
    /// pickup spawned
    Spawn,
    /// a pickup was collected by a character
    Collect,
    Bounce,
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameLaserEventEffect {}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameShotgunEventSound {
    /// pickup spawned
    Spawn,
    /// a pickup was collected by a character
    Collect,
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameShotgunEventEffect {}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameFlagEventCollectTy {
    Friendly,
    Opponent,
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameFlagEventSound {
    /// a flag was collected by a character
    Collect(FlagType),
    /// flag was captured
    Capture,
    /// flag was dropped
    Drop,
    /// flag returned to spawn point
    Return,
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameFlagEventEffect {}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GamePickupHeartEventSound {
    Spawn,
    /// a pickup was collected by a character
    Collect,
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GamePickupHeartEventEffect {}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GamePickupArmorEventSound {
    Spawn,
    /// a pickup was collected by a character
    Collect,
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GamePickupArmorEventEffect {}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GamePickupSoundEvent {
    Heart(GamePickupHeartEventSound),
    Armor(GamePickupArmorEventSound),
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GamePickupEffectEvent {
    Heart(GamePickupHeartEventEffect),
    Armor(GamePickupArmorEventEffect),
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameWorldEntitySoundEvent {
    Character(GameCharacterSoundEvent),
    Grenade(GameGrenadeEventSound),
    Laser(GameLaserEventSound),
    Shotgun(GameShotgunEventSound),
    Flag(GameFlagEventSound),
    Pickup(GamePickupSoundEvent),
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameWorldEntityEffectEvent {
    Character(GameCharacterEffectEvent),
    Grenade(GameGrenadeEventEffect),
    Laser(GameLaserEventEffect),
    Shotgun(GameShotgunEventEffect),
    Flag(GameFlagEventEffect),
    Pickup(GamePickupEffectEvent),
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub struct GameWorldSoundEvent {
    /// 1 tile = 1 integer unit
    ///
    /// A value of `None` means that the sound will be
    /// played globally.
    pub pos: Option<vec2>,
    /// A value of `None` here means that
    /// the event is a "global"/world event.
    /// An owner is a character.
    /// If the owner is `Some`, then
    /// the client side prediction will
    /// predict this event.
    /// Additionally character related
    /// assets are used.
    pub owner_id: Option<CharacterId>,
    pub ev: GameWorldEntitySoundEvent,
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub struct GameWorldEffectEvent {
    /// 1 tile = 1 integer unit
    pub pos: vec2,
    /// A value of `None` here means that
    /// the event is a "global"/world event.
    /// An owner is a character.
    /// If the owner is `Some`, then
    /// the client side prediction will
    /// predict this event.
    /// Additionally character related
    /// assets are used.
    pub owner_id: Option<CharacterId>,
    pub ev: GameWorldEntityEffectEvent,
}

/// Messages produced by the system.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum GameWorldSystemMessage {
    Custom(MtPoolNetworkString<1024>),
    /// A player joined the game.
    PlayerJoined {
        id: PlayerId,
        name: MtPoolNetworkString<MAX_CHARACTER_NAME_LEN>,
        skin: MtPoolNetworkResourceKey<MAX_ASSET_NAME_LEN>,
        skin_info: NetworkSkinInfo,
    },
    /// A player left the game.
    PlayerLeft {
        id: PlayerId,
        name: MtPoolNetworkString<MAX_CHARACTER_NAME_LEN>,
        skin: MtPoolNetworkResourceKey<MAX_ASSET_NAME_LEN>,
        skin_info: NetworkSkinInfo,
        reason: PlayerDropReason,
    },
    /// A character changed it's info.
    CharacterInfoChanged {
        id: PlayerId,
        old_name: MtPoolNetworkString<MAX_CHARACTER_NAME_LEN>,
        old_skin: MtPoolNetworkResourceKey<MAX_ASSET_NAME_LEN>,
        old_skin_info: NetworkSkinInfo,
        new_name: MtPoolNetworkString<MAX_CHARACTER_NAME_LEN>,
        new_skin: MtPoolNetworkResourceKey<MAX_ASSET_NAME_LEN>,
        new_skin_info: NetworkSkinInfo,
    },
}

#[derive(
    Debug, Default, Hiarc, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct KillFlags(u32);
bitflags! {
    impl KillFlags: u32 {
        /// killed by a wallshot, usually only interesting for laser
        const WALLSHOT = (1 << 0);
        /// the killer is dominating over the victims
        const DOMINATING = (1 << 1);
    }
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GameWorldActionKillWeapon {
    Weapon {
        weapon: WeaponType,
    },
    Ninja,
    /// Kill tiles or world border
    World,
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum GameWorldAction {
    Custom(MtPoolNetworkString<1024>),
    Kill {
        killer: Option<CharacterId>,
        /// assists to the killer
        assists: PoolVec<CharacterId>,
        victims: PoolVec<CharacterId>,
        weapon: GameWorldActionKillWeapon,
        flags: KillFlags,
    },
    /// An event indicating that a player
    /// finished a race.
    ///
    /// Note that this event is also used for
    /// demo/ghost recording detection
    /// and thus should always come before a
    /// potential kill message of that player
    RaceFinish {
        character: CharacterId,
        finish_time: Duration,
    },
    /// An event indicating that team of players
    /// finished a race.
    ///
    /// Please also read [`GameWorldAction::RaceFinish`]
    /// for more information.
    RaceTeamFinish {
        characters: PoolVec<CharacterId>,
        team_name: MtPoolNetworkString<MAX_TEAM_NAME_LEN>,
        finish_time: Duration,
    },
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum GameWorldNotificationEvent {
    /// A system message
    System(GameWorldSystemMessage),
    /// An action that is displayed in an action feed, kill message or finish time etc.
    Action(GameWorldAction),
    /// Message of the day
    Motd { msg: MtPoolNetworkString<1024> },
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum GameWorldEvent {
    Sound(GameWorldSoundEvent),
    Effect(GameWorldEffectEvent),
    Notification(GameWorldNotificationEvent),
}

/// # ID (Event-ID)
/// All events have an ID, this ID is always unique across all worlds on the server for every single event.
/// The client tries to match an event by its ID, the client might reset the id generator tho, if
/// the server is out of sync.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct GameWorldEvents {
    pub events: PoolFxLinkedHashMap<EventId, GameWorldEvent>,
}

pub type GameWorldsEvents = PoolFxLinkedHashMap<StageId, GameWorldEvents>;

/// A collection of events that are interpretable by the client.
/// These events are automatically synchronized by the server with all clients.
///
/// # Important
/// Read the ID section of [`GameWorldEvents`]
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct GameEvents {
    pub worlds: GameWorldsEvents,

    /// the next id that would be peeked by
    /// the [`EventIdGenerator::peek_next_id`] function
    /// used to sync client & server ids
    pub event_id: IdGeneratorIdType,
}

impl GameEvents {
    pub fn is_empty(&self) -> bool {
        self.worlds.is_empty()
    }
}

/// When the server (or client) requests events it usually requests it for
/// certain players (from the view of these players).
/// Additionally it might want to opt-in into getting every event etc.
///
/// Generally the implementation is free to ignore any of these. This might
/// lead to inconsitencies in the user experience tho (see also [`crate::types::snapshot::SnapshotClientInfo`])
#[derive(Debug, Hiarc, Serialize, Deserialize)]
pub struct EventClientInfo {
    /// A list of players the client requests the snapshot for.
    /// Usually these are the local players (including the dummy).
    pub client_player_ids: PoolFxLinkedHashSet<PlayerId>,
    /// A hint that everything should be snapped, regardless of the requested players
    pub everything: bool,
    /// A hint that all stages (a.k.a. ddrace teams) should be snapped
    /// (the client usually renders them with some transparency)
    pub other_stages: bool,
}
