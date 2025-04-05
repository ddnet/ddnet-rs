pub mod animations;
pub mod command_value;
pub mod config;
pub mod groups;
pub mod metadata;
pub mod resources;

use anyhow::anyhow;
use base::{
    hash::{generate_hash_for, name_and_hash, Hash},
    join_all,
};
use groups::layers::design::MapLayer;
use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

use crate::{
    map::groups::{MapGroup, MapGroupPhysics},
    utils::compressed_size,
};

use self::{
    animations::Animations, config::Config, groups::MapGroups, metadata::Metadata,
    resources::Resources,
};

/// A `Map` is mainly a collection of resources, layers & animations.
///
/// Additionally it might contain meta data about author, license etc. aswell as
/// config data that a game _can_ interpret, e.g. a list of commands.
///
/// - resources are external resources like images and sounds
/// - layers are either physics related or design related characteristics of the map.
///   layers are grouped, each group has own properties like parallax effects & offsets.
///   their ordering is important for rendering / sound etc.
/// - animations are a collection of animation frames, which can be used to control for example the color,
///   position or similar stuff of elements in the map layers.
///
/// Serialization & Deserialization of all items in the collection happens indepentially (mostly to allow parallel processing).
/// To make it easy use [`Map::read`] &  [`Map::write`], which automatically de-/serializes and compresses the map
///
/// ### De-/serialization notes
/// A common file header is always add in the form of "twmap[u64]" where u64 is a 64-bit sized version number.
/// If performance matters `resources` should be deserialized and processed async to loading the rest of this map file, since resources
/// are always external files and deserializing this map file can be expensive too.  
/// `groups` always de-/serializes the physics group indepentially to design groups,
/// this allows the server to process it without design groups.
#[derive(Debug, Hiarc, Clone)]
pub struct Map {
    pub resources: Resources,
    pub groups: MapGroups,
    pub animations: Animations,

    pub config: Config,
    pub meta: Metadata,
}

impl Map {
    pub const VERSION: u64 = 2024040200;
    pub const FILE_TY: &'static str = "twmap";

    pub(crate) fn validate_resource_and_anim_indices(
        resources: &Resources,
        animations: &Animations,
        groups: &MapGroups,
    ) -> anyhow::Result<()> {
        for group in groups.background.iter() {
            for layer in group.layers.iter() {
                match layer {
                    MapLayer::Abritrary(_) => Ok(()),
                    MapLayer::Tile(layer) => layer
                        .attr
                        .color_anim
                        .is_none_or(|anim| anim < animations.color.len())
                        .then_some(())
                        .ok_or_else(|| anyhow!("color anim is out of bounds"))
                        .and(
                            layer
                                .attr
                                .image_array
                                .is_none_or(|img| img < resources.image_arrays.len())
                                .then_some(())
                                .ok_or_else(|| anyhow!("image index is out of bounds")),
                        ),
                    MapLayer::Quad(layer) => layer
                        .quads
                        .iter()
                        .all(|quad| {
                            quad.color_anim
                                .is_none_or(|anim| anim < animations.color.len())
                        })
                        .then_some(())
                        .ok_or_else(|| anyhow!("color anim is out of bounds for a quad"))
                        .and(
                            layer
                                .quads
                                .iter()
                                .all(|quad| {
                                    quad.pos_anim.is_none_or(|anim| anim < animations.pos.len())
                                })
                                .then_some(())
                                .ok_or_else(|| anyhow!("pos anim is out of bounds for a quad")),
                        )
                        .and(
                            layer
                                .attr
                                .image
                                .is_none_or(|img| img < resources.images.len())
                                .then_some(())
                                .ok_or_else(|| anyhow!("image index is out of bounds")),
                        ),
                    MapLayer::Sound(layer) => layer
                        .sounds
                        .iter()
                        .all(|sound| {
                            sound
                                .sound_anim
                                .is_none_or(|anim| anim < animations.sound.len())
                        })
                        .then_some(())
                        .ok_or_else(|| anyhow!("sound anim is out of bounds for a sound"))
                        .and(
                            layer
                                .sounds
                                .iter()
                                .all(|sound| {
                                    sound
                                        .pos_anim
                                        .is_none_or(|anim| anim < animations.pos.len())
                                })
                                .then_some(())
                                .ok_or_else(|| anyhow!("pos anim is out of bounds for a sound")),
                        )
                        .and(
                            layer
                                .attr
                                .sound
                                .is_none_or(|snd| snd < resources.sounds.len())
                                .then_some(())
                                .ok_or_else(|| anyhow!("sound index is out of bounds")),
                        ),
                }?
            }
        }

        Ok(())
    }

