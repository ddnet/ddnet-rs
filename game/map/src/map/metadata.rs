use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

/// The meta data is not useful for the game.
/// They simply exist for completeness
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Metadata {
    pub authors: Vec<String>,
    pub licenses: Vec<String>,
    pub version: String,
    pub credits: String,
    pub memo: String,
}
