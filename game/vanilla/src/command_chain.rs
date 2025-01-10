use std::collections::HashMap;

use base::network_string::NetworkString;
use command_parser::parser::CommandArg;
use game_interface::rcon_entries::RconEntry;

/// A command entry which usually triggers
/// to add a cmd of type `T` to be added to a
/// handler queue.
///
/// For config variables this is usually
/// one enum variant for all vars.
#[derive(Debug, Clone)]
pub struct Command<T> {
    pub rcon: RconEntry,
    pub cmd: T,
}

/// All commands & config variables together build
/// a command chain for the parser and evaluation.
#[derive(Debug)]
pub struct CommandChain<T> {
    cmds: HashMap<NetworkString<65536>, Command<T>>,
    vars: HashMap<NetworkString<65536>, Command<T>>,
    pub parser: HashMap<NetworkString<65536>, Vec<CommandArg>>,
}

impl<T> CommandChain<T> {
    pub fn new(
        cmds: HashMap<NetworkString<65536>, Command<T>>,
        vars: HashMap<NetworkString<65536>, Command<T>>,
    ) -> Self {
        let parser = cmds
            .iter()
            .map(|(name, cmd)| (name.clone(), cmd.rcon.args.clone()))
            .chain(
                vars.iter()
                    .map(|(name, cmd)| (name.clone(), cmd.rcon.args.clone())),
            )
            .collect();
        Self { cmds, vars, parser }
    }

    pub fn by_ident(&self, ident: &str) -> Option<&Command<T>> {
        self.cmds.get(ident).or_else(|| self.vars.get(ident))
    }

    pub fn cmd_list(&self) -> &HashMap<NetworkString<65536>, Command<T>> {
        &self.cmds
    }

    pub fn var_list(&self) -> &HashMap<NetworkString<65536>, Command<T>> {
        &self.vars
    }
}
