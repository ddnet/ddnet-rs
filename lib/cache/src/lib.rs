use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};

use base::hash::{fmt_hash, generate_hash_for, generate_hash_for_multi, Hash};
use base_fs::filesys::{FileSystem, ScopedDirFileSystem};
use base_io::io::Io;
use base_io_traits::fs_traits::{FileSystemInterface, FileSystemPath};
use hiarc::Hiarc;

#[derive(Debug, Hiarc)]
struct CacheImpl {
    cache_name: String,
    cache_fs: ScopedDirFileSystem,
    #[hiarc_skip_unsafe]
    disk_fs: Arc<dyn FileSystemInterface>,
}

/// Make it easy to cache computational expensive processes
/// that result in serializable data with ddnet's filesystem.
///
/// This is a pure filesystem wrapper and does not hold any states,
/// except the ones required to make sure only one cache item is
/// computed at one time
#[derive(Debug, Hiarc)]
pub struct Cache<const VERSION: usize> {
    cache: CacheImpl,
}

impl<const VERSION: usize> Cache<{ VERSION }> {
    pub async fn new_async(cache_name: &str, fs: &Arc<dyn FileSystemInterface>) -> Self {
        Self {
            cache: CacheImpl {
                cache_name: cache_name.to_string(),
                cache_fs: ScopedDirFileSystem::new(fs.get_cache_path()).unwrap(),
                disk_fs: fs.clone(),
            },
        }
    }

    pub fn new(cache_name: &str, io: &Io) -> Self {
        let cache_name = cache_name.to_string();
        let fs = io.fs.clone();
        io.rt
            .spawn(async move {
                Ok(Self {
                    cache: CacheImpl {
                        cache_name,
                        cache_fs: ScopedDirFileSystem::new(fs.get_cache_path()).unwrap(),
                        disk_fs: fs.clone(),
                    },
                })
            })
            .get_storage()
            .unwrap()
    }

    /// returns the dir and the full path
    fn cache_file_path(cache: &CacheImpl, hash: &Hash) -> (PathBuf, PathBuf) {
        let dir_name = Path::new("cache/")
            .join(Path::new(&cache.cache_name))
            .join(Path::new(&format!("v{VERSION}")));
        let hash_path = dir_name.join(Path::new(&format!("f_{}.cached", fmt_hash(hash))));
        (dir_name, hash_path)
    }

    /// like [`Cache::load_from_binary`], but allows additional bytes to be
    /// respected for the hash function
    pub async fn load_from_binary_ex<F>(
        &self,
        original_binary_data: Vec<u8>,
        additional_hash_bytes: &[u8],
        compute_func: F,
    ) -> anyhow::Result<Vec<u8>>
    where
        F: FnOnce(Vec<u8>) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<u8>>> + Send>>,
    {
        let cache = &self.cache;
        let hash = if !additional_hash_bytes.is_empty() {
            generate_hash_for_multi(
                [original_binary_data.as_slice(), additional_hash_bytes].as_slice(),
            )
        } else {
            generate_hash_for_multi([original_binary_data.as_slice()].as_slice())
        };
        let (dir_name, hash_path) = Self::cache_file_path(cache, &hash);
        let file = FileSystem::read_file_in_fs(&self.cache.cache_fs, &hash_path).await;
        match file {
            Ok(cached_file) => Ok(cached_file),
            Err(_) => {
                if let Err(err) =
                    FileSystem::create_dir_in_fs(&self.cache.cache_fs, &dir_name).await
                {
                    Err(err.into())
                } else {
                    let cached_result = compute_func(original_binary_data).await?;
                    if let Err(err) = FileSystem::write_file_for_fs(
                        &self.cache.cache_fs,
                        &hash_path,
                        cached_result.clone(),
                    )
                    .await
                    {
                        Err(err.into())
                    } else {
                        Ok(cached_result)
                    }
                }
            }
        }
    }

    /// Checks if an binary entry exist in the cache, if not
    /// a compute heavy function passed by the user is called.
    /// This allows to skip this calculation the next time
    /// this function is called.
    pub async fn load_from_binary<F>(
        &self,
        original_binary_data: Vec<u8>,
        compute_func: F,
    ) -> anyhow::Result<Vec<u8>>
    where
        F: FnOnce(Vec<u8>) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<u8>>> + Send>>,
    {
        self.load_from_binary_ex(original_binary_data, &[], compute_func)
            .await
    }

    /// Checks if a file exist in the cache, if not
    /// a compute heavy function passed by the user is called.
    /// This allows to skip this calculation the next time
    /// this function is called.
    pub async fn load<F>(
        &self,
        original_file_path: &Path,
        compute_func: F,
    ) -> anyhow::Result<Vec<u8>>
    where
        F: FnOnce(Vec<u8>) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<u8>>> + Send>>,
    {
        let cache = &self.cache;
        let file = cache.disk_fs.read_file(original_file_path).await?;
        self.load_from_binary(file, compute_func).await
    }

    /// Puts a given file into the cache as is, returns the path of the cached file.
    /// This can be used to simply cache a file against changes.
    /// The file extension is kept.
    pub async fn archieve(
        &self,
        original_file_path: &Path,
        in_path: FileSystemPath,
    ) -> anyhow::Result<PathBuf> {
        let cache = &self.cache;
        let file = cache
            .disk_fs
            .read_file_in(original_file_path, in_path)
            .await?;
        let hash = generate_hash_for(&file);
        let (dir_name, mut file_path) = Self::cache_file_path(cache, &hash);

        // archieving will keep the original file ending
        if let Some(ext) = original_file_path.extension() {
            file_path.set_extension(ext);
        }

        // create file
        if !FileSystem::file_exists_in_fs(&cache.cache_fs, &file_path).await {
            FileSystem::create_dir_in_fs(&cache.cache_fs, &dir_name).await?;
            FileSystem::write_file_for_fs(&cache.cache_fs, &file_path, file).await?;
        }

        Ok(file_path)
    }

    /// Returns the the full path of the named file
    fn cache_named_file_path(cache: &CacheImpl, name_path: &Path) -> PathBuf {
        let dir_name = Path::new("cache/named")
            .join(Path::new(&cache.cache_name))
            .join(Path::new(&format!("v{VERSION}")));
        dir_name.join(name_path)
    }

    /// Read a named files from the cache.
    /// It's up to the caller how the file is cached exactly.
    pub async fn read_named(&self, name_path: &Path) -> std::io::Result<Vec<u8>> {
        FileSystem::read_file_in_fs(
            &self.cache.cache_fs,
            &Self::cache_named_file_path(&self.cache, name_path),
        )
        .await
    }

    /// Write a named files to the cache.
    /// It's up to the caller how the file is cached exactly.
    pub async fn write_named(&self, name_path: &Path, file: Vec<u8>) -> std::io::Result<()> {
        let path = Self::cache_named_file_path(&self.cache, name_path);
        if let Some(dir) = path.parent() {
            FileSystem::create_dir_in_fs(&self.cache.cache_fs, dir).await?;
        }
        FileSystem::write_file_for_fs(&self.cache.cache_fs, &path, file).await
    }
}
