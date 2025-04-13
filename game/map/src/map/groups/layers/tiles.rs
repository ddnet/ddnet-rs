use bitflags::bitflags;
use hiarc::Hiarc;
use math::math::vector::nfvec4;
use serde::{Deserialize, Serialize};

use crate::types::NonZeroU16MinusOne;

#[derive(Debug, Hiarc, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapTileLayerAttr {
    pub width: NonZeroU16MinusOne,
    pub height: NonZeroU16MinusOne,

    pub color: nfvec4,

    /// is a high detail layer
    pub high_detail: bool,

    pub color_anim: Option<usize>,
    pub color_anim_offset: time::Duration,

    pub image_array: Option<usize>,
}

#[derive(
    Debug, Hiarc, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct TileFlags(u8);

bitflags! {
    impl TileFlags: u8 {
        const XFLIP = 1 << 0;
        const YFLIP = 1 << 1;
        const OPAQUE = 1 << 2;
        const ROTATE = 1 << 3;
    }
}

// Rotation
pub const ROTATION_0: TileFlags = TileFlags::empty();
pub const ROTATION_90: TileFlags = TileFlags::ROTATE;
pub fn rotation_180() -> TileFlags {
    TileFlags::XFLIP | TileFlags::YFLIP
}
pub fn rotation_270() -> TileFlags {
    TileFlags::XFLIP | TileFlags::YFLIP | TileFlags::ROTATE
}
pub fn rotate_by_plus_90(flags: &mut TileFlags) {
    if flags.contains(ROTATION_90) {
        flags.toggle(rotation_180());
    }
    flags.toggle(ROTATION_90);
}

#[derive(Debug, Hiarc, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TileBase {
    pub index: u8,
    pub flags: TileFlags,
}

impl AsRef<TileBase> for TileBase {
    fn as_ref(&self) -> &TileBase {
        self
    }
}

impl AsMut<TileBase> for TileBase {
    fn as_mut(&mut self) -> &mut TileBase {
        self
    }
}

pub type Tile = TileBase;

// ddrace
#[derive(Debug, Hiarc, Clone, Default, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeleTile {
    pub base: TileBase,
    pub number: u8,
}

impl AsRef<TileBase> for TeleTile {
    fn as_ref(&self) -> &TileBase {
        &self.base
    }
}

impl AsMut<TileBase> for TeleTile {
    fn as_mut(&mut self) -> &mut TileBase {
        &mut self.base
    }
}

#[derive(Debug, Hiarc, Clone, Default, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpeedupTile {
    pub base: TileBase,
    pub force: u8,
    pub max_speed: u8,
    pub angle: i16,
}

impl AsRef<TileBase> for SpeedupTile {
    fn as_ref(&self) -> &TileBase {
        &self.base
    }
}

impl AsMut<TileBase> for SpeedupTile {
    fn as_mut(&mut self) -> &mut TileBase {
        &mut self.base
    }
}

#[derive(Debug, Hiarc, Clone, Default, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwitchTile {
    pub base: TileBase,
    pub number: u8,
    pub delay: u8,
}

impl AsRef<TileBase> for SwitchTile {
    fn as_ref(&self) -> &TileBase {
        &self.base
    }
}

impl AsMut<TileBase> for SwitchTile {
    fn as_mut(&mut self) -> &mut TileBase {
        &mut self.base
    }
}

#[derive(Debug, Hiarc, Clone, Default, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct TuneTile {
    pub base: TileBase,
    pub number: u8,
}

impl AsRef<TileBase> for TuneTile {
    fn as_ref(&self) -> &TileBase {
        &self.base
    }
}

impl AsMut<TileBase> for TuneTile {
    fn as_mut(&mut self) -> &mut TileBase {
        &mut self.base
    }
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MapTileLayerPhysicsTiles {
    Arbitrary(Vec<u8>),
    Game(Vec<Tile>),
    Front(Vec<Tile>),
    Tele(Vec<TeleTile>),
    Speedup(Vec<SpeedupTile>),
    Switch(Vec<SwitchTile>),
    Tune(Vec<TuneTile>),
}

#[derive(Debug, Hiarc)]
pub enum MapTileLayerPhysicsTilesRef<'a> {
    Arbitrary(&'a Vec<u8>),
    Game(&'a Vec<Tile>),
    Front(&'a Vec<Tile>),
    Tele(&'a Vec<TeleTile>),
    Speedup(&'a Vec<SpeedupTile>),
    Switch(&'a Vec<SwitchTile>),
    Tune(&'a Vec<TuneTile>),
}

impl MapTileLayerPhysicsTiles {
    pub fn tiles_count(&self) -> usize {
        match self {
            Self::Arbitrary(_) => panic!("arbitrary tiles are not supported"),
            Self::Game(tiles) => tiles.len(),
            Self::Front(tiles) => tiles.len(),
            Self::Tele(tiles) => tiles.len(),
            Self::Speedup(tiles) => tiles.len(),
            Self::Switch(tiles) => tiles.len(),
            Self::Tune(tiles) => tiles.len(),
        }
    }

    pub fn non_air_tiles_count(&self) -> usize {
        match self {
            Self::Arbitrary(_) => panic!("arbitrary tiles are not supported"),
            Self::Game(tiles) => tiles.iter().filter(|t| t.index > 0).count(),
            Self::Front(tiles) => tiles.iter().filter(|t| t.index > 0).count(),
            Self::Tele(tiles) => tiles.iter().filter(|t| t.base.index > 0).count(),
            Self::Speedup(tiles) => tiles.iter().filter(|t| t.base.index > 0).count(),
            Self::Switch(tiles) => tiles.iter().filter(|t| t.base.index > 0).count(),
            Self::Tune(tiles) => tiles.iter().filter(|t| t.base.index > 0).count(),
        }
    }

    pub fn as_ref(&self) -> MapTileLayerPhysicsTilesRef {
        match self {
            MapTileLayerPhysicsTiles::Arbitrary(tiles) => {
                MapTileLayerPhysicsTilesRef::Arbitrary(tiles)
            }
            MapTileLayerPhysicsTiles::Game(tiles) => MapTileLayerPhysicsTilesRef::Game(tiles),
            MapTileLayerPhysicsTiles::Front(tiles) => MapTileLayerPhysicsTilesRef::Front(tiles),
            MapTileLayerPhysicsTiles::Tele(tiles) => MapTileLayerPhysicsTilesRef::Tele(tiles),
            MapTileLayerPhysicsTiles::Speedup(tiles) => MapTileLayerPhysicsTilesRef::Speedup(tiles),
            MapTileLayerPhysicsTiles::Switch(tiles) => MapTileLayerPhysicsTilesRef::Switch(tiles),
            MapTileLayerPhysicsTiles::Tune(tiles) => MapTileLayerPhysicsTilesRef::Tune(tiles),
        }
    }
}

/// a datatype that is mostly useful for editor, that represents
/// all possible tiles
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum MapTileLayerTiles {
    Design(Vec<Tile>),
    Physics(MapTileLayerPhysicsTiles),
}

impl MapTileLayerTiles {
    pub fn tiles_count(&self) -> usize {
        match self {
            MapTileLayerTiles::Design(tiles) => tiles.len(),
            MapTileLayerTiles::Physics(tiles) => tiles.tiles_count(),
        }
    }

    pub fn non_air_tiles_count(&self) -> usize {
        match self {
            MapTileLayerTiles::Design(tiles) => tiles.iter().filter(|t| t.index > 0).count(),
            MapTileLayerTiles::Physics(tiles) => tiles.non_air_tiles_count(),
        }
    }
}
