use std::{collections::HashMap, path::PathBuf};

use assets_base::tar::{tar_entry_to_file, tar_file_entries, tar_reader, TarEntries};

/// The map file reader wraps around file in memory.
pub struct MapFileReader {
    pub(crate) entries: TarEntries,
}

impl MapFileReader {
    pub fn new(file: Vec<u8>) -> anyhow::Result<Self> {
        Ok(Self {
            entries: tar_file_entries(&mut tar_reader(file))?,
        })
    }

    pub fn read_all(&self) -> anyhow::Result<HashMap<PathBuf, Vec<u8>>> {
        self.entries
            .iter()
            .map(|(path, entry)| anyhow::Ok((path.clone(), tar_entry_to_file(entry)?.to_vec())))
            .collect::<anyhow::Result<_>>()
    }
}
