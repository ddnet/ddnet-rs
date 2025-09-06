use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::texture::texture::{GraphicsTextureHandle, TextureContainer2dArray},
};
use hiarc::Hiarc;
use rustc_hash::FxHashMap;
use sound::{sound_handle::SoundObjectHandle, sound_mt::SoundMultiThreaded};

use crate::container::{ContainerLoadedItem, ContainerLoadedItemDir};

use super::container::{
    Container, ContainerItemLoadData, ContainerLoad, load_file_part_and_convert_3d_and_upload,
};

#[derive(Debug, Hiarc, Clone)]
pub struct Entities {
    pub physics: FxHashMap<String, TextureContainer2dArray>,
    pub speedup: TextureContainer2dArray,
    pub text_overlay_top: TextureContainer2dArray,
    pub text_overlay_bottom: TextureContainer2dArray,
    pub text_overlay_center: TextureContainer2dArray,
}

impl Entities {
    pub fn get_or_default(&self, name: &str) -> &TextureContainer2dArray {
        self.physics.get(name).unwrap_or_else(|| {
            // loading makes sure ddnet is always a safe choice
            self.physics.get("ddnet").unwrap()
        })
    }
}

#[derive(Debug, Hiarc)]
pub struct LoadEntities {
    physics: FxHashMap<String, ContainerItemLoadData>,
    speedup: ContainerItemLoadData,
    text_overlay_top: ContainerItemLoadData,
    text_overlay_bottom: ContainerItemLoadData,
    text_overlay_center: ContainerItemLoadData,

    entities_name: String,
}

impl LoadEntities {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        files: ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        runtime_thread_pool: &Arc<rayon::ThreadPool>,
        entities_name: &str,
    ) -> anyhow::Result<Self> {
        let load_physics = |files: &ContainerLoadedItemDir,
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
                        load_file_part_and_convert_3d_and_upload(
                            graphics_mt,
                            runtime_thread_pool,
                            files,
                            default_files,
                            entities_name,
                            &[],
                            &name,
                        )?
                        .img,
                    ))
                })
                .collect::<anyhow::Result<_>>()
        };
        let mut physics = load_physics(&files, &|_| true)?;
        let default_physics = load_physics(default_files, &|name| !physics.contains_key(name))?;
        physics.extend(default_physics);
        anyhow::ensure!(
            !physics.is_empty() && physics.contains_key("vanilla") && physics.contains_key("ddnet"),
            "no physics textures found"
        );

        Ok(Self {
            text_overlay_top: load_file_part_and_convert_3d_and_upload(
                graphics_mt,
                runtime_thread_pool,
                &files,
                default_files,
                entities_name,
                &["text_overlay"],
                "top",
            )?
            .img,
            text_overlay_bottom: load_file_part_and_convert_3d_and_upload(
                graphics_mt,
                runtime_thread_pool,
                &files,
                default_files,
                entities_name,
                &["text_overlay"],
                "bottom",
            )?
            .img,
            text_overlay_center: load_file_part_and_convert_3d_and_upload(
                graphics_mt,
                runtime_thread_pool,
                &files,
                default_files,
                entities_name,
                &["text_overlay"],
                "center",
            )?
            .img,
            speedup: load_file_part_and_convert_3d_and_upload(
                graphics_mt,
                runtime_thread_pool,
                &files,
                default_files,
                entities_name,
                &["special"],
                "speedup",
            )?
            .img,
            physics,

            entities_name: entities_name.to_string(),
        })
    }

    fn load_file_into_texture(
        texture_handle: &GraphicsTextureHandle,
        img: ContainerItemLoadData,
        name: &str,
    ) -> TextureContainer2dArray {
        texture_handle
            .load_texture_2d_array_rgba_u8(img.data, name)
            .unwrap()
    }
}

impl ContainerLoad<Entities> for LoadEntities {
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
            ContainerLoadedItem::SingleFile(file) => {
                let mut files: FxHashMap<PathBuf, Vec<u8>> = Default::default();
                files.insert("ddnet.png".into(), file);
                let files = ContainerLoadedItemDir::new(files);
                Self::new(
                    graphics_mt,
                    files,
                    default_files,
                    runtime_thread_pool,
                    item_name,
                )
            }
        }
    }

    fn convert(
        self,
        texture_handle: &GraphicsTextureHandle,
        _sound_object_handle: &SoundObjectHandle,
    ) -> Entities {
        Entities {
            physics: self
                .physics
                .into_iter()
                .map(|(name, physics)| {
                    (
                        name,
                        Self::load_file_into_texture(texture_handle, physics, &self.entities_name),
                    )
                })
                .collect(),
            speedup: Self::load_file_into_texture(
                texture_handle,
                self.speedup,
                &self.entities_name,
            ),
            text_overlay_top: Self::load_file_into_texture(
                texture_handle,
                self.text_overlay_top,
                &self.entities_name,
            ),
            text_overlay_bottom: Self::load_file_into_texture(
                texture_handle,
                self.text_overlay_bottom,
                &self.entities_name,
            ),
            text_overlay_center: Self::load_file_into_texture(
                texture_handle,
                self.text_overlay_center,
                &self.entities_name,
            ),
        }
    }
}

pub type EntitiesContainer = Container<Entities, LoadEntities>;
pub const ENTITIES_CONTAINER_PATH: &str = "entities/";
