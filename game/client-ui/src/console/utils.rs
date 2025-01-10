use std::ops::Range;

use client_types::console::{entries_to_parser, ConsoleEntry};
use command_parser::parser::{
    self, Command, CommandArgType, CommandParseResult, CommandType, CommandTypeRef, CommandsTyped,
    Syn,
};
use config::{
    config::ConfigEngine,
    parsing::find_modifiers,
    traits::{ConfigFromStrErr, ConfigInterface},
};
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use game_config::config::ConfigGame;

#[derive(Debug, Clone)]
pub enum MatchedType {
    Entry(usize),
    ArgList(usize),
    CustomList { index: usize, custom_ty: String },
}

impl MatchedType {
    pub fn index(&self) -> usize {
        match self {
            MatchedType::Entry(index)
            | MatchedType::ArgList(index)
            | MatchedType::CustomList { index, .. } => *index,
        }
    }
}

pub type MatchedResult = (
    Vec<(MatchedType, (i64, Vec<usize>))>,
    Option<Vec<String>>,
    Range<usize>,
);
pub fn find_matches(
    cmds: &CommandsTyped,
    cursor_pos: usize,
    entries: &[ConsoleEntry],
    msg: &str,
    matches_plugin: &dyn Fn(&str) -> Option<Vec<String>>,
) -> MatchedResult {
    fn get_range_of_partial(
        partial_cmd: &Command,
        range: Option<&Range<usize>>,
        allows_args: bool,
        cursor_byte_pos: usize,
    ) -> Option<(Range<usize>, bool)> {
        if let Some(range) = range.and_then(|range| {
            (allows_args && (range.start..=range.end).contains(&cursor_byte_pos)).then_some(range)
        }) {
            Some((range.clone(), false))
        } else {
            let ident_range =
                partial_cmd.cmd_range.start..=partial_cmd.cmd_range.start + partial_cmd.ident.len();
            if ident_range.contains(&cursor_byte_pos) {
                Some((*ident_range.start()..*ident_range.end(), true))
            } else if allows_args {
                for (syn, arg_range) in &partial_cmd.args {
                    if (arg_range.start..=arg_range.end).contains(&cursor_byte_pos) {
                        return match syn {
                            Syn::Command(cmd) => {
                                get_range_of_partial(cmd, None, allows_args, cursor_byte_pos)
                            }
                            Syn::Commands(cmds) => {
                                for cmd in cmds {
                                    match get_range_of_partial(
                                        cmd,
                                        None,
                                        allows_args,
                                        cursor_byte_pos,
                                    ) {
                                        Some(res) => return Some(res),
                                        None => {
                                            // continue searching
                                        }
                                    }
                                }
                                None
                            }
                            Syn::Text(_)
                            | Syn::Number(_)
                            | Syn::Float(_)
                            | Syn::JsonObjectLike(_)
                            | Syn::JsonArrayLike(_) => Some((arg_range.clone(), false)),
                        };
                    }
                }
                None
            } else {
                None
            }
        }
    }
    fn find_err_range(
        err: &CommandParseResult,
        allows_args: bool,
        cursor_byte_pos: usize,
    ) -> Option<(Range<usize>, bool)> {
        match err {
            CommandParseResult::InvalidCommandIdent { range, .. } => {
                if (range.start..=range.end).contains(&cursor_byte_pos) {
                    Some((range.clone(), true))
                } else {
                    None
                }
            }
            CommandParseResult::InvalidCommandArg {
                err, partial_cmd, ..
            }
            | CommandParseResult::InvalidCommandsArg {
                err, partial_cmd, ..
            } => get_range_of_partial(partial_cmd, None, allows_args, cursor_byte_pos)
                .or_else(|| find_err_range(err, allows_args, cursor_byte_pos)),
            CommandParseResult::InvalidArg {
                range, partial_cmd, ..
            } => get_range_of_partial(partial_cmd, Some(range), allows_args, cursor_byte_pos),
            _ => None,
        }
    }
    fn find_range(
        cmd: &CommandType,
        allows_args: bool,
        cursor_byte_pos: usize,
    ) -> Option<(Range<usize>, bool)> {
        match cmd {
            CommandType::Partial(res) => find_err_range(res, allows_args, cursor_byte_pos),
            CommandType::Full(cmd) => get_range_of_partial(cmd, None, true, cursor_byte_pos),
        }
    }

    let cursor_byte_pos = msg
        .char_indices()
        .nth(cursor_pos)
        .map(|(i, _)| i)
        .or_else(|| (msg.chars().count() == cursor_pos).then_some(msg.len()));

    let mut custom_suggestions = None;

    let mut find_arg_suggestions = || {
        cmds.iter().rev().find_map(|cmd| {
            fn cmd_arg_to_suggestions(
                entries: &[ConsoleEntry],
                matches_plugin: &dyn Fn(&str) -> Option<Vec<String>>,
                cmd_ident: &str,
                arg_index: usize,
                custom_suggestions: &mut Option<String>,
            ) -> Option<Vec<String>> {
                let mut entries_parser = entries_to_parser(entries);
                if let Some(cmd) = entries_parser.remove(cmd_ident).and_then(|mut entry| {
                    (entry.get(arg_index).is_some()).then(|| entry.remove(arg_index))
                }) {
                    match cmd.ty {
                        CommandArgType::TextFrom(list)
                        | CommandArgType::TextArrayFrom { from: list, .. } => {
                            Some(list.into_iter().map(|s| s.into()).collect())
                        }
                        CommandArgType::Command
                        | CommandArgType::CommandIdent
                        | CommandArgType::Commands
                        | CommandArgType::CommandDoubleArg
                        | CommandArgType::JsonObjectLike
                        | CommandArgType::JsonArrayLike => None,
                        CommandArgType::Number | CommandArgType::Float | CommandArgType::Text => {
                            cmd.user_ty
                                .as_ref()
                                .and_then(|user_ty| matches_plugin(user_ty))
                                .inspect(|_| {
                                    *custom_suggestions = cmd.user_ty.map(|t| t.to_string());
                                })
                        }
                    }
                } else {
                    None
                }
            }

            fn find_list_in_arg(
                partial_cmd: &Command,
                entries: &[ConsoleEntry],
                matches_plugin: &dyn Fn(&str) -> Option<Vec<String>>,
                cursor_byte_pos: usize,
                arg_and_range: Option<(&Range<usize>, &usize)>,
                custom_suggestions: &mut Option<String>,
            ) -> Option<Vec<String>> {
                if arg_and_range
                    .map(|(range, _)| (range.start..=range.end).contains(&cursor_byte_pos))
                    .unwrap_or_default()
                {
                    arg_and_range.and_then(|(_, arg_index)| {
                        cmd_arg_to_suggestions(
                            entries,
                            matches_plugin,
                            &partial_cmd.ident,
                            *arg_index,
                            custom_suggestions,
                        )
                    })
                } else {
                    let ident_range = partial_cmd.cmd_range.start
                        ..=partial_cmd.cmd_range.start + partial_cmd.ident.len();
                    if ident_range.contains(&cursor_byte_pos) {
                        None
                    } else {
                        for (index, (_, arg_range)) in partial_cmd.args.iter().enumerate() {
                            if (arg_range.start..=arg_range.end).contains(&cursor_byte_pos) {
                                return cmd_arg_to_suggestions(
                                    entries,
                                    matches_plugin,
                                    &partial_cmd.ident,
                                    index,
                                    custom_suggestions,
                                );
                            }
                        }
                        None
                    }
                }
            }

            fn find_invalid_arg_texts(
                entries: &[ConsoleEntry],
                matches_plugin: &dyn Fn(&str) -> Option<Vec<String>>,
                res: &CommandParseResult,
                cursor_byte_pos: usize,
                custom_suggestions: &mut Option<String>,
            ) -> Option<Vec<String>> {
                match res {
                    CommandParseResult::InvalidArg {
                        arg_index,
                        partial_cmd,
                        range,
                        ..
                    } => find_list_in_arg(
                        partial_cmd,
                        entries,
                        matches_plugin,
                        cursor_byte_pos,
                        Some((range, arg_index)),
                        custom_suggestions,
                    ),
                    CommandParseResult::InvalidCommandArg {
                        err, partial_cmd, ..
                    }
                    | CommandParseResult::InvalidCommandsArg {
                        err, partial_cmd, ..
                    } => find_list_in_arg(
                        partial_cmd,
                        entries,
                        matches_plugin,
                        cursor_byte_pos,
                        None,
                        custom_suggestions,
                    )
                    .or_else(|| {
                        find_invalid_arg_texts(
                            entries,
                            matches_plugin,
                            err,
                            cursor_byte_pos,
                            custom_suggestions,
                        )
                    }),
                    _ => None,
                }
            }

            match cmd {
                CommandType::Full(cmd) => (cursor_byte_pos).and_then(|cursor_byte_pos| {
                    for (index, (_, arg_range)) in cmd.args.iter().enumerate() {
                        if (arg_range.start..=arg_range.end).contains(&cursor_byte_pos) {
                            return cmd_arg_to_suggestions(
                                entries,
                                matches_plugin,
                                &cmd.ident,
                                index,
                                &mut custom_suggestions,
                            );
                        }
                    }
                    None
                }),
                CommandType::Partial(res) => cursor_byte_pos.and_then(|cursor_byte_pos| {
                    find_invalid_arg_texts(
                        entries,
                        matches_plugin,
                        res,
                        cursor_byte_pos,
                        &mut custom_suggestions,
                    )
                }),
            }
        })
    };

    let arg_suggestions = find_arg_suggestions();

    // if a cmd was found that wasn't finished, then suggest
    let Some((msg, range, is_cmd_ident)) = cmds.iter().rev().find_map(|cmd| {
        cursor_byte_pos
            .and_then(|cursor_byte_pos| {
                find_range(cmd, arg_suggestions.is_some(), cursor_byte_pos)
                    .map(|r| (r, cursor_byte_pos))
            })
            .map(|((range, is_cmd_ident), cursor_byte_pos)| {
                let safe_range = range.start.min(msg.len())..range.end.min(msg.len());
                let cursor_range =
                    safe_range.start.min(cursor_byte_pos)..safe_range.end.min(cursor_byte_pos);
                (
                    msg[cursor_range.clone()].to_string(),
                    safe_range,
                    is_cmd_ident,
                )
            })
    }) else {
        return (Vec::new(), Default::default(), 0..0);
    };

    let matcher = SkimMatcherV2::default();
    let (console_inp_without_modifiers, modifiers) = find_modifiers(msg.trim());

    type TmpMatchedResult = (
        Vec<(MatchedType, i64, (i64, Vec<usize>))>,
        Option<Vec<String>>,
    );
    let (mut found_entries, arg_entries): TmpMatchedResult = if let Some(arg_suggestions) =
        arg_suggestions
    {
        (
            arg_suggestions
                .iter()
                .enumerate()
                .map(|(index, s)| {
                    (
                        if let Some(user_ty) = &custom_suggestions {
                            MatchedType::CustomList {
                                index,
                                custom_ty: user_ty.clone(),
                            }
                        } else {
                            MatchedType::ArgList(index)
                        },
                        s.len() as i64,
                        matcher.fuzzy_indices(s, &console_inp_without_modifiers),
                    )
                })
                .filter(|(_, _, m)| m.is_some())
                .map(|(index, len, m)| (index, len, m.unwrap()))
                .collect(),
            Some(arg_suggestions),
        )
    } else if is_cmd_ident {
        (
            entries
                .iter()
                .enumerate()
                .map(|(index, e)| match e {
                    ConsoleEntry::Var(v) => {
                        let max_modifiers = v.full_name.matches("$KEY$").count()
                            + v.full_name.matches("$INDEX$").count();
                        (
                            MatchedType::Entry(index),
                            v.full_name.len() as i64,
                            if modifiers.len() <= max_modifiers {
                                matcher.fuzzy_indices(&v.full_name, &console_inp_without_modifiers)
                            } else {
                                None
                            },
                        )
                    }
                    ConsoleEntry::Cmd(c) => (
                        MatchedType::Entry(index),
                        c.name.len() as i64,
                        matcher.fuzzy_indices(&c.name, &console_inp_without_modifiers),
                    ),
                })
                .filter(|(_, _, m)| m.is_some())
                .map(|(index, len, m)| (index, len, m.unwrap()))
                .collect(),
            None,
        )
    } else {
        return Default::default();
    };

    // not the cleanest way to respect the length in a score sorting, but dunno.
    found_entries.sort_by(|(_, len_a, (score_a, _)), (_, len_b, (score_b, _))| {
        (*score_b * u16::MAX as i64 - *len_b).cmp(&(*score_a * u16::MAX as i64 - *len_a))
    });

    (
        found_entries
            .into_iter()
            .map(|(index, _, fuz)| (index, fuz))
            .collect(),
        arg_entries,
        range,
    )
}

