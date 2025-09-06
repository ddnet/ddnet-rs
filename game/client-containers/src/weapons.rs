use std::sync::Arc;

use game_interface::types::weapons::WeaponType;
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
    ContainerLoadedItem, ContainerLoadedItemDir, load_file_part_list_and_upload,
    load_sound_file_part_list_and_upload,
};

use super::container::{
    Container, ContainerItemLoadData, ContainerLoad, load_file_part_and_upload,
};

#[derive(Debug, Hiarc, Clone)]
pub struct WeaponProjectile {
    pub projectile: TextureContainer,
}

#[derive(Debug, Hiarc, Clone)]
pub struct WeaponMuzzles {
    pub muzzles: Vec<TextureContainer>,
}

#[derive(Debug, Hiarc, Clone)]
pub struct Weapon {
    pub tex: TextureContainer,
    pub cursor: TextureContainer,

    pub fire: Vec<SoundObject>,
    pub switch: Vec<SoundObject>,
    pub noammo: Vec<SoundObject>,
}

#[derive(Debug, Hiarc, Clone)]
pub struct Gun {
    pub weapon: Weapon,

    pub projectile: WeaponProjectile,
    pub muzzles: WeaponMuzzles,
}

#[derive(Debug, Hiarc, Clone)]
pub struct Grenade {
    pub weapon: Weapon,
    pub spawn: Vec<SoundObject>,
    pub collect: Vec<SoundObject>,
    pub explosions: Vec<SoundObject>,

    pub projectile: WeaponProjectile,
}

#[derive(Debug, Hiarc, Clone)]
pub struct Laser {
    pub weapon: Weapon,
    pub spawn: Vec<SoundObject>,
    pub collect: Vec<SoundObject>,
    pub bounces: Vec<SoundObject>,
    pub heads: Vec<TextureContainer>,

    pub projectile: WeaponProjectile,
}

#[derive(Debug, Hiarc, Clone)]
pub struct Shotgun {
    pub weapon: Weapon,
    pub spawn: Vec<SoundObject>,
    pub collect: Vec<SoundObject>,

    pub projectile: WeaponProjectile,
    pub muzzles: WeaponMuzzles,
}

#[derive(Debug, Hiarc, Clone)]
pub struct Hammer {
    pub weapon: Weapon,
    pub hits: Vec<SoundObject>,
}

#[derive(Debug, Hiarc, Clone)]
pub struct Weapons {
    pub hammer: Hammer,
    pub gun: Gun,
    pub shotgun: Shotgun,
    pub grenade: Grenade,
    pub laser: Laser,
}

impl Weapons {
    pub fn by_type(&self, weapon: WeaponType) -> &Weapon {
        match weapon {
            WeaponType::Hammer => &self.hammer.weapon,
            WeaponType::Gun => &self.gun.weapon,
            WeaponType::Shotgun => &self.shotgun.weapon,
            WeaponType::Grenade => &self.grenade.weapon,
            WeaponType::Laser => &self.laser.weapon,
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct LoadProjectile {
    projectile: ContainerItemLoadData,
}

impl LoadProjectile {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        files: &ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        weapon_name: &str,
        weapon_part: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            projectile: load_file_part_and_upload(
                graphics_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "projectile",
            )?
            .img,
        })
    }

    fn load(self, texture_handle: &GraphicsTextureHandle, name: &str) -> WeaponProjectile {
        WeaponProjectile {
            projectile: texture_handle
                .load_texture_rgba_u8(self.projectile.data, name)
                .unwrap(),
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct LoadMuzzles {
    muzzles: Vec<ContainerItemLoadData>,
}

impl LoadMuzzles {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        files: &ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        weapon_name: &str,
        weapon_part: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            muzzles: load_file_part_list_and_upload(
                graphics_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "muzzle",
            )?,
        })
    }

