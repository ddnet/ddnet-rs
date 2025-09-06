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
    ContainerLoadedItem, ContainerLoadedItemDir, load_sound_file_part_and_upload,
};

use super::container::{
    Container, ContainerItemLoadData, ContainerLoad, load_file_part_and_upload,
};

#[derive(Debug, Clone)]
pub struct Ctf {
    pub flag_red: TextureContainer,
    pub flag_blue: TextureContainer,

    pub capture: SoundObject,
    pub collect_friendly: SoundObject,
    pub collect_opponents: SoundObject,
    pub drop: SoundObject,
    pub return_sound: SoundObject,
}

#[derive(Debug)]
pub struct LoadCtf {
    flag_red: ContainerItemLoadData,
    flag_blue: ContainerItemLoadData,

    capture: SoundBackendMemory,
    collect_friendly: SoundBackendMemory,
    collect_opponents: SoundBackendMemory,
    drop: SoundBackendMemory,
    return_sound: SoundBackendMemory,

    ctf_name: String,
}

impl LoadCtf {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
        files: ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        ctf_name: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            flag_red: load_file_part_and_upload(
                graphics_mt,
                &files,
                default_files,
                ctf_name,
                &[],
                "flag_red",
            )?
            .img,
            flag_blue: load_file_part_and_upload(
                graphics_mt,
                &files,
                default_files,
                ctf_name,
                &[],
                "flag_blue",
            )?
            .img,

            capture: load_sound_file_part_and_upload(
                sound_mt,
                &files,
                default_files,
                ctf_name,
                &["audio"],
                "capture",
            )?
            .mem,
            collect_friendly: load_sound_file_part_and_upload(
                sound_mt,
                &files,
                default_files,
                ctf_name,
                &["audio"],
                "collect_friendly",
            )?
            .mem,
            collect_opponents: load_sound_file_part_and_upload(
                sound_mt,
                &files,
                default_files,
                ctf_name,
                &["audio"],
                "collect_opponents",
            )?
            .mem,
            drop: load_sound_file_part_and_upload(
                sound_mt,
                &files,
                default_files,
                ctf_name,
                &["audio"],
                "drop",
            )?
            .mem,
            return_sound: load_sound_file_part_and_upload(
                sound_mt,
                &files,
                default_files,
                ctf_name,
                &["audio"],
                "return",
            )?
            .mem,

            ctf_name: ctf_name.to_string(),
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

impl ContainerLoad<Ctf> for LoadCtf {
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
                LoadCtf::new(graphics_mt, sound_mt, files, default_files, item_name)
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
    ) -> Ctf {
        Ctf {
            flag_red: Self::load_file_into_texture(texture_handle, self.flag_red, &self.ctf_name),
            flag_blue: Self::load_file_into_texture(texture_handle, self.flag_blue, &self.ctf_name),

            capture: sound_object_handle.create(self.capture),
            collect_friendly: sound_object_handle.create(self.collect_friendly),
            collect_opponents: sound_object_handle.create(self.collect_opponents),
            drop: sound_object_handle.create(self.drop),
            return_sound: sound_object_handle.create(self.return_sound),
        }
    }
}

pub type CtfContainer = Container<Ctf, LoadCtf>;
pub const CTF_CONTAINER_PATH: &str = "ctfs/";
