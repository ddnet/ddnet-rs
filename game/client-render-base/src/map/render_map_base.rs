use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
};

use super::{
    map::RenderMap,
    map_buffered::{ClientMapBufferUploadData, ClientMapBuffered},
    map_image::{
        ClientMapImageLoading, ClientMapImagesLoading, ClientMapSoundLoading,
        ClientMapSoundsLoading,
    },
};
use anyhow::anyhow;
use assets_base::verify::ogg_vorbis::verify_ogg_vorbis;
use base::{
    benchmark::Benchmark,
    hash::{fmt_hash, Hash},
    join_all,
};
use base_io::{io::Io, path_to_url::relative_path_to_url, runtime::IoRuntimeTask};
use config::config::ConfigDebug;
use graphics::{
    graphics::graphics::Graphics,
    handles::{
        backend::backend::GraphicsBackendHandle,
        buffer_object::buffer_object::GraphicsBufferObjectHandle,
        canvas::canvas::GraphicsCanvasHandle,
        stream::stream::GraphicsStreamHandle,
        texture::texture::{GraphicsTextureHandle, TextureContainer, TextureContainer2dArray},
    },
};
use graphics_types::{commands::TexFlags, types::GraphicsMemoryAllocationType};
use image_utils::{
    png::{is_png_image_valid, load_png_image_as_rgba, resize_rgba, PngValidatorOptions},
    utils::{highest_bit, texture_2d_to_3d},
};
use map::map::Map;
use math::math::vector::vec2;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use sound::{commands::SoundSceneCreateProps, scene_handle::SoundSceneHandle, sound::SoundManager};
use url::Url;
use vanilla::collision::collision::Collision;

pub struct ClientMapFileData {
    pub collision: Box<Collision>,
    pub buffered_map: ClientMapBuffered,
}

pub struct ClientMapRenderAndFile {
    pub data: ClientMapFileData,
    pub render: RenderMap,
}

pub struct ClientMapFileProcessed {
    pub upload_data: ClientMapBufferUploadData,
    pub collision: Box<Collision>,
    pub images: ClientMapImagesLoading,
    pub sounds: ClientMapSoundsLoading,
}

pub struct RenderMapLoading {
    pub task: IoRuntimeTask<ClientMapFileProcessed>,
    pub backend_handle: GraphicsBackendHandle,
    pub buffer_object_handle: GraphicsBufferObjectHandle,
    pub texture_handle: GraphicsTextureHandle,
    pub canvas_handle: GraphicsCanvasHandle,
    pub stream_handle: GraphicsStreamHandle,

    pub sound_scene_handle: SoundSceneHandle,
    pub scene_create_props: SoundSceneCreateProps,

    pub do_benchmarks: bool,
}

