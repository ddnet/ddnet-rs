use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, mpsc::channel, Arc, Mutex, RwLock},
    thread::JoinHandle,
    time::Duration,
};

use async_trait::async_trait;
use base_io_traits::fs_traits::{
    FileSystemEntryTy, FileSystemInterface, FileSystemPath, FileSystemType,
    FileSystemWatcherItemInterface, HashMap,
};
use directories::ProjectDirs;
use hashlink::LinkedHashMap;
use hiarc::Hiarc;
use notify::{event::RenameMode, recommended_watcher, RecommendedWatcher, RecursiveMode, Watcher};
use path_slash::PathBufExt;
use virtual_fs::{host_fs, mem_fs, AsyncReadExt, DirEntry, OpenOptionsConfig};

#[derive(Debug)]
struct FileSystemWatcherPath {
    watchers_of_path: Arc<RwLock<LinkedHashMap<usize, Arc<AtomicBool>>>>,
    watcher: Option<RecommendedWatcher>,
    thread: Option<JoinHandle<()>>,
    path: PathBuf,
}

impl FileSystemWatcherPath {
    pub fn new(path: &Path, file: Option<&Path>) -> Self {
        // Create a channel to receive the events.
        let (tx, rx) = channel();

        // Create a watcher object, delivering debounced events.
        // The notification back-end is selected based on the platform.
        let mut watcher = recommended_watcher(tx).unwrap();

        // Add a path to be watched. All files and directories at that path and
        // below will be monitored for changes.
        if let Err(err) = watcher.watch(path, RecursiveMode::Recursive) {
            log::info!(target: "fs-watch", "could not watch directory/file: {err}");
        }

        let watchers_of_path: Arc<RwLock<LinkedHashMap<usize, Arc<AtomicBool>>>> =
            Arc::new(RwLock::new(Default::default()));
        let watchers_of_path_thread = watchers_of_path.clone();
        let file_thread = file.map(|file| path.join(file));

        let watch_thread = std::thread::Builder::new()
            .name("file-watcher".to_string())
            .spawn(move || loop {
                match rx.recv() {
                    Ok(res) => {
                        if let Ok(ev) = res {
                            let mut handle_ev = match ev.kind {
                                notify::EventKind::Any => false,
                                notify::EventKind::Other => false,
                                notify::EventKind::Access(ev) => match ev {
                                    notify::event::AccessKind::Any => false,
                                    notify::event::AccessKind::Read => false,
                                    notify::event::AccessKind::Open(_) => false,
                                    notify::event::AccessKind::Close(ev) => match ev {
                                        notify::event::AccessMode::Any => false,
                                        notify::event::AccessMode::Execute => false,
                                        notify::event::AccessMode::Read => false,
                                        notify::event::AccessMode::Write => true,
                                        notify::event::AccessMode::Other => false,
                                    },
                                    notify::event::AccessKind::Other => false,
                                },
                                notify::EventKind::Create(ev) => match ev {
                                    notify::event::CreateKind::Any => false,
                                    notify::event::CreateKind::Other => false,
                                    notify::event::CreateKind::File => true,
                                    notify::event::CreateKind::Folder => true,
                                },
                                notify::EventKind::Modify(ev) => {
                                    match ev {
                                        notify::event::ModifyKind::Any
                                        | notify::event::ModifyKind::Data(_)
                                        | notify::event::ModifyKind::Metadata(_)
                                        | notify::event::ModifyKind::Other
                                        | notify::event::ModifyKind::Name(
                                            RenameMode::Any | RenameMode::Other,
                                        ) => {
                                            // only listen for modify events in a pure directory mode
                                            file_thread.is_none()
                                        }
                                        notify::event::ModifyKind::Name(
                                            RenameMode::Both | RenameMode::To | RenameMode::From,
                                        ) => true,
                                    }
                                }
                                notify::EventKind::Remove(ev) => match ev {
                                    notify::event::RemoveKind::Any => false,
                                    notify::event::RemoveKind::Other => false,
                                    notify::event::RemoveKind::File => true,
                                    notify::event::RemoveKind::Folder => true,
                                },
                            };
                            if let Some(file) = &file_thread {
                                // check if the file exists
                                if !ev.paths.iter().any(|path| file.eq(path)) {
                                    handle_ev = false;
                                }
                            }
                            if handle_ev {
                                // if the file exist, make sure the file is not modified for at least 1 second
                                if let Some(file_thread) = &file_thread {
                                    let mut last_modified = None;

                                    while let Ok(file) = std::fs::File::open(file_thread) {
                                        if let Some(modified) = file
                                            .metadata()
                                            .ok()
                                            .and_then(|metadata| metadata.modified().ok())
                                        {
                                            if let Some(file_last_modified) = last_modified {
                                                if modified == file_last_modified {
                                                    break;
                                                } else {
                                                    // else try again
                                                    last_modified = Some(modified);
                                                }
                                            } else {
                                                last_modified = Some(modified);
                                            }
                                        } else {
                                            break;
                                        }
                                        drop(file);
                                        std::thread::sleep(Duration::from_secs(1));
                                    }
                                }

                                watchers_of_path_thread
                                    .read()
                                    .as_ref()
                                    .unwrap()
                                    .values()
                                    .for_each(|watcher_bool| {
                                        watcher_bool
                                            .store(true, std::sync::atomic::Ordering::Relaxed)
                                    });
                            }
                        }
                    }
                    Err(_) => {
                        return;
                    }
                }
            })
            .unwrap();

        Self {
            watchers_of_path,
            watcher: Some(watcher),
            thread: Some(watch_thread),
            path: path.into(),
        }
    }
}

