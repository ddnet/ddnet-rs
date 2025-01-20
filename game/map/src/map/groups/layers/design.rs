use hiarc::Hiarc;
use math::math::vector::{fvec2, nffixed, nfvec4, uffixed, ufvec2};
use serde::{Deserialize, Serialize};

use super::tiles::{MapTileLayerAttr, Tile};

#[derive(Debug, Hiarc, Clone, PartialEq, Eq)]
pub struct MapLayerTile {
    pub attr: MapTileLayerAttr,
    pub tiles: Vec<Tile>,

    /// optional name, mostly intersting for editor
    pub name: String,
}

impl Serialize for MapLayerTile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        (&self.attr, &self.tiles, &self.name).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MapLayerTile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let (attr, tiles, name) =
            <(MapTileLayerAttr, Vec<Tile>, String)>::deserialize(deserializer)?;

        // validate the design tile layer
        if attr.width.get() as u64 * attr.height.get() as u64 != tiles.len() as u64 {
            return Err(serde::de::Error::custom(format!(
                "could not validate design tile layer. \
                width & height did not match tile layer count {} - {} vs {}",
                attr.width.get(),
                attr.height.get(),
                tiles.len()
            )));
        }

        Ok(Self { attr, tiles, name })
    }
}

#[derive(Debug, Hiarc, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quad {
    pub points: [fvec2; 5],
    pub colors: [nfvec4; 4],
    pub tex_coords: [fvec2; 4],

    pub pos_anim: Option<usize>,
    pub pos_anim_offset: time::Duration,

    pub color_anim: Option<usize>,
    pub color_anim_offset: time::Duration,
}

#[derive(Debug, Hiarc, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapLayerQuadsAttrs {
    pub image: Option<usize>,

    /// is a high detail layer
    pub high_detail: bool,
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MapLayerQuad {
    pub attr: MapLayerQuadsAttrs,
    pub quads: Vec<Quad>,

    /// optional name, mostly intersting for editor
    pub name: String,
}

#[derive(Debug, Hiarc, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SoundShape {
    Rect { size: ufvec2 },
    Circle { radius: uffixed },
}

#[derive(Debug, Hiarc, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sound {
    pub pos: fvec2,
    pub looped: bool,
    pub panning: bool,
    pub time_delay: std::time::Duration,
    pub falloff: nffixed,

    pub pos_anim: Option<usize>,
    pub pos_anim_offset: time::Duration,
    pub sound_anim: Option<usize>,
    pub sound_anim_offset: time::Duration,

    pub shape: SoundShape,
}

#[derive(Debug, Hiarc, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapLayerSoundAttrs {
    pub sound: Option<usize>,

    /// is a high detail layer
    pub high_detail: bool,
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MapLayerSound {
    pub attr: MapLayerSoundAttrs,
    pub sounds: Vec<Sound>,

    /// optional name, mostly intersting for editor
    pub name: String,
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MapLayer {
    /// can be used for mods, if client compability is important, while having custom layers
    Abritrary(Vec<u8>),
    Tile(MapLayerTile),
    Quad(MapLayerQuad),
    Sound(MapLayerSound),
}

impl MapLayer {
    pub fn name(&self) -> &str {
        match self {
            MapLayer::Abritrary(_) => "unknown layer",
            MapLayer::Tile(layer) => &layer.name,
            MapLayer::Quad(layer) => &layer.name,
            MapLayer::Sound(layer) => &layer.name,
        }
    }
}