impl RenderMapLoading {
    pub fn new(
        thread_pool: Arc<rayon::ThreadPool>,
        file: Vec<u8>,
        resource_download_server: Option<Url>,
        io: Io,
        sound: &SoundManager,
        scene_create_props: SoundSceneCreateProps,
        graphics: &Graphics,
        config: &ConfigDebug,
        downloaded_path: Option<&Path>,
    ) -> Self {
        let file_system = io.fs.clone();
        let http = io.http.clone();
        let do_benchmark = config.bench;
        let runtime_tp = thread_pool;
        let graphics_mt = graphics.get_graphics_mt();
        let sound_mt = sound.get_sound_mt();
        let downloaded_path = downloaded_path.map(|p| p.to_path_buf());
        let load_hq_assets = false;
        Self {
            task: io.rt.spawn(async move {
                let benchmark = Benchmark::new(do_benchmark);
                // open the map file
                let (resources, resources_bytes_read) = Map::read_resources_and_header(&file)?;
                benchmark.bench("opening the full map file");

                // read content files
                let mut file_map: HashSet<Hash> = Default::default();
                #[derive(Debug, PartialEq, Clone, Copy)]
                enum ReadFileTy {
                    Image,
                    Sound,
                }

                let file_futures = resources
                    .images
                    .iter()
                    .map(|i| (i, ReadFileTy::Image))
                    .chain(
                        resources
                            .image_arrays
                            .iter()
                            .map(|i| (i, ReadFileTy::Image)),
                    )
                    .chain(resources.sounds.iter().map(|s| (s, ReadFileTy::Sound)))
                    .filter(|(i, _)| {
                        let meta = if let Some(hq_meta) =
                            load_hq_assets.then_some(i.hq_meta.as_ref()).flatten()
                        {
                            hq_meta
                        } else {
                            &i.meta
                        };

                        if file_map.contains(&meta.blake3_hash) {
                            false
                        } else {
                            file_map.insert(meta.blake3_hash);
                            true
                        }
                    })
                    .map(|(res, ty)| {
                        let meta = if let Some(hq_meta) =
                            load_hq_assets.then_some(res.hq_meta.as_ref()).flatten()
                        {
                            hq_meta
                        } else {
                            &res.meta
                        };

                        let download_read_file_path = format!(
                            "map/resources/{}/{}_{}.{}",
                            if ty == ReadFileTy::Image {
                                "images"
                            } else {
                                "sounds"
                            },
                            res.name.as_str(),
                            fmt_hash(&meta.blake3_hash),
                            meta.ty.as_str()
                        );
                        let read_file_path = if let Some(downloaded_path) = &downloaded_path {
                            downloaded_path.join(&download_read_file_path)
                        } else {
                            download_read_file_path.as_str().into()
                        };
                        let hash = meta.blake3_hash;
                        let file_ty = meta.ty.clone();
                        let file_name = res.name.clone();
                        let fs = file_system.clone();
                        let http = http.clone();
                        let resource_download_server = resource_download_server.clone();
                        async move {
                            let file = fs.read_file(Path::new(&read_file_path)).await;

                            let file = match file {
                                Ok(file) => Ok(file),
                                Err(err) => {
                                    async move {
                                        // try to download file
                                        if let Some(resource_download_server) =
                                            resource_download_server.and_then(|url| {
                                                relative_path_to_url(
                                                    download_read_file_path.as_ref(),
                                                )
                                                .ok()
                                                .and_then(|name| url.join(&name).ok())
                                            })
                                        {
                                            let file = http
                                                .download_binary(resource_download_server, &hash)
                                                .await
                                                .map_err(|err| {
                                                    anyhow!("failed to download map: {err}")
                                                })?
                                                .to_vec();
                                            Self::verify_resource(
                                                file_ty.as_str(),
                                                file_name.as_str(),
                                                &file,
                                                load_hq_assets,
                                            )?;
                                            let file_path: &Path = read_file_path.as_ref();
                                            if let Some(dir) = file_path.parent() {
                                                fs.create_dir(dir).await?;
                                            }
                                            fs.write_file(read_file_path.as_ref(), file.clone())
                                                .await?;
                                            anyhow::Ok(file)
                                        } else {
                                            Err(anyhow!(err))
                                        }
                                    }
                                    .await
                                }
                            }
                            .map_err(|err| anyhow!(err));

                            (hash, file, ty)
                        }
                    });
                let task_read = futures::future::join_all(file_futures);

                // poll once with a small hack
                let task_read = futures::future::maybe_done(task_read);
                futures::pin_mut!(task_read);
                futures::future::FutureExt::now_or_never(&mut task_read);

                task_read.as_mut().await;
                let files = task_read.as_mut().take_output().unwrap();
                let mut img_files: HashMap<Hash, Vec<u8>> = Default::default();
                let mut sound_files: HashMap<Hash, Vec<u8>> = Default::default();
                for (file_hash, file, ty) in files {
                    let file = file?;
                    match ty {
                        ReadFileTy::Image => {
                            img_files.insert(file_hash, file);
                        }
                        ReadFileTy::Sound => {
                            sound_files.insert(file_hash, file);
                        }
                    }
                }

                let resources_clone = resources.clone();

                let generate_3d_data = |w: usize, h: usize, img_data: &[u8]| {
                    // first check image dimensions
                    let mut convert_width = w;
                    let mut convert_height = h;
                    let image_color_channels = 4;

                    let mut upload_data = img_data;
                    let conv_data: Vec<u8>;

                    if convert_width == 0
                        || (convert_width % 16) != 0
                        || convert_height == 0
                        || (convert_height % 16) != 0
                    {
                        let new_width =
                            std::cmp::max(highest_bit(convert_width as u32) as usize, 16);
                        let new_height =
                            std::cmp::max(highest_bit(convert_height as u32) as usize, 16);
                        conv_data = resize_rgba(
                            upload_data.into(),
                            convert_width as u32,
                            convert_height as u32,
                            new_width as u32,
                            new_height as u32,
                        );
                        log::warn!(
                            "3D/2D array texture had to be resized, {}x{} to {}x{}",
                            convert_width,
                            convert_height,
                            new_width,
                            new_height
                        );

                        convert_width = new_width;
                        convert_height = new_height;

                        upload_data = conv_data.as_slice();
                    }

                    let mut tex_3d =
                        graphics_mt.mem_alloc(GraphicsMemoryAllocationType::TextureRgbaU82dArray {
                            width: (convert_width / 16).try_into().unwrap(),
                            height: (convert_height / 16).try_into().unwrap(),
                            depth: 256.try_into().unwrap(),
                            flags: TexFlags::empty(),
                        });
                    let mut image_3d_width = 0;
                    let mut image_3d_height = 0;
                    if !texture_2d_to_3d(
                        &runtime_tp,
                        upload_data,
                        convert_width,
                        convert_height,
                        image_color_channels,
                        16,
                        16,
                        tex_3d.as_mut_slice(),
                        &mut image_3d_width,
                        &mut image_3d_height,
                    ) {
                        panic!("fatal error, could not convert 2d texture to 2d array texture");
                    }

                    if let Err(err) = graphics_mt.try_flush_mem(&mut tex_3d, false) {
                        // Ignore the error, but log it.
                        log::debug!("err while flushing memory: {}", err);
                    }

                    (image_3d_width, image_3d_height, 256, tex_3d)
                };
                // load images, external images and do map buffering
                let (images_loading, sounds_loading, map_prepare) = runtime_tp.install(|| {
                    join_all!(
                        || {
                            let img_files = img_files
                                .into_par_iter()
                                .map(|(hash, file)| {
                                    let mut img_data: Vec<u8> = Default::default();
                                    let img = load_png_image_as_rgba(
                                        &file,
                                        |width, height, color_channel_count| {
                                            img_data.resize(
                                                width * height * color_channel_count,
                                                Default::default(),
                                            );
                                            &mut img_data
                                        },
                                    )?;
                                    anyhow::Ok((hash, (img.data.to_vec(), img.width, img.height)))
                                })
                                .collect::<anyhow::Result<HashMap<Hash, (Vec<u8>, u32, u32)>>>()?;

                            let images_loading = ClientMapImagesLoading {
                                images: resources_clone
                                    .images
                                    .into_par_iter()
                                    .map(|img| {
                                        let meta = if let Some(hq_meta) =
                                            load_hq_assets.then_some(img.hq_meta.as_ref()).flatten()
                                        {
                                            hq_meta
                                        } else {
                                            &img.meta
                                        };
                                        let (img_data, width, height) = img_files
                                            .get(&meta.blake3_hash)
                                            .ok_or(anyhow!("img with that name not found"))?;
                                        let mut loading_img = ClientMapImageLoading {
                                            mem: graphics_mt.mem_alloc(
                                                GraphicsMemoryAllocationType::TextureRgbaU8 {
                                                    width: (*width as usize).try_into().unwrap(),
                                                    height: (*height as usize).try_into().unwrap(),
                                                    flags: TexFlags::empty(),
                                                },
                                            ),
                                            width: *width,
                                            height: *height,
                                            depth: 1,
                                            name: img.name.to_string(),
                                        };
                                        loading_img.mem.as_mut_slice().copy_from_slice(img_data);
                                        if graphics_mt
                                            .try_flush_mem(&mut loading_img.mem, false)
                                            .is_err()
                                        {
                                            // TODO: handle/log ?
                                        }
                                        anyhow::Ok(loading_img)
                                    })
                                    .collect::<anyhow::Result<Vec<ClientMapImageLoading>>>()?,
                                images_2d_array: resources_clone
                                    .image_arrays
                                    .into_par_iter()
                                    .map(|img| {
                                        let meta = if let Some(hq_meta) =
                                            load_hq_assets.then_some(img.hq_meta.as_ref()).flatten()
                                        {
                                            hq_meta
                                        } else {
                                            &img.meta
                                        };
                                        let (img_data, width, height) = img_files
                                            .get(&meta.blake3_hash)
                                            .ok_or(anyhow!("img with that name not found"))?;
                                        let (width, height, depth, mem) = generate_3d_data(
                                            *width as usize,
                                            *height as usize,
                                            img_data,
                                        );
                                        anyhow::Ok(ClientMapImageLoading {
                                            mem,
                                            width: width as u32,
                                            height: height as u32,
                                            depth: depth as u32,
                                            name: img.name.to_string(),
                                        })
                                    })
                                    .collect::<anyhow::Result<Vec<ClientMapImageLoading>>>()?,
                            };

                            benchmark.bench_multi("decompressing all map images");
                            anyhow::Ok(images_loading)
                        },
                        || {
                            let sounds_loading = sound_files
                                .into_par_iter()
                                .map(|(_, file)| {
                                    let mut mem = sound_mt.mem_alloc(file.len());
                                    mem.as_mut_slice().copy_from_slice(&file);
                                    let _ = sound_mt.try_flush_mem(&mut mem); // ignore error on purpose
                                    anyhow::Ok(ClientMapSoundLoading { mem })
                                })
                                .collect::<anyhow::Result<Vec<ClientMapSoundLoading>>>()?;

                            benchmark.bench_multi("decompressing all sounds");
                            anyhow::Ok(sounds_loading)
                        },
                        || {
                            let map = Map::read_with_resources(
                                resources,
                                &file[resources_bytes_read..],
                                &runtime_tp,
                            )?;

                            benchmark.bench_multi("initialzing the map layers");

                            let physics_group = &map.groups.physics;
                            let collision = Collision::new(physics_group, false)?;

                            let upload_data = ClientMapBuffered::prepare_upload(&graphics_mt, map);
                            benchmark.bench_multi("preparing the map buffering");

                            anyhow::Ok((collision, upload_data))
                        }
                    )
                });

                benchmark.bench("loading the full map (excluding opening it)");

                let (collision, upload_data) = map_prepare?;
                Ok(ClientMapFileProcessed {
                    collision,
                    upload_data,
                    images: images_loading?,
                    sounds: sounds_loading?,
                })
            }),
            backend_handle: graphics.backend_handle.clone(),
            buffer_object_handle: graphics.buffer_object_handle.clone(),
            texture_handle: graphics.texture_handle.clone(),
            canvas_handle: graphics.canvas_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),

            sound_scene_handle: sound.scene_handle.clone(),
            scene_create_props,

            do_benchmarks: config.bench,
        }
    }

    fn verify_resource(
        file_ty: &str,
        file_name: &str,
        file: &[u8],
        allow_hq_assets: bool,
    ) -> anyhow::Result<()> {
        match file_ty {
            "png" => {
                if let Err(err) = is_png_image_valid(
                    file,
                    if allow_hq_assets {
                        PngValidatorOptions {
                            max_width: 4096.try_into().unwrap(),
                            max_height: 4096.try_into().unwrap(),
                            ..Default::default()
                        }
                    } else {
                        Default::default()
                    },
                ) {
                    return Err(anyhow!(
                        "downloaded image resource (png) {}\
                        is not a valid png file: {}",
                        file_name,
                        err
                    ));
                }
            }
            "ogg" => {
                if let Err(err) = verify_ogg_vorbis(file) {
                    return Err(anyhow!(
                        "downloaded sound resource (ogg vorbis) \
                        ({}) is not a valid ogg vorbis file: {}",
                        file_name,
                        err
                    ));
                }
            }
            _ => {
                return Err(anyhow!(
                    "Unsupported resource type {} \
                could not be validated",
                    file_ty
                ))
            }
        }
        Ok(())
    }
}

