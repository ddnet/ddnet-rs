use std::{fmt::Debug, mem::size_of};

use graphics_types::types::GraphicsBackendMemory;
use hiarc::Hiarc;
use num_derive::FromPrimitive;

use math::math::vector::{ivec4, vec2_base};
use serde::{Deserialize, Serialize};
pub trait ReadFromSliceWriteToVec {
    fn read_from_slice(data: &[u8]) -> Self;
    fn write_to_vec(&self, w: &mut Vec<u8>);
}

// layer types
#[derive(PartialEq, PartialOrd)]
pub enum MapLayerTypes {
    Invalid = 0,
    Game,
    Tiles,
    Quads,
    Front,
    Tele,
    Speedup,
    Switch,
    Tune,
    SoundsDeprecated, // deprecated! do not use this, this is just for compatibility reasons
    Sounds,
}

#[derive(FromPrimitive)]
pub enum MapItemTypes {
    // TODO(Shereef Marzouk): fix this for vanilla, make use of LAYERTYPE_GAME instead of using m_game variable in the editor.
    Version = 0,
    Info,
    Image,
    Envelope,
    Group,
    Layer,
    Envpoints,
    Sound,
    // High map item type numbers suggest that they use the alternate
    // format with UUIDs. See src/engine/shared/datafile.cpp for some of
    // the implementation.
    Count,
}

pub enum CurveType {
    Step = 0,
    Linear,
    Slow,
    Fast,
    Smooth,

    Count,
}

#[repr(u8)]
pub enum EEntityTiles {
    // game layer tiles
    // TODO define which Layer uses which tiles (needed for mapeditor)
    Spawn = 192,
    SpawnRed,
    SpawnBlue,
    FlagSpawnRed,
    FlagSpawnBlue,
    Armor,
    Health,
    WeaponShotgun,
    WeaponGrenade,
    PowerupNinja,
    WeaponLaser,
}
/*
//DDRace - Main Lasers
ENTITY_LASER_FAST_CCW,
ENTITY_LASER_NORMAL_CCW,
ENTITY_LASER_SLOW_CCW,
ENTITY_LASER_STOP,
ENTITY_LASER_SLOW_CW,
ENTITY_LASER_NORMAL_CW,
ENTITY_LASER_FAST_CW,
//DDRace - Laser Modifiers
ENTITY_LASER_SHORT,
ENTITY_LASER_MEDIUM,
ENTITY_LASER_LONG,
ENTITY_LASER_C_SLOW,
ENTITY_LASER_C_NORMAL,
ENTITY_LASER_C_FAST,
ENTITY_LASER_O_SLOW,
ENTITY_LASER_O_NORMAL,
ENTITY_LASER_O_FAST,
//DDRace - Plasma
ENTITY_PLASMAE = 29,
ENTITY_PLASMAF,
ENTITY_PLASMA,
ENTITY_PLASMAU,
//DDRace - Shotgun
ENTITY_CRAZY_SHOTGUN_EX,
ENTITY_CRAZY_SHOTGUN,
//DDNet - Removing specific weapon
ENTITY_ARMOR_SHOTGUN,
ENTITY_ARMOR_GRENADE,
ENTITY_ARMOR_NINJA,
ENTITY_ARMOR_LASER,
//DDRace - Draggers
ENTITY_DRAGGER_WEAK = 42,
ENTITY_DRAGGER_NORMAL,
ENTITY_DRAGGER_STRONG,
//Draggers Behind Walls
ENTITY_DRAGGER_WEAK_NW,
ENTITY_DRAGGER_NORMAL_NW,
ENTITY_DRAGGER_STRONG_NW,
//Doors
ENTITY_DOOR = 49,
//End Of Lower Tiles
NUM_ENTITIES,
//Start From Top Left
//Tile Controllers*/

pub fn read_u32_le(data: &[u8]) -> u32 {
    u32::from_le_bytes([data[0], data[1], data[2], data[3]])
}

pub fn read_i32_le(data: &[u8]) -> i32 {
    i32::from_le_bytes([data[0], data[1], data[2], data[3]])
}

pub fn read_i16_le(data: &[u8]) -> i16 {
    i16::from_le_bytes([data[0], data[1]])
}

#[derive(Debug, Hiarc)]
#[repr(C)]
pub struct CMapItemVersion {
    pub version: i32,
}

impl CMapItemVersion {
    pub fn read_from_slice(data: &[u8]) -> Self {
        let v = read_i32_le(data);

        Self { version: v }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.version.to_le_bytes());
    }
}

#[derive(Debug, Hiarc)]
#[repr(C)]
pub struct CMapItemInfo {
    pub version: i32,
    pub author: i32,
    pub map_version: i32,
    pub credits: i32,
    pub license: i32,
}

impl CMapItemInfo {
    pub fn read_from_slice(data: &[u8]) -> Self {
        let (ver, rest) = data.split_at(size_of::<i32>());
        let v = read_i32_le(ver);

        let (author, rest) = rest.split_at(size_of::<i32>());
        let author = read_i32_le(author);

        let (map_ver, rest) = rest.split_at(size_of::<i32>());
        let map_ver = read_i32_le(map_ver);

        let (credits, rest) = rest.split_at(size_of::<i32>());
        let credits = read_i32_le(credits);

        let (license, _rest) = rest.split_at(size_of::<i32>());
        let license = read_i32_le(license);

        Self {
            version: v,
            author,
            map_version: map_ver,
            credits,
            license,
        }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.version.to_le_bytes());
        w.extend(self.author.to_le_bytes());
        w.extend(self.map_version.to_le_bytes());
        w.extend(self.credits.to_le_bytes());
        w.extend(self.license.to_le_bytes());
    }
}