impl Drop for FileSystemWatcherPath {
    fn drop(&mut self) {
        if let Err(err) = self
            .watcher
            .as_mut()
            .unwrap()
            .unwatch(Path::new(&self.path))
        {
            log::info!(target: "fs-watch", "could not stop watching directory/file: {err}");
        }

        let mut watcher_swap = None;
        std::mem::swap(&mut watcher_swap, &mut self.watcher);

        drop(watcher_swap.unwrap());

        let mut thread_swap = None;
        std::mem::swap(&mut thread_swap, &mut self.thread);

        thread_swap.unwrap().join().unwrap();
    }
}

#[derive(Debug, Default)]
struct FileSystemWatcher {
    path_watchers: HashMap<PathBuf, FileSystemWatcherPath>,
    path_watcher_id_generator: usize,
}

pub struct FileSystemWatcherItem {
    fs_watcher: Arc<Mutex<FileSystemWatcher>>,
    path_watcher_id: usize,
    is_changed: Arc<AtomicBool>,
    path: PathBuf,
}

impl FileSystemWatcherItem {
    fn new(path: &Path, file: Option<&Path>, fs_watcher: &Arc<Mutex<FileSystemWatcher>>) -> Self {
        let mut actual_path = PathBuf::from(path);
        if let Some(file) = file {
            actual_path.push(file);
        }
        let mut fs_watcher_write = fs_watcher.lock().unwrap();
        let path_watcher_id = fs_watcher_write.path_watcher_id_generator;
        fs_watcher_write.path_watcher_id_generator += 1;
        let is_changed = Arc::new(AtomicBool::new(false));
        if let Some(path_watcher) = fs_watcher_write.path_watchers.get_mut(&actual_path) {
            path_watcher
                .watchers_of_path
                .write()
                .as_mut()
                .unwrap()
                .insert(path_watcher_id, is_changed.clone());
        } else {
            let path_watcher = FileSystemWatcherPath::new(path, file);
            path_watcher
                .watchers_of_path
                .write()
                .as_mut()
                .unwrap()
                .insert(path_watcher_id, is_changed.clone());
            fs_watcher_write
                .path_watchers
                .insert(actual_path.clone(), path_watcher);
        }

        Self {
            fs_watcher: fs_watcher.clone(),
            path_watcher_id,
            is_changed,
            path: actual_path,
        }
    }
}

impl FileSystemWatcherItemInterface for FileSystemWatcherItem {
    fn has_file_change(&self) -> bool {
        self.is_changed
            .compare_exchange(
                true,
                false,
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
            )
            .unwrap_or(false)
    }
}

impl Drop for FileSystemWatcherItem {
    fn drop(&mut self) {
        let mut fs_watcher_write = self.fs_watcher.lock().unwrap();
        let path_watcher = fs_watcher_write.path_watchers.get_mut(&self.path).unwrap();
        let mut watchers_of_path_guard = path_watcher.watchers_of_path.write();
        let watchers_of_path = watchers_of_path_guard.as_mut().unwrap();
        watchers_of_path.remove(&self.path_watcher_id);
        let watchers_empty = watchers_of_path.is_empty();
        drop(watchers_of_path_guard);
        if watchers_empty {
            fs_watcher_write.path_watchers.remove(&self.path);
        }
    }
}