    fn load(self, texture_handle: &GraphicsTextureHandle, name: &str) -> WeaponMuzzles {
        WeaponMuzzles {
            muzzles: self
                .muzzles
                .into_iter()
                .map(|v| texture_handle.load_texture_rgba_u8(v.data, name).unwrap())
                .collect(),
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct LoadWeapon {
    tex: ContainerItemLoadData,
    cursor_tex: ContainerItemLoadData,

    fire: Vec<SoundBackendMemory>,
    switch: Vec<SoundBackendMemory>,
    noammo: Vec<SoundBackendMemory>,
}

impl LoadWeapon {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
        files: &ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        weapon_name: &str,
        weapon_part: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            // weapon
            tex: load_file_part_and_upload(
                graphics_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "weapon",
            )?
            .img,
            // cursor
            cursor_tex: load_file_part_and_upload(
                graphics_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "cursor",
            )?
            .img,

            fire: load_sound_file_part_list_and_upload(
                sound_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "fire",
            )?,
            switch: load_sound_file_part_list_and_upload(
                sound_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "switch",
            )?,
            noammo: load_sound_file_part_list_and_upload(
                sound_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "noammo",
            )?,
        })
    }

    fn load_file_into_texture(
        texture_handle: &GraphicsTextureHandle,
        img: ContainerItemLoadData,
        name: &str,
    ) -> TextureContainer {
        texture_handle.load_texture_rgba_u8(img.data, name).unwrap()
    }

    fn load_files_into_objects(
        texture_handle: &GraphicsTextureHandle,
        sound_object_handle: &SoundObjectHandle,
        file: LoadWeapon,
        name: &str,
    ) -> Weapon {
        Weapon {
            tex: Self::load_file_into_texture(texture_handle, file.tex, name),
            cursor: Self::load_file_into_texture(texture_handle, file.cursor_tex, name),

            fire: file
                .fire
                .into_iter()
                .map(|snd| sound_object_handle.create(snd))
                .collect(),
            switch: file
                .switch
                .into_iter()
                .map(|snd| sound_object_handle.create(snd))
                .collect(),
            noammo: file
                .noammo
                .into_iter()
                .map(|snd| sound_object_handle.create(snd))
                .collect(),
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct LoadGun {
    weapon: LoadWeapon,

    projectile: LoadProjectile,
    muzzles: LoadMuzzles,
}

impl LoadGun {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
        files: &ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        weapon_name: &str,
        weapon_part: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            weapon: LoadWeapon::new(
                graphics_mt,
                sound_mt,
                files,
                default_files,
                weapon_name,
                weapon_part,
            )?,

            projectile: LoadProjectile::new(
                graphics_mt,
                files,
                default_files,
                weapon_name,
                weapon_part,
            )?,
            muzzles: LoadMuzzles::new(graphics_mt, files, default_files, weapon_name, weapon_part)?,
        })
    }
}

#[derive(Debug, Hiarc)]
pub struct LoadGrenade {
    weapon: LoadWeapon,

    spawn: Vec<SoundBackendMemory>,
    collect: Vec<SoundBackendMemory>,
    explosions: Vec<SoundBackendMemory>,

    projectile: LoadProjectile,
}

impl LoadGrenade {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
        files: &ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        weapon_name: &str,
        weapon_part: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            weapon: LoadWeapon::new(
                graphics_mt,
                sound_mt,
                files,
                default_files,
                weapon_name,
                weapon_part,
            )?,

            explosions: load_sound_file_part_list_and_upload(
                sound_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "explosion",
            )?,

            spawn: load_sound_file_part_list_and_upload(
                sound_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "spawn",
            )?,

            collect: load_sound_file_part_list_and_upload(
                sound_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "collect",
            )?,

            projectile: LoadProjectile::new(
                graphics_mt,
                files,
                default_files,
                weapon_name,
                weapon_part,
            )?,
        })
    }
}

#[derive(Debug, Hiarc)]
pub struct LoadLaser {
    weapon: LoadWeapon,