#[derive(Debug, Hiarc)]
#[repr(C)]
pub struct CMapItemInfoSettings {
    pub info: CMapItemInfo,
    pub settings: i32,
}

impl CMapItemInfoSettings {
    pub fn read_from_slice(data: &[u8]) -> Self {
        let (info, rest) = data.split_at(size_of::<CMapItemInfo>());
        let info = CMapItemInfo::read_from_slice(info);

        let settings = if rest.len() >= size_of::<i32>() {
            let (settings, _rest) = rest.split_at(size_of::<i32>());
            read_i32_le(settings)
        } else {
            -1
        };

        Self { info, settings }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        self.info.write_to_vec(w);
        w.extend(self.settings.to_le_bytes());
    }
}

#[derive(Debug, Hiarc, Copy, Clone, Default)]
#[repr(C)]
pub struct CMapItemImage {
    pub version: i32,
    pub width: i32,
    pub height: i32,
    pub external: i32,
    pub image_name: i32,
    pub image_data: i32,
}

impl CMapItemImage {
    pub fn read_from_slice(data: &[u8]) -> Self {
        let (version, rest) = data.split_at(size_of::<i32>());
        let version = read_i32_le(version);

        let (width, rest) = rest.split_at(size_of::<i32>());
        let width = read_i32_le(width);

        let (height, rest) = rest.split_at(size_of::<i32>());
        let height = read_i32_le(height);

        let (external, rest) = rest.split_at(size_of::<i32>());
        let external = read_i32_le(external);

        let (image_name, rest) = rest.split_at(size_of::<i32>());
        let image_name = read_i32_le(image_name);

        let (image_data, _rest) = rest.split_at(size_of::<i32>());
        let image_data = read_i32_le(image_data);

        Self {
            version,
            width,
            height,
            external,
            image_name,
            image_data,
        }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.version.to_le_bytes());
        w.extend(self.width.to_le_bytes());
        w.extend(self.height.to_le_bytes());
        w.extend(self.external.to_le_bytes());
        w.extend(self.image_name.to_le_bytes());
        w.extend(self.image_data.to_le_bytes());
    }
}

#[derive(Debug, Hiarc, Default)]
pub struct MapImage {
    pub item_data: CMapItemImage,
    pub img_data: Option<GraphicsBackendMemory>,
    pub img_used: bool,
    pub img_3d_data: Option<GraphicsBackendMemory>,
    pub img_3d_used: bool,
    pub img_3d_width: usize,
    pub img_3d_height: usize,
    pub img_3d_depth: usize,
    pub img_name: String,
    pub internal_img: Option<Vec<u8>>,
}

#[derive(Debug, Hiarc)]
pub struct MapInfo {
    pub def: CMapItemInfoSettings,

    pub author: String,
    pub map_version: String,
    pub credits: String,
    pub license: String,
    pub settings: Vec<String>,
}

pub const TILE_SWITCHTIMEDOPEN: u8 = 22;

#[derive(Debug, Hiarc, FromPrimitive, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DdraceTileNum {
    Air = 0,
    Solid,
    Death,
    NoHook,
    NoLaser,
    ThroughCut,
    Through,
    Jump,
    Freeze = 9,
    TeleInEvil,
    Unfreeze,
    DFreeze,
    DUnfreeze,
    TeleInWeapon,
    TeleInHook,
    WallJump = 16,
    EHookEnable,
    EHookDisable,
    HitEnable,
    HitDisable,
    SoloEnable,
    SoloDisable,
    // Switches
    SwitchTimedClose,
    SwitchOpen,
    SwitchClose,
    TeleIn,
    TeleOut,
    Boost,
    TeleCheck,
    TeleCheckOut,
    TeleCheckIn,
    RefillJumps = 32,
    Start,
    Finish,
    TimeCheckpointFirst = 35,
    TimeCheckpointLast = 59,
    Stop = 60,
    StopS,
    StopA,
    TeleCheckInEvil,
    CP,
    CPF,
    ThroughAll,
    ThroughDir,
    Tune,
    OldLaser = 71,
    Npc,
    EHook,
    NoHit,
    NPH,
    UnlockTeam,
    AddTime = 79,
    NpcDisable = 88,
    UnlimitedJumpsDisable,
    JetpackDisable,
    NphDisable,
    SubtractTime = 95,
    TeleGunEnable = 96,
    TeleGunDisable = 97,
    AllowTeleGun = 98,
    AllowBlueTeleGun = 99,
    NpcEnable = 104,
    UnlimitedJumpsEnable,
    JetpackEnable,
    NphEnable,
    TeleGrenadeEnable = 112,
    TeleGrenadeDisable = 113,
    TeleLaserEnable = 128,
    TeleLaserDisable = 129,
    Credits1 = 140,
    Credits2 = 141,
    Credits3 = 142,
    Credits4 = 143,
    LFreeze = 144,
    LUnfreeze = 145,
    Credits5 = 156,
    Credits6 = 157,
    Credits7 = 158,
    Credits8 = 159,
    EntitiesOff1 = 190,
    EntitiesOff2,
}

