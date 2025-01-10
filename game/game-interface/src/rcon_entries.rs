use std::collections::HashMap;

use base::network_string::NetworkString;
use command_parser::parser::CommandArg;
use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

/// A single rcon command.
#[derive(Debug, Hiarc, Default, Clone, Serialize, Deserialize)]
pub struct RconEntry {
    pub args: Vec<CommandArg>,
    pub usage: NetworkString<65536>,
    pub description: NetworkString<65536>,
}

/// Rcon entries supported by the mod.
///
/// Contains a list of commands & config variables and their required args.
///
/// Entry collisions with the server are evaluated in the following order:
/// - Server config variables (highest priority)
/// - Mod rcon commands & variables (this struct field)
/// - Server rcon commands
///
/// This implies that a mod should usually not use with common prefixes
/// for config variables such as `sv` or `net`, since the server might have
/// variables with that name already.
/// Furthermore this also means that a mod can _override_ rcon commands
/// of the server.
#[derive(Debug, Hiarc, Default, Clone, Serialize, Deserialize)]
pub struct RconEntries {
    pub cmds: HashMap<NetworkString<65536>, RconEntry>,
    pub vars: HashMap<NetworkString<65536>, RconEntry>,
}

#[derive(Debug, Hiarc, Default, Clone, Copy, Serialize, Deserialize)]
pub enum AuthLevel {
    #[default]
    None,
    Moderator,
    Admin,
}

/// A remote console input for the mod to execute.
///
/// Note that some rcon entries for config variables
/// can collide with the ones from the server
/// and the server has higher priority here.
///
/// Please see [`RconEntries`] for more information.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct ExecRconInput {
    /// The raw unprocessed input string.
    pub raw: NetworkString<{ 65536 * 2 + 1 }>,
    /// The auth level the client has for this execution.
    pub auth_level: AuthLevel,
}
