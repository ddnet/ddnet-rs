use std::{path::PathBuf, rc::Rc, sync::Arc};

use arrayvec::ArrayVec;

use assets_splitting::skin_split::Skin06Part;
use fixed::{FixedI64, types::extra::U32};
use game_interface::types::{emoticons::EnumCount, render::character::TeeEye};
use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::texture::texture::{GraphicsTextureHandle, TextureContainer},
};
use graphics_types::{
    commands::TexFlags, rendering::ColorRgba, types::GraphicsMemoryAllocationType,
};
use hiarc::Hiarc;
use image_utils::{png::PngResultPersistent, utils::dilate_image};
use math::math::vector::vec3;
use rustc_hash::FxHashMap;
use sound::{
    sound_handle::SoundObjectHandle, sound_mt::SoundMultiThreaded,
    sound_mt_types::SoundBackendMemory, sound_object::SoundObject,
};

use crate::container::{
    ContainerLoadedItem, ContainerLoadedItemDir, load_sound_file_part_list_and_upload,
};

use super::container::{Container, ContainerItemLoadData, ContainerLoad, load_file_part_as_png};

#[derive(Debug, Hiarc, Clone)]
pub struct SkinMetricVariable {
    min_x: FixedI64<U32>,
    min_y: FixedI64<U32>,
    max_x: FixedI64<U32>,
    max_y: FixedI64<U32>,
}

impl Default for SkinMetricVariable {
    fn default() -> Self {
        Self {
            min_x: FixedI64::<U32>::MAX,
            min_y: FixedI64::<U32>::MAX,
            max_x: FixedI64::<U32>::MIN,
            max_y: FixedI64::<U32>::MIN,
        }
    }
}

impl SkinMetricVariable {
    // bb = bounding box
    fn width_bb(&self) -> FixedI64<U32> {
        self.max_x - self.min_x
    }
    fn height_bb(&self) -> FixedI64<U32> {
        self.max_y - self.min_y
    }

    pub fn width(&self) -> FixedI64<U32> {
        self.width_bb()
    }

    pub fn height(&self) -> FixedI64<U32> {
        self.height_bb()
    }

    pub fn x(&self) -> FixedI64<U32> {
        self.min_x
    }

    pub fn y(&self) -> FixedI64<U32> {
        self.min_y
    }

    pub fn from_texture(
        &mut self,
        img: &[u8],
        img_pitch: u32,
        img_x: u32,
        img_y: u32,
        check_width: u32,
        check_height: u32,
    ) {
        let mut max_y = 0;
        let mut min_y = check_height + 1;
        let mut max_x = 0;
        let mut min_x = check_width + 1;

        for y in 0..check_height {
            for x in 0..check_width {
                let offset_alpha = (y + img_y) * img_pitch + (x + img_x) * 4 + 3;
                let alpha_value = img[offset_alpha as usize];
                if alpha_value > 0 {
                    max_y = max_y.max(y + 1);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x + 1);
                    min_x = min_x.min(x);
                }
            }
        }

        self.min_x = self
            .min_x
            .min(FixedI64::<U32>::from_num(min_x) / FixedI64::<U32>::from_num(check_width));
        self.min_y = self
            .min_y
            .min(FixedI64::<U32>::from_num(min_y) / FixedI64::<U32>::from_num(check_height));
        self.max_x = self
            .max_x
            .max(FixedI64::<U32>::from_num(max_x) / FixedI64::<U32>::from_num(check_width));
        self.max_y = self
            .max_y
            .max(FixedI64::<U32>::from_num(max_y) / FixedI64::<U32>::from_num(check_height));
    }
}

#[derive(Debug, Hiarc, Default, Clone)]
pub struct SkinMetrics {
    pub body: SkinMetricVariable,
    pub feet: SkinMetricVariable,
}

#[derive(Debug, Hiarc)]
pub struct SkinSounds {
    pub ground_jump: Vec<SoundObject>,
    pub air_jump: Vec<SoundObject>,
    pub spawn: Vec<SoundObject>,
    pub death: Vec<SoundObject>,
    pub pain_short: Vec<SoundObject>,
    pub pain_long: Vec<SoundObject>,
    pub hit_weak: Vec<SoundObject>,
    pub hit_strong: Vec<SoundObject>,
    pub skid: Vec<SoundObject>,
}

#[derive(Debug, Hiarc)]
pub struct SkinTextures {
    pub body: TextureContainer,
    pub body_outline: TextureContainer,

    pub marking: TextureContainer,
    pub marking_outline: TextureContainer,

    pub decoration: TextureContainer,
    pub decoration_outline: TextureContainer,

    pub left_hand: TextureContainer,
    pub left_hand_outline: TextureContainer,

    pub right_hand: TextureContainer,
    pub right_hand_outline: TextureContainer,

    pub left_foot: TextureContainer,
    pub left_foot_outline: TextureContainer,

    pub right_foot: TextureContainer,
    pub right_foot_outline: TextureContainer,

    pub left_eyes: [TextureContainer; TeeEye::COUNT],
    pub right_eyes: [TextureContainer; TeeEye::COUNT],
}

#[derive(Debug, Hiarc)]
pub struct Skin {
    pub textures: SkinTextures,
    pub grey_scaled_textures: SkinTextures,
    pub frozen_textures: SkinTextures,
    pub metrics: SkinMetrics,

    pub blood_color: ColorRgba,

    pub sounds: SkinSounds,
}

#[derive(Debug, Hiarc)]
pub struct LoadSkinSounds {
    pub ground_jump: Vec<SoundBackendMemory>,
    pub air_jump: Vec<SoundBackendMemory>,
    pub spawn: Vec<SoundBackendMemory>,
    pub death: Vec<SoundBackendMemory>,
    pub pain_short: Vec<SoundBackendMemory>,
    pub pain_long: Vec<SoundBackendMemory>,
    pub hit_weak: Vec<SoundBackendMemory>,
    pub hit_strong: Vec<SoundBackendMemory>,
    pub skid: Vec<SoundBackendMemory>,
}

