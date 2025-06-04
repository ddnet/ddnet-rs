use std::{path::PathBuf, sync::Arc};

use arrayvec::ArrayVec;

use assets_splitting::emoticon_split::Emoticon06Part;
use game_interface::types::emoticons::{EmoticonType, EnumCount};
use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::texture::texture::{GraphicsTextureHandle, TextureContainer},
};
use num_traits::FromPrimitive;
use rustc_hash::FxHashMap;
use sound::{sound_handle::SoundObjectHandle, sound_mt::SoundMultiThreaded};

use crate::container::{ContainerLoadedItem, ContainerLoadedItemDir};

use super::container::{
    load_file_part_and_upload, Container, ContainerItemLoadData, ContainerLoad,
};

#[derive(Debug, Clone)]
pub struct Emoticons {
    pub emoticons: [TextureContainer; EmoticonType::COUNT],
}

#[derive(Debug)]
pub struct LoadEmoticons {
    emoticons: [ContainerItemLoadData; EmoticonType::COUNT],

    emoticon_name: String,
}

impl LoadEmoticons {
    fn load_full(files: &mut FxHashMap<PathBuf, Vec<u8>>, file: Vec<u8>) -> anyhow::Result<()> {
        let mut mem: Vec<u8> = Default::default();
        let img: image_utils::png::PngResult<'_> =
            image_utils::png::load_png_image_as_rgba(&file, |width, height, bytes_per_pixel| {
                mem.resize(width * height * bytes_per_pixel, Default::default());
                &mut mem
            })?;
        let converted =
            assets_splitting::emoticon_split::split_06_emoticon(img.data, img.width, img.height)?;

        let mut insert_part = |name: &str, part: Emoticon06Part| -> anyhow::Result<()> {
            let file = image_utils::png::save_png_image(&part.data, part.width, part.height)?;

            files.insert(format!("{}.png", name).into(), file);
            Ok(())
        };
        insert_part("oop", converted.oop)?;
        insert_part("exclamation", converted.exclamation)?;
        insert_part("hearts", converted.hearts)?;
        insert_part("drop", converted.drop)?;
        insert_part("dotdot", converted.dotdot)?;
        insert_part("music", converted.music)?;
        insert_part("sorry", converted.sorry)?;
        insert_part("ghost", converted.ghost)?;
        insert_part("sushi", converted.sushi)?;
        insert_part("splattee", converted.splattee)?;
        insert_part("deviltee", converted.deviltee)?;
        insert_part("zomg", converted.zomg)?;
        insert_part("zzz", converted.zzz)?;
        insert_part("wtf", converted.wtf)?;
        insert_part("eyes", converted.eyes)?;
        insert_part("question", converted.question)?;

        Ok(())
    }

    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        files: &mut ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        emoticon_name: &str,
    ) -> anyhow::Result<Self> {
        let full_path: PathBuf = "full.png".into();
        if let Some(file) = files.files.remove(&full_path) {
            Self::load_full(&mut files.files, file)?;
        }

        let mut emoticons: [Option<ContainerItemLoadData>; EmoticonType::COUNT] =
            Default::default();
        for (i, emoticon) in emoticons.iter_mut().enumerate() {
            let emoticon_type = EmoticonType::from_usize(i).unwrap();

            let name: &'static str = emoticon_type.into();
            *emoticon = Some(
                load_file_part_and_upload(
                    graphics_mt,
                    files,
                    default_files,
                    emoticon_name,
                    &[],
                    &name.to_string().to_lowercase(),
                )?
                .img,
            );
        }

        Ok(Self {
            emoticons: emoticons
                .into_iter()
                .map(|item| item.unwrap())
                .collect::<ArrayVec<ContainerItemLoadData, { EmoticonType::COUNT }>>()
                .into_inner()
                .unwrap(),
            emoticon_name: emoticon_name.to_string(),
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

impl ContainerLoad<Emoticons> for LoadEmoticons {
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
    ) -> Emoticons {
        Emoticons {
            emoticons: self
                .emoticons
                .into_iter()
                .map(|e| Self::load_file_into_texture(texture_handle, e, &self.emoticon_name))
                .collect::<ArrayVec<TextureContainer, { EmoticonType::COUNT }>>()
                .into_inner()
                .unwrap(),
        }
    }
}

pub type EmoticonsContainer = Container<Emoticons, LoadEmoticons>;
pub const EMOTICONS_CONTAINER_PATH: &str = "emoticons/";
