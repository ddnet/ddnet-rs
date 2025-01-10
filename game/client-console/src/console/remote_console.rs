use std::{collections::HashMap, rc::Rc};

use base::network_string::NetworkString;
use client_types::console::{ConsoleEntry, ConsoleEntryCmd};
use command_parser::parser::{format_args, CommandArg, CommandArgType};
use egui::Color32;
use game_interface::rcon_entries::RconEntry;
use hiarc::{hiarc_safer_rc_refcell, Hiarc};
use ui_base::ui::UiCreator;

use super::console::ConsoleRender;

#[derive(Debug, Hiarc)]
pub enum RemoteConsoleEvent {
    Exec {
        /// The raw ident text (with all modifiers as is)
        ident_text: String,
        /// The args of the command
        args: String,
    },
}

#[hiarc_safer_rc_refcell]
#[derive(Debug, Default, Hiarc)]
pub struct RemoteConsoleEvents {
    events: Vec<RemoteConsoleEvent>,
}

#[hiarc_safer_rc_refcell]
impl RemoteConsoleEvents {
    pub fn push(&mut self, ev: RemoteConsoleEvent) {
        self.events.push(ev)
    }
}

#[hiarc_safer_rc_refcell]
impl super::console::ConsoleEvents<RemoteConsoleEvent> for RemoteConsoleEvents {
    #[hiarc_trait_is_immutable_self]
    fn take(&mut self) -> Vec<RemoteConsoleEvent> {
        std::mem::take(&mut self.events)
    }
    #[hiarc_trait_is_immutable_self]
    fn push(&mut self, ev: RemoteConsoleEvent) {
        self.events.push(ev);
    }
}

pub type RemoteConsole = ConsoleRender<RemoteConsoleEvent, RemoteConsoleEvents>;

impl RemoteConsole {
    fn args_to_usage(args: &[CommandArg]) -> String {
        let mut usage = String::new();

        for arg in args {
            if let Some(user_ty) = &arg.user_ty {
                usage += &format!("<{}>", user_ty);
            } else {
                match &arg.ty {
                    CommandArgType::Command => usage += "<command> <arg> ",
                    CommandArgType::CommandIdent => usage += "<command_name> ",
                    CommandArgType::Commands => usage += "<command_and_args> ",
                    CommandArgType::CommandDoubleArg => usage += "<command> <arg> <arg> ",
                    CommandArgType::Number => usage += "<number> ",
                    CommandArgType::Float => usage += "<float> ",
                    CommandArgType::Text => usage += "<text> ",
                    CommandArgType::JsonObjectLike => usage += "<json_obj> ",
                    CommandArgType::JsonArrayLike => usage += "<json_arr> ",
                    CommandArgType::TextFrom(texts) => usage += &format!("[{}] ", texts.join(", ")),
                    CommandArgType::TextArrayFrom { from, separator } => {
                        usage += &format!("[{}] (serparator: {})", from.join(", "), separator)
                    }
                }
            }
        }

        usage
    }

    pub fn fill_entries(
        &mut self,
        cmds: HashMap<NetworkString<65536>, RconEntry>,
        vars: HashMap<NetworkString<65536>, RconEntry>,
    ) {
        self.entries.clear();
        for (name, cmd) in cmds.into_iter().chain(vars.into_iter()) {
            let cmds = self.user.clone();
            self.entries.push(ConsoleEntry::Cmd(ConsoleEntryCmd {
                name: name.to_string(),
                usage: if cmd.usage.is_empty() {
                    Self::args_to_usage(&cmd.args)
                } else {
                    cmd.usage.to_string()
                },
                description: cmd.description.to_string(),
                cmd: Rc::new(move |_config_engine, _config_game, ident_text, path| {
                    cmds.push(RemoteConsoleEvent::Exec {
                        ident_text: ident_text.to_string(),
                        args: format_args(path),
                    });
                    Ok(format!("{ident_text} {}", format_args(path)))
                }),
                args: cmd.args,
                allows_partial_cmds: true,
            }));
        }
    }
}

#[derive(Debug, Default)]
pub struct RemoteConsoleBuilder {}

impl RemoteConsoleBuilder {
    pub fn build(creator: &UiCreator) -> RemoteConsole {
        let console_events: RemoteConsoleEvents = Default::default();
        let entries: Vec<ConsoleEntry> = Vec::new();
        ConsoleRender::new(
            creator,
            entries,
            Box::new(console_events.clone()),
            Color32::from_rgba_unmultiplied(50, 0, 0, 150),
            console_events,
        )
    }
}
