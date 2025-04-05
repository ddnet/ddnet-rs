use assets_base::{
    loader::read_tar,
    verify::{ogg_vorbis::verify_ogg_vorbis, txt::verify_txt},
    AssetsIndex, AssetsMeta,
};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::{
    borrow::Borrow,
    marker::PhantomData,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use tokio::sync::Semaphore;

use anyhow::anyhow;
use base::hash::{fmt_hash, Hash};
use base_io_traits::{fs_traits::FileSystemInterface, http_traits::HttpClientInterface};

use base_io::{io::Io, path_to_url::relative_path_to_url, runtime::IoRuntimeTask};
use either::Either;
use game_interface::types::resource_key::ResourceKey;
use graphics::{
    graphics::graphics::Graphics, graphics_mt::GraphicsMultiThreaded,
    handles::texture::texture::GraphicsTextureHandle,
};
use graphics_types::{
    commands::TexFlags,
    types::{GraphicsBackendMemory, GraphicsMemoryAllocationType},
};
use hashlink::LinkedHashMap;
use hiarc::Hiarc;
use image_utils::{
    png::{is_png_image_valid, load_png_image_as_rgba, PngResultPersistent, PngValidatorOptions},
    utils::texture_2d_to_3d,
};
use log::{debug, info};
use sound::{
    scene_object::SceneObject, sound::SoundManager, sound_handle::SoundObjectHandle,
    sound_mt::SoundMultiThreaded, sound_mt_types::SoundBackendMemory,
};
use url::Url;

const CONTAINER_MAX_DOWNLOAD_TASKS: usize = 2;
const CONTAINER_MAX_TASKS: usize = 16;

#[derive(Debug, Hiarc)]
pub struct ContainerMaxItems<'a> {
    /// How many items at most are allowed
    pub count: NonZeroUsize,
    /// How long the life time of the entries is if
    /// the limit is hit
    pub entry_lifetime: &'a Duration,
}

#[derive(Debug, Hiarc)]
pub struct ContainerItemLoadData {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub data: GraphicsBackendMemory,
}

#[derive(Debug, Hiarc, Clone)]
pub struct ContainerLoadedItemDir {
    /// key is the relative path
    /// excluding the item's directory
    /// (e.g. `greyfox/*` => `*`)
    pub files: HashMap<PathBuf, Vec<u8>>,

    _dont_construct: PhantomData<()>,
}

impl ContainerLoadedItemDir {
    pub fn new(files: HashMap<PathBuf, Vec<u8>>) -> Self {
        Self {
            files,
            _dont_construct: Default::default(),
        }
    }
}

#[derive(Debug, Hiarc, Clone)]
pub enum ContainerLoadedItem {
    SingleFile(Vec<u8>),
    Directory(ContainerLoadedItemDir),
}

#[derive(Debug, Hiarc)]
struct ContainerItem<A> {
    item: A,
    used_last_in: Duration,
}

pub type ContainerKey = ResourceKey;

/// This a hint is to determine if the container item
/// is likely be loaded by disk or http etc.
#[derive(Debug, Clone, Copy)]
pub enum ContainerItemIndexType {
    Disk,
    Http,
}

type ResourceIndex = HashSet<String>;
type TokioArcMutex<T> = Arc<tokio::sync::Mutex<T>>;

type HttpIndexAndUrl = Option<(TokioArcMutex<Option<Arc<anyhow::Result<AssetsIndex>>>>, Url)>;

#[derive(Debug, Hiarc)]
struct DefaultItemNotifyOnDrop(Arc<tokio::sync::Notify>);

impl Drop for DefaultItemNotifyOnDrop {
    fn drop(&mut self) {
        self.0.notify_one();
    }
}

#[derive(Debug, Hiarc, Default)]
pub struct ContainerLoadOptions {
    pub assume_unused: bool,
    pub allows_single_audio_or_txt_files: bool,
}

#[derive(Debug, Hiarc)]
struct DefaultItem<L> {
    // notifier must be before the task
    notifier: Option<DefaultItemNotifyOnDrop>,
    // notifier must be before the task
    task: IoRuntimeTask<(L, ContainerLoadedItemDir)>,
}

/// Containers are a collection of named assets, e.g. all skins
/// are part of the skins container.
///
/// Assets have a name and corresponding to this name
/// there are textures, sounds, effects or whatever fits the container logically.
/// All containers should have a `default` value/texture/sound etc.
///
/// # Users
/// Users of the containers must call [Container::get_or_default] to get
/// access to a resource. It accepts a file name and an optional hash.
/// The hash must be used if the resource is forced by a game server,
/// else it's optional.
/// Calling [Container::update] causes the container to remove unused
/// resources, to make sure that resources are not unloaded to often
/// you should usually pass the `force_used_items` argument which should
/// be filled with items that are likely used (e.g. from a player list).
///
/// # Implementors
/// Generally items of containers have three load modes and 2 file modes:
/// Load modes:
/// - Http server, used across all game servers + in the UI. Uses a JSON to list all entries.
/// - Game server, used for a specific game server. Must use file hashes.
/// - Local files, reading files without any hash.
///   Both http server & game server mode try to load from a disk cache first.
///   File modes:
/// - Single file: A single file, most commonly a texture, was loaded and the implementations
///   must ensure to load proper default values for other resources of an item (sounds etc.)
/// - Directory: A directory with many different resources was loaded. Missing resources must be filled
///   with values of the default item. A directory might be archieved in a .tar ball, which is automatically
///   unpacked and processed.
#[derive(Debug, Hiarc)]
pub struct Container<A, L> {
    items: LinkedHashMap<ContainerKey, ContainerItem<A>>,
    http_download_tasks: Arc<Semaphore>,
    loading_tasks: HashMap<ContainerKey, IoRuntimeTask<L>>,
    failed_tasks: HashSet<ContainerKey>,

    // containers allow to delay loading the default item as much as possible, to improve startup time
    default_item: Option<DefaultItem<L>>,
    default_loaded_item: Arc<ContainerLoadedItemDir>,
    pub default_key: Rc<ContainerKey>,

    // strict private data
    io: Io,
    graphics_mt: GraphicsMultiThreaded,
    texture_handle: GraphicsTextureHandle,
    sound_mt: SoundMultiThreaded,
    sound_object_handle: SoundObjectHandle,
    runtime_thread_pool: Arc<rayon::ThreadPool>,
    container_name: String,

    /// url for generaly resource downloads
    resource_http_download_url: Option<Url>,
    /// url for resource downloads from a game server
    resource_server_download_url: Option<Url>,
    /// Base path to container items.
    /// This is used for disk aswell as for the http requests.
    /// (So a http server mirrors a local data directory)
    base_path: PathBuf,
    /// For downloaded assets, this is the prefix used before
    /// the [Self::base_path].
    downloaded_path: PathBuf,

    /// Whether single audio or txt files can trigger a load
    /// of a container item. Usually audio or txt only
    /// makes no sense.
    allows_single_audio_or_txt_files: bool,

