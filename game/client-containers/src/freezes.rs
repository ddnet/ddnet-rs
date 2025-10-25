use std::sync::Arc;

use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::texture::texture::{GraphicsTextureHandle, TextureContainer},
};
use sound::{
    sound_handle::SoundObjectHandle, sound_mt::SoundMultiThreaded,
    sound_mt_types::SoundBackendMemory, sound_object::SoundObject,
};

use crate::container::{
    ContainerItemLoadData, ContainerLoadedItem, ContainerLoadedItemDir, load_file_part_and_upload,
    load_sound_file_part_list_and_upload,
};

use super::container::{Container, ContainerLoad};

#[derive(Debug)]
pub struct Freeze {
    pub attacks: Vec<SoundObject>,

    pub freeze_bar_empty: TextureContainer,
    pub freeze_bar_empty_right: TextureContainer,
    pub freeze_bar_full: TextureContainer,
    pub freeze_bar_full_left: TextureContainer,
}

#[derive(Debug)]
pub struct LoadFreeze {
    attacks: Vec<SoundBackendMemory>,

    freeze_bar_empty: ContainerItemLoadData,
    freeze_bar_empty_right: ContainerItemLoadData,
    freeze_bar_full: ContainerItemLoadData,
    freeze_bar_full_left: ContainerItemLoadData,

    freeze_name: String,
}

impl LoadFreeze {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
        files: ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        freeze_name: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            attacks: load_sound_file_part_list_and_upload(
                sound_mt,
                &files,
                default_files,
                freeze_name,
                &["audio"],
                "attack",
            )?,

            freeze_bar_empty: load_file_part_and_upload(
                graphics_mt,
                &files,
                default_files,
                freeze_name,
                &[],
                "freeze_bar_empty",
            )?
            .img,
            freeze_bar_empty_right: load_file_part_and_upload(
                graphics_mt,
                &files,
                default_files,
                freeze_name,
                &[],
                "freeze_bar_empty_right",
            )?
            .img,
            freeze_bar_full: load_file_part_and_upload(
                graphics_mt,
                &files,
                default_files,
                freeze_name,
                &[],
                "freeze_bar_full",
            )?
            .img,
            freeze_bar_full_left: load_file_part_and_upload(
                graphics_mt,
                &files,
                default_files,
                freeze_name,
                &[],
                "freeze_bar_full_left",
            )?
            .img,

            freeze_name: freeze_name.to_string(),
        })
    }

    pub(crate) fn load_file_into_texture(
        texture_handle: &GraphicsTextureHandle,
        img: ContainerItemLoadData,
        name: &str,
    ) -> TextureContainer {
        texture_handle.load_texture_rgba_u8(img.data, name).unwrap()
    }
}

impl ContainerLoad<Freeze> for LoadFreeze {
    fn load(
        item_name: &str,
        files: ContainerLoadedItem,
        default_files: &ContainerLoadedItemDir,
        _runtime_thread_pool: &Arc<rayon::ThreadPool>,
        graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
    ) -> anyhow::Result<Self> {
        match files {
            ContainerLoadedItem::Directory(files) => {
                Self::new(graphics_mt, sound_mt, files, default_files, item_name)
            }
            ContainerLoadedItem::SingleFile(_) => Err(anyhow::anyhow!(
                "single file mode is currently not supported"
            )),
        }
    }

    fn convert(
        self,
        texture_handle: &GraphicsTextureHandle,
        sound_object_handle: &SoundObjectHandle,
    ) -> Freeze {
        Freeze {
            attacks: self
                .attacks
                .into_iter()
                .map(|s| sound_object_handle.create(s))
                .collect(),

            freeze_bar_empty: LoadFreeze::load_file_into_texture(
                texture_handle,
                self.freeze_bar_empty,
                &self.freeze_name,
            ),
            freeze_bar_empty_right: LoadFreeze::load_file_into_texture(
                texture_handle,
                self.freeze_bar_empty_right,
                &self.freeze_name,
            ),
            freeze_bar_full: LoadFreeze::load_file_into_texture(
                texture_handle,
                self.freeze_bar_full,
                &self.freeze_name,
            ),
            freeze_bar_full_left: LoadFreeze::load_file_into_texture(
                texture_handle,
                self.freeze_bar_full_left,
                &self.freeze_name,
            ),
        }
    }
}

pub type FreezeContainer = Container<Freeze, LoadFreeze>;
pub const FREEZE_CONTAINER_PATH: &str = "freezes/";
