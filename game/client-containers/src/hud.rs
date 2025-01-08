use std::{path::PathBuf, sync::Arc};

use client_extra::ddrace_hud_split::DdraceHudPart;
use game_interface::types::weapons::{EnumCount, WeaponType};
use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::texture::texture::{GraphicsTextureHandle, TextureContainer},
};
use rustc_hash::FxHashMap;
use sound::{sound_handle::SoundObjectHandle, sound_mt::SoundMultiThreaded};

use crate::container::{ContainerLoadedItem, ContainerLoadedItemDir};

use super::container::{
    load_file_part_and_upload, Container, ContainerItemLoadData, ContainerLoad,
};

#[derive(Debug, Clone)]
pub struct HudVanilla {
    pub heart: TextureContainer,
    pub heart_empty: TextureContainer,
    pub shield: TextureContainer,
    pub shield_empty: TextureContainer,
}

#[derive(Debug, Clone)]
pub struct HudDdrace {
    pub jump: TextureContainer,
    pub jump_used: TextureContainer,
    pub solo: TextureContainer,
    pub collision_off: TextureContainer,
    pub endless_jump: TextureContainer,
    pub endless_hook: TextureContainer,
    pub jetpack: TextureContainer,
    pub disabled_hook_others: TextureContainer,
    pub disabled_weapons: [TextureContainer; WeaponType::COUNT],
    pub tele_grenade: TextureContainer,
    pub tele_pistol: TextureContainer,
    pub tele_laser: TextureContainer,
    pub deep_frozen: TextureContainer,
    pub live_frozen: TextureContainer,
    pub disabled_finish: TextureContainer,
    pub dummy_hammer: TextureContainer,
    pub dummy_copy: TextureContainer,
    pub stage_locked: TextureContainer,
    pub team0_mode: TextureContainer,
}

#[derive(Debug, Clone)]
pub struct Hud {
    pub vanilla: HudVanilla,
    pub ddrace: HudDdrace,
}

#[derive(Debug)]
pub struct LoadHudVanilla {
    heart: ContainerItemLoadData,
    heart_empty: ContainerItemLoadData,
    shield: ContainerItemLoadData,
    shield_empty: ContainerItemLoadData,
}

#[derive(Debug)]
pub struct LoadHudDdrace {
    pub jump: ContainerItemLoadData,
    pub jump_used: ContainerItemLoadData,
    pub solo: ContainerItemLoadData,
    pub collision_off: ContainerItemLoadData,
    pub endless_jump: ContainerItemLoadData,
    pub endless_hook: ContainerItemLoadData,
    pub jetpack: ContainerItemLoadData,
    pub disabled_hook_others: ContainerItemLoadData,
    pub disabled_weapons: [ContainerItemLoadData; WeaponType::COUNT],
    pub tele_grenade: ContainerItemLoadData,
    pub tele_pistol: ContainerItemLoadData,
    pub tele_laser: ContainerItemLoadData,
    pub deep_frozen: ContainerItemLoadData,
    pub live_frozen: ContainerItemLoadData,
    pub disabled_finish: ContainerItemLoadData,
    pub dummy_hammer: ContainerItemLoadData,
    pub dummy_copy: ContainerItemLoadData,
    pub stage_locked: ContainerItemLoadData,
    pub team0_mode: ContainerItemLoadData,
}

#[derive(Debug)]
pub struct LoadHud {
    vanilla: LoadHudVanilla,
    ddrace: LoadHudDdrace,

    hud_name: String,
}