    /// Deserializes the resources and returns the amount of bytes read
    pub fn deserialize_resources(uncompressed_file: &[u8]) -> anyhow::Result<(Resources, usize)> {
        Ok(bincode::serde::decode_from_slice::<Resources, _>(
            uncompressed_file,
            bincode::config::standard(),
        )?)
    }

    /// Decompresses the resources. Returns the amount of bytes read
    pub fn decompress_resources(file: &[u8]) -> anyhow::Result<(Vec<u8>, usize)> {
        crate::utils::decompress(file)
    }

    /// Deserializes the animations and returns the amount of bytes read
    pub fn deserialize_animations(uncompressed_file: &[u8]) -> anyhow::Result<(Animations, usize)> {
        Ok(bincode::serde::decode_from_slice::<Animations, _>(
            uncompressed_file,
            bincode::config::standard(),
        )?)
    }

    /// Decompresses the animations. Returns the amount of bytes read
    pub fn decompress_animations(file: &[u8]) -> anyhow::Result<(Vec<u8>, usize)> {
        crate::utils::decompress(file)
    }

    /// Deserializes the config and returns the amount of bytes read
    pub fn deserialize_config(uncompressed_file: &[u8]) -> anyhow::Result<(Config, usize)> {
        Ok(bincode::serde::decode_from_slice::<Config, _>(
            uncompressed_file,
            bincode::config::standard(),
        )?)
    }

    /// Decompresses the config. Returns the amount of bytes read
    pub fn decompress_config(file: &[u8]) -> anyhow::Result<(Vec<u8>, usize)> {
        crate::utils::decompress(file)
    }

    /// Deserializes the meta data and returns the amount of bytes read
    pub fn deserialize_meta(uncompressed_file: &[u8]) -> anyhow::Result<(Metadata, usize)> {
        Ok(bincode::serde::decode_from_slice::<Metadata, _>(
            uncompressed_file,
            bincode::config::standard(),
        )?)
    }

    /// Decompresses the meta data. Returns the amount of bytes read
    pub fn decompress_meta(file: &[u8]) -> anyhow::Result<(Vec<u8>, usize)> {
        crate::utils::decompress(file)
    }

    /// Read the map resources. Returns the number of bytes read.
    pub fn read_resources(file: &[u8]) -> anyhow::Result<(Resources, usize)> {
        let (resources_file, read_bytes_res) = Self::decompress_resources(file)?;
        let (resources, _) = Self::deserialize_resources(&resources_file)?;
        Ok((resources, read_bytes_res))
    }

    /// All maps that the client knows MUST start with "twmap", even if the version changes etc.
    pub fn validate_twmap_header(file: &[u8]) -> bool {
        let twmap_str_len = Self::FILE_TY.len();
        file.len() >= twmap_str_len
            && String::from_utf8_lossy(&file[..Self::FILE_TY.len()]) == Self::FILE_TY
    }

