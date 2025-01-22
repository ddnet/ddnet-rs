use hashlink::LinkedHashMap;
use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

/// The map config is a collection of configurable things,
/// that _can_ be interpreted by the game.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct Config {
    /// commands that can be interpreted by server or theoretically even client
    /// e.g. `sv_team_size 2`
    pub commands: LinkedHashMap<String, String>,
}
