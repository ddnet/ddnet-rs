use std::{num::NonZeroU32, time::Duration};

use base::network_string::PoolNetworkString;
use hiarc::Hiarc;
use math::math::vector::{dvec2, ubvec4, vec2};
use pool::{
    datatypes::{PoolFxHashSet, PoolFxLinkedHashMap, PoolFxLinkedHashSet},
    rc::PoolRc,
};
use serde::{Deserialize, Serialize};
pub use strum::{EnumCount, EnumIter, IntoEnumIterator};

use crate::{
    account_info::MAX_ACCOUNT_NAME_LEN,
    types::{
        character_info::{NetworkCharacterInfo, NetworkLaserInfo, NetworkSkinInfo},
        emoticons::EmoticonType,
        game::GameTickType,
        id_types::{CharacterId, StageId},
        weapons::WeaponType,
    },
};

use super::game::game_match::MatchSide;

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub enum CharacterBuff {
    /// the character has a ninja powerup (vanilla like ninja)
    Ninja,
    /// the character is in a ghost state
    /// for ddrace this is basically the /spec mode
    /// no hook or weapon is rendered
    Ghost,
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub struct CharacterBuffInfo {
    /// the remaining time, or `None` if unknown
    pub remaining_time: Option<Duration>,
    /// total time this buff exists
    pub total_time: Duration,
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub enum CharacterDebuff {
    /// character is freezed (e.g. ddrace freeze)
    Freeze,
}

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub struct CharacterDebuffInfo {
    /// the remaining time, or `None` if unknown
    pub remaining_time: Option<Duration>,
    /// total time this debuff exists
    pub total_time: Duration,
}

#[derive(
    Debug,
    Hiarc,
    Default,
    EnumIter,
    EnumCount,
    Copy,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
)]
pub enum TeeEye {
    #[default]
    Normal = 0,
    Pain,
    Happy,
    // TODO: needed? Dead,
    Surprised,
    Angry,
    Blink,
}

/// Information about the hook collision
#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum HookCollisionLineColor {
    /// RGBA color
    Custom(ubvec4),
    /// Hook would hit nothing
    Nothing,
    /// Hook would hit a player
    Player,
    /// Hook would hit hookable
    Hookable,
    /// Hook would hit unhookable
    Unhookable,
}

/// Information about the hook collision
#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub struct HookCollisionLine {
    /// Start position of the hook collision line
    pub start: vec2,
    /// End position of the hook collision line
    pub end: vec2,
    /// color of the hook collision line
    pub color: HookCollisionLineColor,
}

/// The ingame metric is 1 tile = 1.0 float units
#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub struct CharacterHookRenderInfo {
    pub pos: vec2,
    pub hooked_char: Option<CharacterId>,
}

/// The ingame metric is 1 tile = 1.0 float units
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct CharacterRenderInfo {
    pub lerped_pos: vec2,
    pub lerped_vel: vec2,
    /// A value of `None` here means that the hook will not be rendered
    pub lerped_hook: Option<CharacterHookRenderInfo>,
    pub hook_collision: Option<HookCollisionLine>,
    pub has_air_jump: bool,
    /// Smoothed cursor pos of the tee
    pub lerped_cursor_pos: dvec2,
    /// Smoothed dynamic camera offset
    pub lerped_dyn_cam_offset: dvec2,
    pub move_dir: i32,
    pub cur_weapon: WeaponType,
    /// How many ticks passed since the last attack recoil
    /// or `None` if the character never attacked yet
    pub recoil_ticks_passed: Option<GameTickType>,

    pub left_eye: TeeEye,
    pub right_eye: TeeEye,

    pub buffs: PoolFxLinkedHashMap<CharacterBuff, CharacterBuffInfo>,
    pub debuffs: PoolFxLinkedHashMap<CharacterDebuff, CharacterDebuffInfo>,

    /// How many animation ticks have passed for this character.
    /// This is used for synchronized map animations.
    /// If unsure which value to set this to, simply set it to the
    /// same value as `game_ticks_passed`, which is the common use case.
    pub animation_ticks_passed: GameTickType,
    /// How many game ticks have passed for this character.
    /// This is the race time, or ticks in an active round.
    pub game_ticks_passed: GameTickType,

    /// emoticon ticks passed & emoticon type
    pub emoticon: Option<(GameTickType, EmoticonType)>,

    /// Whether the player is inside a state where
    /// interaction with that character is impossible.
    ///
    /// In ddrace this are the solo parts.
    #[doc(alias = "solo")]
    pub phased: bool,
}