    /// An index, downloaded as JSON, that contains file paths + hashes
    /// over all downloadable files of the http download
    /// server. This is downloaded once and must exist
    /// in order to download further assets from the server
    resource_http_download_index: Arc<tokio::sync::Mutex<Option<Arc<anyhow::Result<AssetsIndex>>>>>,
    /// Non async version of [`Self::resource_http_download_index`]
    cached_http_download_index: Option<Arc<anyhow::Result<AssetsIndex>>>,
    // double option is intended here
    http_download_index_task: Option<Option<IoRuntimeTask<()>>>,

    /// An meta data record, downloaded as JSON, that contains meta data about
    /// downloadable items.
    resource_http_download_meta: Arc<tokio::sync::Mutex<Option<Arc<anyhow::Result<AssetsMeta>>>>>,
    /// Non async version of [`Self::resource_http_download_meta`]
    cached_http_download_meta: Option<Arc<anyhow::Result<AssetsMeta>>>,
    // double option is intended here
    http_download_meta_task: Option<Option<IoRuntimeTask<()>>>,

    /// A list of entries the client can load without hashes
    /// usually it makes sense to combine it with `resource_http_download_index`
    /// to get a list of loadable items
    resource_dir_index: Either<
        anyhow::Result<HashSet<String>>,
        Option<IoRuntimeTask<anyhow::Result<ResourceIndex>>>,
    >,

    /// last time the container was updated by [Self::update]
    last_update_time: Option<Duration>,
    last_update_interval_time: Option<Duration>,
}

pub trait ContainerLoad<A>
where
    Self: Sized,
{
    fn load(
        item_name: &str,
        files: ContainerLoadedItem,
        default_files: &ContainerLoadedItemDir,
        runtime_thread_pool: &Arc<rayon::ThreadPool>,
        graphics_mt: &GraphicsMultiThreaded,
        sound_mt: &SoundMultiThreaded,
    ) -> anyhow::Result<Self>;

    fn convert(
        self,
        texture_handle: &GraphicsTextureHandle,
        sound_object_handle: &SoundObjectHandle,
    ) -> A;
}

