use std::{path::PathBuf, sync::Arc};

use assets_splitting::particles_split::Particles06Part;
use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::texture::texture::{GraphicsTextureHandle, TextureContainer},
};
use hiarc::Hiarc;
use math::math::RngSlice;
use rustc_hash::FxHashMap;
use sound::{sound_handle::SoundObjectHandle, sound_mt::SoundMultiThreaded};

use crate::container::{
    ContainerLoadedItem, ContainerLoadedItemDir, load_file_part_list_and_upload,
};

use super::container::{Container, ContainerItemLoadData, ContainerLoad};

#[derive(Debug, Hiarc, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ParticleType {
    Slice,
    Ball,
    Splats,
    Smoke,
    Shell,
    Explosions,
    Airjump,
    Hits,
    Stars,
    Snowflake,
    Sparkle,
}

#[derive(Debug, Clone, Hiarc)]
pub struct Particle {
    pub slice: Vec<TextureContainer>,
    pub ball: Vec<TextureContainer>,
    pub splats: Vec<TextureContainer>,
    pub smoke: Vec<TextureContainer>,
    pub shell: Vec<TextureContainer>,
    pub explosions: Vec<TextureContainer>,
    pub airjump: Vec<TextureContainer>,
    pub hits: Vec<TextureContainer>,
    pub stars: Vec<TextureContainer>,
    pub snowflakes: Vec<TextureContainer>,
    pub sparkle: Vec<TextureContainer>,
}

impl Particle {
    pub fn get_by_ty(&self, ty: ParticleType, rng_val: u64) -> &TextureContainer {
        let rng_val = rng_val as usize;
        match ty {
            ParticleType::Slice => self.slice.random_val_entry(rng_val),
            ParticleType::Ball => self.ball.random_val_entry(rng_val),
            ParticleType::Splats => self.splats.random_val_entry(rng_val),
            ParticleType::Smoke => self.smoke.random_val_entry(rng_val),
            ParticleType::Shell => self.shell.random_val_entry(rng_val),
            ParticleType::Explosions => self.explosions.random_val_entry(rng_val),
            ParticleType::Airjump => self.airjump.random_val_entry(rng_val),
            ParticleType::Hits => self.hits.random_val_entry(rng_val),
            ParticleType::Stars => self.stars.random_val_entry(rng_val),
            ParticleType::Snowflake => self.snowflakes.random_val_entry(rng_val),
            ParticleType::Sparkle => self.sparkle.random_val_entry(rng_val),
        }
    }

