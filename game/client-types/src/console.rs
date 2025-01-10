use std::{collections::HashMap, fmt::Debug, ops::Range, rc::Rc};

use base::network_string::NetworkString;
use command_parser::parser::{CommandArg, Syn};
use config::config::ConfigEngine;
use game_config::config::ConfigGame;

#[derive(Clone)]
pub struct ConsoleEntryVariable {
    pub full_name: String,
    pub usage: String,
    pub description: String,

    /// for parsing
    pub args: Vec<CommandArg>,
    pub on_set: Rc<dyn Fn(&str)>,
}

impl Debug for ConsoleEntryVariable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsoleEntryVariable")
            .field("full_name", &self.full_name)
            .field("usage", &self.usage)
            .field("description", &self.description)
            .field("args", &self.args)
            .field("on_set", &"---")
            .finish()
    }
}

/// A function that takes the config vars,
/// the raw & unparsed ident (so with all modifiers)
/// and the arguments and their range.
pub type ConsoleCmdCb = Rc<
    dyn Fn(
        &mut ConfigEngine,
        &mut ConfigGame,
        &str,
        &[(Syn, Range<usize>)],
    ) -> anyhow::Result<String>,
>;

#[derive(Clone)]
pub struct ConsoleEntryCmd {
    pub name: String,
    pub usage: String,
    pub description: String,
    pub cmd: ConsoleCmdCb,

    /// for parsing
    pub args: Vec<CommandArg>,

    /// Whether this command allows unfinished/partial
    /// commands to be executed.
    ///
    /// The implementation has to deal with invalid or
    /// missing arguments!!
    pub allows_partial_cmds: bool,
}

impl Debug for ConsoleEntryCmd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsoleEntryCmd")
            .field("name", &self.name)
            .field("usage", &self.usage)
            .field("cmd", &"---")
            .field("args", &self.args)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub enum ConsoleEntry {
    Var(ConsoleEntryVariable),
    Cmd(ConsoleEntryCmd),
}

impl ConsoleEntry {
    pub fn args(&self) -> &Vec<CommandArg> {
        match self {
            ConsoleEntry::Var(cmd) => &cmd.args,
            ConsoleEntry::Cmd(cmd) => &cmd.args,
        }
    }
}

pub fn entries_to_parser(
    entries: &[ConsoleEntry],
) -> HashMap<NetworkString<65536>, Vec<CommandArg>> {
    entries
        .iter()
        .map(|entry| match entry {
            ConsoleEntry::Var(entry) => (
                entry.full_name.clone().try_into().unwrap(),
                entry.args.clone(),
            ),
            ConsoleEntry::Cmd(entry) => {
                (entry.name.clone().try_into().unwrap(), entry.args.clone())
            }
        })
        .collect()
}
