pub mod layers;

use std::path::Path;

use anyhow::anyhow;
use assets_base::tar::tar_entry_to_file;
use base::join_all;
use hiarc::Hiarc;
use math::math::vector::{fvec2, ufvec2};
use serde::{Deserialize, Serialize};

use crate::{
    file::MapFileReader,
    types::NonZeroU16MinusOne,
    utils::{deserialize_twmap_bincode, serialize_twmap_bincode},
};

use self::layers::{design::MapLayer, physics::MapLayerPhysics, tiles::TileBase};

#[derive(Debug, Hiarc, Clone, Default, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapGroupAttrClipping {
    pub pos: fvec2,
    pub size: ufvec2,
}

#[derive(Debug, Hiarc, Clone, Default, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapGroupAttr {
    pub offset: fvec2,
    pub parallax: fvec2,

    pub clipping: Option<MapGroupAttrClipping>,
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MapGroup {
    pub attr: MapGroupAttr,
    pub layers: Vec<MapLayer>,

    /// optional name, mostly intersting for editor
    pub name: String,
}

#[derive(Debug, Hiarc, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapGroupPhysicsAttr {
    pub width: NonZeroU16MinusOne,
    pub height: NonZeroU16MinusOne,
}

#[derive(Debug, Hiarc, Clone)]
pub struct MapGroupPhysics {
    pub attr: MapGroupPhysicsAttr,
    pub layers: Vec<MapLayerPhysics>,
}

impl Serialize for MapGroupPhysics {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        (&self.attr, &self.layers).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MapGroupPhysics {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let (attr, layers) =
            <(MapGroupPhysicsAttr, Vec<MapLayerPhysics>)>::deserialize(deserializer)?;

        // validate all layers
        let expected_tile_count = attr.width.get() as u64 * attr.height.get() as u64;
        let mut found_arbitrary_layer = 0;
        let mut found_game_layer = 0;
        let mut found_front_layer = 0;
        let mut found_tele_layer = 0;
        let mut found_speedup_layer = 0;
        let mut found_switch_layer = 0;
        let mut found_tune_layer = 0;
        for layer in &layers {
            if let Err(err) = match layer {
                MapLayerPhysics::Arbitrary(_) => {
                    found_arbitrary_layer += 1;
                    Ok(())
                }
                MapLayerPhysics::Game(layer) => {
                    found_game_layer += 1;
                    (layer.tiles.len() as u64 == expected_tile_count)
                        .then_some(())
                        .ok_or_else(|| anyhow!("invalid tile count in game layer"))
                }
                MapLayerPhysics::Front(layer) => {
                    found_front_layer += 1;
                    (layer.tiles.len() as u64 == expected_tile_count)
                        .then_some(())
                        .ok_or_else(|| anyhow!("invalid tile count in front layer"))
                }
                MapLayerPhysics::Tele(layer) => {
                    found_tele_layer += 1;
                    (layer.base.tiles.len() as u64 == expected_tile_count)
                        .then_some(())
                        .ok_or_else(|| anyhow!("invalid tile count in tele layer"))
                }
                MapLayerPhysics::Speedup(layer) => {
                    found_speedup_layer += 1;
                    (layer.tiles.len() as u64 == expected_tile_count)
                        .then_some(())
                        .ok_or_else(|| anyhow!("invalid tile count in speedup layer"))
                }
                MapLayerPhysics::Switch(layer) => {
                    found_switch_layer += 1;
                    (layer.base.tiles.len() as u64 == expected_tile_count)
                        .then_some(())
                        .ok_or_else(|| anyhow!("invalid tile count in switch layer"))
                }
                MapLayerPhysics::Tune(layer) => {
                    found_tune_layer += 1;
                    (layer.base.tiles.len() as u64 == expected_tile_count)
                        .then_some(())
                        .ok_or_else(|| anyhow!("invalid tile count in tune layer"))
                }
            } {
                return Err(serde::de::Error::custom(format!(
                    "could not validate physics layer: {err}"
                )));
            }
        }

