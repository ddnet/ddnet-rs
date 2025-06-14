pub mod tar;
pub mod verify;

use std::collections::HashMap;

use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

/// An entry on a http server.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct AssetIndexEntry {
    pub ty: String,
    pub hash: base::hash::Hash,
    /// File size in bytes
    pub size: u64,
}

pub type AssetsIndex = HashMap<String, AssetIndexEntry>;

/// The (common) meta data to an entry on a http server.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct AssetMetaEntryCommon {
    /// Authors of the asset in order,
    /// where the first author usually is the creator,
    /// followed by remixers etc.
    pub authors: Vec<String>,
    /// The licenses of the asset.
    pub licenses: Vec<String>,
    /// Searchable tags for this asset.
    pub tags: Vec<String>,
    /// An optional _main_ category of the asset.
    pub category: Option<String>,
    /// A human readable description for this asset.
    pub description: String,
    /// Release date of this asset in UTC format.
    pub release_date_utc: chrono::DateTime<chrono::Utc>,

    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

pub type AssetsMeta = HashMap<String, AssetMetaEntryCommon>;

/// Typical asset upload parameters
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct AssetUpload {
    /// Password to authenticate the upload
    pub upload_password: String,
    /// meta data for the asset
    pub meta: AssetMetaEntryCommon,
    /// asset name
    pub name: String,
    /// asset extension,
    pub extension: String,
    /// asset data
    pub data: Vec<u8>,
}

/// Typical asset upload response
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum AssetUploadResponse {
    Success,
    IncorrectPassword,
    IncompleteMetadata,
    InvalidCategory,
    InvalidName,
    UnsupportedFileType,
    BrokenFile(String),
    WritingFailed,
}

/// Typical asset upload parameters
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct AssetDelete {
    /// Password to authenticate the upload
    pub upload_password: String,
    /// asset name
    pub name: String,
}

/// Typical asset upload response
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum AssetDeleteResponse {
    Success,
    IncorrectPassword,
    DeletingFailed,
}