pub enum LayerFlag {
    Detail = 1,
}

pub enum TilesLayerFlag {
    Game = 1,
    Tele = 2,
    Speedup = 4,
    Front = 8,
    Switch = 16,
    Tune = 32,
}

pub enum ItemType {
    Ex = 0xffff,
}

pub enum CMapItemGroupExVer {
    CurVersion = 1,
}

pub enum CMapItemGroupVer {
    CurVersion = 3,
}

#[derive(Debug, Hiarc, Default, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct CMapItemGroup {
    pub version: i32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub parallax_x: i32,
    pub parallax_y: i32,

    pub start_layer: i32,
    pub num_layers: i32,

    pub use_clipping: i32,
    pub clip_x: i32,
    pub clip_y: i32,
    pub clip_w: i32,
    pub clip_h: i32,

    pub name: [i32; 3],
}

impl CMapItemGroup {
    pub fn size_of_without_name() -> usize {
        std::mem::size_of::<Self>() - std::mem::size_of::<i32>() * 3
    }

    pub fn read_from_slice(data: &[u8]) -> Self {
        let (version, rest) = data.split_at(size_of::<i32>());
        let version = read_i32_le(version);

        let (offset_x, rest) = rest.split_at(size_of::<i32>());
        let offset_x = read_i32_le(offset_x);

        let (offset_y, rest) = rest.split_at(size_of::<i32>());
        let offset_y = read_i32_le(offset_y);

        let (parallax_x, rest) = rest.split_at(size_of::<i32>());
        let parallax_x = read_i32_le(parallax_x);

        let (parallax_y, rest) = rest.split_at(size_of::<i32>());
        let parallax_y = read_i32_le(parallax_y);

        let (start_layer, rest) = rest.split_at(size_of::<i32>());
        let start_layer = read_i32_le(start_layer);

        let (num_layers, rest) = rest.split_at(size_of::<i32>());
        let num_layers = read_i32_le(num_layers);

        let (use_clipping, rest) = rest.split_at(size_of::<i32>());
        let use_clipping = read_i32_le(use_clipping);

        let (clip_x, rest) = rest.split_at(size_of::<i32>());
        let clip_x = read_i32_le(clip_x);

        let (clip_y, rest) = rest.split_at(size_of::<i32>());
        let clip_y = read_i32_le(clip_y);

        let (clip_w, rest) = rest.split_at(size_of::<i32>());
        let clip_w = read_i32_le(clip_w);

        let (clip_h, mut rest) = rest.split_at(size_of::<i32>());
        let clip_h = read_i32_le(clip_h);

        let mut iname: [i32; 3] = Default::default();
        if !rest.is_empty() {
            iname.iter_mut().for_each(|c| {
                let (name, rest2) = rest.split_at(size_of::<i32>());
                *c = read_i32_le(name);
                rest = rest2;
            });
        }

        Self {
            version,
            offset_x,
            offset_y,
            parallax_x,
            parallax_y,

            start_layer,
            num_layers,

            use_clipping,
            clip_x,
            clip_y,
            clip_w,
            clip_h,

            name: iname,
        }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.version.to_le_bytes());
        w.extend(self.offset_x.to_le_bytes());
        w.extend(self.offset_y.to_le_bytes());
        w.extend(self.parallax_x.to_le_bytes());
        w.extend(self.parallax_y.to_le_bytes());
        w.extend(self.start_layer.to_le_bytes());
        w.extend(self.num_layers.to_le_bytes());
        w.extend(self.use_clipping.to_le_bytes());
        w.extend(self.clip_x.to_le_bytes());
        w.extend(self.clip_y.to_le_bytes());
        w.extend(self.clip_w.to_le_bytes());
        w.extend(self.clip_h.to_le_bytes());
        w.extend(self.name.iter().flat_map(|v| v.to_le_bytes()));
    }
}

#[derive(Debug, Hiarc, Default, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct CMapItemLayer {
    pub version: i32,
    pub item_layer: i32,
    pub flags: i32,
}

impl CMapItemLayer {
    pub fn read_from_slice(data: &[u8]) -> Self {
        let (version, rest) = data.split_at(size_of::<i32>());
        let version = read_i32_le(version);

        let (itype, rest) = rest.split_at(size_of::<i32>());
        let itype = read_i32_le(itype);

        let (flags, _rest) = rest.split_at(size_of::<i32>());
        let flags = read_i32_le(flags);

        Self {
            version,
            item_layer: itype,
            flags,
        }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.version.to_le_bytes());
        w.extend(self.item_layer.to_le_bytes());
        w.extend(self.flags.to_le_bytes());
    }
}

type CColor = ivec4;

#[derive(Debug, Hiarc, Default, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct CMapItemLayerTilemap {
    pub layer: CMapItemLayer,
    pub version: i32,

    pub width: i32,
    pub height: i32,
    pub flags: i32,

    pub color: CColor,
    pub color_env: i32,
    pub color_env_offset: i32,

    pub image: i32,
    pub data: i32,

    pub name: [i32; 3],

    // DDRace
    pub tele: i32,
    pub speedup: i32,
    pub front: i32,
    pub switch: i32,
    pub tune: i32,
}