pub trait ScopedDirFileSystemInterface: virtual_fs::FileSystem + virtual_fs::FileOpener {}

impl ScopedDirFileSystemInterface for host_fs::FileSystem {}
impl ScopedDirFileSystemInterface for mem_fs::FileSystem {}

#[derive(Debug, Hiarc)]
pub struct ScopedDirFileSystem {
    #[hiarc_skip_unsafe]
    pub fs: Box<dyn ScopedDirFileSystemInterface>,
    pub host_path: PathBuf,
    pub mount_path: PathBuf,
}

impl ScopedDirFileSystem {
    pub fn new(host_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        Ok(Self {
            fs: Box::new(host_fs::FileSystem::new(
                tokio::runtime::Handle::current(),
                host_path.as_ref(),
            )?),
            host_path: host_path.as_ref().to_path_buf(),
            mount_path: "".into(),
        })
    }

    pub fn get_path(&self, path: impl AsRef<Path>) -> PathBuf {
        path_clean::clean(self.mount_path.join(path.as_ref()))
    }
}

#[derive(Debug)]
pub struct FileSystem {
    scoped_file_systems: Vec<ScopedDirFileSystem>,
    config_dir_index: usize,
    data_dir_index: usize,
    exec_dir_index: usize,

    fs_watcher: Arc<Mutex<FileSystemWatcher>>,

    secure_path: PathBuf,
    cache_path: PathBuf,

    max_operations_semaphore: Arc<tokio::sync::Semaphore>,
}

impl FileSystem {
    #[cfg(not(feature = "bundled_data_dir"))]
    fn get_data_dir_fs() -> anyhow::Result<ScopedDirFileSystem> {
        ScopedDirFileSystem::new("data/")
    }
    #[cfg(feature = "bundled_data_dir")]
    fn get_data_dir_fs() -> anyhow::Result<ScopedDirFileSystem> {
        use virtual_fs::AsyncWriteExt;
        const DATA_DIR: include_dir::Dir =
            include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../data");

        let fs: Box<dyn ScopedDirFileSystemInterface> = Box::new(mem_fs::FileSystem::default());

        fn add_dirs(
            fs: &dyn ScopedDirFileSystemInterface,
            dir: &include_dir::Dir,
        ) -> anyhow::Result<()> {
            let add_file = |dir: &include_dir::Dir| {
                for file in dir.files() {
                    let mut fs_file = fs.open(
                        &PathBuf::from("/").join(file.path()),
                        &OpenOptionsConfig {
                            read: false,
                            write: true,
                            create_new: true,
                            create: true,
                            append: false,
                            truncate: false,
                        },
                    )?;
                    tokio::runtime::Handle::current()
                        .block_on(fs_file.write_all(file.contents()))?;
                }
                anyhow::Ok(())
            };
            for dir in dir.dirs() {
                fs.create_dir(PathBuf::from("/").join(dir.path()).as_path())?;

                add_file(dir)?;
                add_dirs(fs, dir)?;
            }
            Ok(())
        }

        add_dirs(fs.as_ref(), &DATA_DIR)?;

        Ok(ScopedDirFileSystem {
            fs,
            host_path: "data/".into(),
            mount_path: "/".into(),
        })
    }