impl LoadSkinSounds {
    fn load_into_sound_object(self, sound_object_handle: &SoundObjectHandle) -> SkinSounds {
        SkinSounds {
            ground_jump: {
                let mut jumps: Vec<_> = Vec::new();
                for jump in self.ground_jump.into_iter() {
                    jumps.push(sound_object_handle.create(jump));
                }
                jumps
            },
            air_jump: {
                let mut jumps: Vec<_> = Vec::new();
                for jump in self.air_jump.into_iter() {
                    jumps.push(sound_object_handle.create(jump));
                }
                jumps
            },
            spawn: {
                let mut sounds: Vec<_> = Vec::new();
                for snd in self.spawn.into_iter() {
                    sounds.push(sound_object_handle.create(snd));
                }
                sounds
            },
            death: {
                let mut sounds: Vec<_> = Vec::new();
                for snd in self.death.into_iter() {
                    sounds.push(sound_object_handle.create(snd));
                }
                sounds
            },
            pain_short: self
                .pain_short
                .into_iter()
                .map(|snd| sound_object_handle.create(snd))
                .collect::<Vec<_>>(),
            pain_long: self
                .pain_long
                .into_iter()
                .map(|snd| sound_object_handle.create(snd))
                .collect::<Vec<_>>(),
            hit_strong: self
                .hit_strong
                .into_iter()
                .map(|snd| sound_object_handle.create(snd))
                .collect::<Vec<_>>(),
            hit_weak: self
                .hit_weak
                .into_iter()
                .map(|snd| sound_object_handle.create(snd))
                .collect::<Vec<_>>(),
            skid: self
                .skid
                .into_iter()
                .map(|snd| sound_object_handle.create(snd))
                .collect::<Vec<_>>(),
        }
    }
}

#[derive(Default, Clone)]
pub struct LoadSkinTexturesData {
    body: PngResultPersistent,
    body_outline: PngResultPersistent,

    marking: PngResultPersistent,
    marking_outline: PngResultPersistent,

    decoration: PngResultPersistent,
    decoration_outline: PngResultPersistent,

    left_hand: PngResultPersistent,
    left_hand_outline: PngResultPersistent,

    right_hand: PngResultPersistent,
    right_hand_outline: PngResultPersistent,

    left_foot: PngResultPersistent,
    left_foot_outline: PngResultPersistent,

    right_foot: PngResultPersistent,
    right_foot_outline: PngResultPersistent,

    left_eyes: [PngResultPersistent; TeeEye::COUNT],
    right_eyes: [PngResultPersistent; TeeEye::COUNT],
}

impl LoadSkinTexturesData {
    fn load_full(
        files: &mut FxHashMap<PathBuf, Vec<u8>>,
        file: Vec<u8>,
        skin_extra_path: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut mem: Vec<u8> = Default::default();
        let img: image_utils::png::PngResult<'_> =
            image_utils::png::load_png_image_as_rgba(&file, |width, height, bytes_per_pixel| {
                mem.resize(width * height * bytes_per_pixel, Default::default());
                &mut mem
            })?;
        let converted =
            assets_splitting::skin_split::split_06_skin(img.data, img.width, img.height)?;
        let base: PathBuf = if let Some(skin_extra_path) = skin_extra_path {
            skin_extra_path.into()
        } else {
            "".into()
        };
        let mut insert_part =
            |name: &str, part: Skin06Part, copy_right: bool| -> anyhow::Result<()> {
                let file = image_utils::png::save_png_image(&part.data, part.width, part.height)?;

                if copy_right {
                    files.insert(
                        base.join(format!("{}.png", name.replace("left", "right"))),
                        file.clone(),
                    );
                }
                files.insert(base.join(format!("{name}.png")), file);
                Ok(())
            };
        insert_part("body", converted.body, false)?;
        insert_part("body_outline", converted.body_outline, false)?;

        insert_part("hand_left", converted.hand, true)?;
        insert_part("hand_left_outline", converted.hand_outline, true)?;

        insert_part("foot_left", converted.foot, true)?;
        insert_part("foot_left_outline", converted.foot_outline, true)?;

        insert_part("eyes_left/normal", converted.eye_normal, true)?;
        insert_part("eyes_left/angry", converted.eye_angry, true)?;
        insert_part("eyes_left/pain", converted.eye_pain, true)?;
        insert_part("eyes_left/happy", converted.eye_happy, true)?;
        insert_part("eyes_left/dead", converted.eye_dead, true)?;
        insert_part("eyes_left/surprised", converted.eye_surprised, true)?;

        // insert_part("watermark", converted.watermark)?;
        Ok(())
    }

    pub(crate) fn load_skin(
        files: &mut ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        skin_name: &str,
        skin_extra_path: Option<&str>,
    ) -> anyhow::Result<Self> {
        // check if full skin or individual skin parts were used
        let full_path: PathBuf = if let Some(skin_extra_path) = skin_extra_path {
            skin_extra_path.into()
        } else {
            "".into()
        };
        let full_path = full_path.join("full.png");
        if let Some(file) = files.files.remove(&full_path) {
            Self::load_full(&mut files.files, file, skin_extra_path)?;
        }

        let load_eyes =
            |eye_name: &'static str| -> anyhow::Result<[PngResultPersistent; TeeEye::COUNT]> {
                {
                    let mut eyes: [PngResultPersistent; TeeEye::COUNT] = Default::default();
                    let extra_paths = [skin_extra_path.as_slice(), &[eye_name]].concat();
                    eyes[TeeEye::Angry as usize] = load_file_part_as_png(
                        files,
                        default_files,
                        skin_name,
                        extra_paths.as_slice(),
                        "angry",
                    )?
                    .png;
                    /*eyes[TeeEye::Dead as usize] = load_file_part_as_png(
                        files,
                        default_files,
                        skin_name,
                        extra_paths.as_slice(),
                        "dead",
                    )?;*/
                    eyes[TeeEye::Happy as usize] = load_file_part_as_png(
                        files,
                        default_files,
                        skin_name,
                        extra_paths.as_slice(),
                        "happy",
                    )?
                    .png;
                    eyes[TeeEye::Normal as usize] = load_file_part_as_png(
                        files,
                        default_files,
                        skin_name,
                        extra_paths.as_slice(),
                        "normal",
                    )?
                    .png;
                    eyes[TeeEye::Blink as usize] = load_file_part_as_png(
                        files,
                        default_files,
                        skin_name,
                        extra_paths.as_slice(),
                        "normal",
                    )?
                    .png;
                    eyes[TeeEye::Pain as usize] = load_file_part_as_png(
                        files,
                        default_files,
                        skin_name,
                        extra_paths.as_slice(),
                        "pain",
                    )?
                    .png;
                    eyes[TeeEye::Surprised as usize] = load_file_part_as_png(
                        files,
                        default_files,
                        skin_name,
                        extra_paths.as_slice(),
                        "surprised",
                    )?
                    .png;
                    Ok(eyes)
                }
            };

