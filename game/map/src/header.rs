use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct Header {
    /// The type of map
    pub ty: String,
    /// Map version code
    pub version: u64,
}

impl Header {
    pub const VERSION: u64 = 2025061000;
    pub const FILE_TY: &'static str = "twmap";
}
