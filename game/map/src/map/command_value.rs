use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

/// Represents a usually command value with optional
/// comment.
///
/// This is usually used for commands, config variables
/// tune values etc.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandValue {
    /// The value itself
    pub value: String,
    /// An optional comment
    pub comment: Option<String>,
}
