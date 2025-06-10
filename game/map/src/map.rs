pub mod animations;
pub mod command_value;
pub mod config;
pub mod groups;
pub mod metadata;
pub mod resources;

use std::path::{Path, PathBuf};

use anyhow::anyhow;
use assets_base::{
    tar::{new_tar, tar_add_file, tar_entry_to_file},
    verify::{json::verify_json, ogg_vorbis::verify_ogg_vorbis, txt::verify_txt},
};
use base::{
    hash::{generate_hash_for, name_and_hash, Hash},
    join_all,
};
use groups::layers::design::MapLayer;
use hiarc::Hiarc;
use image_utils::png::is_png_image_valid;
pub use image_utils::png::PngValidatorOptions;
use serde::{Deserialize, Serialize};

use crate::{
    file::MapFileReader,
    header::Header,
    map::groups::{MapGroup, MapGroupPhysics},
    utils::{deserialize_twmap_bincode, serialize_twmap_bincode, verify_twmap_bincode},
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
/// To make it easy use [`Map::read`] &  [`Map::write`], which automatically de-/serializes and compresses the map components.
///
/// ### De-/serialization notes
/// A map file file must contain a [`Header`], whos type is of "twmap".
/// If the whole map is not needed, it's also possible to only load parts of the map (e.g. only the physics group).
#[derive(Debug, Hiarc, Clone)]
pub struct Map {
    pub resources: Resources,
    pub groups: MapGroups,
    pub animations: Animations,

    pub config: Config,
    pub meta: Metadata,
}

impl Map {
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

    /// Deserializes the header
    pub fn deserialize_header(uncompressed_file: &[u8]) -> anyhow::Result<Header> {
        let file = String::from_utf8(uncompressed_file.into())?;
        let mut lines = file.lines();
        Ok(Header {
            ty: lines
                .next()
                .ok_or_else(|| anyhow!("type in header is missing"))?
                .into(),
            version: lines
                .next()
                .ok_or_else(|| anyhow!("version in header is missing"))?
                .parse()?,
        })
    }

    /// Deserializes the resources
    pub fn deserialize_resources(uncompressed_file: &[u8]) -> anyhow::Result<Resources> {
        Ok(serde_json::from_slice::<Resources>(uncompressed_file)?)
    }

    /// Decompresses the resources.
    pub fn decompress_resources(file: &[u8]) -> anyhow::Result<Vec<u8>> {
        crate::utils::decompress(file)
    }

    /// Deserializes the animations
    pub fn deserialize_animations(uncompressed_file: &[u8]) -> anyhow::Result<Animations> {
        deserialize_twmap_bincode::<Animations>(uncompressed_file)
    }

    /// Decompresses the animations.
    pub fn decompress_animations(file: &[u8]) -> anyhow::Result<Vec<u8>> {
        crate::utils::decompress(file)
    }

    /// Deserializes the config
    pub fn deserialize_config(uncompressed_file: &[u8]) -> anyhow::Result<Config> {
        Ok(serde_json::from_slice::<Config>(uncompressed_file)?)
    }

    /// Decompresses the config.
    pub fn decompress_config(file: &[u8]) -> anyhow::Result<Vec<u8>> {
        crate::utils::decompress(file)
    }

    /// Deserializes the meta data
    pub fn deserialize_meta(uncompressed_file: &[u8]) -> anyhow::Result<Metadata> {
        Ok(serde_json::from_slice::<Metadata>(uncompressed_file)?)
    }

    /// Decompresses the meta data.
    pub fn decompress_meta(file: &[u8]) -> anyhow::Result<Vec<u8>> {
        crate::utils::decompress(file)
    }

    /// Read the map resources.
    pub fn read_resources(reader: &MapFileReader) -> anyhow::Result<Resources> {
        let file = tar_entry_to_file(
            reader
                .entries
                .get(Path::new("resource_index.json.zst"))
                .ok_or_else(|| anyhow!("resource index was not found in map file"))?,
        )?;
        let resources_file = Self::decompress_resources(file)?;
        let resources = Self::deserialize_resources(&resources_file)?;
        Ok(resources)
    }

    /// All maps that the client knows MUST be of type "twmap", even if the version changes etc.
    pub fn validate_twmap_header_type(header: &Header) -> bool {
        header.ty == Header::FILE_TY
    }

    /// All maps that the client knows MUST be of type "twmap", even if the version changes etc.
    pub fn read_twmap_header(reader: &MapFileReader) -> anyhow::Result<Header> {
        let file = tar_entry_to_file(
            reader
                .entries
                .get(Path::new("header.txt"))
                .ok_or_else(|| anyhow!("header was not found in map file"))?,
        )?;
        let header = Self::deserialize_header(file)?;
        Ok(header)
    }

    /// Read the map resources (and validate the file header).
    pub fn read_resources_and_header(reader: &MapFileReader) -> anyhow::Result<Resources> {
        let header = Self::read_twmap_header(reader)?;
        anyhow::ensure!(
            Self::validate_twmap_header_type(&header),
            "header validation failed."
        );
        anyhow::ensure!(header.version == Header::VERSION, "file version mismatch.");

        let resources = Self::read_resources(reader)?;
        Ok(resources)
    }

    /// Read the map animations.
    pub fn read_animations(reader: &MapFileReader) -> anyhow::Result<Animations> {
        let file = tar_entry_to_file(
            reader
                .entries
                .get(Path::new("animations.twmap_bincode.zst"))
                .ok_or_else(|| anyhow!("animations were not found in map file"))?,
        )?;
        let animations_file = Self::decompress_animations(file)?;
        let animations = Self::deserialize_animations(&animations_file)?;
        Ok(animations)
    }

    /// Read the map config.
    pub fn read_config(reader: &MapFileReader) -> anyhow::Result<Config> {
        let file = tar_entry_to_file(
            reader
                .entries
                .get(Path::new("config.json.zst"))
                .ok_or_else(|| anyhow!("config was not found in map file"))?,
        )?;
        let config_file = Self::decompress_config(file)?;
        let config = Self::deserialize_config(&config_file)?;
        Ok(config)
    }

    /// Read the map meta data.
    pub fn read_meta(reader: &MapFileReader) -> anyhow::Result<Metadata> {
        let file = tar_entry_to_file(
            reader
                .entries
                .get(Path::new("meta.json.zst"))
                .ok_or_else(|| anyhow!("meta was not found in map file"))?,
        )?;
        let meta_file = Self::decompress_resources(file)?;
        let meta_data = Self::deserialize_meta(&meta_file)?;
        Ok(meta_data)
    }

    /// Read a map file
    pub fn read(reader: &MapFileReader, tp: &rayon::ThreadPool) -> anyhow::Result<Self> {
        let header = Self::read_twmap_header(reader)?;
        anyhow::ensure!(
            Self::validate_twmap_header_type(&header),
            "header validation failed."
        );
        anyhow::ensure!(header.version == Header::VERSION, "file version mismatch.");

        let resources = Self::read_resources(reader)?;

        let groups = MapGroups::read(reader, tp)?;
        let animations = Self::read_animations(reader)?;
        let config = Self::read_config(reader)?;
        let meta = Self::read_meta(reader)?;

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
    pub fn read_physics_group_and_config(
        reader: &MapFileReader,
    ) -> anyhow::Result<(MapGroupPhysics, Config)> {
        let header = Self::read_twmap_header(reader)?;
        anyhow::ensure!(
            Self::validate_twmap_header_type(&header),
            "header validation failed."
        );
        anyhow::ensure!(header.version == Header::VERSION, "file version mismatch.");

        let groups = MapGroups::read_physics_group(reader)?;

        let config = Self::read_config(reader)?;

        Ok((groups, config))
    }

    /// Read a map file, whos resources were already loaded (the file header was read/checked too).
    /// See [`Map::read_resources_and_header`]
    pub fn read_with_resources(
        resources: Resources,
        reader: &MapFileReader,
        tp: &rayon::ThreadPool,
    ) -> anyhow::Result<Self> {
        let groups = MapGroups::read(reader, tp)?;

        let animations = Self::read_animations(reader)?;
        let config = Self::read_config(reader)?;
        let meta = Self::read_meta(reader)?;

        Self::validate_resource_and_anim_indices(&resources, &animations, &groups)?;

        Ok(Self {
            resources,
            groups,
            animations,
            config,
            meta,
        })
    }

    /// Serializes the header
    pub fn serialize_header<W: std::io::Write>(res: &Header, writer: &mut W) -> anyhow::Result<()> {
        let ty = format!("{}\n", res.ty).into_bytes();
        let version = format!("{}\n", res.version).into_bytes();
        writer.write_all(&ty)?;
        writer.write_all(&version)?;
        Ok(())
    }

    /// Serializes the resources
    pub fn serialize_resources<W: std::io::Write>(
        res: &Resources,
        writer: &mut W,
    ) -> anyhow::Result<()> {
        serde_json::to_writer_pretty(writer, res)?;
        Ok(())
    }

    pub fn compress_resources(uncompressed_file: &[u8]) -> anyhow::Result<Vec<u8>> {
        crate::utils::compress(uncompressed_file)
    }

    /// Serializes the animations and returns the amount of bytes written
    pub fn serialize_animations<W: std::io::Write>(
        anims: &Animations,
        writer: &mut W,
    ) -> anyhow::Result<usize> {
        serialize_twmap_bincode(anims, writer)
    }

    pub fn compress_animations(uncompressed_file: &[u8]) -> anyhow::Result<Vec<u8>> {
        crate::utils::compress(uncompressed_file)
    }

    /// Serializes the config
    pub fn serialize_config<W: std::io::Write>(
        config: &Config,
        writer: &mut W,
    ) -> anyhow::Result<()> {
        serde_json::to_writer_pretty(writer, config)?;
        Ok(())
    }

    pub fn compress_config(uncompressed_file: &[u8]) -> anyhow::Result<Vec<u8>> {
        crate::utils::compress(uncompressed_file)
    }

    /// Serializes the meta
    pub fn serialize_meta<W: std::io::Write>(
        meta_data: &Metadata,
        writer: &mut W,
    ) -> anyhow::Result<()> {
        serde_json::to_writer_pretty(writer, meta_data)?;
        Ok(())
    }

    pub fn compress_meta(uncompressed_file: &[u8]) -> anyhow::Result<Vec<u8>> {
        crate::utils::compress(uncompressed_file)
    }

    /// Write a map file to a writer
    pub fn write(&self, tp: &rayon::ThreadPool) -> anyhow::Result<Vec<u8>> {
        let (header, resources, groups, animations, config, meta) = tp.install(|| {
            join_all!(
                || {
                    let mut serializer_helper: Vec<u8> = Default::default();
                    Self::serialize_header(
                        &Header {
                            ty: Header::FILE_TY.to_string(),
                            version: Header::VERSION,
                        },
                        &mut serializer_helper,
                    )?;
                    anyhow::Ok(serializer_helper)
                },
                || {
                    let mut serializer_helper: Vec<u8> = Default::default();
                    Self::serialize_resources(&self.resources, &mut serializer_helper)?;
                    Self::compress_resources(&serializer_helper)
                },
                || { MapGroups::write(&self.groups, tp) },
                || {
                    let mut serializer_helper: Vec<u8> = Default::default();
                    Self::serialize_animations(&self.animations, &mut serializer_helper)?;
                    Self::compress_animations(&serializer_helper)
                },
                || {
                    let mut serializer_helper: Vec<u8> = Default::default();
                    Self::serialize_config(&self.config, &mut serializer_helper)?;
                    Self::compress_config(&serializer_helper)
                },
                || {
                    let mut serializer_helper: Vec<u8> = Default::default();
                    Self::serialize_meta(&self.meta, &mut serializer_helper)?;
                    Self::compress_meta(&serializer_helper)
                }
            )
        });

        let mut builder = new_tar();

        tar_add_file(&mut builder, "header.txt", &header?);
        tar_add_file(&mut builder, "resource_index.json.zst", &resources?);

        let (physics, bg, fg) = groups?;
        tar_add_file(&mut builder, "groups/physics.twmap_bincode.zst", &physics);
        tar_add_file(&mut builder, "groups/background.twmap_bincode.zst", &bg);
        tar_add_file(&mut builder, "groups/foreground.twmap_bincode.zst", &fg);

        tar_add_file(&mut builder, "animations.twmap_bincode.zst", &animations?);
        tar_add_file(&mut builder, "config.json.zst", &config?);
        tar_add_file(&mut builder, "meta.json.zst", &meta?);

        Ok(builder.into_inner()?)
    }

    /// Validate a downloaded map's entries.
    ///
    /// This includes:
    /// - image files
    /// - text files
    /// - sound files
    /// - the map header (without a version check)
    /// - twmap_bincode files (the map internal bincode file format), __BUT__
    ///   it does not check it's file contents (it does not deserialize it).
    /// - zstd file (the content must be one of the above types)
    pub fn validate_downloaded_map_file(
        reader: &MapFileReader,
        png_options: PngValidatorOptions,
    ) -> anyhow::Result<()> {
        let header = Self::read_twmap_header(reader)?;
        anyhow::ensure!(
            Self::validate_twmap_header_type(&header),
            "header validation failed."
        );

        let files = reader.read_all()?;

        for (path, file) in files {
            fn verify_file(
                path: PathBuf,
                file: Vec<u8>,
                png_options: PngValidatorOptions,
            ) -> anyhow::Result<()> {
                let file_ext = path
                    .extension()
                    .ok_or_else(|| {
                        anyhow!(
                            "no file extension found during \
                            downloaded map validation."
                        )
                    })?
                    .to_str()
                    .ok_or_else(|| {
                        anyhow!(
                            "file extension check during downloaded \
                            map contained invalid characters"
                        )
                    })?;
                let file_name = path
                    .file_stem()
                    .ok_or_else(|| {
                        anyhow!(
                            "no file stem found during \
                            downloaded map validation."
                        )
                    })?
                    .to_str()
                    .ok_or_else(|| {
                        anyhow!(
                            "file steam check during downloaded \
                            map contained invalid characters"
                        )
                    })?;
                match file_ext {
                    "ogg" => verify_ogg_vorbis(&file)?,
                    "png" => is_png_image_valid(&file, png_options)?,
                    "txt" => verify_txt(&file, file_name)?,
                    "json" => verify_json(&file)?,
                    "twmap_bincode" => verify_twmap_bincode(&file)?,
                    "zst" => {
                        let file = crate::utils::decompress(&file)?;
                        verify_file(file_name.into(), file, png_options)?;
                    }
                    _ => anyhow::bail!(
                        "file extension: {} is unknown and cannot be validated.",
                        file_ext
                    ),
                }
                Ok(())
            }
            verify_file(path, file, png_options)?;
        }

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