impl CMapItemLayerTilemap {
    pub fn size_of_without_ddrace() -> usize {
        std::mem::size_of::<Self>() - std::mem::size_of::<i32>() * 5
    }

    pub fn size_of_without_name() -> usize {
        Self::size_of_without_ddrace() - std::mem::size_of::<i32>() * 3
    }

    pub fn read_from_slice(data: &[u8]) -> Self {
        let (tile_layer, rest) = data.split_at(size_of::<CMapItemLayer>());
        let tile_layer = CMapItemLayer::read_from_slice(tile_layer);

        let (version, rest) = rest.split_at(size_of::<i32>());
        let version = read_i32_le(version);

        let (width, rest) = rest.split_at(size_of::<i32>());
        let width = read_i32_le(width);

        let (height, rest) = rest.split_at(size_of::<i32>());
        let height = read_i32_le(height);

        let (flags, rest) = rest.split_at(size_of::<i32>());
        let flags = read_i32_le(flags);

        let (color, rest) = rest.split_at(size_of::<CColor>());
        let color = CColor::read_from_slice(color);

        let (color_env, rest) = rest.split_at(size_of::<i32>());
        let color_env = read_i32_le(color_env);

        let (color_env_off, rest) = rest.split_at(size_of::<i32>());
        let color_env_off = read_i32_le(color_env_off);

        let (img, rest) = rest.split_at(size_of::<i32>());
        let img = read_i32_le(img);

        let (data, mut rest) = rest.split_at(size_of::<i32>());
        let data = read_i32_le(data);

        let mut name: [i32; 3] = Default::default();
        if !rest.is_empty() {
            name.iter_mut().for_each(|n| {
                let (c, rest2) = rest.split_at(size_of::<i32>());
                *n = read_i32_le(c);
                rest = rest2;
            });
        }

        let mut tele = -1;
        if !rest.is_empty() {
            let (tel, rest2) = rest.split_at(size_of::<i32>());
            tele = read_i32_le(tel);
            rest = rest2;
        }

        let mut speedup = -1;
        if !rest.is_empty() {
            let (val, rest2) = rest.split_at(size_of::<i32>());
            speedup = read_i32_le(val);
            rest = rest2;
        }

        let mut front = -1;
        if !rest.is_empty() {
            let (val, rest2) = rest.split_at(size_of::<i32>());
            front = read_i32_le(val);
            rest = rest2;
        }

        let mut switch = -1;
        if !rest.is_empty() {
            let (val, rest2) = rest.split_at(size_of::<i32>());
            switch = read_i32_le(val);
            rest = rest2;
        }

        let mut tune = -1;
        if !rest.is_empty() {
            let (val, _) = rest.split_at(size_of::<i32>());
            tune = read_i32_le(val);
        }

        Self {
            layer: tile_layer,
            version,

            width,
            height,
            flags,

            color,
            color_env,
            color_env_offset: color_env_off,

            image: img,
            data,

            name,

            // DDRace
            tele,
            speedup,
            front,
            switch,
            tune,
        }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        self.layer.write_to_vec(w);
        w.extend(self.version.to_le_bytes());
        w.extend(self.width.to_le_bytes());
        w.extend(self.height.to_le_bytes());
        w.extend(self.flags.to_le_bytes());
        self.color.write_to_vec(w);
        w.extend(self.color_env.to_le_bytes());
        w.extend(self.color_env_offset.to_le_bytes());
        w.extend(self.image.to_le_bytes());
        w.extend(self.data.to_le_bytes());
        w.extend(self.name.iter().flat_map(|v| v.to_le_bytes()));
        // DDRace
        w.extend(self.tele.to_le_bytes());
        w.extend(self.speedup.to_le_bytes());
        w.extend(self.front.to_le_bytes());
        w.extend(self.switch.to_le_bytes());
        w.extend(self.tune.to_le_bytes());
    }
}

#[derive(Debug, Hiarc, Default, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct CMapItemLayerQuads {
    pub layer: CMapItemLayer,
    pub version: i32,

    pub num_quads: i32,
    pub data: i32,
    pub image: i32,

    pub name: [i32; 3],
}

impl CMapItemLayerQuads {
    pub fn size_of_without_name() -> usize {
        std::mem::size_of::<Self>() - std::mem::size_of::<i32>() * 3
    }

    pub fn read_from_slice(data: &[u8]) -> Self {
        let (tile_layer, rest) = data.split_at(size_of::<CMapItemLayer>());
        let tile_layer = CMapItemLayer::read_from_slice(tile_layer);

        let (version, rest) = rest.split_at(size_of::<i32>());
        let version = read_i32_le(version);

        let (num_quads, rest) = rest.split_at(size_of::<i32>());
        let num_quads = read_i32_le(num_quads);

        let (data, rest) = rest.split_at(size_of::<i32>());
        let data = read_i32_le(data);

        let (img, mut rest) = rest.split_at(size_of::<i32>());
        let img = read_i32_le(img);

        let mut name: [i32; 3] = Default::default();
        if !rest.is_empty() {
            name.iter_mut().for_each(|n| {
                let (c, rest2) = rest.split_at(size_of::<i32>());
                *n = read_i32_le(c);
                rest = rest2;
            });
        }

        Self {
            layer: tile_layer,
            version,

            num_quads,

            data,
            image: img,

            name,
        }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        self.layer.write_to_vec(w);
        w.extend(self.version.to_le_bytes());
        w.extend(self.num_quads.to_le_bytes());
        w.extend(self.data.to_le_bytes());
        w.extend(self.image.to_le_bytes());
        w.extend(self.name.iter().flat_map(|v| v.to_le_bytes()));
    }
}

