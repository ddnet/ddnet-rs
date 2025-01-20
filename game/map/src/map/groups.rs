pub mod layers;

use anyhow::anyhow;
use base::join_all;
use hiarc::Hiarc;
use math::math::vector::{fvec2, ufvec2};
use serde::{Deserialize, Serialize};

use crate::types::NonZeroU16MinusOne;

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
                    "could not validate physics layer: {}",
                    err
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
                "could not validate physics layer: {}",
                err
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
    /// Deserializes the physics group and returns the amount of bytes read
    pub fn deserialize_physics_group(
        uncompressed_file: &[u8],
    ) -> anyhow::Result<(MapGroupPhysics, usize)> {
        Ok(bincode::serde::decode_from_slice::<MapGroupPhysics, _>(
            uncompressed_file,
            bincode::config::standard(),
        )?)
    }

    /// Serializes the physics group and returns the amount of bytes written
    pub fn serialize_physics_group<W: std::io::Write>(
        grp: &MapGroupPhysics,
        writer: &mut W,
    ) -> anyhow::Result<usize> {
        Ok(bincode::serde::encode_into_std_write(
            grp,
            writer,
            bincode::config::standard(),
        )?)
    }

    /// Decompresses the physics group, returns the amount of bytes read
    pub fn decompress_physics_group(file: &[u8]) -> anyhow::Result<(Vec<u8>, usize)> {
        crate::utils::decompress(file)
    }

    /// Compresses the physics group, returns the amount of bytes written
    pub fn compress_physics_group<W: std::io::Write>(
        uncompressed_file: &[u8],
        writer: &mut W,
    ) -> anyhow::Result<()> {
        crate::utils::compress(uncompressed_file, writer)
    }

    fn deserialize_design_groups(
        uncompressed_file: &[u8],
    ) -> anyhow::Result<(Vec<MapGroup>, usize)> {
        Ok(bincode::serde::decode_from_slice::<Vec<MapGroup>, _>(
            uncompressed_file,
            bincode::config::standard(),
        )?)
    }

    fn serialize_design_groups<W: std::io::Write>(
        grps: &Vec<MapGroup>,
        writer: &mut W,
    ) -> anyhow::Result<usize> {
        Ok(bincode::serde::encode_into_std_write(
            grps,
            writer,
            bincode::config::standard(),
        )?)
    }

    /// Deserializes the foreground groups and returns the amount of bytes read
    pub(crate) fn deserialize_foreground_groups(
        uncompressed_file: &[u8],
    ) -> anyhow::Result<(Vec<MapGroup>, usize)> {
        Self::deserialize_design_groups(uncompressed_file)
    }

    /// Serializes the foreground groups and returns the amount of bytes written
    pub fn serialize_foreground_groups<W: std::io::Write>(
        grps: &Vec<MapGroup>,
        writer: &mut W,
    ) -> anyhow::Result<usize> {
        Self::serialize_design_groups(grps, writer)
    }

    /// Deserializes the background groups and returns the amount of bytes read
    pub(crate) fn deserialize_background_groups(
        uncompressed_file: &[u8],
    ) -> anyhow::Result<(Vec<MapGroup>, usize)> {
        Self::deserialize_design_groups(uncompressed_file)
    }

    /// Serializes the background groups and returns the amount of bytes written
    pub fn serialize_background_groups<W: std::io::Write>(
        grps: &Vec<MapGroup>,
        writer: &mut W,
    ) -> anyhow::Result<usize> {
        Self::serialize_design_groups(grps, writer)
    }

    /// Decompresses the background & foreground groups, returns the amount of bytes read
    pub fn decompress_design_groups(file: &[u8]) -> anyhow::Result<(Vec<u8>, usize)> {
        crate::utils::decompress(file)
    }

    /// Compresses the background & foreground groups, returns the amount of bytes read
    pub fn compress_design_groups<W: std::io::Write>(
        uncompressed_file: &[u8],
        writer: &mut W,
    ) -> anyhow::Result<()> {
        crate::utils::compress(uncompressed_file, writer)
    }

    /// Read the map's game group. returns the amount of bytes read.
    pub(crate) fn read(file: &[u8], tp: &rayon::ThreadPool) -> anyhow::Result<(Self, usize)> {
        let (physics_group_file, bytes_read) = Self::decompress_physics_group(file)?;
        let (physics_group, design_groups) = tp.install(|| {
            join_all!(
                || {
                    let (physics_group, _) = Self::deserialize_physics_group(&physics_group_file)?;
                    anyhow::Ok(physics_group)
                },
                || {
                    let (design_groups_file, bytes_read_group) =
                        Self::decompress_design_groups(&file[bytes_read..])?;

                    let (background_groups, bytes_read) =
                        Self::deserialize_background_groups(&design_groups_file)?;
                    let (foreground_groups, _) =
                        Self::deserialize_foreground_groups(&design_groups_file[bytes_read..])?;
                    anyhow::Ok((bytes_read_group, background_groups, foreground_groups))
                }
            )
        });
        let (bytes_read_group, background_groups, foreground_groups) = design_groups?;

        Ok((
            Self {
                physics: physics_group?,
                background: background_groups,
                foreground: foreground_groups,
            },
            bytes_read + bytes_read_group,
        ))
    }

    /// Returns the physics group and the amount of bytes read
    pub fn read_physics_group(file: &[u8]) -> anyhow::Result<(MapGroupPhysics, usize)> {
        let (physics_group_file, bytes_read) = Self::decompress_physics_group(file)?;

        let (physics_group, _) = Self::deserialize_physics_group(&physics_group_file)?;
        anyhow::Ok((physics_group, bytes_read))
    }

    /// Write a map file to a writer
    pub fn write<W: std::io::Write>(
        &self,
        writer: &mut W,
        tp: &rayon::ThreadPool,
    ) -> anyhow::Result<()> {
        let (physics, bg_fg) = tp.install(|| {
            tp.join(
                || {
                    let mut physics: Vec<u8> = Default::default();
                    let mut serialized_physics: Vec<u8> = Default::default();
                    Self::serialize_physics_group(&self.physics, &mut serialized_physics)?;
                    Self::compress_physics_group(&serialized_physics, &mut physics)?;
                    anyhow::Ok(physics)
                },
                || {
                    let mut bg_fg: Vec<u8> = Default::default();
                    let (serialized_bg, serialized_fg) = tp.join(
                        || {
                            let mut serialized_bg: Vec<u8> = Default::default();
                            Self::serialize_background_groups(
                                &self.background,
                                &mut serialized_bg,
                            )?;
                            anyhow::Ok(serialized_bg)
                        },
                        || {
                            let mut serialized_fg: Vec<u8> = Default::default();
                            Self::serialize_foreground_groups(
                                &self.foreground,
                                &mut serialized_fg,
                            )?;
                            anyhow::Ok(serialized_fg)
                        },
                    );
                    Self::compress_design_groups(
                        &[serialized_bg?, serialized_fg?].concat(),
                        &mut bg_fg,
                    )?;
                    anyhow::Ok(bg_fg)
                },
            )
        });

        writer.write_all(&physics?)?;
        writer.write_all(&bg_fg?)?;
        Ok(())
    }
}
