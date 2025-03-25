use std::{path::Path, sync::Arc};

use anyhow::anyhow;
use base_io::{io::Io, runtime::IoRuntimeTask};
use game_base::assets_url::HTTP_RESOURCE_URL;
use graphics::{
    graphics::graphics::Graphics, graphics_mt::GraphicsMultiThreaded,
    handles::texture::texture::GraphicsTextureHandle,
};
use hiarc::Hiarc;
use sound::{
    scene_object::SceneObject, sound::SoundManager, sound_handle::SoundObjectHandle,
    sound_mt::SoundMultiThreaded, sound_object::SoundObject,
};

use client_containers::container::{
    load_sound_file_part_and_upload, ContainerLoadOptions, ContainerLoadedItem,
    ContainerLoadedItemDir, SoundFilePartResult,
};

use client_containers::container::{Container, ContainerLoad};

#[derive(Debug, Hiarc, Clone)]
pub struct SoundStore {
    pub snd: SoundObject,
    pub file: Vec<u8>,

    pub name: String,
}

#[derive(Debug, Hiarc)]
pub struct LoadSoundStore {
    sound: SoundFilePartResult,
    file: Vec<u8>,

    name: String,
}

impl LoadSoundStore {
    pub fn new(
        sound_mt: &SoundMultiThreaded,
        files: ContainerLoadedItemDir,
        default_files: &ContainerLoadedItemDir,
        name: &str,
    ) -> anyhow::Result<Self> {
        let path: &Path = "preview.ogg".as_ref();
        let file = files
            .files
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow!("File missing"))?;
        Ok(Self {
            sound: load_sound_file_part_and_upload(
                sound_mt,
                &files,
                default_files,
                name,
                &[],
                "preview",
            )?,
            file,

            name: name.to_string(),
        })
    }

    fn load_file_into_sound(sound: &SoundObjectHandle, snd: SoundFilePartResult) -> SoundObject {
        sound.create(snd.mem)
    }
}

impl ContainerLoad<SoundStore> for LoadSoundStore {
    fn load(
        item_name: &str,
        files: ContainerLoadedItem,
        default_files: &ContainerLoadedItemDir,
        _runtime_thread_pool: &Arc<rayon::ThreadPool>,
        _graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
    ) -> anyhow::Result<Self> {
        match files {
            ContainerLoadedItem::Directory(files) => {
                Self::new(sound_mt, files, default_files, item_name)
            }
            ContainerLoadedItem::SingleFile(file) => {
                let mut files = ContainerLoadedItemDir::new(Default::default());

                files.files.insert("preview.ogg".into(), file);

                Self::new(sound_mt, files, default_files, item_name)
            }
        }
    }

    fn convert(
        self,
        _texture_handle: &GraphicsTextureHandle,
        sound_object_handle: &SoundObjectHandle,
    ) -> SoundStore {
        let snd = LoadSoundStore::load_file_into_sound(sound_object_handle, self.sound);
        SoundStore {
            snd,
            file: self.file,
            name: self.name,
        }
    }
}

/// Sound container for map resources.
pub type SoundStoreContainer = Container<SoundStore, LoadSoundStore>;

pub fn load_sound_store_container(
    io: Io,
    tp: Arc<rayon::ThreadPool>,
    container_name: &str,
    graphics: &Graphics,
    sound: &SoundManager,
    scene: SceneObject,
) -> SoundStoreContainer {
    let default_item: IoRuntimeTask<client_containers::container::ContainerLoadedItem> =
        SoundStoreContainer::load_default(&io, "map/resources/sounds/".as_ref());
    SoundStoreContainer::new(
        io,
        tp,
        default_item,
        Some(HTTP_RESOURCE_URL.try_into().unwrap()),
        None,
        container_name,
        graphics,
        sound,
        &scene,
        "map/resources/sounds".as_ref(),
        ContainerLoadOptions {
            assume_unused: true,
            allows_single_audio_or_txt_files: true,
        },
    )
}