pub enum CMapItemLayerSoundsVer {
    CurVersion = 2,
}

#[derive(Debug, Hiarc, Default, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct CMapItemLayerSounds {
    pub layer: CMapItemLayer,
    pub version: i32,

    pub num_sources: i32,
    pub data: i32,
    pub sound: i32,

    pub name: [i32; 3],
}

impl CMapItemLayerSounds {
    pub fn read_from_slice(data: &[u8]) -> Self {
        let (tile_layer, rest) = data.split_at(size_of::<CMapItemLayer>());
        let tile_layer = CMapItemLayer::read_from_slice(tile_layer);

        let (version, rest) = rest.split_at(size_of::<i32>());
        let version = read_i32_le(version);

        let (num_sources, rest) = rest.split_at(size_of::<i32>());
        let num_sources = read_i32_le(num_sources);

        let (data, rest) = rest.split_at(size_of::<i32>());
        let data = read_i32_le(data);

        let (snd, mut rest) = rest.split_at(size_of::<i32>());
        let snd = read_i32_le(snd);

        let mut name: [i32; 3] = Default::default();
        if !rest.is_empty() {
            name.iter_mut().for_each(|n| {
                let (c, rest2) = rest.split_at(size_of::<i32>());
                *n = read_i32_le(c);
                rest = rest2;
            });
        }

        Self {
            layer: tile_layer,
            version,

            num_sources,

            data,
            sound: snd,

            name,
        }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        self.layer.write_to_vec(w);
        w.extend(self.version.to_le_bytes());
        w.extend(self.num_sources.to_le_bytes());
        w.extend(self.data.to_le_bytes());
        w.extend(self.sound.to_le_bytes());
        w.extend(self.name.iter().flat_map(|v| v.to_le_bytes()));
    }
}

impl CMapItemLayerSounds {
    pub fn size_of_without_name() -> usize {
        std::mem::size_of::<Self>() - std::mem::size_of::<i32>() * 3
    }
}

#[derive(Debug, Hiarc, Copy, Clone, Default, Serialize, Deserialize)]
#[repr(C)]
pub struct CTile {
    pub index: u8,
    pub flags: u8,
    pub skip: u8,
    pub reserved: u8,
}

impl ReadFromSliceWriteToVec for CTile {
    fn read_from_slice(data: &[u8]) -> Self {
        let (index, rest) = data.split_at(size_of::<u8>());
        let index = index[0];

        let (flags, rest) = rest.split_at(size_of::<u8>());
        let flags = flags[0];

        let (skip, rest) = rest.split_at(size_of::<u8>());
        let skip = skip[0];

        let (reserved, _rest) = rest.split_at(size_of::<u8>());
        let reserved = reserved[0];

        Self {
            index,
            flags,
            skip,
            reserved,
        }
    }

    fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.index.to_le_bytes());
        w.extend(self.flags.to_le_bytes());
        w.extend(self.skip.to_le_bytes());
        w.extend(self.reserved.to_le_bytes());
    }
}

// ddrace
#[derive(Debug, Hiarc, Default, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct CTeleTile {
    pub number: u8,
    pub tile_type: u8,
}

impl ReadFromSliceWriteToVec for CTeleTile {
    fn read_from_slice(data: &[u8]) -> Self {
        let (number, rest) = data.split_at(size_of::<u8>());
        let number = number[0];

        let (ttype, _rest) = rest.split_at(size_of::<u8>());
        let ttype = ttype[0];

        Self {
            number,
            tile_type: ttype,
        }
    }

    fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.number.to_le_bytes());
        w.extend(self.tile_type.to_le_bytes());
    }
}

#[derive(Debug, Hiarc, Default, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct CSpeedupTile {
    pub force: u8,
    pub max_speed: u8,
    pub tile_type: u8,
    pub angle: i16,
}

impl ReadFromSliceWriteToVec for CSpeedupTile {
    fn read_from_slice(data: &[u8]) -> Self {
        let (force, rest) = data.split_at(size_of::<u8>());
        let force = force[0];

        let (max_speed, rest) = rest.split_at(size_of::<u8>());
        let max_speed = max_speed[0];

        let (ttype, rest) = rest.split_at(size_of::<u8>());
        let ttype = ttype[0];

        // padding
        let (_, rest) = rest.split_at(size_of::<u8>());

        let (angle, _rest) = rest.split_at(size_of::<i16>());
        let angle = read_i16_le(angle);

        Self {
            force,
            max_speed,
            tile_type: ttype,
            angle,
        }
    }

    fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.force.to_le_bytes());
        w.extend(self.max_speed.to_le_bytes());
        w.extend(self.tile_type.to_le_bytes());
        // padding
        w.extend(0_u8.to_le_bytes());
        w.extend(self.angle.to_le_bytes());
    }
}

#[derive(Debug, Hiarc, Default, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct CSwitchTile {
    pub number: u8,
    pub tile_type: u8,
    pub flags: u8,
    pub delay: u8,
}

impl ReadFromSliceWriteToVec for CSwitchTile {
    fn read_from_slice(data: &[u8]) -> Self {
        let (number, rest) = data.split_at(size_of::<u8>());
        let number = number[0];

        let (ttype, rest) = rest.split_at(size_of::<u8>());
        let ttype = ttype[0];

        let (flags, rest) = rest.split_at(size_of::<u8>());
        let flags = flags[0];

        let (delay, _rest) = rest.split_at(size_of::<u8>());
        let delay = delay[0];

        Self {
            number,
            tile_type: ttype,
            flags,
            delay,
        }
    }

    fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.number.to_le_bytes());
        w.extend(self.tile_type.to_le_bytes());
        w.extend(self.flags.to_le_bytes());
        w.extend(self.delay.to_le_bytes());
    }
}

#[derive(Debug, Hiarc, Default, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct CDoorTile {
    pub index: u8,
    pub flags: u8,
    pub number: i32,
}

#[derive(Debug, Hiarc, Default, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct CTuneTile {
    pub number: u8,
    pub tile_type: u8,
}

impl ReadFromSliceWriteToVec for CTuneTile {
    fn read_from_slice(data: &[u8]) -> Self {
        let (number, rest) = data.split_at(size_of::<u8>());
        let number = number[0];

        let (ttype, _rest) = rest.split_at(size_of::<u8>());
        let ttype = ttype[0];

        Self {
            number,
            tile_type: ttype,
        }
    }

    fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.number.to_le_bytes());
        w.extend(self.tile_type.to_le_bytes());
    }
}

pub type CPoint = vec2_base<i32>;

impl ReadFromSliceWriteToVec for CPoint {
    fn read_from_slice(data: &[u8]) -> Self {
        let (x, rest) = data.split_at(size_of::<i32>());
        let x = read_i32_le(x);

        let (y, _rest) = rest.split_at(size_of::<i32>());
        let y = read_i32_le(y);

        Self { x, y }
    }

    fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.x.to_le_bytes());
        w.extend(self.y.to_le_bytes());
    }
}

#[derive(Debug, Hiarc, Default, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct CQuad {
    pub points: [CPoint; 5],
    pub colors: [CColor; 4],
    pub tex_coords: [CPoint; 4],

    pub pos_env: i32,
    pub pos_env_offset: i32,

    pub color_env: i32,
    pub color_env_offset: i32,
}

impl ReadFromSliceWriteToVec for CQuad {
    fn read_from_slice(data: &[u8]) -> Self {
        let mut rest = data;

        let mut points: [CPoint; 5] = Default::default();
        points.iter_mut().for_each(|point| {
            let (point_slice, rest_inner) = rest.split_at(size_of::<CPoint>());
            *point = CPoint::read_from_slice(point_slice);
            rest = rest_inner;
        });

        let mut colors: [CColor; 4] = Default::default();
        colors.iter_mut().for_each(|color| {
            let (point_slice, rest_inner) = rest.split_at(size_of::<CColor>());
            *color = CColor::read_from_slice(point_slice);
            rest = rest_inner;
        });

        let mut tex_coords: [CPoint; 4] = Default::default();
        tex_coords.iter_mut().for_each(|tex_coord| {
            let (tex_coord_slice, rest_inner) = rest.split_at(size_of::<CPoint>());
            *tex_coord = CPoint::read_from_slice(tex_coord_slice);
            rest = rest_inner;
        });

        let (pos_env, rest) = rest.split_at(size_of::<i32>());
        let pos_env = read_i32_le(pos_env);

        let (pos_env_off, rest) = rest.split_at(size_of::<i32>());
        let pos_env_off = read_i32_le(pos_env_off);

        let (color_env, rest) = rest.split_at(size_of::<i32>());
        let color_env = read_i32_le(color_env);

        let (color_env_off, _rest) = rest.split_at(size_of::<i32>());
        let color_env_off = read_i32_le(color_env_off);

        Self {
            points,
            colors,
            tex_coords,
            pos_env,
            pos_env_offset: pos_env_off,
            color_env,
            color_env_offset: color_env_off,
        }
    }