    spawn: Vec<SoundBackendMemory>,
    collect: Vec<SoundBackendMemory>,
    bounces: Vec<SoundBackendMemory>,
    heads: Vec<ContainerItemLoadData>,

    projectile: LoadProjectile,
}

impl LoadLaser {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
        files: &ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        weapon_name: &str,
        weapon_part: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            weapon: LoadWeapon::new(
                graphics_mt,
                sound_mt,
                files,
                default_files,
                weapon_name,
                weapon_part,
            )?,

            bounces: load_sound_file_part_list_and_upload(
                sound_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "bounce",
            )?,

            spawn: load_sound_file_part_list_and_upload(
                sound_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "spawn",
            )?,

            collect: load_sound_file_part_list_and_upload(
                sound_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "collect",
            )?,

            heads: load_file_part_list_and_upload(
                graphics_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "bounce",
            )?,

            projectile: LoadProjectile::new(
                graphics_mt,
                files,
                default_files,
                weapon_name,
                weapon_part,
            )?,
        })
    }
}

#[derive(Debug, Hiarc)]
pub struct LoadShotgun {
    weapon: LoadWeapon,

    projectile: LoadProjectile,
    muzzles: LoadMuzzles,

    spawn: Vec<SoundBackendMemory>,
    collect: Vec<SoundBackendMemory>,
}

impl LoadShotgun {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
        files: &ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        weapon_name: &str,
        weapon_part: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            weapon: LoadWeapon::new(
                graphics_mt,
                sound_mt,
                files,
                default_files,
                weapon_name,
                weapon_part,
            )?,

            spawn: load_sound_file_part_list_and_upload(
                sound_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "spawn",
            )?,

            collect: load_sound_file_part_list_and_upload(
                sound_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "collect",
            )?,

            projectile: LoadProjectile::new(
                graphics_mt,
                files,
                default_files,
                weapon_name,
                weapon_part,
            )?,
            muzzles: LoadMuzzles::new(graphics_mt, files, default_files, weapon_name, weapon_part)?,
        })
    }
}

#[derive(Debug, Hiarc)]
pub struct LoadHammer {
    weapon: LoadWeapon,

    hits: Vec<SoundBackendMemory>,
}

impl LoadHammer {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
        files: &ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        weapon_name: &str,
        weapon_part: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            weapon: LoadWeapon::new(
                graphics_mt,
                sound_mt,
                files,
                default_files,
                weapon_name,
                weapon_part,
            )?,

            hits: load_sound_file_part_list_and_upload(
                sound_mt,
                files,
                default_files,
                weapon_name,
                &[weapon_part],
                "hit",
            )?,
        })
    }
}

#[derive(Debug, Hiarc)]
pub struct LoadWeapons {
    hammer: LoadHammer,
    gun: LoadGun,
    shotgun: LoadShotgun,
    grenade: LoadGrenade,
    laser: LoadLaser,

    weapon_name: String,
}