impl<A, L> Container<A, L>
where
    L: ContainerLoad<A> + Sync + Send + 'static,
{
    /// Creates a new container instance.
    ///
    /// Interesting parameters are:
    /// - `resource_http_download_url`:
    ///   The resource server for general purpose, cross server resources
    /// - `resource_server_download_url`:
    ///   The resource for a game server, which are only downloaded if a hash
    ///   is provided.
    /// - `sound_scene`:
    ///   The scene in which the sounds are created in.
    /// - `assume_unused`:
    ///   Assumes that the container will most likely never be used.
    pub fn new(
        io: Io,
        runtime_thread_pool: Arc<rayon::ThreadPool>,
        default_item: IoRuntimeTask<ContainerLoadedItem>,
        resource_http_download_url: Option<Url>,
        resource_server_download_url: Option<Url>,
        container_name: &str,
        graphics: &Graphics,
        sound: &SoundManager,
        sound_scene: &SceneObject,
        base_path: &Path,
        options: ContainerLoadOptions,
    ) -> Self {
        let items = LinkedHashMap::new();
        Self {
            items,
            http_download_tasks: Arc::new(Semaphore::const_new(CONTAINER_MAX_DOWNLOAD_TASKS)),
            loading_tasks: HashMap::default(),
            failed_tasks: Default::default(),

            default_item: Some({
                let runtime_thread_pool = runtime_thread_pool.clone();
                let mut graphics_mt = graphics.get_graphics_mt();
                let notifier: Option<Arc<tokio::sync::Notify>> = if options.assume_unused {
                    graphics_mt.do_lazy_allocs();
                    Some(Default::default())
                } else {
                    None
                };
                let sound_mt = sound.get_sound_mt();
                let notifier_thread = notifier.clone();
                DefaultItem {
                    task: io
                        .rt
                        .then(default_item, |default_item| async move {
                            let ContainerLoadedItem::Directory(default_item) = default_item else {
                                return Err(anyhow::anyhow!("default item must be a directory"));
                            };

                            if let Some(notifier) = notifier_thread {
                                notifier.notified().await;
                            }

                            L::load(
                                "default",
                                ContainerLoadedItem::Directory(default_item.clone()),
                                // dummy
                                &ContainerLoadedItemDir::new(Default::default()),
                                &runtime_thread_pool,
                                &graphics_mt,
                                &sound_mt,
                            )
                            .map(|item| (item, default_item))
                        })
                        .abortable(),
                    notifier: notifier.map(DefaultItemNotifyOnDrop),
                }
            }),
            // create a dummy, all paths must have checked if default item was loaded
            default_loaded_item: Arc::new(ContainerLoadedItemDir::new(Default::default())),
            default_key: Rc::new("default".try_into().unwrap()),

            allows_single_audio_or_txt_files: options.allows_single_audio_or_txt_files,

            io,
            graphics_mt: graphics.get_graphics_mt(),
            texture_handle: graphics.texture_handle.clone(),
            sound_mt: sound.get_sound_mt(),
            sound_object_handle: sound_scene.sound_object_handle.clone(),
            runtime_thread_pool,

            container_name: container_name.to_string(),

            resource_http_download_url,
            resource_server_download_url,
            base_path: base_path.to_path_buf(),
            downloaded_path: "downloaded".into(),

            resource_http_download_index: Default::default(),
            cached_http_download_index: Default::default(),
            http_download_index_task: None,
            resource_dir_index: Either::Right(None),

            resource_http_download_meta: Default::default(),
            cached_http_download_meta: Default::default(),
            http_download_meta_task: None,

            last_update_time: None,
            last_update_interval_time: None,
        }
    }

    fn check_default_loaded(&mut self) {
        // make sure default is loaded
        if let Some(DefaultItem { task, notifier }) = self.default_item.take() {
            // explicit drop here
            drop(notifier);
            let (default_item, default_loaded_item) = task
                .get_storage()
                .map_err(|err| {
                    anyhow!(
                        "failed to load default files for \"{}\": {err}",
                        self.base_path.to_string_lossy()
                    )
                })
                .unwrap();
            self.default_loaded_item = Arc::new(default_loaded_item);
            self.items.insert(
                (*self.default_key).clone(),
                ContainerItem {
                    item: default_item.convert(&self.texture_handle, &self.sound_object_handle),
                    used_last_in: Duration::ZERO,
                },
            );
        }
    }

    /// Update this container, removing unused items
    ///
    /// `update_interval` is the time to wait before doing another update check.
    /// This allows to save some runtime cost.
    /// `try_max_items` tries to unload items as soon as the limit is hit. which does not mean
    /// that it must unload items (e.g. if they were used in this frame).
    pub fn update<'a>(
        &mut self,
        cur_time: &Duration,
        entry_lifetime: &Duration,
        update_interval: &Duration,
        force_used_items: impl Iterator<Item = &'a ContainerKey>,
        try_max_items: Option<ContainerMaxItems<'_>>,
    ) {
        let above_threshold = try_max_items
            .as_ref()
            .is_some_and(|max_items| self.items.len() > max_items.count.get());
        if self
            .last_update_interval_time
            .is_none_or(|time| cur_time.saturating_sub(time) >= *update_interval)
            || above_threshold
        {
            self.last_update_interval_time = Some(*cur_time);

            self.check_default_loaded();

            // make sure these entries are always kept loaded
            for force_used_item in force_used_items {
                if let Some(item) = self.items.to_back(force_used_item) {
                    item.used_last_in = *cur_time;
                }
            }

            // If max items is hit, then use the lifetime specified in the max items info
            let entry_lifetime = match above_threshold.then_some(try_max_items).flatten() {
                Some(max_items) => max_items.entry_lifetime,
                None => entry_lifetime,
            };

            // all items that were not used lately
            // are always among the first items
            // delete them if they were not used lately
            while !self.items.is_empty() {
                let (name, item) = self.items.iter_mut().next().unwrap();
                if self.last_update_time.is_some_and(|last_update_time| {
                    last_update_time.saturating_sub(item.used_last_in) > *entry_lifetime
                }) && name.ne(&self.default_key)
                {
                    let name_clone = name.clone();
                    let _ = self.items.remove(&name_clone).unwrap();
                } else {
                    break;
                }
            }
            let item = self.items.to_back(&self.default_key).unwrap();
            item.used_last_in = *cur_time;
        }
        self.last_update_time = Some(*cur_time);
    }

    /// Verifies a resource, prints warnings on error
    fn verify_resource(file_ty: &str, file_name: &str, file: &[u8], allow_hq_assets: bool) -> bool {
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
                    log::warn!(
                        "downloaded image resource (png) {}\
                        is not a valid png file: {}",
                        file_name,
                        err
                    );
                    return false;
                }
            }
            "ogg" => {
                if let Err(err) = verify_ogg_vorbis(file) {
                    log::warn!(
                        "downloaded sound resource (ogg vorbis) \
                        ({}) is not a valid ogg vorbis file: {}",
                        file_name,
                        err
                    );
                    return false;
                }
            }
            "txt" => match verify_txt(file, file_name) {
                Ok(_) => {}
                Err(err) => {
                    log::warn!("{err}");
                    return false;
                }
            },
            _ => {
                log::warn!(
                    "Unsupported resource type {} for {} \
                    could not be validated",
                    file_ty,
                    file_name,
                );
                return false;
            }
        }
        true
    }

    async fn try_load_container_http_index(
        container_name: &str,
        http: &Arc<dyn HttpClientInterface>,
        http_index: &mut Option<Arc<anyhow::Result<AssetsIndex>>>,
        base_path: &Path,
        resource_http_download_url: &Url,
    ) {
        // try to download index
        if http_index.is_none() {
            if let Some(download_url) = relative_path_to_url(&base_path.join("index.json"))
                .ok()
                .and_then(|name| resource_http_download_url.join(&name).ok())
            {
                let r = http
                    .download_text(download_url)
                    .await
                    .map_err(|err| anyhow!(err))
                    .and_then(|index_file| {
                        serde_json::from_str::<AssetsIndex>(&index_file)
                            .map_err(|err| anyhow::anyhow!(err))
                    });

                if let Err(err) = &r {
                    info!(target: &container_name, "failed to create http index for {container_name}: {err}");
                }

                *http_index = Some(Arc::new(r));
            }
        }
    }

    async fn try_load_container_http_meta(
        container_name: &str,
        http: &Arc<dyn HttpClientInterface>,
        http_meta: &mut Option<Arc<anyhow::Result<AssetsMeta>>>,
        base_path: &Path,
        resource_http_download_url: &Url,
    ) {
        // try to download meta index
        if http_meta.is_none() {
            if let Some(download_url) = relative_path_to_url(&base_path.join("meta.json"))
                .ok()
                .and_then(|name| resource_http_download_url.join(&name).ok())
            {
                let r = http
                    .download_text(download_url)
                    .await
                    .map_err(|err| anyhow!(err))
                    .and_then(|file| {
                        serde_json::from_str::<AssetsMeta>(&file)
                            .map_err(|err| anyhow::anyhow!(err))
                    });

                if let Err(err) = &r {
                    debug!(target: &container_name, "failed to create http meta index for {container_name}: {err}");
                }

                *http_meta = Some(Arc::new(r));
            }
        }
    }

    async fn load_container_item(
        container_name: String,
        fs: Arc<dyn FileSystemInterface>,
        http: Arc<dyn HttpClientInterface>,
        http_download_tasks: Arc<Semaphore>,
        base_path: PathBuf,
        downloaded_path: PathBuf,
        key: ContainerKey,
        game_server_http: Option<Url>,
        resource_http_download: HttpIndexAndUrl,
        allows_single_audio_or_txt_files: bool,
    ) -> anyhow::Result<ContainerLoadedItem> {
        let allow_hq_assets = false;

        let save_to_disk = |name: &str, file: &[u8]| {
            let name = name.to_string();
            let file = file.to_vec();
            let fs = fs.clone();
            let dir_path = downloaded_path.join(&base_path);
            let key_name = key.name.to_string();
            Box::pin(async move {
                {
                    if let Err(err) = fs.create_dir(&dir_path).await {
                        log::warn!(
                            "Failed to create directory for downloaded file {} to disk: {err}",
                            key_name
                        );
                    } else if let Err(err) = fs.write_file(dir_path.join(name).as_ref(), file).await
                    {
                        log::warn!(
                            "Failed to write downloaded file {} to disk: {err}",
                            key_name
                        );
                    }
                }
            })
        };

        let download_base_path = downloaded_path.join(&base_path);

        // if key hash a hash, try to load item with that hash from disk
        // or download it from the game server if supported
        // else it will be ignored
        let files = if let Some(hash) = key.hash {
            // try to load dir with that name
            let mut files = None;

            if let Ok(dir_files) = fs
                .files_in_dir_recursive(&download_base_path.join(format!(
                    "{}_{}",
                    key.name.as_str(),
                    fmt_hash(&hash)
                )))
                .await
            {
                files = Some(ContainerLoadedItem::Directory(ContainerLoadedItemDir::new(
                    dir_files,
                )));
            }

            // else try to load tar with that name
            if files.is_none() {
                if let Ok(file) = fs
                    .read_file(&download_base_path.join(format!(
                        "{}_{}.tar",
                        key.name.as_str(),
                        fmt_hash(&hash)
                    )))
                    .await
                {
                    if let Ok(tar_files) = read_tar(&file) {
                        files = Some(ContainerLoadedItem::Directory(ContainerLoadedItemDir::new(
                            tar_files,
                        )));
                    }
                }
            }

            // else try to load single file (.png, .ogg or similar)
            // Note: for now only try image files, doesn't seem worth it for sound files
            if files.is_none() {
                if let Ok(file) = fs
                    .read_file(&download_base_path.join(format!(
                        "{}_{}.png",
                        key.name.as_str(),
                        fmt_hash(&hash)
                    )))
                    .await
                {
                    files = Some(ContainerLoadedItem::SingleFile(file));
                }
            }

            // if loading still failed, switch to http download
            if files.is_none() {
                let name = format!("{}_{}.tar", key.name.as_str(), fmt_hash(&hash));
                if let Some(game_server_http) = game_server_http.as_ref().and_then(|url| {
                    url.join(relative_path_to_url(&base_path.join(&name)).ok()?.as_str())
                        .ok()
                }) {
                    let _g = http_download_tasks.acquire().await?;
                    if let Ok(file) = http.download_binary(game_server_http, &hash).await {
                        if let Ok(tar_files) = read_tar(&file) {
                            let mut verified = true;
                            for (name, file) in &tar_files {
                                if !Self::verify_resource(
                                    name.extension().and_then(|s| s.to_str()).unwrap_or(""),
                                    name.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
                                    file,
                                    allow_hq_assets,
                                ) {
                                    verified = false;
                                    break;
                                }
                            }
                            if verified {
                                save_to_disk(&name, &file).await;
                                files = Some(ContainerLoadedItem::Directory(
                                    ContainerLoadedItemDir::new(tar_files),
                                ));
                            }
                        }
                    }
                }
            }

            // at last, try a single .png, .ogg file etc.
            // Note: for now only try image files, doesn't seem worth it for sound files
            if files.is_none() {
                let name = format!("{}_{}.png", key.name.as_str(), fmt_hash(&hash));
                if let Some(game_server_http) = game_server_http.as_ref().and_then(|url| {
                    url.join(&relative_path_to_url(&base_path.join(&name)).ok()?)
                        .ok()
                }) {
                    let _g = http_download_tasks.acquire().await?;
                    if let Ok(file) = http.download_binary(game_server_http, &hash).await {
                        if Self::verify_resource("png", &name, &file, allow_hq_assets) {
                            save_to_disk(&name, &file).await;
                            files = Some(ContainerLoadedItem::SingleFile(file.to_vec()));
                        }
                    }
                }
            }

            match files {
                Some(files) => Ok(files),
                None => Err(anyhow!(
                    "Could not load/download resource with name {} and hash {}",
                    key.name.as_str(),
                    fmt_hash(&hash)
                )),
            }
        } else {
            let http_entry =
                if let Some((http_index, resource_http_download_url)) = resource_http_download {
                    let mut http_index = http_index.lock().await;

                    // try to download index
                    Self::try_load_container_http_index(
                        &container_name,
                        &http,
                        &mut http_index,
                        &base_path,
                        &resource_http_download_url,
                    )
                    .await;

                    http_index
                        .as_mut()
                        .and_then(|entries| {
                            entries
                                .as_ref()
                                .as_ref()
                                .ok()
                                .map(|entries| entries.get(key.name.as_str()).cloned())
                        })
                        .flatten()
                        .map(|entry| (entry, resource_http_download_url))
                } else {
                    None
                };

            let mut files = None;

            // first try to load from local files without any hash from entry
            {
                // first try png (or .ogg etc., which currently are not supported)
                if let Ok(file) = fs
                    .read_file(&base_path.join(format!("{}.png", key.name.as_str())))
                    .await
                {
                    files = Some(ContainerLoadedItem::SingleFile(file.to_vec()));
                }
                // else try tar
                else if let Ok(file) = fs
                    .read_file(&base_path.join(format!("{}.tar", key.name.as_str())))
                    .await
                {
                    if let Ok(tar_files) = read_tar(&file) {
                        files = Some(ContainerLoadedItem::Directory(ContainerLoadedItemDir::new(
                            tar_files,
                        )));
                    }
                }
                // if still not found, try directory
                if files.is_none() || key.name.as_str() == "default" {
                    if let Ok(dir_files) = fs
                        .files_in_dir_recursive(&base_path.join(key.name.as_str()))
                        .await
                    {
                        files = Some(ContainerLoadedItem::Directory(ContainerLoadedItemDir::new(
                            dir_files,
                        )));
                    }
                }
            }

            // else if an entry exists, first try to load from disk using the entries hash
            if let Some((entry, _)) = files.is_none().then_some(http_entry.as_ref()).flatten() {
                if let Ok(file) = fs
                    .read_file(
                        download_base_path
                            .join(format!(
                                "{}_{}.{}",
                                key.name.as_str(),
                                fmt_hash(&entry.hash),
                                entry.ty
                            ))
                            .as_ref(),
                    )
                    .await
                {
                    if entry.ty == "tar" {
                        if let Ok(tar_files) = read_tar(&file) {
                            files = Some(ContainerLoadedItem::Directory(
                                ContainerLoadedItemDir::new(tar_files),
                            ));
                        }
                    } else if entry.ty == "png"
                        || (allows_single_audio_or_txt_files && entry.ty == "ogg")
                        || (allows_single_audio_or_txt_files && entry.ty == "txt")
                    {
                        files = Some(ContainerLoadedItem::SingleFile(file.to_vec()));
                    }
                }
            }

            // else try to load the entry from http (if active)
            if files.is_none() {
                if let Some((url, name, hash, ty)) = http_entry.and_then(|(entry, download_url)| {
                    let name = format!(
                        "{}_{}.{}",
                        key.name.as_str(),
                        fmt_hash(&entry.hash),
                        entry.ty
                    );
                    download_url
                        .join(&relative_path_to_url(&base_path.join(&name)).ok()?)
                        .map(|url| (url, name, entry.hash, entry.ty))
                        .ok()
                }) {
                    let res = {
                        let _g = http_download_tasks.acquire().await?;
                        http.download_binary(url, &hash).await
                    };
                    match res {
                        Ok(file) => {
                            let write_to_disk = if ty == "tar" {
                                if let Ok(tar_files) = read_tar(&file) {
                                    let mut verified = true;
                                    for (name, file) in &tar_files {
                                        if !Self::verify_resource(
                                            name.extension().and_then(|s| s.to_str()).unwrap_or(""),
                                            name.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
                                            file,
                                            allow_hq_assets,
                                        ) {
                                            verified = false;
                                            break;
                                        }
                                    }
                                    if verified {
                                        files = Some(ContainerLoadedItem::Directory(
                                            ContainerLoadedItemDir::new(tar_files),
                                        ));
                                        true
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            } else if ty == "png" {
                                if Self::verify_resource("png", &name, &file, allow_hq_assets) {
                                    files = Some(ContainerLoadedItem::SingleFile(file.to_vec()));
                                    true
                                } else {
                                    false
                                }
                            } else if allows_single_audio_or_txt_files && ty == "ogg" {
                                if Self::verify_resource("ogg", &name, &file, allow_hq_assets) {
                                    files = Some(ContainerLoadedItem::SingleFile(file.to_vec()));
                                    true
                                } else {
                                    false
                                }
                            } else if allows_single_audio_or_txt_files && ty == "txt" {
                                if Self::verify_resource("txt", &name, &file, allow_hq_assets) {
                                    files = Some(ContainerLoadedItem::SingleFile(file.to_vec()));
                                    true
                                } else {
                                    false
                                }
                            } else {
                                false
                            };
                            if write_to_disk {
                                save_to_disk(&name, &file).await;
                            }
                        }
                        Err(err) => {
                            log::warn!(
                                "Download for {} failed, even tho it was part of the index: {err}",
                                key.name.as_str()
                            );
                        }
                    }
                }
            }

            match files {
                Some(files) => Ok(files),
                None => Err(anyhow!(
                    "Could not load/download resource with name {} (without hash)",
                    key.name.as_str(),
                )),
            }
        };
        files
    }

    pub fn load_default(io: &Io, base_path: &Path) -> IoRuntimeTask<ContainerLoadedItem> {
        let fs = io.fs.clone();
        let http = io.http.clone();
        let base_path = base_path.to_path_buf();

        let container_name_dummy: String = Default::default();
        io.rt.spawn(async move {
            Self::load_container_item(
                container_name_dummy,
                fs,
                http,
                Arc::new(Semaphore::const_new(1)),
                base_path,
                "".into(),
                "default".try_into().unwrap(),
                None,
                None,
                false,
            )
            .await
        })
    }

    fn load(
        container_name: String,
        graphics_mt: GraphicsMultiThreaded,
        sound_mt: SoundMultiThreaded,
        runtime_thread_pool: &Arc<rayon::ThreadPool>,
        io: &Io,
        http_download_tasks: &Arc<Semaphore>,
        base_path: PathBuf,
        downloaded_path: PathBuf,
        key: ContainerKey,
        game_server_http: Option<Url>,
        resource_http_download: HttpIndexAndUrl,
        default_loaded_item: Arc<ContainerLoadedItemDir>,
        allows_single_audio_or_txt_files: bool,
    ) -> IoRuntimeTask<L> {
        let fs = io.fs.clone();
        let http = io.http.clone();
        let runtime_thread_pool = runtime_thread_pool.clone();
        let http_download_tasks = http_download_tasks.clone();

        io.rt.spawn(async move {
            let item_name = key.name.clone();

            let files = Self::load_container_item(
                container_name,
                fs,
                http,
                http_download_tasks,
                base_path,
                downloaded_path,
                key,
                game_server_http,
                resource_http_download,
                allows_single_audio_or_txt_files,
            )
            .await;

            match files {
                Ok(files) => Ok(L::load(
                    item_name.as_str(),
                    files,
                    &default_loaded_item,
                    &runtime_thread_pool,
                    &graphics_mt,
                    &sound_mt,
                )?),
                Err(err) => Err(err),
            }
        })
    }

    /// Get the item for the given key,
    /// or if not exist, try to load it.
    /// Return default as long as the item is loading
    /// or if the item was not found.
    pub fn get_or_default<Q>(&mut self, name: &Q) -> &A
    where
        Q: Borrow<ContainerKey>,
    {
        self.check_default_loaded();

        let item_res = self.items.get(name.borrow());
        if item_res.is_some() {
            let item = self.items.to_back(name.borrow()).unwrap();
            item.used_last_in = self.last_update_time.unwrap_or_default();
            &item.item
        } else {
            // try to load the item
            let task = if let Some(load_item_res) = self.loading_tasks.get_mut(name.borrow()) {
                Some((name.borrow(), load_item_res, true))
            }
            // Rate limit the requests a bit
            else if self.loading_tasks.len() < CONTAINER_MAX_TASKS
                && !self.failed_tasks.contains(name.borrow())
            {
                let base_path = self.base_path.clone();
                let downloaded_path = self.downloaded_path.clone();
                let key = name.borrow().clone();
                let game_server_http = self.resource_server_download_url.clone();
                let resource_http = self.resource_http_download_url.clone();
                let default_loaded_item = self.default_loaded_item.clone();
                self.loading_tasks.insert(
                    key.clone(),
                    Self::load(
                        self.container_name.clone(),
                        self.graphics_mt.clone(),
                        self.sound_mt.clone(),
                        &self.runtime_thread_pool,
                        &self.io,
                        &self.http_download_tasks,
                        base_path,
                        downloaded_path,
                        key,
                        game_server_http,
                        resource_http.map(|url| (self.resource_http_download_index.clone(), url)),
                        default_loaded_item,
                        self.allows_single_audio_or_txt_files,
                    ),
                );
                None
            }
            // make sure the loading continues at any cost
            else {
                self.loading_tasks
                    .iter_mut()
                    .next()
                    .map(|(q, v)| (q, v, false))
            };

            if let Some((name, load_item, should_return_new_item)) = task {
                if load_item.is_finished() {
                    let name = name.clone();
                    if let Some(load_item) = self.loading_tasks.remove(&name) {
                        let loaded_item = load_item.get_storage();
                        match loaded_item {
                            Ok(item) => {
                                let new_item =
                                    item.convert(&self.texture_handle, &self.sound_object_handle);
                                self.items.insert(
                                    name.clone(),
                                    ContainerItem {
                                        item: new_item,
                                        used_last_in: self.last_update_time.unwrap_or_default(),
                                    },
                                );
                                if should_return_new_item {
                                    return &self.items.get(&name).unwrap().item;
                                }
                            }
                            Err(err) => {
                                log::info!(
                                    target: &self.container_name,
                                    "Error while loading item \"{}\": {}",
                                    name.name.as_str(),
                                    err
                                );
                                self.failed_tasks.insert(name);
                            }
                        }
                    }
                }
            }

            let item = self.items.to_back(&self.default_key).unwrap();
            item.used_last_in = self.last_update_time.unwrap_or_default();
            &item.item
        }
    }

    /// Automatically uses the default key if the given key is `None`,
    /// otherwise identical to [`Container::get_or_default`].
    pub fn get_or_default_opt<Q>(&mut self, name: Option<&Q>) -> &A
    where
        Q: Borrow<ContainerKey>,
    {
        let default_key = self.default_key.clone();
        self.get_or_default(name.map(|name| name.borrow()).unwrap_or(&default_key))
    }

    /// Checks if the given key is fully loaded.
    ///
    /// Note: does not trigger any kind of loading process.
    pub fn contains_key<Q>(&self, name: &Q) -> bool
    where
        Q: Borrow<ContainerKey>,
    {
        self.items.contains_key(name.borrow())
    }

    /// Remove all items and load tasks, except for the default item.
    pub fn clear_except_default(&mut self) {
        let default_item = self.items.remove(&self.default_key);
        self.items.clear();
        self.loading_tasks.clear();
        self.failed_tasks.clear();
        if let Some(default_item) = default_item {
            self.items.insert((*self.default_key).clone(), default_item);
        }
    }

    /// Checks if default already loaded without initiating the loading process
    pub fn is_default_loaded(&self) -> bool {
        self.default_item.is_none()
    }

    /// Sets the resource download url and if
    /// different to the last url, then also
    /// clears the container.
    pub fn set_resource_download_url_and_clear_on_change(&mut self, url: Option<Url>) {
        if url != self.resource_server_download_url {
            self.resource_server_download_url = url;

            self.clear_except_default();
        }
    }

    /// Returns `true` if the resource was loaded without a fail.
    ///
    /// This call is non-blocking.
    pub fn is_loaded<Q>(&mut self, name: &Q) -> bool
    where
        Q: Borrow<ContainerKey>,
    {
        {
            let item_res = self.items.get(name.borrow());
            if item_res.is_none() {
                // try to load the resource
                self.get_or_default(name);
                false
            } else {
                true
            }
        }
    }

    /// Returns `true` if the resource was loaded or failed to load (and will not be loaded),
    /// `false` otherwise.
    ///
    /// This call is non-blocking.
    pub fn is_loaded_or_failed<Q>(&mut self, name: &Q) -> bool
    where
        Q: Borrow<ContainerKey>,
    {
        self.failed_tasks.contains(name.borrow()) || self.is_loaded(name)
    }

    /// Blocking wait for the item to be finished.
    ///
    /// This is only useful for programs that don't run
    /// in real time.
    pub fn blocking_wait_loaded<Q>(&mut self, name: &Q)
    where
        Q: Borrow<ContainerKey>,
    {
        let item_res = self.items.get(name.borrow());
        if item_res.is_none() {
            self.get_or_default(name);
            if let Some(load_item) = self.loading_tasks.get_mut(name.borrow()) {
                load_item.blocking_wait_finished();
            }
            self.get_or_default(name);
        }
    }

    /// Get a list of entries that can potentially be loaded by this
    /// container.
    /// This also includes assets downloaded over http (if supported/active)
    pub fn entries_index(&mut self) -> HashMap<String, ContainerItemIndexType> {
        let mut entries: HashMap<String, ContainerItemIndexType> = Default::default();
        // do http first so that on collision, disk overwrites the ContainerItemIndexType
        {
            // check if there already is a http index
            if let Some(dir_index) = &self.cached_http_download_index {
                if let Ok(dir_index) = dir_index.as_ref() {
                    entries.extend(
                        dir_index
                            .iter()
                            .map(|(name, _)| (name.clone(), ContainerItemIndexType::Http)),
                    );
                }
            } else {
                let mut needs_download = false;
                // else first check if it was already downloaded
                if let Ok(dir_index) = self.resource_http_download_index.try_lock() {
                    needs_download |= dir_index.is_none();
                    self.cached_http_download_index = (*dir_index).clone();
                }
                // else start a download
                else {
                    needs_download = true;
                }
                if needs_download {
                    match self.http_download_index_task.as_mut() {
                        Some(task) => {
                            match task {
                                Some(inner_task) => {
                                    if inner_task.is_finished() {
                                        if let Err(err) =
                                            task.take().map(|t| t.get_storage()).transpose()
                                        {
                                            log::error!(
                                                "Failed to download http index for {}: {err}",
                                                self.container_name
                                            );
                                        }
                                    }
                                }
                                None => {
                                    // ignore
                                }
                            }
                        }
                        None => {
                            let resource_http_download_index =
                                self.resource_http_download_index.clone();
                            let container_name = self.container_name.clone();
                            let http = self.io.http.clone();
                            let base_path = self.base_path.clone();
                            let resource_http_download_url =
                                self.resource_http_download_url.clone();
                            self.http_download_index_task =
                                Some(Some(self.io.rt.spawn(async move {
                                    let mut index = resource_http_download_index.lock().await;
                                    if let Some(resource_http_download_url) =
                                        resource_http_download_url
                                    {
                                        if index.is_none() {
                                            Self::try_load_container_http_index(
                                                &container_name,
                                                &http,
                                                &mut index,
                                                &base_path,
                                                &resource_http_download_url,
                                            )
                                            .await;
                                        }
                                    } else {
                                        *index = Some(Arc::new(Err(anyhow!(
                                            "http download url is not given"
                                        ))));
                                    }
                                    Ok(())
                                })));
                        }
                    }
                }
            }
        }
        let dir_index = &mut self.resource_dir_index;
        match dir_index {
            Either::Left(dir_index) => {
                if let Ok(dir_index) = dir_index {
                    entries.extend(
                        dir_index
                            .clone()
                            .into_iter()
                            .map(|i| (i, ContainerItemIndexType::Disk)),
                    );
                }
            }
            Either::Right(task) => {
                match task {
                    Some(task) => {
                        if task.is_finished() {
                            let res_dir_index = std::mem::replace(
                                &mut self.resource_dir_index,
                                Either::Right(None),
                            );
                            if let Either::Right(Some(task)) = res_dir_index {
                                let res_dir_index = task.get_storage().ok().unwrap_or_else(|| {
                                    Err(anyhow!("get entries in dir task failed."))
                                });
                                self.resource_dir_index = Either::Left(res_dir_index);
                            }
                        }
                    }
                    None => {
                        // load index
                        let fs = self.io.fs.clone();
                        let path = self.base_path.clone();
                        let container_name = self.container_name.clone();
                        let task = self.io.rt.spawn(async move {
                            let entries = fs.entries_in_dir(&path).await;

                            if let Err(err) = &entries {
                                info!(target: &container_name,
                                    "failed to create index for {container_name}: {err}");
                            }

                            let entries = entries.map(|mut entries| {
                                // filter entries that end with an hash
                                entries.retain(|entry, _| {
                                    let entry: &Path = entry.as_ref();
                                    if let Some((_, name_hash)) = entry
                                        .file_stem()
                                        .and_then(|s| s.to_str())
                                        .and_then(|s| s.rsplit_once('_'))
                                    {
                                        if name_hash.len() == Hash::default().len() * 2
                                            && name_hash
                                                .find(|c: char| !c.is_ascii_hexdigit())
                                                .is_none()
                                        {
                                            return false;
                                        }
                                    }
                                    true
                                });
                                entries
                                    .keys()
                                    .map(|entry| {
                                        let entry: &Path = entry.as_ref();
                                        entry
                                            .file_stem()
                                            .map(|s| s.to_string_lossy().to_string())
                                            .unwrap_or_default()
                                    })
                                    .collect()
                            });

                            Ok(entries)
                        });

                        *dir_index = Either::Right(Some(task));
                    }
                }
            }
        }
        entries
    }

    /// Get a list of meta data for entries. It should generally be assumed that
    /// not all entries also exist in the [`Self::entries_index`] or vise versa.
    pub fn entries_meta_data(&mut self) -> AssetsMeta {
        let mut entries: AssetsMeta = Default::default();
        // check if there already is a http index
        if let Some(dir_index) = &self.cached_http_download_meta {
            if let Ok(dir_index) = dir_index.as_ref() {
                entries.extend(dir_index.clone());
            }
        } else {
            let mut needs_download = false;
            // else first check if it was already downloaded
            if let Ok(dir_index) = self.resource_http_download_meta.try_lock() {
                needs_download |= dir_index.is_none();
                self.cached_http_download_meta = (*dir_index).clone();
            }
            // else start a download
            else {
                needs_download = true;
            }
            if needs_download {
                match self.http_download_meta_task.as_mut() {
                    Some(task) => {
                        match task {
                            Some(inner_task) => {
                                if inner_task.is_finished() {
                                    if let Err(err) =
                                        task.take().map(|t| t.get_storage()).transpose()
                                    {
                                        log::error!(
                                            "Failed to download http meta index for {}: {err}",
                                            self.container_name
                                        );
                                    }
                                }
                            }
                            None => {
                                // ignore
                            }
                        }
                    }
                    None => {
                        let resource_http_download_meta = self.resource_http_download_meta.clone();
                        let container_name = self.container_name.clone();
                        let http = self.io.http.clone();
                        let base_path = self.base_path.clone();
                        let resource_http_download_url = self.resource_http_download_url.clone();
                        self.http_download_meta_task = Some(Some(self.io.rt.spawn(async move {
                            let mut meta_index = resource_http_download_meta.lock().await;
                            if let Some(resource_http_download_url) = resource_http_download_url {
                                if meta_index.is_none() {
                                    Self::try_load_container_http_meta(
                                        &container_name,
                                        &http,
                                        &mut meta_index,
                                        &base_path,
                                        &resource_http_download_url,
                                    )
                                    .await;
                                }
                            } else {
                                *meta_index =
                                    Some(Arc::new(Err(anyhow!("http download url is not given"))));
                            }
                            Ok(())
                        })));
                    }
                }
            }
        }
        entries
    }
}

pub struct DataFilePartResult<'a> {
    data: &'a Vec<u8>,
    /// Was loaded by the default fallback mechanism
    from_default: bool,
}