pub enum ClientMapRender {
    UploadingBuffersAndTextures(RenderMapLoading),
    Map(ClientMapRenderAndFile),
    None,
    Err(anyhow::Error),
}

impl ClientMapRender {
    pub fn new(loading: RenderMapLoading) -> Self {
        Self::UploadingBuffersAndTextures(loading)
    }

    pub fn try_get(&self) -> Option<&ClientMapRenderAndFile> {
        if let Self::Map(map_file) = self {
            Some(map_file)
        } else {
            None
        }
    }

    pub fn continue_loading(&mut self) -> anyhow::Result<Option<&ClientMapRenderAndFile>> {
        let mut eval = || {
            let mut self_helper = Self::None;
            std::mem::swap(&mut self_helper, self);
            match self_helper {
                Self::UploadingBuffersAndTextures(map_upload) => {
                    if map_upload.task.is_finished() {
                        // the task might be cleared by a higher function call, so make sure it still exists
                        let map_file = map_upload.task.get_storage()?;

                        let do_benchmark = map_upload.do_benchmarks;
                        let benchmark = Benchmark::new(do_benchmark);

                        let images = map_file
                            .images
                            .images
                            .into_iter()
                            .map(|img| {
                                map_upload
                                    .texture_handle
                                    .load_texture_rgba_u8(img.mem, &img.name)
                            })
                            .collect::<anyhow::Result<Vec<TextureContainer>>>()?;
                        let images_2d_array = map_file
                            .images
                            .images_2d_array
                            .into_iter()
                            .map(|img| {
                                map_upload
                                    .texture_handle
                                    .load_texture_2d_array_rgba_u8(img.mem, &img.name)
                            })
                            .collect::<anyhow::Result<Vec<TextureContainer2dArray>>>()?;

                        // sound scene
                        let scene = map_upload
                            .sound_scene_handle
                            .create(map_upload.scene_create_props);
                        let listener = scene.sound_listener_handle.create(vec2::default());
                        let sound_objects: Vec<_> = map_file
                            .sounds
                            .into_iter()
                            .map(|sound| scene.sound_object_handle.create(sound.mem))
                            .collect();

                        benchmark.bench("creating the image graphics cmds");

                        let map_buffered = ClientMapBuffered::new(
                            &map_upload.backend_handle,
                            &map_upload.buffer_object_handle,
                            map_file.upload_data,
                            images,
                            images_2d_array,
                            scene,
                            listener,
                            sound_objects,
                        );

                        benchmark.bench("creating the map buffers graphics cmds");

                        *self = Self::Map(ClientMapRenderAndFile {
                            data: ClientMapFileData {
                                collision: map_file.collision,
                                buffered_map: map_buffered,
                            },
                            render: RenderMap::new(
                                &map_upload.backend_handle,
                                &map_upload.canvas_handle,
                                &map_upload.stream_handle,
                            ),
                        });
                    } else {
                        *self = Self::UploadingBuffersAndTextures(map_upload)
                    }
                }
                Self::Map(map) => *self = Self::Map(map),
                Self::None => {}
                Self::Err(err) => {
                    *self = Self::Err(anyhow!("{}", err));
                    return Err(err);
                }
            }
            anyhow::Ok(())
        };
        match eval() {
            Ok(_) => {
                // ignore
            }
            Err(err) => {
                *self = Self::Err(anyhow!("{}", err));
                return Err(err);
            }
        }
        Ok(self.try_get())
    }
}
