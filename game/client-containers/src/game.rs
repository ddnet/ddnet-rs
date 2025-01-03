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
    load_file_part_and_upload_ex, load_sound_file_part_and_upload,
    load_sound_file_part_and_upload_ex, ContainerLoadedItem, ContainerLoadedItemDir,
};

use super::container::{
    load_file_part_and_upload, Container, ContainerItemLoadData, ContainerLoad,
};

#[derive(Debug, Clone)]
pub struct Pickup {
    pub tex: TextureContainer,

    pub spawn: SoundObject,
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

    pub stars: Vec<TextureContainer>,
}

#[derive(Debug)]
pub struct LoadPickup {
    pub tex: ContainerItemLoadData,

    pub spawn: SoundBackendMemory,
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

    pub stars: Vec<ContainerItemLoadData>,

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

                spawn: load_sound_file_part_and_upload(
                    sound_mt,
                    &files,
                    default_files,
                    game_name,
                    &["audio", "heart"],
                    "spawn",
                )?
                .mem,
                collects: {
                    let mut sounds = Vec::new();
                    let mut i = 0;
                    let mut allow_default = true;
                    loop {
                        match load_sound_file_part_and_upload_ex(
                            sound_mt,
                            &files,
                            default_files,
                            game_name,
                            &["audio", "heart"],
                            &format!("collect{}", i + 1),
                            allow_default,
                        ) {
                            Ok(sound) => {
                                allow_default &= sound.from_default;
                                sounds.push(sound.mem);
                            }
                            Err(err) => {
                                if i == 0 {
                                    return Err(err);
                                } else {
                                    break;
                                }
                            }
                        }
                        i += 1;
                    }
                    sounds
                },
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

                spawn: load_sound_file_part_and_upload(
                    sound_mt,
                    &files,
                    default_files,
                    game_name,
                    &["audio", "shield"],
                    "spawn",
                )?
                .mem,
                collects: {
                    let mut sounds = Vec::new();
                    let mut i = 0;
                    let mut allow_default = true;
                    loop {
                        match load_sound_file_part_and_upload_ex(
                            sound_mt,
                            &files,
                            default_files,
                            game_name,
                            &["audio", "shield"],
                            &format!("collect{}", i + 1),
                            allow_default,
                        ) {
                            Ok(sound) => {
                                allow_default &= sound.from_default;
                                sounds.push(sound.mem);
                            }
                            Err(err) => {
                                if i == 0 {
                                    return Err(err);
                                } else {
                                    break;
                                }
                            }
                        }
                        i += 1;
                    }
                    sounds
                },
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

            stars: {
                let mut textures = Vec::new();
                let mut i = 0;
                let mut allow_default = true;
                loop {
                    match load_file_part_and_upload_ex(
                        graphics_mt,
                        &files,
                        default_files,
                        game_name,
                        &[],
                        &format!("star{}", i + 1),
                        allow_default,
                    ) {
                        Ok(img) => {
                            allow_default &= img.from_default;
                            textures.push(img.img);
                        }
                        Err(err) => {
                            if i == 0 {
                                return Err(err);
                            } else {
                                break;
                            }
                        }
                    }

                    i += 1;
                }
                textures
            },

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

                spawn: sound_object_handle.create(self.heart.spawn),
                collects: self
                    .heart
                    .collects
                    .into_iter()
                    .map(|collect| sound_object_handle.create(collect))
                    .collect::<Vec<_>>(),
            },
            shield: Pickup {
                tex: Self::load_file_into_texture(texture_handle, self.shield.tex, &self.game_name),

                spawn: sound_object_handle.create(self.shield.spawn),
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

            stars: self
                .stars
                .into_iter()
                .map(|star| Self::load_file_into_texture(texture_handle, star, &self.game_name))
                .collect::<Vec<_>>(),
        }
    }
}

pub type GameContainer = Container<Game, LoadGame>;
pub const GAME_CONTAINER_PATH: &str = "games/";
