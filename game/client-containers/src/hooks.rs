use std::sync::Arc;

use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::texture::texture::{GraphicsTextureHandle, TextureContainer},
};
use hiarc::Hiarc;
use sound::{
    sound_handle::SoundObjectHandle, sound_mt::SoundMultiThreaded,
    sound_mt_types::SoundBackendMemory, sound_object::SoundObject,
};

use crate::container::{
    ContainerLoadedItem, ContainerLoadedItemDir, load_sound_file_part_list_and_upload,
};

use super::container::{
    Container, ContainerItemLoadData, ContainerLoad, load_file_part_and_upload,
};

#[derive(Debug, Hiarc, Clone)]
pub struct Hook {
    pub hook_chain: TextureContainer,
    pub hook_head: TextureContainer,

    pub hit_hookable: Vec<SoundObject>,
    pub hit_player: Vec<SoundObject>,
    pub hit_unhookable: Vec<SoundObject>,
}

#[derive(Debug, Hiarc)]
pub struct LoadHook {
    hook_chain: ContainerItemLoadData,
    hook_head: ContainerItemLoadData,

    hit_hookable: Vec<SoundBackendMemory>,
    hit_player: Vec<SoundBackendMemory>,
    hit_unhookable: Vec<SoundBackendMemory>,

    hook_name: String,
}

impl LoadHook {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
        files: ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        hook_name: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            hook_chain: load_file_part_and_upload(
                graphics_mt,
                &files,
                default_files,
                hook_name,
                &[],
                "hook_chain",
            )?
            .img,
            hook_head: load_file_part_and_upload(
                graphics_mt,
                &files,
                default_files,
                hook_name,
                &[],
                "hook_head",
            )?
            .img,

            hit_hookable: load_sound_file_part_list_and_upload(
                sound_mt,
                &files,
                default_files,
                hook_name,
                &[],
                "hit_hookable",
            )?,
            hit_player: load_sound_file_part_list_and_upload(
                sound_mt,
                &files,
                default_files,
                hook_name,
                &[],
                "hit_player",
            )?,
            hit_unhookable: load_sound_file_part_list_and_upload(
                sound_mt,
                &files,
                default_files,
                hook_name,
                &[],
                "hit_unhookable",
            )?,

            hook_name: hook_name.to_string(),
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

impl ContainerLoad<Hook> for LoadHook {
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
    ) -> Hook {
        Hook {
            hook_chain: Self::load_file_into_texture(
                texture_handle,
                self.hook_chain,
                &self.hook_name,
            ),
            hook_head: Self::load_file_into_texture(
                texture_handle,
                self.hook_head,
                &self.hook_name,
            ),

            hit_hookable: self
                .hit_hookable
                .into_iter()
                .map(|sound| sound_object_handle.create(sound))
                .collect::<Vec<_>>(),
            hit_player: self
                .hit_player
                .into_iter()
                .map(|sound| sound_object_handle.create(sound))
                .collect::<Vec<_>>(),
            hit_unhookable: self
                .hit_unhookable
                .into_iter()
                .map(|sound| sound_object_handle.create(sound))
                .collect::<Vec<_>>(),
        }
    }
}

pub type HookContainer = Container<Hook, LoadHook>;
pub const HOOK_CONTAINER_PATH: &str = "hooks/";