    /// Read the map resources (and the file header). Returns the number of bytes read.
    pub fn read_resources_and_header(file: &[u8]) -> anyhow::Result<(Resources, usize)> {
        let header_len = Self::FILE_TY.len() + std::mem::size_of::<u64>();
        anyhow::ensure!(
            file.len() >= header_len && Self::validate_twmap_header(file),
            "file smaller than the size of the header."
        );
        anyhow::ensure!(
            u64::from_le_bytes(
                file[Self::FILE_TY.len()..Self::FILE_TY.len() + std::mem::size_of::<u64>()]
                    .try_into()?
            ) == Self::VERSION,
            "file version mismatch."
        );
        let file = &file[header_len..];

        let (resources_file, read_bytes_res) = Self::decompress_resources(file)?;
        let (resources, _) = Self::deserialize_resources(&resources_file)?;
        Ok((resources, read_bytes_res + header_len))
    }

    /// Read the map animations. Returns the number of bytes read.
    pub fn read_animations(file: &[u8]) -> anyhow::Result<(Animations, usize)> {
        let (animations_file, read_bytes_res) = Self::decompress_resources(file)?;
        let (animations, _) = Self::deserialize_animations(&animations_file)?;
        Ok((animations, read_bytes_res))
    }

    /// Skip the map animations. Returns the number of bytes read.
    pub fn skip_animations(file: &[u8]) -> anyhow::Result<usize> {
        let (size, bytes_read) = super::utils::compressed_size(file)?;
        Ok(size as usize + bytes_read)
    }

    /// Read the map config. Returns the number of bytes read.
    pub fn read_config(file: &[u8]) -> anyhow::Result<(Config, usize)> {
        let (config_file, read_bytes_res) = Self::decompress_resources(file)?;
        let (config, _) = Self::deserialize_config(&config_file)?;
        Ok((config, read_bytes_res))
    }

    /// Read the map meta data. Returns the number of bytes read.
    pub fn read_meta(file: &[u8]) -> anyhow::Result<(Metadata, usize)> {
        let (meta_file, read_bytes_res) = Self::decompress_resources(file)?;
        let (meta_data, _) = Self::deserialize_meta(&meta_file)?;
        Ok((meta_data, read_bytes_res))
    }

    /// Read a map file
    pub fn read(file: &[u8], tp: &rayon::ThreadPool) -> anyhow::Result<Self> {
        let header_len = Self::FILE_TY.len() + std::mem::size_of::<u64>();
        anyhow::ensure!(
            file.len() >= header_len && Self::validate_twmap_header(file),
            "file smaller than the size of the header."
        );
        anyhow::ensure!(
            u64::from_le_bytes(
                file[Self::FILE_TY.len()..Self::FILE_TY.len() + std::mem::size_of::<u64>()]
                    .try_into()?
            ) == Self::VERSION,
            "file version mismatch."
        );
        let file = &file[header_len..];

        let (resources, read_bytes_res) = Self::read_resources(file)?;

        let (groups, read_bytes_groups) = MapGroups::read(&file[read_bytes_res..], tp)?;
        let (animations, read_bytes_animations) =
            Self::read_animations(&file[read_bytes_res + read_bytes_groups..])?;
        let (config, read_bytes_config) =
            Self::read_config(&file[read_bytes_res + read_bytes_groups + read_bytes_animations..])?;
        let (meta, _) = Self::read_meta(
            &file[read_bytes_res + read_bytes_groups + read_bytes_animations + read_bytes_config..],
        )?;

        Self::validate_resource_and_anim_indices(&resources, &animations, &groups)?;

        Ok(Self {
            resources,
            groups,
            animations,
            config,
            meta,
        })
    }