        if let Err(err) = if found_arbitrary_layer > 1 {
            Err(anyhow!(
                "More than one arbitrary physics layer found, the limit is 1"
            ))
        } else if found_game_layer > 1 {
            Err(anyhow!(
                "More than one game physics layer found, the limit is 1"
            ))
        } else if found_front_layer > 1 {
            Err(anyhow!(
                "More than one front physics layer found, the limit is 1"
            ))
        } else if found_tele_layer > 1 {
            Err(anyhow!(
                "More than one tele physics layer found, the limit is 1"
            ))
        } else if found_speedup_layer > 1 {
            Err(anyhow!(
                "More than one speedup physics layer found, the limit is 1"
            ))
        } else if found_switch_layer > 1 {
            Err(anyhow!(
                "More than one switch physics layer found, the limit is 1"
            ))
        } else if found_tune_layer > 1 {
            Err(anyhow!(
                "More than one tune physics layer found, the limit is 1"
            ))
        } else {
            Ok(())
        } {
            return Err(serde::de::Error::custom(format!(
                "could not validate physics layer: {err}"
            )));
        }

        Ok(Self { attr, layers })
    }
}

impl MapGroupPhysics {
    pub fn get_game_layer_tiles(&self) -> &Vec<TileBase> {
        self.layers
            .iter()
            .find_map(|layer| {
                if let MapLayerPhysics::Game(layer) = &layer {
                    Some(&layer.tiles)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                panic!(
                    "FATAL ERROR: did not find a game layer (layers: {:?})",
                    &self.layers
                )
            })
    }
}

#[derive(Debug, Hiarc, Clone)]
pub struct MapGroups {
    pub physics: MapGroupPhysics,

    pub background: Vec<MapGroup>,
    pub foreground: Vec<MapGroup>,
}

impl MapGroups {
    /// Deserializes the physics group
    pub fn deserialize_physics_group(uncompressed_file: &[u8]) -> anyhow::Result<MapGroupPhysics> {
        deserialize_twmap_bincode::<MapGroupPhysics>(uncompressed_file)
    }

    /// Serializes the physics group and returns the amount of bytes written
    pub fn serialize_physics_group<W: std::io::Write>(
        grp: &MapGroupPhysics,
        writer: &mut W,
    ) -> anyhow::Result<usize> {
        serialize_twmap_bincode(grp, writer)
    }

    /// Decompresses the physics group
    pub fn decompress_physics_group(file: &[u8]) -> anyhow::Result<Vec<u8>> {
        crate::utils::decompress(file)
    }

    /// Compresses the physics group
    pub fn compress_physics_group(uncompressed_file: &[u8]) -> anyhow::Result<Vec<u8>> {
        crate::utils::compress(uncompressed_file)
    }

    fn deserialize_design_groups(uncompressed_file: &[u8]) -> anyhow::Result<Vec<MapGroup>> {
        deserialize_twmap_bincode::<Vec<MapGroup>>(uncompressed_file)
    }

    fn serialize_design_groups<W: std::io::Write>(
        grps: &Vec<MapGroup>,
        writer: &mut W,
    ) -> anyhow::Result<usize> {
        serialize_twmap_bincode(grps, writer)
    }

    /// Deserializes the foreground groups
    pub(crate) fn deserialize_foreground_groups(
        uncompressed_file: &[u8],
    ) -> anyhow::Result<Vec<MapGroup>> {
        Self::deserialize_design_groups(uncompressed_file)
    }

    /// Serializes the foreground groups and returns the amount of bytes written
    pub fn serialize_foreground_groups<W: std::io::Write>(
        grps: &Vec<MapGroup>,
        writer: &mut W,
    ) -> anyhow::Result<usize> {
        Self::serialize_design_groups(grps, writer)
    }

    /// Deserializes the background groups
    pub(crate) fn deserialize_background_groups(
        uncompressed_file: &[u8],
    ) -> anyhow::Result<Vec<MapGroup>> {
        Self::deserialize_design_groups(uncompressed_file)
    }

