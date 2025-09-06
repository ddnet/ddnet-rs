use std::sync::Arc;

use anyhow::anyhow;
use base_io::{io::Io, runtime::IoRuntimeTask};
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
use url::Url;

#[derive(Debug, Hiarc, Clone)]
pub struct Thumbnail {
    pub thumbnail: TextureContainer,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Hiarc)]
pub struct LoadThumbnail {
    thumbnail: ContainerItemLoadData,

    thumbnail_name: String,
}

impl LoadThumbnail {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        files: ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        thumbnail_name: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            thumbnail: load_file_part_and_upload(
                graphics_mt,
                &files,
                default_files,
                thumbnail_name,
                &[],
                "thumbnail",
            )
            .or_else(|err| {
                // also try icon as alternative
                load_file_part_and_upload(
                    graphics_mt,
                    &files,
                    default_files,
                    thumbnail_name,
                    &[],
                    "icon",
                )
                .map_err(|err2| anyhow!("{err}. {err2}"))
            })?
            .img,

            thumbnail_name: thumbnail_name.to_string(),
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

impl ContainerLoad<Thumbnail> for LoadThumbnail {
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

                files.files.insert("thumbnail.png".into(), file);

                Self::new(graphics_mt, files, default_files, item_name)
            }
        }
    }

    fn convert(
        self,
        texture_handle: &GraphicsTextureHandle,
        _sound_object_handle: &SoundObjectHandle,
    ) -> Thumbnail {
        let (thumbnail, width, height) = LoadThumbnail::load_file_into_texture(
            texture_handle,
            self.thumbnail,
            &self.thumbnail_name,
        );
        Thumbnail {
            thumbnail,
            width,
            height,
        }
    }
}

/// General purpose image container.
///
/// Can be used for thumbnails, icons etc.
pub type ThumbnailContainer = Container<Thumbnail, LoadThumbnail>;
pub const DEFAULT_THUMBNAIL_CONTAINER_PATH: &str = "thumbnails/";

pub fn load_thumbnail_container(
    io: Io,
    tp: Arc<rayon::ThreadPool>,
    path: &str,
    container_name: &str,
    graphics: &Graphics,
    sound: &SoundManager,
    scene: SceneObject,
    resource_server_download_url: Option<Url>,
) -> ThumbnailContainer {
    let default_item: IoRuntimeTask<client_containers::container::ContainerLoadedItem> =
        ThumbnailContainer::load_default(&io, path.as_ref());
    ThumbnailContainer::new(
        io,
        tp,
        default_item,
        None,
        resource_server_download_url,
        container_name,
        graphics,
        sound,
        &scene,
        path.as_ref(),
        ContainerLoadOptions {
            assume_unused: true,
            ..Default::default()
        },
    )
}