    pub fn new_with_data_dir(
        rt: &tokio::runtime::Runtime,
        qualifier: &str,
        organization: &str,
        application: &str,
        secure_appl: &str,
        data_dir: ScopedDirFileSystem,
    ) -> anyhow::Result<Self> {
        let config_dir: PathBuf =
            if let Some(proj_dirs) = ProjectDirs::from(qualifier, organization, application) {
                proj_dirs.config_dir().into()
            } else {
                application.into()
            };
        std::fs::DirBuilder::new()
            .recursive(true)
            .create(&config_dir)?;
        let secure_dir: PathBuf =
            if let Some(proj_dirs) = ProjectDirs::from(qualifier, organization, secure_appl) {
                proj_dirs.data_dir().into()
            } else {
                secure_appl.into()
            };
        std::fs::DirBuilder::new()
            .recursive(true)
            .create(&secure_dir)?;

        let cache_dir: PathBuf =
            if let Some(cache_dirs) = ProjectDirs::from(qualifier, organization, application) {
                cache_dirs.cache_dir().into()
            } else {
                application.into()
            };
        std::fs::DirBuilder::new()
            .recursive(true)
            .create(&cache_dir)?;

        // enter tokio runtime for [ScopedDirectoryFileSystem::new_with_default_runtime]
        let g = rt.enter();
        let mut scoped_file_systems: Vec<ScopedDirFileSystem> = Vec::new();
        log::info!(target: "fs", "Found config dir in {config_dir:?}");
        scoped_file_systems.push(ScopedDirFileSystem::new(config_dir)?);
        let config_dir_index = scoped_file_systems.len() - 1;

        scoped_file_systems.push(data_dir);
        let data_dir_index = scoped_file_systems.len() - 1;

        if let Ok(exec_path) = std::env::current_dir() {
            scoped_file_systems.push(ScopedDirFileSystem::new(exec_path)?);
        }
        drop(g);
        // if worst case this is equal to the data dir
        let exec_dir_index = scoped_file_systems.len() - 1;
        Ok(Self {
            scoped_file_systems,
            config_dir_index,
            data_dir_index,
            exec_dir_index,
            fs_watcher: Arc::new(Mutex::new(FileSystemWatcher::default())),

            secure_path: secure_dir,
            cache_path: cache_dir,

            // at most allow 64 files to be read/written at the same time.
            max_operations_semaphore: Arc::new(tokio::sync::Semaphore::new(64)),
        })
    }

    pub fn new(
        rt: &tokio::runtime::Runtime,
        qualifier: &str,
        organization: &str,
        application: &str,
        secure_appl: &str,
    ) -> anyhow::Result<Self> {
        let g = rt.enter();
        let data_dir = Self::get_data_dir_fs()?;
        drop(g);

        Self::new_with_data_dir(
            rt,
            qualifier,
            organization,
            application,
            secure_appl,
            data_dir,
        )
    }

    fn get_scoped_fs(&self, fs_path: FileSystemPath) -> &ScopedDirFileSystem {
        let index: usize;
        match fs_path {
            FileSystemPath::OfType(of_type) => match of_type {
                FileSystemType::ReadWrite => index = self.config_dir_index,
                FileSystemType::Read => index = self.data_dir_index,
                FileSystemType::Exec => index = self.exec_dir_index,
            },
            FileSystemPath::Index(path_index) => index = path_index,
        }
        &self.scoped_file_systems[index]
    }

    fn get_path(&self, path: &Path, fs_path: FileSystemPath) -> PathBuf {
        let mut res = self.get_scoped_fs(fs_path).host_path.clone();
        res.push(path);
        res
    }

    async fn entries_in_dir_impl(
        &self,
        path: &Path,
        fs: &ScopedDirFileSystem,
        ignore_list: &HashMap<String, FileSystemEntryTy>,
    ) -> anyhow::Result<HashMap<String, FileSystemEntryTy>> {
        let mut file_list: HashMap<String, FileSystemEntryTy> = Default::default();
        let mut dir_read = virtual_fs::FileSystem::read_dir(&fs.fs, path)?;

        while let Some(Ok(DirEntry {
            path,
            metadata: Ok(metadata),
        })) = dir_read.next()
        {
            if !metadata.is_dir() && !metadata.is_file() {
                continue;
            }
            let Some(file_name) = path.file_name() else {
                continue;
            };
            let file_name = file_name.to_string_lossy().to_string();
            if ignore_list.contains_key(&file_name) {
                continue;
            }
            file_list.insert(
                file_name,
                if metadata.is_dir() {
                    FileSystemEntryTy::Directory
                } else {
                    let timestamp = Duration::from_nanos(metadata.created);
                    FileSystemEntryTy::File {
                        date: <chrono::DateTime<chrono::Utc>>::from_timestamp(
                            timestamp.as_secs() as i64,
                            timestamp.subsec_nanos(),
                        )
                        .map(|d| {
                            <chrono::DateTime<chrono::Local>>::from(d)
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string()
                        })
                        .unwrap_or_default(),
                    }
                },
            );
        }
        Ok(file_list)
    }