impl LoadWeapons {
    pub fn load_weapon(
        graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
        files: ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        weapon_name: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            // hammer
            hammer: LoadHammer::new(
                graphics_mt,
                sound_mt,
                &files,
                default_files,
                weapon_name,
                "hammer",
            )?,
            // gun
            gun: LoadGun::new(
                graphics_mt,
                sound_mt,
                &files,
                default_files,
                weapon_name,
                "gun",
            )?,
            // shotgun
            shotgun: LoadShotgun::new(
                graphics_mt,
                sound_mt,
                &files,
                default_files,
                weapon_name,
                "shotgun",
            )?,
            // grenade
            grenade: LoadGrenade::new(
                graphics_mt,
                sound_mt,
                &files,
                default_files,
                weapon_name,
                "grenade",
            )?,
            // laser
            laser: LoadLaser::new(
                graphics_mt,
                sound_mt,
                &files,
                default_files,
                weapon_name,
                "laser",
            )?,

            weapon_name: weapon_name.to_string(),
        })
    }

    fn load_files_into_textures(
        self,
        texture_handle: &GraphicsTextureHandle,
        sound_object_handle: &SoundObjectHandle,
    ) -> Weapons {
        Weapons {
            hammer: Hammer {
                weapon: LoadWeapon::load_files_into_objects(
                    texture_handle,
                    sound_object_handle,
                    self.hammer.weapon,
                    &self.weapon_name,
                ),
                hits: self
                    .hammer
                    .hits
                    .into_iter()
                    .map(|hammer| sound_object_handle.create(hammer))
                    .collect::<Vec<_>>(),
            },
            gun: Gun {
                weapon: LoadWeapon::load_files_into_objects(
                    texture_handle,
                    sound_object_handle,
                    self.gun.weapon,
                    &self.weapon_name,
                ),
                projectile: self.gun.projectile.load(texture_handle, &self.weapon_name),
                muzzles: self.gun.muzzles.load(texture_handle, &self.weapon_name),
            },
            shotgun: Shotgun {
                weapon: LoadWeapon::load_files_into_objects(
                    texture_handle,
                    sound_object_handle,
                    self.shotgun.weapon,
                    &self.weapon_name,
                ),
                spawn: self
                    .shotgun
                    .spawn
                    .into_iter()
                    .map(|s| sound_object_handle.create(s))
                    .collect(),
                collect: self
                    .shotgun
                    .collect
                    .into_iter()
                    .map(|s| sound_object_handle.create(s))
                    .collect(),

                projectile: self
                    .shotgun
                    .projectile
                    .load(texture_handle, &self.weapon_name),
                muzzles: self.shotgun.muzzles.load(texture_handle, &self.weapon_name),
            },
            grenade: Grenade {
                weapon: LoadWeapon::load_files_into_objects(
                    texture_handle,
                    sound_object_handle,
                    self.grenade.weapon,
                    &self.weapon_name,
                ),
                spawn: self
                    .grenade
                    .spawn
                    .into_iter()
                    .map(|s| sound_object_handle.create(s))
                    .collect(),
                collect: self
                    .grenade
                    .collect
                    .into_iter()
                    .map(|s| sound_object_handle.create(s))
                    .collect(),
                explosions: self
                    .grenade
                    .explosions
                    .into_iter()
                    .map(|bounce| sound_object_handle.create(bounce))
                    .collect::<Vec<_>>(),
                projectile: self
                    .grenade
                    .projectile
                    .load(texture_handle, &self.weapon_name),
            },
            laser: Laser {
                weapon: LoadWeapon::load_files_into_objects(
                    texture_handle,
                    sound_object_handle,
                    self.laser.weapon,
                    &self.weapon_name,
                ),
                spawn: self
                    .laser
                    .spawn
                    .into_iter()
                    .map(|s| sound_object_handle.create(s))
                    .collect(),
                collect: self
                    .laser
                    .collect
                    .into_iter()
                    .map(|s| sound_object_handle.create(s))
                    .collect(),
                bounces: self
                    .laser
                    .bounces
                    .into_iter()
                    .map(|bounce| sound_object_handle.create(bounce))
                    .collect::<Vec<_>>(),
                heads: self
                    .laser
                    .heads
                    .into_iter()
                    .map(|head| LoadWeapon::load_file_into_texture(texture_handle, head, "head"))
                    .collect::<Vec<_>>(),
                projectile: self
                    .laser
                    .projectile
                    .load(texture_handle, &self.weapon_name),
            },
        }
    }
}

impl ContainerLoad<Weapons> for LoadWeapons {
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
                Self::load_weapon(graphics_mt, sound_mt, files, default_files, item_name)
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
    ) -> Weapons {
        self.load_files_into_textures(texture_handle, sound_object_handle)
    }
}

pub type WeaponContainer = Container<Weapons, LoadWeapons>;
pub const WEAPON_CONTAINER_PATH: &str = "weapons/";