    /// Read only the physics group and the config (skips all other stuff).
    ///
    /// This is usually nice to use on the server.
    pub fn read_physics_group_and_config(file: &[u8]) -> anyhow::Result<(MapGroupPhysics, Config)> {
        let header_len = Self::FILE_TY.len() + std::mem::size_of::<u64>();
        anyhow::ensure!(
            file.len() >= header_len && Self::validate_twmap_header(file),
            "file smaller than the size of the header."
        );
        anyhow::ensure!(
            u64::from_le_bytes(
                file[Self::FILE_TY.len()..Self::FILE_TY.len() + std::mem::size_of::<u64>()]
                    .try_into()?
            ) == Self::VERSION,
            "file version mismatch."
        );
        let file = &file[header_len..];

        // size of resources + the size information itself
        let (resource_size, read_size) = compressed_size(file)?;

        let file = &file[resource_size as usize + read_size..];
        let (groups, bytes_read) = MapGroups::read_physics_group(file)?;

        let file = &file[bytes_read..];
        let bytes_read = MapGroups::skip_design_group(file)?;

        let file = &file[bytes_read..];
        let read_bytes_animations = Self::skip_animations(file)?;
        let file = &file[read_bytes_animations..];
        let (config, _) = Self::read_config(file)?;

        Ok((groups, config))
    }

    /// Read a map file, whos resources were already loaded (the file header was read/checked too).
    /// See [`Map::read_resources_and_header`]
    pub fn read_with_resources(
        resources: Resources,
        file_without_res: &[u8],
        tp: &rayon::ThreadPool,
    ) -> anyhow::Result<Self> {
        let (groups, read_bytes_groups) = MapGroups::read(file_without_res, tp)?;

        let (animations, read_bytes_animations) =
            Self::read_animations(&file_without_res[read_bytes_groups..])?;
        let (config, read_bytes_config) =
            Self::read_config(&file_without_res[read_bytes_groups + read_bytes_animations..])?;
        let (meta, _) = Self::read_meta(
            &file_without_res[read_bytes_groups + read_bytes_animations + read_bytes_config..],
        )?;

        Self::validate_resource_and_anim_indices(&resources, &animations, &groups)?;

        Ok(Self {
            resources,
            groups,
            animations,
            config,
            meta,
        })
    }

    /// Serializes the resources and returns the amount of bytes written
    pub fn serialize_resources<W: std::io::Write>(
        res: &Resources,
        writer: &mut W,
    ) -> anyhow::Result<usize> {
        Ok(bincode::serde::encode_into_std_write(
            res,
            writer,
            bincode::config::standard(),
        )?)
    }

    pub fn compress_resources<W: std::io::Write>(
        uncompressed_file: &[u8],
        writer: &mut W,
    ) -> anyhow::Result<()> {
        crate::utils::compress(uncompressed_file, writer)
    }

    /// Serializes the animations and returns the amount of bytes written
    pub fn serialize_animations<W: std::io::Write>(
        anims: &Animations,
        writer: &mut W,
    ) -> anyhow::Result<usize> {
        Ok(bincode::serde::encode_into_std_write(
            anims,
            writer,
            bincode::config::standard(),
        )?)
    }

    pub fn compress_animations<W: std::io::Write>(
        uncompressed_file: &[u8],
        writer: &mut W,
    ) -> anyhow::Result<()> {
        crate::utils::compress(uncompressed_file, writer)
    }

    /// Serializes the config and returns the amount of bytes written
    pub fn serialize_config<W: std::io::Write>(
        config: &Config,
        writer: &mut W,
    ) -> anyhow::Result<usize> {
        Ok(bincode::serde::encode_into_std_write(
            config,
            writer,
            bincode::config::standard(),
        )?)
    }

    pub fn compress_config<W: std::io::Write>(
        uncompressed_file: &[u8],
        writer: &mut W,
    ) -> anyhow::Result<()> {
        crate::utils::compress(uncompressed_file, writer)
    }

    /// Serializes the meta and returns the amount of bytes written
    pub fn serialize_meta<W: std::io::Write>(
        meta_data: &Metadata,
        writer: &mut W,
    ) -> anyhow::Result<usize> {
        Ok(bincode::serde::encode_into_std_write(
            meta_data,
            writer,
            bincode::config::standard(),
        )?)
    }

