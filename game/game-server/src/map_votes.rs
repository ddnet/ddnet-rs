use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::Arc,
};

use anyhow::anyhow;
use base::{hash::decode_hash, network_string::NetworkString};
use base_io_traits::fs_traits::FileSystemInterface;
use game_interface::votes::{MAX_CATEGORY_NAME_LEN, MapVote, MapVoteKey};
use serde::{Deserialize, Serialize};

/// How the json file for map votes is built.
pub type MapCategoriesSerde = HashMap<String, HashMap<String, MapVote>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapVotesFile {
    pub categories: MapCategoriesSerde,
    pub has_unfinished_map_votes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerMapVotes {
    pub categories: BTreeMap<NetworkString<MAX_CATEGORY_NAME_LEN>, BTreeMap<MapVoteKey, MapVote>>,
    pub has_unfinished_map_votes: bool,
}

/// Load map votes from the map votes file.
pub struct MapVotes {
    pub votes: ServerMapVotes,
}

impl MapVotes {
    pub async fn new(
        fs: &Arc<dyn FileSystemInterface>,
        map_votes_file_path: &Path,
    ) -> anyhow::Result<Self> {
        let votes_file: MapVotesFile =
            serde_json::from_slice(&fs.read_file(map_votes_file_path).await?)?;
        Ok(Self {
            votes: ServerMapVotes {
                categories: votes_file
                    .categories
                    .into_iter()
                    .map(|(key, val)| {
                        Ok((
                            key.as_str().try_into()?,
                            val.into_iter()
                                .map(|(key, val)| {
                                    key.rsplit_once("_")
                                        .map(|(name, hash)| {
                                            let name = name.try_into()?;
                                            let hash = decode_hash(hash)
                                                .ok_or_else(|| anyhow!("no hash decoded."))?;
                                            Ok((
                                                MapVoteKey {
                                                    name,
                                                    hash: Some(hash),
                                                },
                                                val.clone(),
                                            ))
                                        })
                                        .unwrap_or_else(|| {
                                            Ok((
                                                MapVoteKey {
                                                    name: key.as_str().try_into()?,
                                                    hash: None,
                                                },
                                                val,
                                            ))
                                        })
                                })
                                .collect::<anyhow::Result<_>>()?,
                        ))
                    })
                    .collect::<anyhow::Result<_>>()?,
                has_unfinished_map_votes: votes_file.has_unfinished_map_votes,
            },
        })
    }
}