impl LoadHud {
    fn load_full(files: &mut FxHashMap<PathBuf, Vec<u8>>, file: Vec<u8>) -> anyhow::Result<()> {
        let mut mem: Vec<u8> = Default::default();
        let img: image::png::PngResult<'_> =
            image::png::load_png_image_as_rgba(&file, |width, height, bytes_per_pixel| {
                mem.resize(width * height * bytes_per_pixel, Default::default());
                &mut mem
            })?;
        let converted =
            client_extra::ddrace_hud_split::split_ddrace_hud(img.data, img.width, img.height)?;

        let mut insert_part = |name: &str, part: DdraceHudPart| -> anyhow::Result<()> {
            let file = image::png::save_png_image(&part.data, part.width, part.height)?;

            files.insert(format!("{}.png", name).into(), file);
            Ok(())
        };
        insert_part("jump", converted.jump)?;
        insert_part("jump_used", converted.jump_used)?;
        insert_part("solo", converted.solo)?;
        insert_part("collision_off", converted.collision_off)?;
        insert_part("endless_jump", converted.endless_jump)?;
        insert_part("endless_hook", converted.endless_hook)?;
        insert_part("jetpack", converted.jetpack)?;

        insert_part("freeze_left", converted.freeze_left)?;
        insert_part("freeze_right", converted.freeze_right)?;
        insert_part("disabled_hook_others", converted.disabled_hook_others)?;
        insert_part("disabled_hammer", converted.disabled_hammer)?;
        insert_part("disabled_shotgun", converted.disabled_shotgun)?;
        insert_part("disabled_grenade", converted.disabled_grenade)?;
        insert_part("disabled_laser", converted.disabled_laser)?;
        insert_part("disabled_gun", converted.disabled_gun)?;

        insert_part("ninja_left", converted.ninja_left)?;
        insert_part("ninja_right", converted.ninja_right)?;
        insert_part("tele_grenade", converted.tele_grenade)?;
        insert_part("tele_pistol", converted.tele_pistol)?;
        insert_part("tele_laser", converted.tele_laser)?;
        insert_part("deep_frozen", converted.deep_frozen)?;
        insert_part("live_frozen", converted.live_frozen)?;

        insert_part("disabled_finish", converted.disabled_finish)?;
        insert_part("dummy_hammer", converted.dummy_hammer)?;
        insert_part("dummy_copy", converted.dummy_copy)?;
        insert_part("stage_locked", converted.stage_locked)?;
        insert_part("team0_mode", converted.team0_mode)?;
        Ok(())
    }

    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        files: &mut ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        hud_name: &str,
    ) -> anyhow::Result<Self> {
        let full_path: PathBuf = "full.png".into();
        if let Some(file) = files.files.remove(&full_path) {
            Self::load_full(&mut files.files, file)?;
        }

        Ok(Self {
            vanilla: LoadHudVanilla {
                // heart
                heart: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["vanilla"],
                    "heart",
                )?
                .img,
                heart_empty: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["vanilla"],
                    "heart_empty",
                )?
                .img,
                // cursor
                shield: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["vanilla"],
                    "shield",
                )?
                .img,
                shield_empty: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["vanilla"],
                    "shield_empty",
                )?
                .img,
            },
            ddrace: LoadHudDdrace {
                jump: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "jump",
                )?
                .img,
                jump_used: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "jump_used",
                )?
                .img,
                solo: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "solo",
                )?
                .img,
                collision_off: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "collision_off",
                )?
                .img,
                endless_jump: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "endless_jump",
                )?
                .img,
                endless_hook: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "endless_hook",
                )?
                .img,
                jetpack: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "jetpack",
                )?
                .img,
                disabled_hook_others: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "disabled_hook_others",
                )?
                .img,
                disabled_weapons: [
                    load_file_part_and_upload(
                        graphics_mt,
                        files,
                        default_files,
                        hud_name,
                        &["ddrace"],
                        "disabled_hammer",
                    )?
                    .img,
                    load_file_part_and_upload(
                        graphics_mt,
                        files,
                        default_files,
                        hud_name,
                        &["ddrace"],
                        "disabled_shotgun",
                    )?
                    .img,
                    load_file_part_and_upload(
                        graphics_mt,
                        files,
                        default_files,
                        hud_name,
                        &["ddrace"],
                        "disabled_grenade",
                    )?
                    .img,
                    load_file_part_and_upload(
                        graphics_mt,
                        files,
                        default_files,
                        hud_name,
                        &["ddrace"],
                        "disabled_laser",
                    )?
                    .img,
                    load_file_part_and_upload(
                        graphics_mt,
                        files,
                        default_files,
                        hud_name,
                        &["ddrace"],
                        "disabled_gun",
                    )?
                    .img,
                ],
                tele_grenade: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "tele_grenade",
                )?
                .img,
                tele_pistol: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "tele_pistol",
                )?
                .img,
                tele_laser: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "tele_laser",
                )?
                .img,
                deep_frozen: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "deep_frozen",
                )?
                .img,
                live_frozen: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "live_frozen",
                )?
                .img,
                disabled_finish: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "disabled_finish",
                )?
                .img,
                dummy_hammer: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "dummy_hammer",
                )?
                .img,
                dummy_copy: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "dummy_copy",
                )?
                .img,
                stage_locked: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "stage_locked",
                )?
                .img,
                team0_mode: load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    hud_name,
                    &["ddrace"],
                    "team0_mode",
                )?
                .img,
            },

            hud_name: hud_name.to_string(),
        })
    }

    fn load_file_into_texture(
        texture_handle: &GraphicsTextureHandle,
        img: ContainerItemLoadData,
        name: &str,
    ) -> TextureContainer {
        texture_handle.load_texture_rgba_u8(img.data, name).unwrap()
    }

    fn load_files_into_textures(self, texture_handle: &GraphicsTextureHandle) -> Hud {
        Hud {
            vanilla: HudVanilla {
                heart: Self::load_file_into_texture(
                    texture_handle,
                    self.vanilla.heart,
                    &self.hud_name,
                ),
                heart_empty: Self::load_file_into_texture(
                    texture_handle,
                    self.vanilla.heart_empty,
                    &self.hud_name,
                ),
                shield: Self::load_file_into_texture(
                    texture_handle,
                    self.vanilla.shield,
                    &self.hud_name,
                ),
                shield_empty: Self::load_file_into_texture(
                    texture_handle,
                    self.vanilla.shield_empty,
                    &self.hud_name,
                ),
            },
            ddrace: HudDdrace {
                jump: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.jump,
                    &self.hud_name,
                ),
                jump_used: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.jump_used,
                    &self.hud_name,
                ),
                solo: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.solo,
                    &self.hud_name,
                ),
                collision_off: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.collision_off,
                    &self.hud_name,
                ),
                endless_jump: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.endless_jump,
                    &self.hud_name,
                ),
                endless_hook: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.endless_hook,
                    &self.hud_name,
                ),
                jetpack: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.jetpack,
                    &self.hud_name,
                ),
                disabled_hook_others: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.disabled_hook_others,
                    &self.hud_name,
                ),
                disabled_weapons: self
                    .ddrace
                    .disabled_weapons
                    .into_iter()
                    .map(|t| Self::load_file_into_texture(texture_handle, t, &self.hud_name))
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap(),
                tele_grenade: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.tele_grenade,
                    &self.hud_name,
                ),
                tele_pistol: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.tele_pistol,
                    &self.hud_name,
                ),
                tele_laser: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.tele_laser,
                    &self.hud_name,
                ),
                deep_frozen: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.deep_frozen,
                    &self.hud_name,
                ),
                live_frozen: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.live_frozen,
                    &self.hud_name,
                ),
                disabled_finish: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.disabled_finish,
                    &self.hud_name,
                ),
                dummy_hammer: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.dummy_hammer,
                    &self.hud_name,
                ),
                dummy_copy: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.dummy_copy,
                    &self.hud_name,
                ),
                stage_locked: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.stage_locked,
                    &self.hud_name,
                ),
                team0_mode: Self::load_file_into_texture(
                    texture_handle,
                    self.ddrace.team0_mode,
                    &self.hud_name,
                ),
            },
        }
    }
}

impl ContainerLoad<Hud> for LoadHud {
    fn load(
        item_name: &str,
        files: ContainerLoadedItem,
        default_files: &ContainerLoadedItemDir,
        _runtime_thread_pool: &Arc<rayon::ThreadPool>,
        graphics_mt: &GraphicsMultiThreaded,
        _sound_mt: &SoundMultiThreaded,
    ) -> anyhow::Result<Self> {
        match files {
            ContainerLoadedItem::Directory(mut files) => {
                Self::new(graphics_mt, &mut files, default_files, item_name)
            }
            ContainerLoadedItem::SingleFile(file) => {
                let mut files: FxHashMap<PathBuf, Vec<u8>> = Default::default();

                files.insert("full.png".into(), file);

                let mut files = ContainerLoadedItemDir::new(files);
                Self::new(graphics_mt, &mut files, default_files, item_name)
            }
        }
    }

    fn convert(
        self,
        texture_handle: &GraphicsTextureHandle,
        _sound_object_handle: &SoundObjectHandle,
    ) -> Hud {
        self.load_files_into_textures(texture_handle)
    }
}

pub type HudContainer = Container<Hud, LoadHud>;
pub const HUD_CONTAINER_PATH: &str = "huds/";