        Ok(Self {
            // body file
            body: load_file_part_as_png(
                files,
                default_files,
                skin_name,
                skin_extra_path.as_slice(),
                "body",
            )?
            .png,
            body_outline: load_file_part_as_png(
                files,
                default_files,
                skin_name,
                skin_extra_path.as_slice(),
                "body_outline",
            )?
            .png,

            // foot_left file
            left_foot: load_file_part_as_png(
                files,
                default_files,
                skin_name,
                skin_extra_path.as_slice(),
                "foot_left",
            )?
            .png,
            left_foot_outline: load_file_part_as_png(
                files,
                default_files,
                skin_name,
                skin_extra_path.as_slice(),
                "foot_left_outline",
            )?
            .png,

            // foot_right file
            right_foot: load_file_part_as_png(
                files,
                default_files,
                skin_name,
                skin_extra_path.as_slice(),
                "foot_right",
            )?
            .png,
            right_foot_outline: load_file_part_as_png(
                files,
                default_files,
                skin_name,
                skin_extra_path.as_slice(),
                "foot_right_outline",
            )?
            .png,

            // hand_left file
            left_hand: load_file_part_as_png(
                files,
                default_files,
                skin_name,
                skin_extra_path.as_slice(),
                "hand_left",
            )?
            .png,
            left_hand_outline: load_file_part_as_png(
                files,
                default_files,
                skin_name,
                skin_extra_path.as_slice(),
                "hand_left_outline",
            )?
            .png,

            // hand_right file
            right_hand: load_file_part_as_png(
                files,
                default_files,
                skin_name,
                skin_extra_path.as_slice(),
                "hand_right",
            )?
            .png,
            right_hand_outline: load_file_part_as_png(
                files,
                default_files,
                skin_name,
                skin_extra_path.as_slice(),
                "hand_right_outline",
            )?
            .png,

            // eyes file
            left_eyes: load_eyes("eyes_left")?,
            right_eyes: load_eyes("eyes_right")?,

            // decoration file
            decoration: load_file_part_as_png(
                files,
                default_files,
                skin_name,
                skin_extra_path.as_slice(),
                "decoration",
            )?
            .png,
            decoration_outline: load_file_part_as_png(
                files,
                default_files,
                skin_name,
                skin_extra_path.as_slice(),
                "decoration",
            )?
            .png,

            // marking file
            marking: load_file_part_as_png(
                files,
                default_files,
                skin_name,
                skin_extra_path.as_slice(),
                "marking",
            )?
            .png,
            marking_outline: load_file_part_as_png(
                files,
                default_files,
                skin_name,
                skin_extra_path.as_slice(),
                "marking",
            )?
            .png,
        })
    }

    fn load_single(
        graphics_mt: &GraphicsMultiThreaded,
        img: PngResultPersistent,
    ) -> ContainerItemLoadData {
        let mut img_mem = graphics_mt.mem_alloc(GraphicsMemoryAllocationType::TextureRgbaU8 {
            width: (img.width as usize).try_into().unwrap(),
            height: (img.height as usize).try_into().unwrap(),
            flags: TexFlags::empty(),
        });
        img_mem.as_mut_slice().copy_from_slice(&img.data);
        if let Err(err) = graphics_mt.try_flush_mem(&mut img_mem, true) {
            // Ignore the error, but log it.
            log::debug!("err while flushing memory: {err}");
        }
        ContainerItemLoadData {
            width: img.width,
            height: img.height,
            depth: 1,
            data: img_mem,
        }
    }

    fn load_into_texture(self, graphics_mt: &GraphicsMultiThreaded) -> LoadSkinTextures {
        LoadSkinTextures {
            body: Self::load_single(graphics_mt, self.body),
            body_outline: Self::load_single(graphics_mt, self.body_outline),
            marking: Self::load_single(graphics_mt, self.marking),
            marking_outline: Self::load_single(graphics_mt, self.marking_outline),
            decoration: Self::load_single(graphics_mt, self.decoration),
            decoration_outline: Self::load_single(graphics_mt, self.decoration_outline),
            left_hand: Self::load_single(graphics_mt, self.left_hand),
            left_hand_outline: Self::load_single(graphics_mt, self.left_hand_outline),
            right_hand: Self::load_single(graphics_mt, self.right_hand),
            right_hand_outline: Self::load_single(graphics_mt, self.right_hand_outline),
            left_foot: Self::load_single(graphics_mt, self.left_foot),
            left_foot_outline: Self::load_single(graphics_mt, self.left_foot_outline),
            right_foot: Self::load_single(graphics_mt, self.right_foot),
            right_foot_outline: Self::load_single(graphics_mt, self.right_foot_outline),
            left_eyes: self
                .left_eyes
                .into_iter()
                .map(|eye| Self::load_single(graphics_mt, eye))
                .collect::<ArrayVec<_, { TeeEye::COUNT }>>()
                .into_inner()
                .unwrap(),
            right_eyes: self
                .right_eyes
                .into_iter()
                .map(|eye| Self::load_single(graphics_mt, eye))
                .collect::<ArrayVec<_, { TeeEye::COUNT }>>()
                .into_inner()
                .unwrap(),
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct LoadSkinTextures {
    body: ContainerItemLoadData,
    body_outline: ContainerItemLoadData,

    marking: ContainerItemLoadData,
    marking_outline: ContainerItemLoadData,

    decoration: ContainerItemLoadData,
    decoration_outline: ContainerItemLoadData,

    left_hand: ContainerItemLoadData,
    left_hand_outline: ContainerItemLoadData,

    right_hand: ContainerItemLoadData,
    right_hand_outline: ContainerItemLoadData,

    left_foot: ContainerItemLoadData,
    left_foot_outline: ContainerItemLoadData,

    right_foot: ContainerItemLoadData,
    right_foot_outline: ContainerItemLoadData,

    left_eyes: [ContainerItemLoadData; TeeEye::COUNT],
    right_eyes: [ContainerItemLoadData; TeeEye::COUNT],
}

impl LoadSkinTextures {
    fn load_skin_into_texture(
        self,
        skin_name: &str,
        texture_handle: &GraphicsTextureHandle,
    ) -> SkinTextures {
        SkinTextures {
            body: LoadSkin::load_file_into_texture(texture_handle, self.body, skin_name),
            body_outline: LoadSkin::load_file_into_texture(
                texture_handle,
                self.body_outline,
                skin_name,
            ),
            marking: LoadSkin::load_file_into_texture(texture_handle, self.marking, skin_name),
            marking_outline: LoadSkin::load_file_into_texture(
                texture_handle,
                self.marking_outline,
                skin_name,
            ),
            decoration: LoadSkin::load_file_into_texture(
                texture_handle,
                self.decoration,
                skin_name,
            ),
            decoration_outline: LoadSkin::load_file_into_texture(
                texture_handle,
                self.decoration_outline,
                skin_name,
            ),
            left_hand: LoadSkin::load_file_into_texture(texture_handle, self.left_hand, skin_name),
            left_hand_outline: LoadSkin::load_file_into_texture(
                texture_handle,
                self.left_hand_outline,
                skin_name,
            ),
            right_hand: LoadSkin::load_file_into_texture(
                texture_handle,
                self.right_hand,
                skin_name,
            ),
            right_hand_outline: LoadSkin::load_file_into_texture(
                texture_handle,
                self.right_hand_outline,
                skin_name,
            ),
            left_foot: LoadSkin::load_file_into_texture(texture_handle, self.left_foot, skin_name),
            left_foot_outline: LoadSkin::load_file_into_texture(
                texture_handle,
                self.left_foot_outline,
                skin_name,
            ),
            right_foot: LoadSkin::load_file_into_texture(
                texture_handle,
                self.right_foot,
                skin_name,
            ),
            right_foot_outline: LoadSkin::load_file_into_texture(
                texture_handle,
                self.right_foot_outline,
                skin_name,
            ),
            left_eyes: self
                .left_eyes
                .into_iter()
                .map(|eye| LoadSkin::load_file_into_texture(texture_handle, eye, skin_name))
                .collect::<ArrayVec<_, { TeeEye::COUNT }>>()
                .into_inner()
                .unwrap(),
            right_eyes: self
                .right_eyes
                .into_iter()
                .map(|eye| LoadSkin::load_file_into_texture(texture_handle, eye, skin_name))
                .collect::<ArrayVec<_, { TeeEye::COUNT }>>()
                .into_inner()
                .unwrap(),
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct LoadSkin {
    textures: LoadSkinTextures,
    grey_scaled_textures: LoadSkinTextures,
    frozen_textures: LoadSkinTextures,

    blood_color: ColorRgba,

    metrics: SkinMetrics,

    sound: LoadSkinSounds,

    skin_name: String,
}

impl LoadSkin {
    fn get_blood_color(body_img: &[u8], body_width: usize, body_height: usize) -> ColorRgba {
        let pixel_step = 4;

        // dig out blood color
        let mut colors: [i32; 3] = Default::default();
        for y in 0..body_height {
            for x in 0..body_width {
                let alpha_value = body_img[y + x * pixel_step + 3];
                if alpha_value > 128 {
                    colors[0] += body_img[y + x * pixel_step + 0] as i32;
                    colors[1] += body_img[y + x * pixel_step + 1] as i32;
                    colors[2] += body_img[y + x * pixel_step + 2] as i32;
                }
            }
        }
        if colors[0] != 0 && colors[1] != 0 && colors[2] != 0 {
            let color = vec3 {
                x: colors[0] as f32,
                y: colors[1] as f32,
                z: colors[2] as f32,
            }
            .normalize();
            ColorRgba::new(color.x, color.y, color.z, 1.0)
        } else {
            ColorRgba::new(0.0, 0.0, 0.0, 1.0)
        }
    }

    fn make_grey_scale(tp: &rayon::ThreadPool, tex: &mut PngResultPersistent) {
        let pixel_step = 4;

        // make the texture gray scale
        for i in 0..tex.width as usize * tex.height as usize {
            let [r, g, b] = tex.data[i * pixel_step..=i * pixel_step + 2] else {
                panic!("greyscale rgb assign bug, can't happen");
            };
            let luma = (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) as u8;

            tex.data[i * pixel_step..=i * pixel_step + 2].copy_from_slice(&[luma, luma, luma]);
        }
        dilate_image(
            tp,
            tex.data.as_mut(),
            tex.width as usize,
            tex.height as usize,
            4,
        );
    }

    fn frost_noise(x: u32, y: u32) -> f32 {
        let mut v = x.wrapping_mul(0x5165_7a7f) ^ y.wrapping_mul(0x27d4_eb2d);
        v ^= v >> 15;
        v = v.wrapping_mul(0x85eb_ca6b);
        ((v >> 24) & 0xff) as f32 / 255.0
    }

    fn make_frozen(tp: &rayon::ThreadPool, tex: &mut PngResultPersistent) {
        if tex.width == 0 || tex.height == 0 {
            return;
        }

        const FROST_PALETTE: [[f32; 3]; 4] = [
            [0.10, 0.26, 0.56],
            [0.32, 0.58, 0.90],
            [0.62, 0.84, 1.0],
            [0.92, 0.98, 1.0],
        ];

        let pixel_step = 4;
        let width = tex.width as usize;
        let height = tex.height as usize;
        let pitch = width * pixel_step;
        let original = tex.data.clone();

        let mut tone = vec![0.0f32; width * height];
        let mut highlight = vec![0.0f32; width * height];
        let mut top_surface: Vec<Option<usize>> = vec![None; width];

        for y in 0..height {
            for x in 0..width {
                let idx = y * pitch + x * pixel_step;
                let alpha = original[idx + 3];
                if alpha == 0 {
                    continue;
                }

                if top_surface[x].is_none() && original[idx + 3] > 96 {
                    top_surface[x] = Some(y);
                }

                let r = original[idx] as f32 / 255.0;
                let g = original[idx + 1] as f32 / 255.0;
                let b = original[idx + 2] as f32 / 255.0;

                let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                let inverse_luma = (1.0 - luma).clamp(0.0, 1.0);

                let noise_primary = Self::frost_noise(x as u32, y as u32);
                let noise_secondary = Self::frost_noise(x as u32 + 37, y as u32 ^ 0x15_72);
                let noise_tertiary = Self::frost_noise(x as u32 ^ 0x4213, y as u32 + 91);
                let avg_noise =
                    (noise_primary + noise_secondary * 0.5 + noise_tertiary * 0.25) / 1.75;
                let centered_noise = (avg_noise - 0.5) * 0.12;

                let neighbors = [
                    (-1, 0),
                    (1, 0),
                    (0, -1),
                    (0, 1),
                    (-1, -1),
                    (1, -1),
                    (-1, 1),
                    (1, 1),
                ];
                let neighbor_count = neighbors.len() as f32;
                let mut edge = 0.0;
                for (dx, dy) in neighbors {
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize {
                        edge += 1.0;
                        continue;
                    }
                    let n_idx = ny as usize * pitch + nx as usize * pixel_step;
                    if original[n_idx + 3] == 0 {
                        edge += 1.0;
                    }
                }
                let edge_factor = (edge / neighbor_count).clamp(0.0, 1.0);

                let tone_value =
                    (inverse_luma * 0.65 + edge_factor * 0.25 + centered_noise).clamp(0.0, 1.0);
                tone[y * width + x] = tone_value;

                let shine_value =
                    (edge_factor.powf(1.4) * 0.7 + avg_noise.powf(4.0) * 0.3).clamp(0.0, 1.0);
                highlight[y * width + x] = shine_value;
            }
        }

        const GAUSS_KERNEL: [f32; 7] = [0.00443, 0.0540, 0.2420, 0.3991, 0.2420, 0.0540, 0.00443];
        let radius = 3i32;
        let mut blur_temp = vec![0.0f32; width * height];

        for y in 0..height {
            for x in 0..width {
                let mut acc = 0.0;
                for k in -radius..=radius {
                    let sx = (x as i32 + k).clamp(0, width as i32 - 1) as usize;
                    let weight = GAUSS_KERNEL[(k + radius) as usize];
                    acc += tone[y * width + sx] * weight;
                }
                blur_temp[y * width + x] = acc;
            }
        }
        for y in 0..height {
            for x in 0..width {
                let mut acc = 0.0;
                for k in -radius..=radius {
                    let sy = (y as i32 + k).clamp(0, height as i32 - 1) as usize;
                    let weight = GAUSS_KERNEL[(k + radius) as usize];
                    acc += blur_temp[sy * width + x] * weight;
                }
                tone[y * width + x] = acc;
            }
        }

        let mut icicle_mask = vec![0.0f32; width * height];
        let mut icicle_tone = vec![0.0f32; width * height];
        let mut icicle_shine = vec![0.0f32; width * height];
        let mut solid_run_length = vec![0usize; width * height];
        let mut solid_alpha_acc = vec![0.0f32; width * height];
        let max_length = ((height as f32) * 0.28).round() as usize;

        for x in 0..width {
            let Some(top) = top_surface[x] else {
                continue;
            };

            let spawn = Self::frost_noise(x as u32 ^ 0x9e37_79b9, top as u32 + 0x2d);
            if spawn < 0.62 {
                continue;
            }

            let mut length =
                ((spawn - 0.62) / 0.38 * max_length as f32).clamp(0.0, max_length as f32);
            length = length.max(3.0);
            let length = length.round() as usize;
            if length < 3 {
                continue;
            }

            let base_tone = tone[top * width + x];
            let base_shine = highlight[top * width + x].max(0.35);
            let sparkle_seed = Self::frost_noise(x as u32 + 0x1357, top as u32 ^ 0x2468);

            for offset in 0..length {
                let y = top + offset;
                if y >= height {
                    break;
                }

                let alpha_idx = y * pitch + x * pixel_step + 3;
                let alpha = original[alpha_idx] as f32 / 255.0;
                let density = alpha.powf(2.2);
                if density < 0.35 {
                    break;
                }

                let idx_current = y * width + x;
                let prev_index = if offset == 0 {
                    None
                } else {
                    Some((y - 1) * width + x)
                };
                let previous_run = prev_index.map_or(0, |idx| solid_run_length[idx]);
                let previous_alpha = prev_index.map_or(1.0, |idx| solid_alpha_acc[idx]);

                solid_run_length[idx_current] = previous_run + 1;
                solid_alpha_acc[idx_current] = previous_alpha * 0.6 + density * 0.4;

                let progress = offset as f32 / length as f32;
                let taper =
                    (1.0 - progress).powf(1.8) * (density * previous_alpha.max(0.35)).powf(0.7);
                let base_spread = 1.0 + (1.0 - progress) * 2.3;
                let max_dx = base_spread.ceil() as i32;
                for dx in -max_dx..=max_dx {
                    let nx = x as i32 + dx;
                    if nx < 0 || nx >= width as i32 {
                        continue;
                    }

                    let dist = (dx as f32).abs() / base_spread;
                    if dist > 1.1 {
                        continue;
                    }
                    let radial = (1.0 - dist.powf(1.45)).clamp(0.0, 1.0);
                    let neighbor_alpha =
                        original[y * pitch + nx as usize * pixel_step + 3] as f32 / 255.0;
                    let cross_density = density * neighbor_alpha;
                    let weight = (taper * radial * (1.0 - 0.25 * progress)
                        + sparkle_seed * 0.05
                        + cross_density * 0.1)
                        .clamp(0.0, 1.0);
                    if weight <= 0.001 {
                        continue;
                    }

                    let index = y * width + nx as usize;
                    let strength = weight;
                    if strength > icicle_mask[index] {
                        icicle_mask[index] = strength;
                        let tone_boost =
                            (base_tone * 0.6 + (0.55 + 0.35 * (1.0 - progress))).clamp(0.0, 1.0);
                        icicle_tone[index] = tone_boost;
                        icicle_shine[index] =
                            (base_shine + 0.25 + sparkle_seed * 0.15 + (1.0 - progress) * 0.3)
                                .clamp(0.0, 1.0);
                    }
                }
            }
        }

        const ICICLE_KERNEL: [f32; 5] = [0.0625, 0.25, 0.375, 0.25, 0.0625];
        let kernel_radius = (ICICLE_KERNEL.len() / 2) as i32;
        let blur_field = |src: &[f32]| -> Vec<f32> {
            let mut tmp = vec![0.0f32; width * height];
            let mut dst = vec![0.0f32; width * height];

            for y in 0..height {
                for x in 0..width {
                    let mut acc = 0.0f32;
                    for (k_idx, weight) in ICICLE_KERNEL.iter().enumerate() {
                        let k = k_idx as i32 - kernel_radius;
                        let sx = (x as i32 + k).clamp(0, width as i32 - 1) as usize;
                        acc += src[y * width + sx] * weight;
                    }
                    tmp[y * width + x] = acc;
                }
            }

            for y in 0..height {
                for x in 0..width {
                    let mut acc = 0.0f32;
                    for (k_idx, weight) in ICICLE_KERNEL.iter().enumerate() {
                        let k = k_idx as i32 - kernel_radius;
                        let sy = (y as i32 + k).clamp(0, height as i32 - 1) as usize;
                        acc += tmp[sy * width + x] * weight;
                    }
                    dst[y * width + x] = acc;
                }
            }

            dst
        };

        let blurred_mask = blur_field(&icicle_mask);
        let weighted_tone: Vec<f32> = icicle_tone
            .iter()
            .zip(icicle_mask.iter())
            .map(|(tone, mask)| tone * mask)
            .collect();
        let weighted_shine: Vec<f32> = icicle_shine
            .iter()
            .zip(icicle_mask.iter())
            .map(|(shine, mask)| shine * mask)
            .collect();
        let blurred_tone = blur_field(&weighted_tone);
        let blurred_shine = blur_field(&weighted_shine);

        let field_len = width * height;
        for i in 0..field_len {
            let m = blurred_mask[i].clamp(0.0, 1.0).powf(0.82);
            if m > 0.002 {
                let denom = m.max(1e-4);
                icicle_mask[i] = m;
                icicle_tone[i] = (blurred_tone[i] / denom).clamp(0.0, 1.0);
                icicle_shine[i] = (blurred_shine[i] / denom).clamp(0.0, 1.0);
            } else {
                icicle_mask[i] = 0.0;
                icicle_tone[i] = 0.0;
                icicle_shine[i] = 0.0;
            }
        }

        for y in 0..height {
            for x in 0..width {
                let idx_pix = y * width + x;
                let mut tone_value = tone[idx_pix];
                let mut shine_value = highlight[idx_pix];
                let icicle = icicle_mask[idx_pix];
                if icicle > 0.0 {
                    tone_value = tone_value.max(icicle_tone[idx_pix]);
                    shine_value = shine_value.max(icicle_shine[idx_pix]);
                }
                let idx = y * pitch + x * pixel_step;
                let alpha = original[idx + 3];
                if alpha == 0 && icicle <= 0.0 {
                    continue;
                }

                let max_index = FROST_PALETTE.len() as f32 - 1.0;
                let palette_pos = (tone_value * max_index).clamp(0.0, max_index);
                let base_idx = palette_pos.floor() as usize;
                let next_idx = (base_idx + 1).min(FROST_PALETTE.len() - 1);
                let mix = if next_idx == base_idx {
                    0.0
                } else {
                    palette_pos - base_idx as f32
                };

                let base = FROST_PALETTE[base_idx];
                let next = FROST_PALETTE[next_idx];
                let mut frost_tint = [
                    base[0] + (next[0] - base[0]) * mix,
                    base[1] + (next[1] - base[1]) * mix,
                    base[2] + (next[2] - base[2]) * mix,
                ];

                let highlight_mix = (shine_value * 0.55 + icicle * 0.3).clamp(0.0, 0.7);
                frost_tint.iter_mut().for_each(|c| {
                    *c = (*c + highlight_mix * (1.0 - *c)).clamp(0.0, 1.0);
                });

                let mut final_alpha = alpha as f32;
                if icicle > 0.0 {
                    let icicle_alpha =
                        ((icicle * 0.9) + icicle.powf(1.6) * 0.4).clamp(0.0, 1.0) * 255.0;
                    final_alpha = final_alpha.max(icicle_alpha);
                }

                if final_alpha <= 0.0 {
                    continue;
                }

                final_alpha = (final_alpha * (1.0 + 0.08 * shine_value + 0.04 * tone_value))
                    .clamp(0.0, 255.0);
                final_alpha = final_alpha.max(alpha as f32);
                let final_a = final_alpha.round();

                tex.data[idx] = (frost_tint[0] * 255.0).round().clamp(0.0, 255.0) as u8;
                tex.data[idx + 1] = (frost_tint[1] * 255.0).round().clamp(0.0, 255.0) as u8;
                tex.data[idx + 2] = (frost_tint[2] * 255.0).round().clamp(0.0, 255.0) as u8;
                tex.data[idx + 3] = final_a as u8;
            }
        }
        dilate_image(
            tp,
            tex.data.as_mut(),
            tex.width as usize,
            tex.height as usize,
            4,
        );
    }

    fn grey_scale(
        tp: &rayon::ThreadPool,
        body_and_outline: (&mut PngResultPersistent, &mut PngResultPersistent),
        left_hand_and_outline: (&mut PngResultPersistent, &mut PngResultPersistent),
        right_hand_and_outline: (&mut PngResultPersistent, &mut PngResultPersistent),
        left_foot_and_outline: (&mut PngResultPersistent, &mut PngResultPersistent),
        right_foot_and_outline: (&mut PngResultPersistent, &mut PngResultPersistent),
        left_eyes: &mut [PngResultPersistent; TeeEye::COUNT],
        right_eyes: &mut [PngResultPersistent; TeeEye::COUNT],
    ) {
        let pixel_step = 4;
        // create grey scales
        let (body, body_outline) = body_and_outline;
        Self::make_grey_scale(tp, body);
        Self::make_grey_scale(tp, body_outline);

        let (left_hand, left_hand_outline) = left_hand_and_outline;
        Self::make_grey_scale(tp, left_hand);
        Self::make_grey_scale(tp, left_hand_outline);

        let (right_hand, right_hand_outline) = right_hand_and_outline;
        Self::make_grey_scale(tp, right_hand);
        Self::make_grey_scale(tp, right_hand_outline);

        let (left_foot, left_foot_outline) = left_foot_and_outline;
        Self::make_grey_scale(tp, left_foot);
        Self::make_grey_scale(tp, left_foot_outline);

        let (right_foot, right_foot_outline) = right_foot_and_outline;
        Self::make_grey_scale(tp, right_foot);
        Self::make_grey_scale(tp, right_foot_outline);

        left_eyes.iter_mut().for_each(|tex| {
            Self::make_grey_scale(tp, tex);
        });

        right_eyes.iter_mut().for_each(|tex| {
            Self::make_grey_scale(tp, tex);
        });

        let mut freq: [i32; 256] = [0; 256];
        let mut org_weight: u8 = 1;
        let new_weight: u8 = 192;

        let body_pitch = body.width as usize * pixel_step;

        // find most common non-zero frequence
        for y in 0..body.height as usize {
            for x in 0..body.width as usize {
                if body.data[y * body_pitch + x * pixel_step + 3] > 128 {
                    freq[body.data[y * body_pitch + x * pixel_step] as usize] += 1;
                }
            }
        }

        for i in 1..=255 {
            if freq[org_weight as usize] < freq[i as usize] {
                org_weight = i;
            }
        }

        // reorder
        let inv_org_weight = 255 - org_weight;
        let inv_new_weight = 255 - new_weight;
        for y in 0..body.height as usize {
            for x in 0..body.width as usize {
                let mut v = body.data[y * body_pitch + x * pixel_step];
                if v <= org_weight {
                    v = ((v as f32 / org_weight as f32) * new_weight as f32) as u8;
                } else {
                    v = (((v - org_weight) as f32 / inv_org_weight as f32) * inv_new_weight as f32
                        + new_weight as f32) as u8;
                }
                body.data[y * body_pitch + x * pixel_step] = v;
                body.data[y * body_pitch + x * pixel_step + 1] = v;
                body.data[y * body_pitch + x * pixel_step + 2] = v;
            }
        }
        dilate_image(
            tp,
            body.data.as_mut(),
            body.width as usize,
            body.height as usize,
            4,
        );
    }

    fn freeze(
        tp: &rayon::ThreadPool,
        body_and_outline: (&mut PngResultPersistent, &mut PngResultPersistent),
        marking_and_outline: (&mut PngResultPersistent, &mut PngResultPersistent),
        decoration_and_outline: (&mut PngResultPersistent, &mut PngResultPersistent),
        left_hand_and_outline: (&mut PngResultPersistent, &mut PngResultPersistent),
        right_hand_and_outline: (&mut PngResultPersistent, &mut PngResultPersistent),
        left_foot_and_outline: (&mut PngResultPersistent, &mut PngResultPersistent),
        right_foot_and_outline: (&mut PngResultPersistent, &mut PngResultPersistent),
        left_eyes: &mut [PngResultPersistent; TeeEye::COUNT],
        right_eyes: &mut [PngResultPersistent; TeeEye::COUNT],
    ) {
        let (body, body_outline) = body_and_outline;
        Self::make_frozen(tp, body);
        Self::make_frozen(tp, body_outline);

        let (marking, marking_outline) = marking_and_outline;
        Self::make_frozen(tp, marking);
        Self::make_frozen(tp, marking_outline);

        let (decoration, decoration_outline) = decoration_and_outline;
        Self::make_frozen(tp, decoration);
        Self::make_frozen(tp, decoration_outline);

        let (left_hand, left_hand_outline) = left_hand_and_outline;
        Self::make_frozen(tp, left_hand);
        Self::make_frozen(tp, left_hand_outline);

        let (right_hand, right_hand_outline) = right_hand_and_outline;
        Self::make_frozen(tp, right_hand);
        Self::make_frozen(tp, right_hand_outline);

        let (left_foot, left_foot_outline) = left_foot_and_outline;
        Self::make_frozen(tp, left_foot);
        Self::make_frozen(tp, left_foot_outline);

        let (right_foot, right_foot_outline) = right_foot_and_outline;
        Self::make_frozen(tp, right_foot);
        Self::make_frozen(tp, right_foot_outline);

        left_eyes
            .iter_mut()
            .for_each(|eye| Self::make_frozen(tp, eye));
        right_eyes
            .iter_mut()
            .for_each(|eye| Self::make_frozen(tp, eye));
    }

    pub(crate) fn new(
        tp: &rayon::ThreadPool,
        graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
        files: &mut ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        skin_name: &str,
        skin_extra_path: Option<&str>,
    ) -> anyhow::Result<Self> {
        let textures_data =
            LoadSkinTexturesData::load_skin(files, default_files, skin_name, skin_extra_path)?;

        let mut grey_scaled_textures_data = textures_data.clone();
        Self::grey_scale(
            tp,
            (
                &mut grey_scaled_textures_data.body,
                &mut grey_scaled_textures_data.body_outline,
            ),
            (
                &mut grey_scaled_textures_data.left_hand,
                &mut grey_scaled_textures_data.left_hand_outline,
            ),
            (
                &mut grey_scaled_textures_data.right_hand,
                &mut grey_scaled_textures_data.right_hand_outline,
            ),
            (
                &mut grey_scaled_textures_data.left_foot,
                &mut grey_scaled_textures_data.left_foot_outline,
            ),
            (
                &mut grey_scaled_textures_data.right_foot,
                &mut grey_scaled_textures_data.right_foot_outline,
            ),
            &mut grey_scaled_textures_data.left_eyes,
            &mut grey_scaled_textures_data.right_eyes,
        );

        let mut frozen_textures_data = textures_data.clone();
        Self::freeze(
            tp,
            (
                &mut frozen_textures_data.body,
                &mut frozen_textures_data.body_outline,
            ),
            (
                &mut frozen_textures_data.marking,
                &mut frozen_textures_data.marking_outline,
            ),
            (
                &mut frozen_textures_data.decoration,
                &mut frozen_textures_data.decoration_outline,
            ),
            (
                &mut frozen_textures_data.left_hand,
                &mut frozen_textures_data.left_hand_outline,
            ),
            (
                &mut frozen_textures_data.right_hand,
                &mut frozen_textures_data.right_hand_outline,
            ),
            (
                &mut frozen_textures_data.left_foot,
                &mut frozen_textures_data.left_foot_outline,
            ),
            (
                &mut frozen_textures_data.right_foot,
                &mut frozen_textures_data.right_foot_outline,
            ),
            &mut frozen_textures_data.left_eyes,
            &mut frozen_textures_data.right_eyes,
        );

        let mut metrics_body = SkinMetricVariable::default();
        metrics_body.from_texture(
            &textures_data.body.data,
            textures_data.body.width * 4,
            0,
            0,
            textures_data.body.width,
            textures_data.body.height,
        );
        metrics_body.from_texture(
            &textures_data.body_outline.data,
            textures_data.body_outline.width * 4,
            0,
            0,
            textures_data.body_outline.width,
            textures_data.body_outline.height,
        );

        let mut metrics_feet = SkinMetricVariable::default();
        metrics_feet.from_texture(
            &textures_data.left_foot.data,
            textures_data.left_foot.width * 4,
            0,
            0,
            textures_data.left_foot.width,
            textures_data.left_foot.height,
        );
        metrics_feet.from_texture(
            &textures_data.left_foot_outline.data,
            textures_data.left_foot_outline.width * 4,
            0,
            0,
            textures_data.left_foot_outline.width,
            textures_data.left_foot_outline.height,
        );

        let blood_color = Self::get_blood_color(
            &textures_data.body.data,
            textures_data.body.width as usize,
            textures_data.body.height as usize,
        );
        let textures = textures_data.load_into_texture(graphics_mt);
        let grey_scaled_textures = grey_scaled_textures_data.load_into_texture(graphics_mt);
        let frozen_textures = frozen_textures_data.load_into_texture(graphics_mt);

        Ok(Self {
            blood_color,
            metrics: SkinMetrics {
                body: metrics_body,
                feet: metrics_feet,
            },

            textures,
            grey_scaled_textures,
            frozen_textures,

            sound: LoadSkinSounds {
                ground_jump: load_sound_file_part_list_and_upload(
                    sound_mt,
                    files,
                    default_files,
                    skin_name,
                    &[skin_extra_path.as_slice(), &["audio"]].concat(),
                    "ground_jump",
                )?,
                air_jump: load_sound_file_part_list_and_upload(
                    sound_mt,
                    files,
                    default_files,
                    skin_name,
                    &[skin_extra_path.as_slice(), &["audio"]].concat(),
                    "air_jump",
                )?,
                spawn: load_sound_file_part_list_and_upload(
                    sound_mt,
                    files,
                    default_files,
                    skin_name,
                    &[skin_extra_path.as_slice(), &["audio"]].concat(),
                    "spawn",
                )?,
                death: load_sound_file_part_list_and_upload(
                    sound_mt,
                    files,
                    default_files,
                    skin_name,
                    &[skin_extra_path.as_slice(), &["audio"]].concat(),
                    "death",
                )?,
                pain_short: load_sound_file_part_list_and_upload(
                    sound_mt,
                    files,
                    default_files,
                    skin_name,
                    &[skin_extra_path.as_slice(), &["audio"]].concat(),
                    "pain_short",
                )?,
                pain_long: load_sound_file_part_list_and_upload(
                    sound_mt,
                    files,
                    default_files,
                    skin_name,
                    &[skin_extra_path.as_slice(), &["audio"]].concat(),
                    "pain_long",
                )?,
                hit_strong: load_sound_file_part_list_and_upload(
                    sound_mt,
                    files,
                    default_files,
                    skin_name,
                    &[skin_extra_path.as_slice(), &["audio"]].concat(),
                    "hit_strong",
                )?,
                hit_weak: load_sound_file_part_list_and_upload(
                    sound_mt,
                    files,
                    default_files,
                    skin_name,
                    &[skin_extra_path.as_slice(), &["audio"]].concat(),
                    "hit_weak",
                )?,
                skid: load_sound_file_part_list_and_upload(
                    sound_mt,
                    files,
                    default_files,
                    skin_name,
                    &[skin_extra_path.as_slice(), &["audio"]].concat(),
                    "skid",
                )?,
            },

            skin_name: skin_name.to_string(),
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

impl ContainerLoad<Rc<Skin>> for LoadSkin {
    fn load(
        item_name: &str,
        files: ContainerLoadedItem,
        default_files: &ContainerLoadedItemDir,
        runtime_thread_pool: &Arc<rayon::ThreadPool>,
        graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
    ) -> anyhow::Result<Self> {
        match files {
            ContainerLoadedItem::Directory(mut files) => Self::new(
                runtime_thread_pool,
                graphics_mt,
                sound_mt,
                &mut files,
                default_files,
                item_name,
                None,
            ),
            ContainerLoadedItem::SingleFile(file) => {
                let mut files: FxHashMap<PathBuf, Vec<u8>> = Default::default();

                files.insert("full.png".into(), file);

                let mut files = ContainerLoadedItemDir::new(files);
                Self::new(
                    runtime_thread_pool,
                    graphics_mt,
                    sound_mt,
                    &mut files,
                    default_files,
                    item_name,
                    None,
                )
            }
        }
    }

    fn convert(
        self,
        texture_handle: &GraphicsTextureHandle,
        sound_object_handle: &SoundObjectHandle,
    ) -> Rc<Skin> {
        Rc::new(Skin {
            textures: self
                .textures
                .load_skin_into_texture(&self.skin_name, texture_handle),
            grey_scaled_textures: self
                .grey_scaled_textures
                .load_skin_into_texture(&self.skin_name, texture_handle),
            frozen_textures: self
                .frozen_textures
                .load_skin_into_texture(&self.skin_name, texture_handle),
            metrics: self.metrics,
            blood_color: self.blood_color,

            sounds: self.sound.load_into_sound_object(sound_object_handle),
        })
    }
}

pub type SkinContainer = Container<Rc<Skin>, LoadSkin>;
pub const SKIN_CONTAINER_PATH: &str = "skins/";