    /// Serializes the background groups and returns the amount of bytes written
    pub fn serialize_background_groups<W: std::io::Write>(
        grps: &Vec<MapGroup>,
        writer: &mut W,
    ) -> anyhow::Result<usize> {
        Self::serialize_design_groups(grps, writer)
    }

    /// Decompresses the background & foreground groups
    pub fn decompress_design_group(file: &[u8]) -> anyhow::Result<Vec<u8>> {
        crate::utils::decompress(file)
    }

    /// Compresses the background & foreground groups
    pub fn compress_design_group(uncompressed_file: &[u8]) -> anyhow::Result<Vec<u8>> {
        crate::utils::compress(uncompressed_file)
    }

    /// Read the map's game group.
    pub(crate) fn read(reader: &MapFileReader, tp: &rayon::ThreadPool) -> anyhow::Result<Self> {
        let physics_file = tar_entry_to_file(
            reader
                .entries
                .get(Path::new("groups/physics.twmap_bincode.zst"))
                .ok_or_else(|| anyhow!("physics group was not found in map file"))?,
        )?;
        let bg_file = tar_entry_to_file(
            reader
                .entries
                .get(Path::new("groups/background.twmap_bincode.zst"))
                .ok_or_else(|| anyhow!("background groups was not found in map file"))?,
        )?;
        let fg_file = tar_entry_to_file(
            reader
                .entries
                .get(Path::new("groups/foreground.twmap_bincode.zst"))
                .ok_or_else(|| anyhow!("foreground groups was not found in map file"))?,
        )?;
        let (physics_group, background_groups, foreground_groups) = tp.install(|| {
            join_all!(
                || {
                    let physics_group_file = Self::decompress_physics_group(physics_file)?;
                    let physics_group = Self::deserialize_physics_group(&physics_group_file)?;
                    anyhow::Ok(physics_group)
                },
                || {
                    let bg_group_file = Self::decompress_design_group(bg_file)?;

                    let background_groups = Self::deserialize_background_groups(&bg_group_file)?;
                    anyhow::Ok(background_groups)
                },
                || {
                    let fg_group_file = Self::decompress_design_group(fg_file)?;

                    let foreground_groups = Self::deserialize_foreground_groups(&fg_group_file)?;
                    anyhow::Ok(foreground_groups)
                }
            )
        });

        Ok(Self {
            physics: physics_group?,
            background: background_groups?,
            foreground: foreground_groups?,
        })
    }

    /// Returns the physics group
    pub fn read_physics_group(reader: &MapFileReader) -> anyhow::Result<MapGroupPhysics> {
        let physics_file = tar_entry_to_file(
            reader
                .entries
                .get(Path::new("groups/physics.twmap_bincode.zst"))
                .ok_or_else(|| anyhow!("physics group was not found in map file"))?,
        )?;
        let physics_group_file = Self::decompress_physics_group(physics_file)?;

        let physics_group = Self::deserialize_physics_group(&physics_group_file)?;
        anyhow::Ok(physics_group)
    }

    /// Write a map file to a writer
    pub fn write(&self, tp: &rayon::ThreadPool) -> anyhow::Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        let (physics, bg_fg) = tp.install(|| {
            tp.join(
                || {
                    let mut serialized_physics: Vec<u8> = Default::default();
                    Self::serialize_physics_group(&self.physics, &mut serialized_physics)?;
                    Self::compress_physics_group(&serialized_physics)
                },
                || {
                    let (bg, fg) = tp.join(
                        || {
                            let mut serialized_bg: Vec<u8> = Default::default();
                            Self::serialize_background_groups(
                                &self.background,
                                &mut serialized_bg,
                            )?;
                            Self::compress_design_group(&serialized_bg)
                        },
                        || {
                            let mut serialized_fg: Vec<u8> = Default::default();
                            Self::serialize_foreground_groups(
                                &self.foreground,
                                &mut serialized_fg,
                            )?;
                            Self::compress_design_group(&serialized_fg)
                        },
                    );
                    anyhow::Ok((bg?, fg?))
                },
            )
        });

        let (bg, fg) = bg_fg?;
        Ok((physics?, bg, fg))
    }
}