    async fn files_in_dir_recursive_impl(
        &self,
        path: &Path,
        rec_path: PathBuf,
        fs: &ScopedDirFileSystem,
        fs_path: FileSystemPath,
        ignore_list: &HashMap<PathBuf, Vec<u8>>,
    ) -> anyhow::Result<HashMap<PathBuf, Vec<u8>>> {
        let mut read_dirs = vec![rec_path.clone()];
        let mut file_list: HashMap<PathBuf, Vec<u8>> = Default::default();

        while let Some(rec_path) = read_dirs.pop() {
            let path = path.join(&rec_path);
            let mut dir_reader = virtual_fs::FileSystem::read_dir(&fs.fs, &path)?;

            while let Some(Ok(entry)) = dir_reader.next() {
                let file_type_res = entry.file_type();

                let entry_name = entry.file_name();
                let file_path = rec_path.join(&entry_name);
                if let Ok(file_type) = file_type_res {
                    let file_path_slash = file_path.to_slash_lossy().as_ref().into();
                    if file_type.is_file() && !ignore_list.contains_key(&file_path_slash) {
                        let file = self
                            .read_file_in(path.join(&entry_name).as_ref(), fs_path)
                            .await?;
                        file_list.insert(file_path_slash, file);
                    } else if file_type.is_dir() {
                        read_dirs.push(file_path);
                    }
                }
            }
        }

        Ok(file_list)
    }