    fn write_to_vec(&self, w: &mut Vec<u8>) {
        self.points.iter().for_each(|v| v.write_to_vec(w));
        self.colors.iter().for_each(|v| v.write_to_vec(w));
        self.tex_coords.iter().for_each(|v| v.write_to_vec(w));
        w.extend(self.pos_env.to_le_bytes());
        w.extend(self.pos_env_offset.to_le_bytes());
        w.extend(self.color_env.to_le_bytes());
        w.extend(self.color_env_offset.to_le_bytes());
    }
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum MapTileLayerDetail {
    Tile(Vec<CTile>),
    Tele(Vec<CTeleTile>),
    Speedup(Vec<CSpeedupTile>),
    Switch(Vec<CSwitchTile>),
    Tune(Vec<CTuneTile>),
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct MapLayerTile(
    pub CMapItemLayerTilemap,
    pub MapTileLayerDetail,
    pub Vec<CTile>,
);

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct MapLayerQuad(pub CMapItemLayerQuads, pub Vec<CQuad>);

#[derive(Debug, Hiarc, Clone)]
pub enum MapLayer {
    Tile(MapLayerTile),
    Quads(MapLayerQuad),
    Sound {
        def: CMapItemLayerSounds,
        sounds: Vec<CSoundSource>,
    },
    Unknown(CMapItemLayer),
}

impl MapLayer {
    pub fn get_tile_layer_base(&self) -> &CMapItemLayer {
        match self {
            MapLayer::Tile(layer) => &layer.0.layer,
            MapLayer::Quads(layer) => &layer.0.layer,
            MapLayer::Sound { def: layer, .. } => &layer.layer,
            MapLayer::Unknown(layer) => layer,
        }
    }
}

#[derive(Debug, Hiarc)]
#[repr(C)]
pub struct CEnvPoint {
    pub time: i32, // in ms
    pub curve_type: i32,
    pub values: [i32; 4], // 1-4 depending on envelope (22.10 fixed point)
}

impl CEnvPoint {
    pub fn read_from_slice(data: &[u8]) -> Self {
        let (time_slice, rest) = data.split_at(size_of::<i32>());
        let time = read_i32_le(time_slice);

        let (curve_type_slice, mut rest) = rest.split_at(size_of::<i32>());
        let curve_type = read_i32_le(curve_type_slice);

        let mut values: [i32; 4] = Default::default();
        values.iter_mut().for_each(|c| {
            let (val, rest2) = rest.split_at(size_of::<i32>());
            *c = read_i32_le(val);
            rest = rest2;
        });

        Self {
            time,
            curve_type,
            values,
        }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.time.to_le_bytes());
        w.extend(self.curve_type.to_le_bytes());
        w.extend(self.values.iter().flat_map(|v| v.to_le_bytes()));
    }
}

pub enum CMapItemEnvelopeVer {
    CurVersion = 2,
}

#[derive(Debug, Hiarc, Default)]
#[repr(C)]
pub struct CMapItemEnvelope {
    pub version: i32,
    pub channels: i32,
    pub start_point: i32,
    pub num_points: i32,
    pub name: [i32; 8],
    pub synchronized: i32,
}

impl CMapItemEnvelope {
    pub fn size_without_sync() -> usize {
        std::mem::size_of::<Self>() - std::mem::size_of::<i32>()
    }

    pub fn read_from_slice(data: &[u8]) -> Self {
        let (version, rest) = data.split_at(size_of::<i32>());
        let version = read_i32_le(version);

        let (channels, rest) = rest.split_at(size_of::<i32>());
        let channels = read_i32_le(channels);

        let (start_point, rest) = rest.split_at(size_of::<i32>());
        let start_point = read_i32_le(start_point);

        let (num_points, mut rest) = rest.split_at(size_of::<i32>());
        let num_points = read_i32_le(num_points);

        let mut iname: [i32; 8] = Default::default();
        iname.iter_mut().for_each(|c| {
            let (name, rest2) = rest.split_at(size_of::<i32>());
            *c = read_i32_le(name);
            rest = rest2;
        });

        let synchronized = if !rest.is_empty() {
            let (synchronized, _rest) = rest.split_at(size_of::<i32>());
            read_i32_le(synchronized)
        } else {
            0
        };

        let synchronized = if version < 2 { 1 } else { synchronized };

        Self {
            version,
            channels,
            start_point,
            num_points,
            name: iname,
            synchronized,
        }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.version.to_le_bytes());
        w.extend(self.channels.to_le_bytes());
        w.extend(self.start_point.to_le_bytes());
        w.extend(self.num_points.to_le_bytes());
        w.extend(self.name.iter().flat_map(|v| v.to_le_bytes()));
        w.extend(self.synchronized.to_le_bytes());
    }
}

pub enum SoundShapeTy {
    ShapeRectangle = 0,
    ShapeCircle,
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct CRectangle {
    pub width: i32,  // fxp 22.10
    pub height: i32, // fxp 22.10
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct CCircle {
    pub radius: i32,
}

#[derive(Copy, Clone)]
#[repr(C)]
pub union SoundShapeProps {
    pub rect: CRectangle,
    pub circle: CCircle,
}

#[derive(Hiarc, Clone)]
#[repr(C)]
pub struct CSoundShape {
    pub ty: i32,

    #[hiarc_skip_unsafe]
    pub props: SoundShapeProps,
}

impl Debug for CSoundShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CSoundShape").field("ty", &self.ty).finish()
    }
}

