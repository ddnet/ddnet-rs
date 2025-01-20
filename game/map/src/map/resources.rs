use base::{hash::Hash, reduced_ascii_str::ReducedAsciiString};
use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

/// a reference to an external resource
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MapResourceMetaData {
    pub blake3_hash: Hash,
    pub ty: ReducedAsciiString,
}

/// a reference to an external resource
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MapResourceRef {
    pub name: ReducedAsciiString,
    pub meta: MapResourceMetaData,
    /// Optional high quality variant. Whether a client
    /// loads this or not depends completely on the client
    /// settings.
    pub hq_meta: Option<MapResourceMetaData>,
}

/// a reference to an external resource
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct Resources {
    pub images: Vec<MapResourceRef>,
    /// images with certain restrictions (divisible by x/y)
    /// e.g. used for tile layers
    pub image_arrays: Vec<MapResourceRef>,
    pub sounds: Vec<MapResourceRef>,
}