    pub async fn write_file_for_fs(
        fs: &ScopedDirFileSystem,
        file_path: &Path,
        data: Vec<u8>,
    ) -> std::io::Result<()> {
        let host_path = std::path::absolute(&fs.host_path)?;
        let tmp_file_path = host_path.join("tmp");
        std::fs::create_dir_all(&tmp_file_path)?;
        let file = tokio::task::spawn_blocking(move || {
            let mut file = tempfile::NamedTempFile::new_in(tmp_file_path)?;
            file.write_all(&data)?;
            file.flush()?;
            Ok::<_, std::io::Error>(file)
        })
        .await??;
        let (_, tmp_path) = file.keep()?;

        let rel_path = tmp_path.strip_prefix(&host_path).map_err(|err| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "could not strip {:?} from {:?}: {err}",
                    &host_path, &tmp_path
                ),
            )
        })?;

        let file_path = fs.get_path(file_path);
        fs.fs.rename(rel_path, &file_path).await?;
        Ok(())
    }

    pub async fn read_file_in_fs(
        fs: &ScopedDirFileSystem,
        file_path: &Path,
    ) -> std::io::Result<Vec<u8>> {
        let file_path = fs.get_path(file_path);
        let mut file = fs.fs.open(
            &file_path,
            &OpenOptionsConfig {
                read: true,
                write: false,
                create_new: false,
                create: false,
                append: false,
                truncate: false,
            },
        )?;
        let mut file_res: Vec<_> = Default::default();
        file.read_to_end(&mut file_res).await?;
        file_res.shrink_to_fit();
        Ok(file_res)
    }

    pub async fn file_exists_in_fs(fs: &ScopedDirFileSystem, file_path: &Path) -> bool {
        let file_path = fs.get_path(file_path);
        fs.fs
            .open(
                &file_path,
                &OpenOptionsConfig {
                    read: true,
                    write: false,
                    create_new: false,
                    create: false,
                    append: false,
                    truncate: false,
                },
            )
            .is_ok()
    }

    pub async fn create_dir_in_fs(
        fs: &ScopedDirFileSystem,
        dir_path: &Path,
    ) -> std::io::Result<()> {
        let mut cur_dir = fs.mount_path.clone();
        let dir_path = path_clean::clean(dir_path);
        let components = dir_path.components();
        for comp in components {
            cur_dir.push(comp);
            if let Err(err) = virtual_fs::FileSystem::create_dir(&fs.fs, &cur_dir) {
                match err {
                    virtual_fs::FsError::AlreadyExists => {
                        // ignore
                    }
                    err => {
                        return Err(err.into());
                    }
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl FileSystemInterface for FileSystem {
    async fn read_file(&self, file_path: &Path) -> std::io::Result<Vec<u8>> {
        let _g = self
            .max_operations_semaphore
            .acquire()
            .await
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::BrokenPipe, err.to_string()))?;
        for fs in self.scoped_file_systems.iter() {
            if let Ok(file) = Self::read_file_in_fs(fs, file_path).await {
                return Ok(file);
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("file not found: {file_path:?}"),
        ))
    }

    async fn read_file_in(
        &self,
        file_path: &Path,
        path: FileSystemPath,
    ) -> std::io::Result<Vec<u8>> {
        let _g = self
            .max_operations_semaphore
            .acquire()
            .await
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::BrokenPipe, err.to_string()))?;
        let fs = self.get_scoped_fs(path);
        Self::read_file_in_fs(fs, file_path).await
    }

    async fn file_exists(&self, file_path: &Path) -> bool {
        let Ok(_g) = self.max_operations_semaphore.acquire().await else {
            return false;
        };
        let fs = self.get_scoped_fs(FileSystemPath::OfType(FileSystemType::ReadWrite));
        Self::file_exists_in_fs(fs, file_path).await
    }

    async fn write_file(&self, file_path: &Path, data: Vec<u8>) -> std::io::Result<()> {
        let _g = self
            .max_operations_semaphore
            .acquire()
            .await
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::BrokenPipe, err.to_string()))?;
        let fs = self.get_scoped_fs(FileSystemPath::OfType(FileSystemType::ReadWrite));

        Self::write_file_for_fs(fs, file_path, data).await
    }

    async fn create_dir(&self, dir_path: &Path) -> std::io::Result<()> {
        let _g = self
            .max_operations_semaphore
            .acquire()
            .await
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::BrokenPipe, err.to_string()))?;
        let fs = self.get_scoped_fs(FileSystemPath::OfType(FileSystemType::ReadWrite));
        Self::create_dir_in_fs(fs, dir_path).await
    }

    async fn entries_in_dir(
        &self,
        path: &Path,
    ) -> anyhow::Result<HashMap<String, FileSystemEntryTy>> {
        let _g = self
            .max_operations_semaphore
            .acquire()
            .await
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::BrokenPipe, err.to_string()))?;
        let mut file_list: HashMap<String, FileSystemEntryTy> = Default::default();
        let mut found_one_entry = false;
        for fs in self.scoped_file_systems.iter() {
            let path = fs.get_path(path);
            if let Ok(ext_file_list) = self.entries_in_dir_impl(&path, fs, &file_list).await {
                found_one_entry = true;
                file_list.extend(ext_file_list.into_iter());
            }
        }
        if found_one_entry {
            Ok(file_list)
        } else {
            Err(anyhow::anyhow!("no entry within {:?} was found", path))
        }
    }

    async fn files_in_dir_recursive(
        &self,
        path: &Path,
    ) -> anyhow::Result<HashMap<PathBuf, Vec<u8>>> {
        let _g = self
            .max_operations_semaphore
            .acquire()
            .await
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::BrokenPipe, err.to_string()))?;
        let mut file_list = HashMap::<PathBuf, Vec<u8>>::default();
        let mut found_one_dir = false;
        for (path_index, fs) in self.scoped_file_systems.iter().enumerate() {
            let path = fs.get_path(path);
            if let Ok(list) = self
                .files_in_dir_recursive_impl(
                    &path,
                    "".into(),
                    fs,
                    FileSystemPath::Index(path_index),
                    &file_list,
                )
                .await
            {
                found_one_dir = true;
                file_list.extend(list.into_iter());
            }
        }

        if found_one_dir {
            Ok(file_list)
        } else {
            Err(anyhow::anyhow!("no directory within {:?} was found", path))
        }
    }

    fn get_save_path(&self) -> PathBuf {
        self.get_path(
            ".".as_ref(),
            FileSystemPath::OfType(FileSystemType::ReadWrite),
        )
    }

    fn get_secure_path(&self) -> PathBuf {
        self.secure_path.clone()
    }

    fn get_cache_path(&self) -> PathBuf {
        self.cache_path.clone()
    }

    fn watch_for_change(
        &self,
        path: &Path,
        file: Option<&Path>,
    ) -> Box<dyn FileSystemWatcherItemInterface> {
        let watch_path = self.get_path(path, FileSystemPath::OfType(FileSystemType::ReadWrite));
        Box::new(FileSystemWatcherItem::new(
            &watch_path,
            file,
            &self.fs_watcher,
        ))
    }
}