    pub fn len_by_ty(&self, ty: ParticleType) -> usize {
        match ty {
            ParticleType::Slice => self.slice.len(),
            ParticleType::Ball => self.ball.len(),
            ParticleType::Splats => self.splats.len(),
            ParticleType::Smoke => self.smoke.len(),
            ParticleType::Shell => self.shell.len(),
            ParticleType::Explosions => self.explosions.len(),
            ParticleType::Airjump => self.airjump.len(),
            ParticleType::Hits => self.hits.len(),
            ParticleType::Stars => self.stars.len(),
            ParticleType::Snowflake => self.snowflakes.len(),
            ParticleType::Sparkle => self.sparkle.len(),
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct LoadParticle {
    slice: Vec<ContainerItemLoadData>,
    ball: Vec<ContainerItemLoadData>,
    splats: Vec<ContainerItemLoadData>,

    smoke: Vec<ContainerItemLoadData>,
    shell: Vec<ContainerItemLoadData>,
    explosions: Vec<ContainerItemLoadData>,
    airjump: Vec<ContainerItemLoadData>,
    hits: Vec<ContainerItemLoadData>,
    stars: Vec<ContainerItemLoadData>,

    snowflakes: Vec<ContainerItemLoadData>,
    sparkle: Vec<ContainerItemLoadData>,

    particle_name: String,
}

impl LoadParticle {
    fn load_full(files: &mut FxHashMap<PathBuf, Vec<u8>>, file: Vec<u8>) -> anyhow::Result<()> {
        let mut mem: Vec<u8> = Default::default();
        let img: image_utils::png::PngResult<'_> =
            image_utils::png::load_png_image_as_rgba(&file, |width, height, bytes_per_pixel| {
                mem.resize(width * height * bytes_per_pixel, Default::default());
                &mut mem
            })?;
        let converted =
            assets_splitting::particles_split::split_06_particles(img.data, img.width, img.height)?;

        let mut insert_part = |name: &str, part: Particles06Part| -> anyhow::Result<()> {
            let file = image_utils::png::save_png_image(&part.data, part.width, part.height)?;

            files.insert(format!("{name}.png").into(), file);
            Ok(())
        };
        insert_part("slice_001", converted.slice)?;
        insert_part("ball_001", converted.ball)?;
        for (i, part) in converted.splat.into_iter().enumerate() {
            insert_part(&format!("splat_{:03}", i + 1), part)?;
        }
        insert_part("smoke_001", converted.smoke)?;
        insert_part("shell_001", converted.shell)?;

        for (i, part) in converted.explosion.into_iter().enumerate() {
            insert_part(&format!("explosion_{:03}", i + 1), part)?;
        }

        insert_part("airjump_001", converted.airjump)?;

        for (i, part) in converted.hit.into_iter().enumerate() {
            insert_part(&format!("hit_{:03}", i + 1), part)?;
        }

        Ok(())
    }

    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        files: &mut ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        particle_name: &str,
    ) -> anyhow::Result<Self> {
        let full_path: PathBuf = "full.png".into();
        if let Some(file) = files.files.remove(&full_path) {
            Self::load_full(&mut files.files, file)?;
        }

        Ok(Self {
            slice: load_file_part_list_and_upload(
                graphics_mt,
                files,
                default_files,
                particle_name,
                &[],
                "slice",
            )?,
            ball: load_file_part_list_and_upload(
                graphics_mt,
                files,
                default_files,
                particle_name,
                &[],
                "ball",
            )?,
            splats: load_file_part_list_and_upload(
                graphics_mt,
                files,
                default_files,
                particle_name,
                &[],
                "splat",
            )?,
            smoke: load_file_part_list_and_upload(
                graphics_mt,
                files,
                default_files,
                particle_name,
                &[],
                "smoke",
            )?,
            shell: load_file_part_list_and_upload(
                graphics_mt,
                files,
                default_files,
                particle_name,
                &[],
                "shell",
            )?,
            explosions: load_file_part_list_and_upload(
                graphics_mt,
                files,
                default_files,
                particle_name,
                &[],
                "explosion",
            )?,
            airjump: load_file_part_list_and_upload(
                graphics_mt,
                files,
                default_files,
                particle_name,
                &[],
                "airjump",
            )?,
            hits: load_file_part_list_and_upload(
                graphics_mt,
                files,
                default_files,
                particle_name,
                &[],
                "hit",
            )?,
            stars: load_file_part_list_and_upload(
                graphics_mt,
                files,
                default_files,
                particle_name,
                &[],
                "star",
            )?,
            snowflakes: load_file_part_list_and_upload(
                graphics_mt,
                files,
                default_files,
                particle_name,
                &[],
                "snowflake",
            )?,
            sparkle: load_file_part_list_and_upload(
                graphics_mt,
                files,
                default_files,
                particle_name,
                &[],
                "sparkle",
            )?,

            particle_name: particle_name.to_string(),
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

impl ContainerLoad<Particle> for LoadParticle {
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
    ) -> Particle {
        Particle {
            slice: self
                .slice
                .into_iter()
                .map(|obj| Self::load_file_into_texture(texture_handle, obj, &self.particle_name))
                .collect(),
            ball: self
                .ball
                .into_iter()
                .map(|obj| Self::load_file_into_texture(texture_handle, obj, &self.particle_name))
                .collect(),
            splats: self
                .splats
                .into_iter()
                .map(|splat| {
                    Self::load_file_into_texture(texture_handle, splat, &self.particle_name)
                })
                .collect(),

            smoke: self
                .smoke
                .into_iter()
                .map(|obj| Self::load_file_into_texture(texture_handle, obj, &self.particle_name))
                .collect(),
            shell: self
                .shell
                .into_iter()
                .map(|obj| Self::load_file_into_texture(texture_handle, obj, &self.particle_name))
                .collect(),
            explosions: self
                .explosions
                .into_iter()
                .map(|explosion| {
                    Self::load_file_into_texture(texture_handle, explosion, &self.particle_name)
                })
                .collect(),
            airjump: self
                .airjump
                .into_iter()
                .map(|obj| Self::load_file_into_texture(texture_handle, obj, &self.particle_name))
                .collect(),
            hits: self
                .hits
                .into_iter()
                .map(|hit| Self::load_file_into_texture(texture_handle, hit, &self.particle_name))
                .collect(),
            stars: self
                .stars
                .into_iter()
                .map(|star| Self::load_file_into_texture(texture_handle, star, &self.particle_name))
                .collect(),
            snowflakes: self
                .snowflakes
                .into_iter()
                .map(|obj| Self::load_file_into_texture(texture_handle, obj, &self.particle_name))
                .collect(),
            sparkle: self
                .sparkle
                .into_iter()
                .map(|obj| Self::load_file_into_texture(texture_handle, obj, &self.particle_name))
                .collect(),
        }
    }
}

pub type ParticlesContainer = Container<Particle, LoadParticle>;
pub const PARTICLES_CONTAINER_PATH: &str = "particles/";
