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
    load_sound_file_part_list_and_upload, ContainerLoadedItem, ContainerLoadedItemDir,
};

use super::container::{
    load_file_part_and_upload, Container, ContainerItemLoadData, ContainerLoad,
};

#[derive(Debug, Clone)]
pub struct Pickup {
    pub tex: TextureContainer,

    pub spawns: Vec<SoundObject>,
    pub collects: Vec<SoundObject>,
}

#[derive(Debug, Clone)]
pub struct Game {
    pub heart: Pickup,
    pub shield: Pickup,

    pub lose_grenade: TextureContainer,
    pub lose_laser: TextureContainer,
    pub lose_ninja: TextureContainer,
    pub lose_shotgun: TextureContainer,
}

#[derive(Debug)]
pub struct LoadPickup {
    pub tex: ContainerItemLoadData,

    pub spawns: Vec<SoundBackendMemory>,
    pub collects: Vec<SoundBackendMemory>,
}

#[derive(Debug)]
pub struct LoadGame {
    pub heart: LoadPickup,
    pub shield: LoadPickup,

    pub lose_grenade: ContainerItemLoadData,
    pub lose_laser: ContainerItemLoadData,
    pub lose_ninja: ContainerItemLoadData,
    pub lose_shotgun: ContainerItemLoadData,

    game_name: String,
}

impl LoadGame {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
        files: ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        game_name: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            heart: LoadPickup {
                tex: load_file_part_and_upload(
                    graphics_mt,
                    &files,
                    default_files,
                    game_name,
                    &[],
                    "heart",
                )?
                .img,

                spawns: load_sound_file_part_list_and_upload(
                    sound_mt,
                    &files,
                    default_files,
                    game_name,
                    &["audio", "heart"],
                    "spawn",
                )?,
                collects: load_sound_file_part_list_and_upload(
                    sound_mt,
                    &files,
                    default_files,
                    game_name,
                    &["audio", "heart"],
                    "collect",
                )?,
            },
            shield: LoadPickup {
                tex: load_file_part_and_upload(
                    graphics_mt,
                    &files,
                    default_files,
                    game_name,
                    &[],
                    "shield",
                )?
                .img,

                spawns: load_sound_file_part_list_and_upload(
                    sound_mt,
                    &files,
                    default_files,
                    game_name,
                    &["audio", "shield"],
                    "spawn",
                )?,
                collects: load_sound_file_part_list_and_upload(
                    sound_mt,
                    &files,
                    default_files,
                    game_name,
                    &["audio", "shield"],
                    "collect",
                )?,
            },

            lose_grenade: load_file_part_and_upload(
                graphics_mt,
                &files,
                default_files,
                game_name,
                &[],
                "lose_grenade",
            )?
            .img,
            lose_laser: load_file_part_and_upload(
                graphics_mt,
                &files,
                default_files,
                game_name,
                &[],
                "lose_laser",
            )?
            .img,
            lose_ninja: load_file_part_and_upload(
                graphics_mt,
                &files,
                default_files,
                game_name,
                &[],
                "lose_ninja",
            )?
            .img,
            lose_shotgun: load_file_part_and_upload(
                graphics_mt,
                &files,
                default_files,
                game_name,
                &[],
                "lose_shotgun",
            )?
            .img,

            game_name: game_name.to_string(),
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

impl ContainerLoad<Game> for LoadGame {
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
    ) -> Game {
        Game {
            heart: Pickup {
                tex: Self::load_file_into_texture(texture_handle, self.heart.tex, &self.game_name),

                spawns: self
                    .heart
                    .spawns
                    .into_iter()
                    .map(|obj| sound_object_handle.create(obj))
                    .collect::<Vec<_>>(),
                collects: self
                    .heart
                    .collects
                    .into_iter()
                    .map(|collect| sound_object_handle.create(collect))
                    .collect::<Vec<_>>(),
            },
            shield: Pickup {
                tex: Self::load_file_into_texture(texture_handle, self.shield.tex, &self.game_name),

                spawns: self
                    .shield
                    .spawns
                    .into_iter()
                    .map(|obj| sound_object_handle.create(obj))
                    .collect::<Vec<_>>(),
                collects: self
                    .shield
                    .collects
                    .into_iter()
                    .map(|collect| sound_object_handle.create(collect))
                    .collect::<Vec<_>>(),
            },

            lose_grenade: Self::load_file_into_texture(
                texture_handle,
                self.lose_grenade,
                &self.game_name,
            ),
            lose_laser: Self::load_file_into_texture(
                texture_handle,
                self.lose_laser,
                &self.game_name,
            ),
            lose_ninja: Self::load_file_into_texture(
                texture_handle,
                self.lose_ninja,
                &self.game_name,
            ),
            lose_shotgun: Self::load_file_into_texture(
                texture_handle,
                self.lose_shotgun,
                &self.game_name,
            ),
        }
    }
}

pub type GameContainer = Container<Game, LoadGame>;
pub const GAME_CONTAINER_PATH: &str = "games/";
