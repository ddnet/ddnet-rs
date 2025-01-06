use std::{collections::HashSet, path::PathBuf, sync::Arc};

use base_io_traits::fs_traits::FileSystemInterface;

/// Automatically create votes for all maps locally stored.
pub struct AutoMapVotes {
    pub map_files: HashSet<PathBuf>,
}

impl AutoMapVotes {
    pub async fn new(fs: &Arc<dyn FileSystemInterface>) -> anyhow::Result<Self> {
        let dir = fs.entries_in_dir("map/maps".as_ref()).await?;

        let map_files: HashSet<PathBuf> = dir
            .into_iter()
            .filter_map(|(p, _)| p.ends_with(".twmap").then::<PathBuf, _>(|| p.into()))
            .collect();

        #[cfg(feature = "legacy")]
        let map_files = {
            let mut map_files = map_files;
            let dir = fs.entries_in_dir("legacy/maps".as_ref()).await?;

            map_files.extend(
                dir.into_iter()
                    .filter_map(|(p, _)| p.ends_with(".map").then::<PathBuf, _>(|| p.into())),
            );
            map_files
        };

        Ok(Self { map_files })
    }
}
