use std::{rc::Rc, sync::Arc};

use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::texture::texture::{GraphicsTextureHandle, TextureContainer},
};
use hiarc::Hiarc;
use sound::{
    sound_handle::SoundObjectHandle, sound_mt::SoundMultiThreaded,
    sound_mt_types::SoundBackendMemory, sound_object::SoundObject,
};

use crate::{
    container::{
        load_file_part_list_and_upload, load_sound_file_part_list_and_upload, ContainerLoadedItem,
        ContainerLoadedItemDir,
    },
    skins::{LoadSkin, Skin},
};

use super::container::{
    load_file_part_and_upload, Container, ContainerItemLoadData, ContainerLoad,
};

#[derive(Debug, Hiarc)]
pub struct Ninja {
    pub cursor: TextureContainer,
    pub weapon: TextureContainer,
    pub muzzles: Vec<TextureContainer>,

    pub skin: Rc<Skin>,

    pub spawn: Vec<SoundObject>,
    pub collect: Vec<SoundObject>,
    pub attacks: Vec<SoundObject>,
    pub hits: Vec<SoundObject>,
}

#[derive(Debug)]
pub struct LoadNinja {
    cursor: ContainerItemLoadData,
    muzzles: Vec<ContainerItemLoadData>,
    weapon: ContainerItemLoadData,

    skin: LoadSkin,

    spawn: Vec<SoundBackendMemory>,
    collect: Vec<SoundBackendMemory>,
    attacks: Vec<SoundBackendMemory>,
    hits: Vec<SoundBackendMemory>,

    ninja_name: String,
}

impl LoadNinja {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
        mut files: ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        ninja_name: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            cursor: load_file_part_and_upload(
                graphics_mt,
                &files,
                default_files,
                ninja_name,
                &[],
                "cursor",
            )?
            .img,
            weapon: load_file_part_and_upload(
                graphics_mt,
                &files,
                default_files,
                ninja_name,
                &[],
                "weapon",
            )?
            .img,
            muzzles: load_file_part_list_and_upload(
                graphics_mt,
                &files,
                default_files,
                ninja_name,
                &[],
                "muzzle",
            )?,

            skin: LoadSkin::new(
                graphics_mt,
                sound_mt,
                &mut files,
                default_files,
                ninja_name,
                Some("skin"),
            )?,

            spawn: load_sound_file_part_list_and_upload(
                sound_mt,
                &files,
                default_files,
                ninja_name,
                &["audio"],
                "spawn",
            )?,
            collect: load_sound_file_part_list_and_upload(
                sound_mt,
                &files,
                default_files,
                ninja_name,
                &["audio"],
                "collect",
            )?,
            attacks: load_sound_file_part_list_and_upload(
                sound_mt,
                &files,
                default_files,
                ninja_name,
                &["audio"],
                "attack",
            )?,
            hits: load_sound_file_part_list_and_upload(
                sound_mt,
                &files,
                default_files,
                ninja_name,
                &["audio"],
                "hit",
            )?,
            ninja_name: ninja_name.to_string(),
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

impl ContainerLoad<Ninja> for LoadNinja {
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
    ) -> Ninja {
        Ninja {
            cursor: Self::load_file_into_texture(texture_handle, self.cursor, &self.ninja_name),
            weapon: Self::load_file_into_texture(texture_handle, self.weapon, &self.ninja_name),
            muzzles: self
                .muzzles
                .into_iter()
                .map(|muzzle| {
                    Self::load_file_into_texture(texture_handle, muzzle, &self.ninja_name)
                })
                .collect(),

            skin: self.skin.convert(texture_handle, sound_object_handle),

            spawn: self
                .spawn
                .into_iter()
                .map(|obj| sound_object_handle.create(obj))
                .collect::<Vec<_>>(),
            collect: self
                .collect
                .into_iter()
                .map(|obj| sound_object_handle.create(obj))
                .collect::<Vec<_>>(),
            attacks: self
                .attacks
                .into_iter()
                .map(|attack| sound_object_handle.create(attack))
                .collect::<Vec<_>>(),
            hits: self
                .hits
                .into_iter()
                .map(|hit| sound_object_handle.create(hit))
                .collect::<Vec<_>>(),
        }
    }
}

pub type NinjaContainer = Container<Ninja, LoadNinja>;
pub const NINJA_CONTAINER_PATH: &str = "ninjas/";