impl CSoundShape {
    pub fn read_from_slice(data: &[u8]) -> Self {
        let (ty, rest) = data.split_at(size_of::<i32>());
        let ty = read_i32_le(ty);

        let (props, _) = rest.split_at(size_of::<CRectangle>().max(size_of::<CCircle>()));
        let props = match ty {
            x if x == SoundShapeTy::ShapeCircle as i32 => {
                let (radius, _) = props.split_at(size_of::<i32>());
                let radius = read_i32_le(radius);
                SoundShapeProps {
                    circle: CCircle { radius },
                }
            }
            x if x == SoundShapeTy::ShapeRectangle as i32 => {
                let (width, rest) = props.split_at(size_of::<i32>());
                let width = read_i32_le(width);
                let (height, _) = rest.split_at(size_of::<i32>());
                let height = read_i32_le(height);
                SoundShapeProps {
                    rect: CRectangle { width, height },
                }
            }
            _ => panic!("unknown sound shape"),
        };

        Self { ty, props }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.ty.to_le_bytes());
        unsafe {
            match self.ty {
                x if x == SoundShapeTy::ShapeCircle as i32 => {
                    w.extend(self.props.circle.radius.to_le_bytes());
                    // extend the data so it matches union size
                    w.extend(0i32.to_le_bytes());
                }
                x if x == SoundShapeTy::ShapeRectangle as i32 => {
                    w.extend(self.props.rect.width.to_le_bytes());
                    w.extend(self.props.rect.height.to_le_bytes());
                }
                _ => panic!("unknown sound shape"),
            }
        }
    }
}

impl Default for CSoundShape {
    fn default() -> Self {
        Self {
            ty: Default::default(),
            props: SoundShapeProps {
                circle: CCircle { radius: 0 },
            },
        }
    }
}

#[derive(Debug, Default, Hiarc, Clone)]
#[repr(C)]
pub struct CSoundSource {
    pub pos: CPoint,
    pub looped: i32,
    pub panning: i32,    // 0 - no panning, 1 - panning
    pub time_delay: i32, // in s
    pub falloff: i32,    // [0,255] // 0 - No falloff, 255 - full

    pub pos_env: i32,
    pub pos_env_offset: i32,
    pub sound_env: i32,
    pub sound_env_offset: i32,

    pub shape: CSoundShape,
}

impl CSoundSource {
    pub fn read_from_slice(data: &[u8]) -> Self {
        let (data_slice, rest) = data.split_at(size_of::<CPoint>());
        let pos = CPoint::read_from_slice(data_slice);

        let (looped, rest) = rest.split_at(size_of::<i32>());
        let looped = read_i32_le(looped);

        let (panning, rest) = rest.split_at(size_of::<i32>());
        let panning = read_i32_le(panning);

        let (time_delay, rest) = rest.split_at(size_of::<i32>());
        let time_delay = read_i32_le(time_delay);

        let (falloff, rest) = rest.split_at(size_of::<i32>());
        let falloff = read_i32_le(falloff);

        let (pos_env, rest) = rest.split_at(size_of::<i32>());
        let pos_env = read_i32_le(pos_env);

        let (pos_env_offset, rest) = rest.split_at(size_of::<i32>());
        let pos_env_offset = read_i32_le(pos_env_offset);

        let (sound_env, rest) = rest.split_at(size_of::<i32>());
        let sound_env = read_i32_le(sound_env);

        let (sound_env_offset, rest) = rest.split_at(size_of::<i32>());
        let sound_env_offset = read_i32_le(sound_env_offset);

        let (shape, _) = rest.split_at(size_of::<CSoundShape>());
        let shape = CSoundShape::read_from_slice(shape);

        Self {
            pos,
            looped,
            panning,
            time_delay,
            falloff,
            pos_env,
            pos_env_offset,
            sound_env,
            sound_env_offset,
            shape,
        }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        self.pos.write_to_vec(w);
        w.extend(self.looped.to_le_bytes());
        w.extend(self.panning.to_le_bytes());
        w.extend(self.time_delay.to_le_bytes());
        w.extend(self.falloff.to_le_bytes());

        w.extend(self.pos_env.to_le_bytes());
        w.extend(self.pos_env_offset.to_le_bytes());
        w.extend(self.sound_env.to_le_bytes());
        w.extend(self.sound_env_offset.to_le_bytes());

        self.shape.write_to_vec(w);
    }
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct CMapItemSound {
    pub version: i32,

    pub external: i32,

    pub sound_name: i32,
    pub sound_data: i32,
    pub sound_data_size: i32,
}

impl CMapItemSound {
    pub fn read_from_slice(data: &[u8]) -> Self {
        let (version, rest) = data.split_at(size_of::<i32>());
        let version = read_i32_le(version);

        let (external, rest) = rest.split_at(size_of::<i32>());
        let external = read_i32_le(external);

        let (sound_name, rest) = rest.split_at(size_of::<i32>());
        let sound_name = read_i32_le(sound_name);

        let (sound_data, rest) = rest.split_at(size_of::<i32>());
        let sound_data = read_i32_le(sound_data);

        let (sound_data_size, _rest) = rest.split_at(size_of::<i32>());
        let sound_data_size = read_i32_le(sound_data_size);

        Self {
            version,

            external,

            sound_name,
            sound_data,
            sound_data_size,
        }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.version.to_le_bytes());
        w.extend(self.external.to_le_bytes());
        w.extend(self.sound_name.to_le_bytes());
        w.extend(self.sound_data.to_le_bytes());
        w.extend(self.sound_data_size.to_le_bytes());
    }
}

#[derive(Debug, Hiarc, Clone)]
pub struct MapSound {
    pub name: String,
    pub def: CMapItemSound,
    pub data: Option<Vec<u8>>,
}