pub fn find_matches_old(entries: &[ConsoleEntry], msg: &str) -> Vec<(usize, (i64, Vec<usize>))> {
    let matcher = SkimMatcherV2::default();
    let (console_inp_without_modifiers, modifiers) = find_modifiers(msg.trim());

    let mut found_entries: Vec<(usize, (i64, Vec<usize>))> = entries
        .iter()
        .enumerate()
        .map(|(index, e)| match e {
            ConsoleEntry::Var(v) => {
                let max_modifiers =
                    v.full_name.matches("$KEY$").count() + v.full_name.matches("$INDEX$").count();
                (
                    index,
                    if modifiers.len() <= max_modifiers {
                        matcher.fuzzy_indices(&v.full_name, &console_inp_without_modifiers)
                    } else {
                        None
                    },
                )
            }
            ConsoleEntry::Cmd(c) => (
                index,
                matcher.fuzzy_indices(&c.name, &console_inp_without_modifiers),
            ),
        })
        .filter(|(_, m)| m.is_some())
        .map(|(index, m)| (index, m.unwrap()))
        .collect();
    found_entries.sort_by(|(_, (score_a, _)), (_, (score_b, _))| score_b.cmp(score_a));
    found_entries
}

pub fn syn_vec_to_config_val(args: &[(Syn, Range<usize>)]) -> Option<String> {
    args.first().map(|(arg, _)| match arg {
        parser::Syn::Command(cmd) => cmd.cmd_text.clone(),
        parser::Syn::Commands(cmds) => cmds
            .first()
            .map(|cmd| cmd.cmd_text.clone())
            .unwrap_or_default(),
        parser::Syn::Text(text) => text.clone(),
        parser::Syn::Number(num) => num.clone(),
        parser::Syn::Float(num) => num.clone(),
        parser::Syn::JsonObjectLike(obj) => obj.clone(),
        parser::Syn::JsonArrayLike(obj) => obj.clone(),
    })
}

