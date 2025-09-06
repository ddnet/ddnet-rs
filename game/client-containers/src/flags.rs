use std::{path::Path, sync::Arc};

use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::texture::texture::{GraphicsTextureHandle, TextureContainer},
};
use hiarc::Hiarc;
use rustc_hash::FxHashMap;
use sound::{sound_handle::SoundObjectHandle, sound_mt::SoundMultiThreaded};

use crate::container::{ContainerLoadedItem, ContainerLoadedItemDir, load_file_part_and_upload};

use super::container::{Container, ContainerItemLoadData, ContainerLoad};

#[derive(Debug, Hiarc, Clone)]
pub struct Flags {
    pub flags: FxHashMap<String, TextureContainer>,
}

impl Flags {
    pub fn get_or_default(&self, name: &str) -> &TextureContainer {
        self.flags
            .get(&name.to_ascii_uppercase().replace("_", "-"))
            .unwrap_or_else(|| {
                // loading makes sure ddnet is always a safe choice
                self.flags.get("default").unwrap()
            })
    }
}

#[derive(Debug, Hiarc)]
pub struct LoadFlags {
    flags: FxHashMap<String, ContainerItemLoadData>,

    flags_name: String,
}

impl LoadFlags {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        files: ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        _runtime_thread_pool: &Arc<rayon::ThreadPool>,
        flags_name: &str,
    ) -> anyhow::Result<Self> {
        let load_flags = |files: &ContainerLoadedItemDir,
                          extra_filter: &dyn Fn(&String) -> bool|
         -> anyhow::Result<FxHashMap<_, _>> {
            files
                .files
                .iter()
                .filter_map(|(name, _)| {
                    if name.parent().is_some_and(|p| p.eq(Path::new("")))
                        && !name.is_absolute()
                        && name.file_stem().is_some_and(|n| n.to_str().is_some())
                        && !name.has_root()
                    {
                        Some(name.file_stem().unwrap().to_str().unwrap().to_string())
                    } else {
                        None
                    }
                })
                .filter(|name| extra_filter(name))
                .map(|name| {
                    anyhow::Ok((
                        name.clone(),
                        load_file_part_and_upload(
                            graphics_mt,
                            files,
                            default_files,
                            flags_name,
                            &[],
                            &name,
                        )?
                        .img,
                    ))
                })
                .collect::<anyhow::Result<_>>()
        };
        let mut flags = load_flags(&files, &|_| true)?;
        let default_flags = load_flags(default_files, &|name| !flags.contains_key(name))?;
        flags.extend(default_flags);
        anyhow::ensure!(
            !flags.is_empty() && flags.contains_key("default"),
            "no flags textures found"
        );

        Ok(Self {
            flags,

            flags_name: flags_name.to_string(),
        })
    }

    fn load_file_into_texture(
        texture_handle: &GraphicsTextureHandle,
        img: ContainerItemLoadData,
        name: &str,
    ) -> TextureContainer {
        texture_handle.load_texture_rgba_u8(img.data, name).unwrap()
    }
}

impl ContainerLoad<Flags> for LoadFlags {
    fn load(
        item_name: &str,
        files: ContainerLoadedItem,
        default_files: &ContainerLoadedItemDir,
        runtime_thread_pool: &Arc<rayon::ThreadPool>,
        graphics_mt: &GraphicsMultiThreaded,
        _sound_mt: &SoundMultiThreaded,
    ) -> anyhow::Result<Self> {
        match files {
            ContainerLoadedItem::Directory(files) => Self::new(
                graphics_mt,
                files,
                default_files,
                runtime_thread_pool,
                item_name,
            ),
            ContainerLoadedItem::SingleFile(_) => Err(anyhow::anyhow!(
                "single file mode is currently not supported"
            )),
        }
    }

    fn convert(
        self,
        texture_handle: &GraphicsTextureHandle,
        _sound_object_handle: &SoundObjectHandle,
    ) -> Flags {
        Flags {
            flags: self
                .flags
                .into_iter()
                .map(|(name, flags)| {
                    (
                        name,
                        Self::load_file_into_texture(texture_handle, flags, &self.flags_name),
                    )
                })
                .collect(),
        }
    }
}

pub type FlagsContainer = Container<Flags, LoadFlags>;
pub const FLAGS_CONTAINER_PATH: &str = "flags/";