/// The camera mode of the local player
#[derive(Debug, Default, Clone, Hiarc, Serialize, Deserialize)]
pub enum PlayerCameraMode {
    /// Follows the own character
    #[default]
    Default,
    /// Free camera, the user can look around in the map
    /// as wanted.
    Free,
    /// The camera is currently locked to a specific
    /// position in a map (e.g. a kill cam).
    LockedTo {
        pos: vec2,
        /// If the camera is considered ingame, e.g. kill cam,
        /// then this should be `true`.
        /// This prevents letting the client zooming out if a
        /// forced zoom level is set.
        locked_ingame: bool,
    },
    /// The camera follows another player.
    LockedOn {
        character_ids: PoolFxHashSet<CharacterId>,
        /// If the camera is considered ingame, e.g. the player is
        /// not a _free_ spectator, then this should be `true`.
        /// This prevents letting the client zooming out if a
        /// forced zoom level is set.
        locked_ingame: bool,
    },
}

/// Information about in which mode the player is considered ingame.
#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum PlayerIngameMode {
    /// The player is spectating the game.
    Spectator,
    /// The player is playing inside the game.
    InGame {
        /// If the character is in a stage that is not the default (in ddrace team 0).
        /// Then this variable should be true.
        in_custom_stage: bool,
    },
}

/// Information about the player of a character.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct CharacterPlayerInfo {
    /// What camera mode the player currently uses
    pub cam_mode: PlayerCameraMode,
    /// Force the scoreboard to be open, e.g. while the match is over
    /// or while the player is in dead cam.
    pub force_scoreboard_visible: bool,
    /// Which mode does the player have ingame
    pub ingame_mode: PlayerIngameMode,
}

pub const MAX_SCORE_STR_LEN: usize = 32;
/// General information about the character
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct CharacterInfo {
    pub info: PoolRc<NetworkCharacterInfo>,

    /// Since overloading the skin color is such a common thing (sided pvp),
    /// this skin info should be preferred over the one in [`CharacterInfo::info`]
    /// for rendering. The one in [`CharacterInfo::info`] can instead be the
    /// original requested one.
    pub skin_info: NetworkSkinInfo,

    /// Since overloading the laser color is such a common thing (sided pvp),
    /// this laser info should be preferred over the one in [`CharacterInfo::info`]
    /// for rendering. The one in [`CharacterInfo::info`] can instead be the
    /// original requested one.
    pub laser_info: NetworkLaserInfo,

    /// The stage in which the character currently is.
    ///
    /// Should also be filled for dead characters that will respawn.
    ///
    /// If `None` that implies the character is a spectator or otherwise
    /// not inside the game, which will disallow things like voting.
    pub stage_id: Option<StageId>,

    /// If the game uses red/blue vanilla teams, then
    /// this should be filled with the characters match side.
    pub side: Option<MatchSide>,

    /// Does a player own this character.
    /// `None` for server side dummies or similar.
    pub player_info: Option<CharacterPlayerInfo>,

    /// If this user has an account, this can be filled with the account
    /// name on this game server.
    ///
    /// Typically shown in the server browser or nameplate.
    pub account_name: Option<PoolNetworkString<MAX_ACCOUNT_NAME_LEN>>,

    /// The score that should be displayed in the server browser.
    /// Can e.g. also be a finish time.
    pub browser_score: PoolNetworkString<MAX_SCORE_STR_LEN>,
    /// Which Tee eyes to show in the browser (e.g. for afk Tees).
    pub browser_eye: TeeEye,
}

/// The local character info for vanilla based mods
#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub struct LocalCharacterVanilla {
    pub health: u32,
    pub armor: u32,

    /// A value of `None` means unlimited
    pub ammo_of_weapon: Option<u32>,
}

/// The local character info for ddrace based mods
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct LocalCharacterDdrace {
    pub jumps: u32,
    /// None == infinite jumps
    pub max_jumps: Option<NonZeroU32>,
    pub endless_hook: bool,
    pub can_hook_others: bool,
    pub jetpack: bool,
    pub deep_frozen: bool,
    pub live_frozen: bool,
    /// E.g. in practice mode you can't
    pub can_finish: bool,
    pub owned_weapons: PoolFxLinkedHashSet<WeaponType>,
    /// Cannot hammer/shoot self/others
    pub disabled_weapons: PoolFxLinkedHashSet<WeaponType>,
    pub tele_weapons: PoolFxLinkedHashSet<WeaponType>,
    pub solo: bool,
    /// Super
    pub invincible: bool,
    pub dummy_hammer: bool,
    pub dummy_copy: bool,
    /// DDrace team is locked
    pub stage_locked: bool,
    pub team0_mode: bool,
    /// Whether player <-> player collision is on or off
    pub can_collide: bool,
    /// The current checkpoint of the user
    pub checkpoint: Option<u8>,
}

/// Information about the local character
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum LocalCharacterRenderInfo {
    Vanilla(LocalCharacterVanilla),
    Ddrace(LocalCharacterDdrace),
    /// E.g. for spectators
    Unavailable,
}