/// helper functions the containers can use to quickly load
/// one part or if not existing, the default part
pub fn load_file_part<'a>(
    files: &'a ContainerLoadedItemDir,
    default_files: &'a ContainerLoadedItemDir,
    item_name: &str,
    extra_paths: &[&str],
    part_name: &str,
    allow_default: bool,
) -> anyhow::Result<DataFilePartResult<'a>> {
    let mut part_full_path = PathBuf::new();
    extra_paths.iter().for_each(|extra_path| {
        part_full_path.push(extra_path);
    });
    part_full_path.push(part_name);
    part_full_path.set_extension("png");

    let is_default = item_name == "default";

    let file = files.files.get(&part_full_path);

    match file {
        None => {
            if !is_default && allow_default {
                // try to load default part instead
                let mut png_path_def = PathBuf::new();
                extra_paths.iter().for_each(|extra_path| {
                    png_path_def.push(extra_path);
                });
                png_path_def.push(part_name);
                png_path_def.set_extension("png");
                let file_def = default_files.files.get(&png_path_def);
                if let Some(file_def) = file_def {
                    Ok(DataFilePartResult {
                        data: file_def,
                        from_default: true,
                    })
                } else {
                    Err(anyhow!("default asset part ({part_name}) not found"))
                }
            } else {
                Err(anyhow!(
                    "default asset part ({}) not found in {:?}",
                    part_name,
                    part_full_path,
                ))
            }
        }
        Some(file) => Ok(DataFilePartResult {
            data: file,
            from_default: false,
        }),
    }
}