pub fn try_apply_config_val(
    cmd_text: &str,
    args: &[(Syn, Range<usize>)],
    config_engine: &mut ConfigEngine,
    config_game: &mut ConfigGame,
) -> anyhow::Result<String, String> {
    let set_val = syn_vec_to_config_val(args);

    config_engine
        .try_set_from_str(
            cmd_text.to_owned(),
            None,
            set_val.clone(),
            None,
            Default::default(),
        )
        .or_else(|err| {
            config_game
                .try_set_from_str(
                    cmd_text.to_owned(),
                    None,
                    set_val.clone(),
                    None,
                    Default::default(),
                )
                .map_err(|err_game| {
                    let mut was_fatal = false;
                    let mut msgs: String = Default::default();
                    match err {
                        ConfigFromStrErr::PathErr(_) => {
                            msgs.push_str(&format!("Parsing error: {}\n", err,));
                        }
                        ConfigFromStrErr::FatalErr(_) => {
                            was_fatal = true;
                        }
                    }
                    match err_game {
                        ConfigFromStrErr::PathErr(_) => {
                            msgs.push_str(&format!("Parsing error: {}\n", err_game,));
                            was_fatal = false;
                        }
                        ConfigFromStrErr::FatalErr(_) => {}
                    }
                    if was_fatal {
                        msgs.push_str(&format!("Parsing errors: {}, {}\n", err, err_game,));
                    }
                    msgs
                })
        })
}

