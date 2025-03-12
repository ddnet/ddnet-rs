use hashlink::LinkedHashMap;
use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

use super::command_value::CommandValue;

/// The map config is a collection of configurable things,
/// that _can_ be interpreted by the game.
///
/// The main members to understand are commands and config variables.
/// While config variables change exactly one variable, commands
/// can be called multiple times.
///
/// Usually config variables have the advantage of being executed before the
/// current game mod itself is constructed, so you could say it's executed earlier.
///
/// This for example allows to set the game mode (ctf, dm) etc. before the game mode
/// state is created.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Config variables that can be interpreted by server's or theoretically even client's
    /// physics mod, e.g. `sv_team_size 2`.
    pub config_variables: LinkedHashMap<String, CommandValue>,
    /// Commands that can be interpreted by server's or theoretically even client's
    /// game mod, e.g. global tunes or `echo hello`.
    ///
    /// Commands are just a list of raw command strings.
    pub commands: Vec<CommandValue>,
}
