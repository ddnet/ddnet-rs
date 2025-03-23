use std::{io::Read, path::PathBuf};

use anyhow::anyhow;
use rustc_hash::FxHashMap;
use tar::EntryType;

pub fn read_tar(file: &[u8]) -> anyhow::Result<FxHashMap<PathBuf, Vec<u8>>> {
    let mut file = tar::Archive::new(std::io::Cursor::new(file));
    match file.entries() {
        Ok(entries) => entries
            .filter_map(|entry| {
                entry
                    .map_err(|err| anyhow::anyhow!(err))
                    .map(|mut entry| {
                        let ty = entry.header().entry_type();
                        if let EntryType::Regular = ty {
                            Some(
                                entry
                                    .path()
                                    .map(|path| path.to_path_buf())
                                    .map_err(|err| anyhow::anyhow!(err))
                                    .and_then(|path| {
                                        let mut file: Vec<_> = Default::default();

                                        entry
                                            .read_to_end(&mut file)
                                            .map(|_| (path, file))
                                            .map_err(|err| anyhow::anyhow!(err))
                                    }),
                            )
                        } else if matches!(ty, EntryType::Directory) {
                            None
                        } else {
                            Some(Err(anyhow!(
                                "The tar reader expects files & dictionaries only"
                            )))
                        }
                    })
                    .transpose()
                    .map(|r| r.and_then(|r| r))
            })
            .collect::<anyhow::Result<FxHashMap<_, _>>>(),
        Err(err) => Err(anyhow::anyhow!(err)),
    }
}