pub struct PngFilePartResult {
    pub png: PngResultPersistent,
    /// Was loaded by the default fallback mechanism
    pub from_default: bool,
}

pub fn load_file_part_as_png(
    files: &ContainerLoadedItemDir,
    default_files: &ContainerLoadedItemDir,
    item_name: &str,
    extra_paths: &[&str],
    part_name: &str,
) -> anyhow::Result<PngFilePartResult> {
    load_file_part_as_png_ex(
        files,
        default_files,
        item_name,
        extra_paths,
        part_name,
        true,
    )
}

pub fn load_file_part_as_png_ex(
    files: &ContainerLoadedItemDir,
    default_files: &ContainerLoadedItemDir,
    item_name: &str,
    extra_paths: &[&str],
    part_name: &str,
    allow_default: bool,
) -> anyhow::Result<PngFilePartResult> {
    let file = load_file_part(
        files,
        default_files,
        item_name,
        extra_paths,
        part_name,
        allow_default,
    )?;
    let mut img_data = Vec::<u8>::new();
    let part_img = load_png_image_as_rgba(file.data, |width, height, bytes_per_pixel| {
        img_data = vec![0; width * height * bytes_per_pixel];
        &mut img_data
    })?;
    Ok(PngFilePartResult {
        png: part_img.prepare_moved_persistent().to_persistent(img_data),
        from_default: file.from_default,
    })
}

