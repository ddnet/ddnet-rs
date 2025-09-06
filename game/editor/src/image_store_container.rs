use std::{path::Path, sync::Arc};

use anyhow::anyhow;
use base_io::{io::Io, runtime::IoRuntimeTask};
use game_base::assets_url::HTTP_RESOURCE_URL;
use graphics::{
    graphics::graphics::Graphics,
    graphics_mt::GraphicsMultiThreaded,
    handles::texture::texture::{GraphicsTextureHandle, TextureContainer},
};
use hiarc::Hiarc;
use sound::{
    scene_object::SceneObject, sound::SoundManager, sound_handle::SoundObjectHandle,
    sound_mt::SoundMultiThreaded,
};

use client_containers::container::{
    ContainerLoadOptions, ContainerLoadedItem, ContainerLoadedItemDir, load_file_part_and_upload,
};

use client_containers::container::{Container, ContainerItemLoadData, ContainerLoad};

#[derive(Debug, Hiarc, Clone)]
pub struct ImageStore {
    pub tex: TextureContainer,
    pub width: u32,
    pub height: u32,
    pub file: Vec<u8>,
    pub name: String,
}

#[derive(Debug, Hiarc)]
pub struct LoadImageStore {
    image: ContainerItemLoadData,
    file: Vec<u8>,

    name: String,
}

impl LoadImageStore {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        files: ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        name: &str,
    ) -> anyhow::Result<Self> {
        let path: &Path = "icon.png".as_ref();
        let file = files
            .files
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow!("File missing"))?;
        Ok(Self {
            image: load_file_part_and_upload(
                graphics_mt,
                &files,
                default_files,
                name,
                &[],
                "icon",
            )?
            .img,
            file,

            name: name.to_string(),
        })
    }

    fn load_file_into_texture(
        texture_handle: &GraphicsTextureHandle,
        img: ContainerItemLoadData,
        name: &str,
    ) -> (TextureContainer, u32, u32) {
        (
            texture_handle.load_texture_rgba_u8(img.data, name).unwrap(),
            img.width,
            img.height,
        )
    }
}

impl ContainerLoad<ImageStore> for LoadImageStore {
    fn load(
        item_name: &str,
        files: ContainerLoadedItem,
        default_files: &ContainerLoadedItemDir,
        _runtime_thread_pool: &Arc<rayon::ThreadPool>,
        graphics_mt: &GraphicsMultiThreaded,
        _sound_mt: &SoundMultiThreaded,
    ) -> anyhow::Result<Self> {
        match files {
            ContainerLoadedItem::Directory(files) => {
                Self::new(graphics_mt, files, default_files, item_name)
            }
            ContainerLoadedItem::SingleFile(file) => {
                let mut files = ContainerLoadedItemDir::new(Default::default());

                files.files.insert("icon.png".into(), file);

                Self::new(graphics_mt, files, default_files, item_name)
            }
        }
    }

    fn convert(
        self,
        texture_handle: &GraphicsTextureHandle,
        _sound_object_handle: &SoundObjectHandle,
    ) -> ImageStore {
        let (tex, width, height) =
            LoadImageStore::load_file_into_texture(texture_handle, self.image, &self.name);
        ImageStore {
            tex,
            width,
            height,
            file: self.file,
            name: self.name,
        }
    }
}

/// Image container for map resources.
pub type ImageStoreContainer = Container<ImageStore, LoadImageStore>;

pub fn load_image_store_container(
    io: Io,
    tp: Arc<rayon::ThreadPool>,
    container_name: &str,
    graphics: &Graphics,
    sound: &SoundManager,
    scene: SceneObject,
) -> ImageStoreContainer {
    let default_item: IoRuntimeTask<client_containers::container::ContainerLoadedItem> =
        ImageStoreContainer::load_default(&io, "map/resources/images/".as_ref());
    ImageStoreContainer::new(
        io,
        tp,
        default_item,
        Some(HTTP_RESOURCE_URL.try_into().unwrap()),
        None,
        container_name,
        graphics,
        sound,
        &scene,
        "map/resources/images".as_ref(),
        ContainerLoadOptions {
            assume_unused: true,
            ..Default::default()
        },
    )
}