/// Returns `false` if the command was considered partially or fully failed.
pub fn run_command(
    cmd: CommandTypeRef<'_>,
    entries: &[ConsoleEntry],
    config_engine: &mut ConfigEngine,
    config_game: &mut ConfigGame,
    can_change_config: bool,
) -> anyhow::Result<String, String> {
    if let Some(entry_cmd) = entries
        .iter()
        .filter_map(|e| match e {
            client_types::console::ConsoleEntry::Var(_) => None,
            client_types::console::ConsoleEntry::Cmd(c) => Some(c),
        })
        .find(|c| {
            if c.allows_partial_cmds {
                match cmd {
                    CommandTypeRef::Full(cmd) => Some(&cmd.ident),
                    CommandTypeRef::Partial(res) => res.ref_cmd_partial().map(|c| &c.ident),
                }
                .is_some_and(|ident| ident == c.name.as_str())
            } else if let CommandTypeRef::Full(cmd) = cmd {
                cmd.ident == c.name
            } else {
                false
            }
        })
    {
        let cmd = cmd.unwrap_full_or_partial_cmd_ref();
        match (entry_cmd.cmd)(config_engine, config_game, &cmd.cmd_text, &cmd.args) {
            Ok(msg) => Ok(msg),
            Err(err) => Err(format!("Parsing error: {}\n", err)),
        }
    } else {
        let Some((args, cmd_text)) = (match cmd {
            CommandTypeRef::Full(cmd) => Some((cmd.args.clone(), &cmd.cmd_text)),
            CommandTypeRef::Partial(cmd) => {
                if let CommandParseResult::InvalidArg { partial_cmd, .. } = cmd {
                    Some((Vec::new(), &partial_cmd.cmd_text))
                } else {
                    None
                }
            }
        }) else {
            return Err(format!("Invalid argument: {:?}", cmd));
        };

        let set_val = syn_vec_to_config_val(&args);

        if can_change_config {
            match try_apply_config_val(cmd_text, &args, config_engine, config_game) {
                Ok(cur_val) => {
                    if set_val.is_some() {
                        if let Some(var) = entries
                            .iter()
                            .filter_map(|cmd| {
                                if let ConsoleEntry::Var(v) = cmd {
                                    Some(v)
                                } else {
                                    None
                                }
                            })
                            .find(|cmd| cmd.full_name == *cmd_text)
                        {
                            (var.on_set)(cmd_text);
                        }
                        Ok(format!("Updated value for \"{}\": {}\n", cmd_text, cur_val))
                    } else {
                        Ok(format!("Current value for \"{}\": {}\n", cmd_text, cur_val))
                    }
                }
                Err(err) => Err(err),
            }
        } else {
            Err(format!("Failed to apply command: {:?}", cmd))
        }
    }
}

/// Returns `false` if at least one command failed.
pub fn run_commands(
    cmds: &CommandsTyped,
    entries: &[ConsoleEntry],
    config_engine: &mut ConfigEngine,
    config_game: &mut ConfigGame,
    msgs: &mut String,
    can_change_config: bool,
) -> bool {
    let mut res = true;
    for cmd in cmds {
        let msg = match run_command(
            cmd.as_ref(),
            entries,
            config_engine,
            config_game,
            can_change_config,
        ) {
            Ok(msg) => msg,
            Err(msg) => {
                res = false;
                msg
            }
        };
        if !msg.is_empty() {
            msgs.push_str(&format!("{msg}\n"));
        }
    }
    res
}