pub struct ImgFilePartResult {
    pub img: ContainerItemLoadData,
    /// Was loaded by the default fallback mechanism
    pub from_default: bool,
}

pub fn load_file_part_and_upload(
    graphics_mt: &GraphicsMultiThreaded,
    files: &ContainerLoadedItemDir,
    default_files: &ContainerLoadedItemDir,
    item_name: &str,
    extra_paths: &[&str],
    part_name: &str,
) -> anyhow::Result<ImgFilePartResult> {
    load_file_part_and_upload_ex(
        graphics_mt,
        files,
        default_files,
        item_name,
        extra_paths,
        part_name,
        true,
    )
}

pub fn load_file_part_and_upload_ex(
    graphics_mt: &GraphicsMultiThreaded,
    files: &ContainerLoadedItemDir,
    default_files: &ContainerLoadedItemDir,
    item_name: &str,
    extra_paths: &[&str],
    part_name: &str,
    allow_default: bool,
) -> anyhow::Result<ImgFilePartResult> {
    let part_img = load_file_part_as_png_ex(
        files,
        default_files,
        item_name,
        extra_paths,
        part_name,
        allow_default,
    )?;
    let mut img = graphics_mt.mem_alloc(GraphicsMemoryAllocationType::TextureRgbaU8 {
        width: (part_img.png.width as usize).try_into().unwrap(),
        height: (part_img.png.height as usize).try_into().unwrap(),
        flags: TexFlags::empty(),
    });
    img.as_mut_slice().copy_from_slice(&part_img.png.data);
    if let Err(err) = graphics_mt.try_flush_mem(&mut img, true) {
        // Ignore the error, but log it.
        log::debug!("err while flushing memory: {} for {part_name}", err);
    }
    Ok(ImgFilePartResult {
        img: ContainerItemLoadData {
            width: part_img.png.width,
            height: part_img.png.height,
            depth: 1,
            data: img,
        },
        from_default: part_img.from_default,
    })
}