    pub fn compress_meta<W: std::io::Write>(
        uncompressed_file: &[u8],
        writer: &mut W,
    ) -> anyhow::Result<()> {
        crate::utils::compress(uncompressed_file, writer)
    }

    /// Write a map file to a writer
    pub fn write<W: std::io::Write>(
        &self,
        writer: &mut W,
        tp: &rayon::ThreadPool,
    ) -> anyhow::Result<()> {
        let (resources, groups, animations, config, meta) = tp.install(|| {
            join_all!(
                || {
                    let mut resources: Vec<u8> = Vec::new();
                    let mut serializer_helper: Vec<u8> = Default::default();
                    Self::serialize_resources(&self.resources, &mut serializer_helper)?;
                    Self::compress_resources(&serializer_helper, &mut resources)?;
                    anyhow::Ok(resources)
                },
                || {
                    let mut groups: Vec<u8> = Vec::new();
                    MapGroups::write(&self.groups, &mut groups, tp)?;
                    anyhow::Ok(groups)
                },
                || {
                    let mut animations: Vec<u8> = Vec::new();
                    let mut serializer_helper: Vec<u8> = Default::default();
                    Self::serialize_animations(&self.animations, &mut serializer_helper)?;
                    Self::compress_animations(&serializer_helper, &mut animations)?;
                    anyhow::Ok(animations)
                },
                || {
                    let mut config: Vec<u8> = Vec::new();
                    let mut serializer_helper: Vec<u8> = Default::default();
                    Self::serialize_config(&self.config, &mut serializer_helper)?;
                    Self::compress_config(&serializer_helper, &mut config)?;
                    anyhow::Ok(config)
                },
                || {
                    let mut meta_data: Vec<u8> = Vec::new();
                    let mut serializer_helper: Vec<u8> = Default::default();
                    Self::serialize_meta(&self.meta, &mut serializer_helper)?;
                    Self::compress_meta(&serializer_helper, &mut meta_data)?;
                    anyhow::Ok(meta_data)
                }
            )
        });

        writer.write_all(Self::FILE_TY.as_bytes())?;
        writer.write_all(&Self::VERSION.to_le_bytes())?;
        writer.write_all(&resources?)?;
        writer.write_all(&groups?)?;
        writer.write_all(&animations?)?;
        writer.write_all(&config?)?;
        writer.write_all(&meta?)?;

        Ok(())
    }

    /// generates the blake3 hash for the given slice
    pub fn generate_hash_for(data: &[u8]) -> Hash {
        generate_hash_for(data)
    }

    /// Split name & hash from a file name.
    /// This even works, if the file name never contained
    /// the hash in first place.
    /// The given name should always be without extension.
    /// It also works for resources.
    /// E.g. mymap_<HASH> => (mymap, <HASH>)
    pub fn name_and_hash(name: &str, file: &[u8]) -> (String, Hash) {
        name_and_hash(name, file)
    }

    pub fn as_json(&self) -> String {
        #[derive(Debug, Serialize, Deserialize)]
        struct MapGroupAsJson {
            pub physics: MapGroupPhysics,

            pub background: Vec<MapGroup>,
            pub foreground: Vec<MapGroup>,
        }
        #[derive(Debug, Serialize, Deserialize)]
        struct MapAsJson {
            pub resources: Resources,
            pub groups: MapGroupAsJson,
            pub animations: Animations,
            pub config: Config,
            pub meta: Metadata,
        }

        serde_json::to_string_pretty(&MapAsJson {
            resources: self.resources.clone(),
            groups: MapGroupAsJson {
                physics: self.groups.physics.clone(),
                background: self.groups.background.clone(),
                foreground: self.groups.foreground.clone(),
            },
            animations: self.animations.clone(),
            config: self.config.clone(),
            meta: self.meta.clone(),
        })
        .unwrap()
    }
}
