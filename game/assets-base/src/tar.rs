use std::borrow::Cow;
use std::ops::Range;
use std::path::Path;

use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::{io::Read, path::PathBuf};
use tar::Header;

use anyhow::anyhow;
use rustc_hash::FxHashMap;
use tar::EntryType;

pub struct TarReaderWrapper {
    cur_pos: Arc<AtomicUsize>,
    file: Arc<Vec<u8>>,
}

impl std::io::Read for TarReaderWrapper {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let pos = self.cur_pos.load(std::sync::atomic::Ordering::Relaxed);
        if pos >= self.file.len() {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, ""));
        }
        let file = &self.file[pos..];
        let read_len = file.len().min(buf.len());
        if read_len == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, ""));
        }
        buf.copy_from_slice(&file[0..read_len]);
        self.cur_pos.store(
            self.cur_pos.load(std::sync::atomic::Ordering::Relaxed) + read_len,
            std::sync::atomic::Ordering::Relaxed,
        );
        Ok(read_len)
    }
}

impl std::io::Seek for TarReaderWrapper {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        match pos {
            std::io::SeekFrom::Start(off) => {
                self.cur_pos
                    .store(off as usize, std::sync::atomic::Ordering::Relaxed);
                Ok(off)
            }
            std::io::SeekFrom::End(off) => {
                let pos = self.file.len() as i64 + off;
                if pos < 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "seek pos below 0",
                    ));
                }
                let pos = pos as usize;
                self.cur_pos
                    .store(pos, std::sync::atomic::Ordering::Relaxed);
                Ok(pos as u64)
            }
            std::io::SeekFrom::Current(off) => {
                let pos = self.cur_pos.load(std::sync::atomic::Ordering::Relaxed) as i64 + off;
                if pos < 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "seek pos below 0",
                    ));
                }
                let pos = pos as usize;
                self.cur_pos
                    .store(pos, std::sync::atomic::Ordering::Relaxed);
                Ok(pos as u64)
            }
        }
    }
}

pub type TarBuilder = tar::Builder<Vec<u8>>;
pub struct TarReader {
    tar: tar::Archive<TarReaderWrapper>,

    cur_pos: Arc<AtomicUsize>,
    file: Arc<Vec<u8>>,
}
pub struct TarEntry {
    file: Arc<Vec<u8>>,
    range: Range<usize>,
}
pub type TarEntries = FxHashMap<PathBuf, TarEntry>;

pub fn new_tar() -> TarBuilder {
    let mut builder = tar::Builder::new(Vec::new());
    builder.mode(tar::HeaderMode::Deterministic);
    builder
}

pub fn tar_add_file(builder: &mut TarBuilder, path: impl AsRef<Path>, file: &[u8]) {
    let mut header = Header::new_gnu();
    header.set_cksum();
    header.set_size(file.len() as u64);
    header.set_mode(0o644);
    header.set_uid(1000);
    header.set_gid(1000);
    builder
        .append_data(&mut header, path, std::io::Cursor::new(file))
        .unwrap();
}

/// Constructs a reader for tar files.
///
/// ### Performance
///
/// Causes smaller heap allocations.
pub fn tar_reader(file: Vec<u8>) -> TarReader {
    let file = Arc::new(file);
    let cur_pos: Arc<AtomicUsize> = Default::default();
    TarReader {
        tar: tar::Archive::new(TarReaderWrapper {
            cur_pos: cur_pos.clone(),
            file: file.clone(),
        }),
        cur_pos,
        file,
    }
}

fn entry_to_path<T: std::io::Read>(
    entry: &mut tar::Entry<'_, T>,
) -> Option<Result<PathBuf, anyhow::Error>> {
    let ty = entry.header().entry_type();
    if let EntryType::Regular = ty {
        Some(
            entry
                .path()
                .map(|path| path.to_path_buf())
                .map_err(|err| anyhow::anyhow!(err)),
        )
    } else if matches!(ty, EntryType::Directory) {
        None
    } else {
        Some(Err(anyhow!(
            "The tar reader expects files & dictionaries only"
        )))
    }
}

fn tar_entry_to_vec(
    entry: &mut tar::Entry<'_, std::io::Cursor<Cow<[u8]>>>,
) -> anyhow::Result<Vec<u8>> {
    let mut file: Vec<_> = Default::default();
    entry.read_to_end(&mut file)?;
    Ok(file)
}

pub fn read_tar_files(file: Cow<[u8]>) -> anyhow::Result<FxHashMap<PathBuf, Vec<u8>>> {
    let mut file = tar::Archive::new(std::io::Cursor::new(file));
    match file.entries_with_seek() {
        Ok(entries) => entries
            .filter_map(|entry| {
                entry
                    .map_err(|err| anyhow::anyhow!(err))
                    .map(|mut entry| {
                        entry_to_path(&mut entry).map(|p| {
                            p.and_then(|path| {
                                let file = tar_entry_to_vec(&mut entry)?;
                                Ok((path, file))
                            })
                        })
                    })
                    .transpose()
                    .map(|r| r.and_then(|r| r))
            })
            .collect::<anyhow::Result<FxHashMap<_, _>>>(),
        Err(err) => Err(anyhow::anyhow!(err)),
    }
}

pub fn tar_file_entries(reader: &mut TarReader) -> anyhow::Result<FxHashMap<PathBuf, TarEntry>> {
    reader
        .tar
        .entries_with_seek()?
        .filter_map(|entry| {
            entry
                .map_err(|err| anyhow::anyhow!(err))
                .map(|mut entry| {
                    entry_to_path(&mut entry).map(|p| {
                        p.map(|p| {
                            let pos = reader.cur_pos.load(std::sync::atomic::Ordering::Relaxed);
                            (
                                p,
                                TarEntry {
                                    file: reader.file.clone(),
                                    range: pos..pos + entry.size() as usize,
                                },
                            )
                        })
                    })
                })
                .transpose()
                .map(|r| r.and_then(|r| r))
        })
        .collect::<anyhow::Result<FxHashMap<_, _>>>()
}

pub fn tar_entry_to_file(entry: &TarEntry) -> anyhow::Result<&[u8]> {
    Ok(entry.file.get(entry.range.clone()).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "tar file's size in header was \
                bigger than the actual file",
        )
    })?)
}