pub fn load_file_part_list_and_upload(
    graphics_mt: &GraphicsMultiThreaded,
    files: &ContainerLoadedItemDir,
    default_files: &ContainerLoadedItemDir,
    item_name: &str,
    extra_paths: &[&str],
    part_name_base: &str,
) -> anyhow::Result<Vec<ContainerItemLoadData>> {
    let mut textures = Vec::new();
    let mut i = 0;
    let mut allow_default = true;
    loop {
        match load_file_part_and_upload_ex(
            graphics_mt,
            files,
            default_files,
            item_name,
            extra_paths,
            &format!("{part_name_base}_{:03}", i + 1),
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
    Ok(textures)
}

#[derive(Debug, Hiarc)]
pub struct SoundFilePartResult {
    pub mem: SoundBackendMemory,
    /// Was loaded by the default fallback mechanism
    pub from_default: bool,
}

pub fn load_sound_file_part_and_upload(
    sound_mt: &SoundMultiThreaded,
    files: &ContainerLoadedItemDir,
    default_files: &ContainerLoadedItemDir,
    item_name: &str,
    extra_paths: &[&str],
    part_name: &str,
) -> anyhow::Result<SoundFilePartResult> {
    load_sound_file_part_and_upload_ex(
        sound_mt,
        files,
        default_files,
        item_name,
        extra_paths,
        part_name,
        true,
    )
}

pub fn load_sound_file_part_and_upload_ex(
    sound_mt: &SoundMultiThreaded,
    files: &ContainerLoadedItemDir,
    default_files: &ContainerLoadedItemDir,
    item_name: &str,
    extra_paths: &[&str],
    part_name: &str,
    allow_default: bool,
) -> anyhow::Result<SoundFilePartResult> {
    let mut sound_path = PathBuf::new();

    for extra_path in extra_paths {
        sound_path = sound_path.join(Path::new(extra_path));
    }

    sound_path = sound_path.join(Path::new(&format!("{}.ogg", part_name)));

    let is_default = item_name == "default";

    let (file, from_default) = match files.files.get(&sound_path) {
        Some(file) => Ok((file, false)),
        None => {
            if !is_default && allow_default {
                // try to load default part instead
                let mut path_def = PathBuf::new();
                extra_paths.iter().for_each(|extra_path| {
                    path_def.push(extra_path);
                });
                path_def.push(part_name);
                path_def.set_extension("ogg");
                default_files
                    .files
                    .get(&path_def)
                    .ok_or_else(|| {
                        anyhow!(
                            "requested sound file {} didn't exist in default items",
                            item_name
                        )
                    })
                    .map(|s| (s, true))
            } else {
                Err(anyhow!(
                    "requested sound file for {} not found: {}",
                    item_name,
                    part_name
                ))
            }
        }
    }?;

    let mut mem = sound_mt.mem_alloc(file.len());
    mem.as_mut_slice().copy_from_slice(file);
    if let Err(err) = sound_mt.try_flush_mem(&mut mem) {
        // Ignore the error, but log it.
        log::debug!("err while flushing memory: {} for {part_name}", err);
    }

    Ok(SoundFilePartResult { mem, from_default })
}

pub fn load_sound_file_part_list_and_upload(
    sound_mt: &SoundMultiThreaded,
    files: &ContainerLoadedItemDir,
    default_files: &ContainerLoadedItemDir,
    item_name: &str,
    extra_paths: &[&str],
    part_name_base: &str,
) -> anyhow::Result<Vec<SoundBackendMemory>> {
    let mut sounds = Vec::new();
    let mut i = 0;
    let mut allow_default = true;
    loop {
        match load_sound_file_part_and_upload_ex(
            sound_mt,
            files,
            default_files,
            item_name,
            extra_paths,
            &format!("{part_name_base}_{:03}", i + 1),
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
    Ok(sounds)
}

/// returns the png data, the width and height are the 3d texture w & h, additionally the depth is returned
pub fn load_file_part_as_png_and_convert_3d(
    runtime_thread_pool: &Arc<rayon::ThreadPool>,
    files: &ContainerLoadedItemDir,
    default_files: &ContainerLoadedItemDir,
    item_name: &str,
    extra_paths: &[&str],
    part_name: &str,
) -> anyhow::Result<(PngFilePartResult, usize)> {
    let file = load_file_part(
        files,
        default_files,
        item_name,
        extra_paths,
        part_name,
        true,
    )?;
    let mut img_data = Vec::<u8>::new();
    let part_img = load_png_image_as_rgba(file.data, |width, height, bytes_per_pixel| {
        img_data = vec![0; width * height * bytes_per_pixel];
        &mut img_data
    })?;

    let mut part_img = part_img.prepare_moved_persistent().to_persistent(img_data);

    let mut tex_3d: Vec<u8> = Vec::new();
    tex_3d.resize(
        part_img.width as usize * part_img.height as usize * 4,
        Default::default(),
    );
    let mut image_3d_width = 0;
    let mut image_3d_height = 0;
    if !texture_2d_to_3d(
        runtime_thread_pool,
        &part_img.data,
        part_img.width as usize,
        part_img.height as usize,
        4,
        16,
        16,
        tex_3d.as_mut_slice(),
        &mut image_3d_width,
        &mut image_3d_height,
    ) {
        Err(anyhow!("error while converting entities to 3D"))?
    }

    part_img.width = image_3d_width as u32;
    part_img.height = image_3d_height as u32;
    part_img.data = tex_3d;

    Ok((
        PngFilePartResult {
            png: part_img,
            from_default: file.from_default,
        },
        16 * 16,
    ))
}

pub fn load_file_part_and_convert_3d_and_upload(
    graphics_mt: &GraphicsMultiThreaded,
    runtime_thread_pool: &Arc<rayon::ThreadPool>,
    files: &ContainerLoadedItemDir,
    default_files: &ContainerLoadedItemDir,
    item_name: &str,
    extra_paths: &[&str],
    part_name: &str,
) -> anyhow::Result<ImgFilePartResult> {
    let (part_img, depth) = load_file_part_as_png_and_convert_3d(
        runtime_thread_pool,
        files,
        default_files,
        item_name,
        extra_paths,
        part_name,
    )?;
    let mut img = graphics_mt.mem_alloc(GraphicsMemoryAllocationType::TextureRgbaU82dArray {
        width: (part_img.png.width as usize).try_into().unwrap(),
        height: (part_img.png.height as usize).try_into().unwrap(),
        depth: depth.try_into().unwrap(),
        flags: TexFlags::empty(),
    });
    img.as_mut_slice().copy_from_slice(&part_img.png.data);
    if let Err(err) = graphics_mt.try_flush_mem(&mut img, true) {
        // Ignore the error, but log it.
        log::debug!("err while flushing memory: {} for {part_name}", err);
    }
    Ok(ImgFilePartResult {
        img: ContainerItemLoadData {
            width: part_img.png.width,
            height: part_img.png.height,
            depth: depth as u32,
            data: img,
        },
        from_default: part_img.from_default,
    })
}
