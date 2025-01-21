use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    fmt::Display,
    ops::{Deref, Range},
};

use anyhow::anyhow;
use base::network_string::NetworkString;
use hiarc::Hiarc;
use logos::Logos;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::escape::{escape, unescape};

use super::tokenizer::{tokenize, HumanReadableToken, Token};

/// Which argument type the command expects next
#[derive(Debug, Hiarc, Clone, PartialEq, Serialize, Deserialize)]
pub enum CommandArgType {
    /// Expects a whole command (including parsing the arguments the command requires)
    Command,
    /// Expects a the name of a command
    CommandIdent,
    /// Expects one or more commands (including parsing the arguments the command requires)
    Commands,
    /// Expects a command and the args of the command twice (special casing for toggle command)
    CommandDoubleArg,
    /// Expects an integer
    Number,
    /// Expects a floating point number
    Float,
    /// Expects a text/string
    Text,
    /// Expects a json object like string
    JsonObjectLike,
    /// Expects a json array like string
    JsonArrayLike,
    /// Expects a text that is part of the given array
    TextFrom(Vec<NetworkString<65536>>),
    /// Expects a list of text that is part of the given array
    /// separated by a given character
    TextArrayFrom {
        from: Vec<NetworkString<65536>>,
        separator: char,
    },
}

impl HumanReadableToken for CommandArgType {
    fn human_readable(&self) -> String {
        match self {
            CommandArgType::Command => "command/variable".to_string(),
            CommandArgType::CommandIdent => "command name".to_string(),
            CommandArgType::Commands => "command(s)".to_string(),
            CommandArgType::CommandDoubleArg => "command arg arg".to_string(),
            CommandArgType::Number => "number".to_string(),
            CommandArgType::Float => "float".to_string(),
            CommandArgType::Text => "text".to_string(),
            CommandArgType::JsonObjectLike => "json-like object".to_string(),
            CommandArgType::JsonArrayLike => "json-like array".to_string(),
            CommandArgType::TextFrom(texts) => format!(
                "one of [{}]",
                texts
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            CommandArgType::TextArrayFrom { from, separator } => {
                format!(
                    "list of [{}] serparated by {}",
                    from.iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    separator
                )
            }
        }
    }
}

pub fn format_args(args: &[(Syn, Range<usize>)]) -> String {
    let mut res = String::new();

    for (index, (syn, _)) in args.iter().enumerate() {
        res.push_str(
            escape(&match syn {
                Syn::Command(cmd) => cmd.to_string(),
                Syn::Commands(cmds) => cmds
                    .iter()
                    .map(|cmd| cmd.to_string())
                    .collect::<Vec<_>>()
                    .join(";"),
                Syn::Text(text) => {
                    if let Some(Ok(Token::Text)) = Token::lexer(text).next() {
                        text.clone()
                    } else {
                        escape(text).to_string()
                    }
                }
                Syn::Number(num) => num.clone(),
                Syn::Float(num) => num.clone(),
                Syn::JsonObjectLike(s) => s.clone(),
                Syn::JsonArrayLike(s) => s.clone(),
            })
            .deref(),
        );
        if index + 1 < args.len() {
            res.push(' ');
        }
    }

    res
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub ident: String,
    /// original unmodified text that lead to the above ident
    pub cmd_text: String,
    /// original unmodified text range
    pub cmd_range: Range<usize>,
    pub args: Vec<(Syn, Range<usize>)>,
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.cmd_text, format_args(&self.args))
    }
}

#[derive(Debug)]
pub enum CommandTypeRef<'a> {
    /// Fully parsed command
    Full(&'a Command),
    /// Partially parsed command, e.g. a syntax error or smth
    Partial(&'a CommandParseResult),
}
impl CommandTypeRef<'_> {
    /// # Panics
    ///
    /// Panics if this type has no partial command.
    pub fn unwrap_full_or_partial_cmd_ref(&self) -> &Command {
        match self {
            CommandTypeRef::Full(cmd) => cmd,
            CommandTypeRef::Partial(res) => res
                .ref_cmd_partial()
                .expect("has no partial parsed command"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum CommandType {
    /// Fully parsed command
    Full(Command),
    /// Partially parsed command, e.g. a syntax error or smth
    Partial(CommandParseResult),
}
impl CommandType {
    pub fn unwrap_ref_full(&self) -> &Command {
        let Self::Full(cmd) = self else {
            panic!("not a fully parsed command")
        };
        cmd
    }
    pub fn unwrap_ref_partial(&self) -> &CommandParseResult {
        let Self::Partial(cmd) = self else {
            panic!("not a partially parsed command")
        };
        cmd
    }
    pub fn as_ref(&self) -> CommandTypeRef {
        match self {
            CommandType::Full(cmd) => CommandTypeRef::Full(cmd),
            CommandType::Partial(cmd) => CommandTypeRef::Partial(cmd),
        }
    }
}
pub type Commands = Vec<Command>;
pub type CommandsTyped = Vec<CommandType>;

#[derive(Debug, Hash, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Syn {
    Command(Box<Command>),
    Commands(Commands),
    Text(String),
    Number(String),
    Float(String),
    JsonObjectLike(String),
    JsonArrayLike(String),
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct CommandArg {
    /// Defines the type of command to parse
    pub ty: CommandArgType,
    /// A type for the higher level implementation that uses this
    /// parser to identify the argument as special value.
    /// (e.g. the player id of a player is numeric but should be parsed
    /// as an id instead.)
    pub user_ty: Option<NetworkString<1024>>,
}

pub struct TokenStackEntry {
    pub tokens: VecDeque<(Token, String, Range<usize>)>,
    pub raw_str: String,
    pub offset_in_parent: usize,
}

type TokenStackAndRawCmd = (VecDeque<(Token, String, Range<usize>)>, String);

pub struct TokenStack {
    pub tokens: Vec<TokenStackEntry>,
}

impl TokenStack {
    pub fn peek(&self) -> Option<(&Token, &String, Range<usize>)> {
        self.tokens.last().and_then(|tokens| {
            tokens.tokens.front().map(|(token, text, range)| {
                (
                    token,
                    text,
                    range.start + tokens.offset_in_parent..range.end + tokens.offset_in_parent,
                )
            })
        })
    }
    pub fn next_token(&mut self) -> Option<(Token, String, Range<usize>)> {
        if let Some(tokens) = self.tokens.last_mut() {
            let res = tokens.tokens.pop_front();
            let offset_in_parent = tokens.offset_in_parent;
            res.map(|(token, text, range)| {
                (
                    token,
                    text,
                    range.start + offset_in_parent..range.end + offset_in_parent,
                )
            })
        } else {
            None
        }
    }
    pub fn can_pop(&mut self) {
        if let Some(tokens) = self.tokens.last_mut() {
            if tokens.tokens.is_empty() {
                self.tokens.pop();
            }
        }
    }
    pub fn token_cur_stack_left_count(&self) -> usize {
        self.tokens
            .last()
            .map(|tokens| tokens.tokens.len())
            .unwrap_or_default()
    }
    pub fn cur_stack_raw(&self) -> &str {
        self.tokens
            .last()
            .map(|tokens| tokens.raw_str.as_str())
            .unwrap_or("")
    }
    pub fn take_cur_stack(&mut self) -> Option<TokenStackAndRawCmd> {
        self.tokens
            .last_mut()
            .map(|stack| (std::mem::take(&mut stack.tokens), stack.raw_str.clone()))
    }
    pub fn cur_stack(&mut self) -> Option<&TokenStackEntry> {
        self.tokens.last()
    }
    /// If there are no tokens left, the this can be used to get a range
    /// that is after the current command range + 1..range + 2
    /// (e.g. `my_command ~`, where a space is imaginary added and then a `~` as error range)
    pub fn cur_stack_end_range_plus_one(&mut self) -> Option<Range<usize>> {
        self.cur_stack().map(|stack| {
            stack.offset_in_parent + stack.raw_str.len() + 1
                ..stack.offset_in_parent + stack.raw_str.len() + 2
        })
    }
}

type ParseIdentResult =
    anyhow::Result<(String, Range<usize>), Option<(anyhow::Error, Range<usize>)>>;
fn parse_command_ident<const S: usize>(
    tokens: &mut TokenStack,
    commands: &HashMap<NetworkString<S>, Vec<CommandArg>>,
) -> ParseIdentResult {
    if let Some((token, text, range)) = tokens.peek() {
        let res = if let Token::Quoted = token {
            let text = unescape(text).map_err(|err| Some((err.into(), range.clone())))?;

            Ok(text)
        } else if let Token::Text = token {
            let text = text.clone();

            Ok(text)
        } else {
            Err(anyhow!(
                "Expected a text or literal, but found a {:?}",
                token
            ))
        };

        res.and_then(|text| {
            if commands.contains_key(&text) {
                tokens.next_token();
                Ok((text, range.clone()))
            } else {
                Err(anyhow!("Found text was not a command ident"))
            }
        })
        .map_err(|err| Some((err, range)))
    } else {
        Err(None)
    }
}

fn parse_text_token(
    tokens: &mut TokenStack,
    allow_text: &impl Fn(&str) -> anyhow::Result<()>,
) -> anyhow::Result<(String, Range<usize>)> {
    if let Some((token, text, range)) = tokens.peek() {
        if let Token::Text = token {
            allow_text(text)?;
            let text = text.clone();
            tokens.next_token();

            Ok((text, range))
        } else {
            Err(anyhow!("Expected a text, but found a {:?}", token))
        }
    } else {
        Err(anyhow!("Expected a text, but found end of string."))
    }
}

fn parse_json_like_obj(tokens: &mut TokenStack) -> anyhow::Result<(String, Range<usize>)> {
    if let Some((token, text, range)) = tokens.peek() {
        if let Token::Text = token {
            serde_json::from_str::<serde_json::value::Map<String, _>>(text)
                .map_err(|err| anyhow!(err))?;
            let text = text.clone();
            tokens.next_token();

            Ok((text, range))
        } else if let Token::Quoted = token {
            let text = unescape(text)?;
            serde_json::from_str::<serde_json::value::Map<String, _>>(&text)
                .map_err(|err| anyhow!(err))?;
            let text = text.clone();
            tokens.next_token();

            Ok((text, range))
        } else {
            Err(anyhow!("Expected a text, but found a {:?}", token))
        }
    } else {
        Err(anyhow!("Expected a text, but found end of string."))
    }
}

fn parse_json_like_arr(tokens: &mut TokenStack) -> anyhow::Result<(String, Range<usize>)> {
    if let Some((token, text, range)) = tokens.peek() {
        if let Token::Text = token {
            serde_json::from_str::<Vec<serde_json::Value>>(text).map_err(|err| anyhow!(err))?;
            let text = text.clone();
            tokens.next_token();

            Ok((text, range))
        } else if let Token::Quoted = token {
            let text = unescape(text)?;
            serde_json::from_str::<Vec<serde_json::Value>>(&text).map_err(|err| anyhow!(err))?;
            let text = text.clone();
            tokens.next_token();

            Ok((text, range))
        } else {
            Err(anyhow!("Expected a text, but found a {:?}", token))
        }
    } else {
        Err(anyhow!("Expected a text, but found end of string."))
    }
}

fn parse_text(
    tokens: &mut TokenStack,
    is_last_arg: bool,
    allow_text: &impl Fn(&str) -> anyhow::Result<()>,
) -> anyhow::Result<(String, Range<usize>)> {
    if is_last_arg {
        parse_raw_non_empty(tokens, allow_text)
            .or_else(|_| parse_text_token(tokens, allow_text))
            .or_else(|_| parse_literal(tokens, allow_text))
            .or_else(|_| parse_raw(tokens, allow_text))
    } else {
        parse_text_token(tokens, allow_text).or_else(|_| parse_literal(tokens, allow_text))
    }
}

fn parse_literal(
    tokens: &mut TokenStack,
    allow_text: &impl Fn(&str) -> anyhow::Result<()>,
) -> anyhow::Result<(String, Range<usize>)> {
    if let Some((token, text, range)) = tokens.peek() {
        if let Token::Quoted = token {
            let text = unescape(text)?;
            allow_text(&text)?;
            tokens.next_token();

            Ok((text, range))
        } else {
            Err(anyhow!("Expected a literal, but found a {:?}", token))
        }
    } else {
        Err(anyhow!("Expected a literal, but found end of string."))
    }
}

fn parse_number(tokens: &mut TokenStack) -> anyhow::Result<(String, Range<usize>)> {
    if let Some((token, text, range)) = tokens.peek() {
        if let Token::Text = token {
            anyhow::ensure!(
                text.parse::<i64>().is_ok() || text.parse::<u64>().is_ok(),
                "Expected a number, found {text}"
            );
            let text = text.clone();
            tokens.next_token();
            Ok((text, range))
        } else if let Token::Quoted = token {
            let text = unescape(text)?;
            anyhow::ensure!(
                text.parse::<i64>().is_ok() || text.parse::<u64>().is_ok(),
                "Expected a number, found {text}"
            );
            tokens.next_token();

            Ok((text, range))
        } else {
            Err(anyhow!("Expected a number, but found a {:?}", token))
        }
    } else {
        Err(anyhow!("Expected a number, but found end of string."))
    }
}

fn parse_float(tokens: &mut TokenStack) -> anyhow::Result<(String, Range<usize>)> {
    if let Some((token, text, range)) = tokens.peek() {
        if let Token::Text = token {
            anyhow::ensure!(
                text.parse::<f64>().is_ok() || text.parse::<u64>().is_ok(),
                "Expected a float, found {text}"
            );
            let text = text.clone();
            tokens.next_token();
            Ok((text, range))
        } else if let Token::Quoted = token {
            let text = unescape(text)?;
            anyhow::ensure!(
                text.parse::<f64>().is_ok() || text.parse::<u64>().is_ok(),
                "Expected a float, found {text}"
            );
            tokens.next_token();

            Ok((text, range))
        } else {
            Err(anyhow!("Expected a float, but found a {:?}", token))
        }
    } else {
        Err(anyhow!("Expected a float, but found end of string."))
    }
}

fn parse_raw(
    tokens: &mut TokenStack,
    allow_text: &impl Fn(&str) -> anyhow::Result<()>,
) -> anyhow::Result<(String, Range<usize>)> {
    tokens
        .take_cur_stack()
        .and_then(|(mut tokens, raw_str)| {
            tokens.pop_front().and_then(|(_, _, range)| {
                raw_str.get(range.start..).map(|str| {
                    (
                        ToString::to_string(str),
                        range.start..range.start + raw_str.len(),
                    )
                })
            })
        })
        .ok_or_else(|| anyhow!("Expected a token stack, but there was none."))
        .and_then(|res| allow_text(&res.0).map(|_| res))
}

fn parse_raw_non_empty(
    tokens: &mut TokenStack,
    allow_text: &impl Fn(&str) -> anyhow::Result<()>,
) -> anyhow::Result<(String, Range<usize>)> {
    if tokens.token_cur_stack_left_count() > 1 {
        parse_raw(tokens, allow_text)
    } else {
        Err(anyhow!("Expected a token stack, but there was none."))
    }
}

/// The result of parsing a command.
#[derive(Error, Debug, Serialize, Deserialize)]
pub enum CommandParseResult {
    #[error("Expected a command name, found: {err}")]
    InvalidCommandIdent { range: Range<usize>, err: String },
    #[error("{err}")]
    InvalidArg {
        arg_index: usize,
        partial_cmd: Command,
        err: String,
        range: Range<usize>,
    },
    #[error("Expected a command as argument, but the command is invalid or incomplete: {err}")]
    InvalidCommandArg {
        partial_cmd: Command,
        range: Range<usize>,
        err: Box<CommandParseResult>,
    },
    #[error(
        "Expected commands as argument, but at least one command is invalid or incomplete: {err}"
    )]
    InvalidCommandsArg {
        partial_cmd: Command,
        range: Range<usize>,
        /// Arguments of type command, that were already successfully parsed.
        full_arg_cmds: Vec<Command>,
        err: Box<CommandParseResult>,
    },
    #[error("Failed to tokenize quoted string")]
    InvalidQuoteParsing(Range<usize>),
    #[error("{err}")]
    Other { range: Range<usize>, err: String },
}

impl CommandParseResult {
    pub fn range(&self) -> &Range<usize> {
        match self {
            CommandParseResult::InvalidCommandIdent { range, .. }
            | CommandParseResult::InvalidArg { range, .. }
            | CommandParseResult::InvalidCommandArg { range, .. }
            | CommandParseResult::InvalidCommandsArg { range, .. }
            | CommandParseResult::InvalidQuoteParsing(range)
            | CommandParseResult::Other { range, .. } => range,
        }
    }
    pub fn ref_cmd_partial(&self) -> Option<&Command> {
        let Self::InvalidArg { partial_cmd, .. } = self else {
            return None;
        };
        Some(partial_cmd)
    }
    pub fn unwrap_ref_cmd_partial(&self) -> &Command {
        self.ref_cmd_partial()
            .expect("not a partial parsed command")
    }
}

#[allow(clippy::result_large_err)]
fn parse_command<const S: usize>(
    tokens: &mut TokenStack,
    commands: &HashMap<NetworkString<S>, Vec<CommandArg>>,
    double_arg_mode: bool,
    index_key_regex: &regex::Regex,
) -> anyhow::Result<Command, CommandParseResult> {
    // if literal, then unescape the literal and push to stack
    while let Some((Token::Quoted, text, range)) = tokens.peek() {
        let text = text.clone();
        tokens.next_token();
        let text = unescape(&text).map_err(|err| CommandParseResult::Other {
            err: err.to_string(),
            range: range.clone(),
        })?;
        let stack_tokens = tokenize(&text)
            .map_err(|(_, (_, range))| CommandParseResult::InvalidQuoteParsing(range))?;

        let offset_in_parent = range.start + 1;
        let mut token_entries = generate_token_stack_entries(stack_tokens, &text, offset_in_parent);
        tokens.tokens.append(&mut token_entries);
    }
    if let Some((Token::Text, (Some(cmd_args), text, original_text, mut range))) =
        tokens.peek().map(|(token, text, range)| {
            // parse ident
            let ident = text;
            let reg = index_key_regex;
            let replace = |caps: &regex::Captures| -> String {
                if caps
                    .get(1)
                    .map(|cap| cap.as_str().starts_with(|c: char| c.is_ascii_digit()))
                    .unwrap_or_default()
                {
                    "$INDEX$".into()
                } else {
                    "$KEY$".into()
                }
            };
            let ident = reg.replace_all(ident, replace).to_string();
            (
                token,
                (commands.get(&ident), ident, text.clone(), range.clone()),
            )
        })
    {
        let mut cmd = Command {
            ident: text.clone(),
            cmd_text: original_text,
            cmd_range: range.clone(),
            args: Default::default(),
        };
        tokens.next_token();

        let args = cmd_args.iter().chain(cmd_args.iter());

        let args_logic_len = if double_arg_mode {
            cmd_args.len() * 2
        } else {
            cmd_args.len()
        };
        let mut args_res = Ok(());
        for (arg_index, arg) in args.take(args_logic_len).enumerate() {
            let is_last = arg_index == args_logic_len - 1;
            // find the required arg in tokens
            // respect the allowed syn
            enum SynOrErr {
                Syn((Syn, Range<usize>)),
                ParseRes(CommandParseResult),
                /// Unrecoverable here means that parsing further commands could potentially have unexpected results
                ParseResUnrecoverable(CommandParseResult),
                /// Like [`SynOrErr::ParseResUnrecoverable`], but has finished arguments in it.
                ParseResUnrecoverablePartial {
                    res: CommandParseResult,
                    finished_cmds: Vec<Command>,
                },
            }
            let mut syn = || match &arg.ty {
                CommandArgType::Command => Some(
                    parse_command(tokens, commands, false, index_key_regex)
                        .map(|s| {
                            let range = s.cmd_range.start
                                ..s.args
                                    .last()
                                    .map(|(_, arg_range)| arg_range.end)
                                    .unwrap_or(s.cmd_range.end);
                            SynOrErr::Syn((Syn::Command(Box::new(s)), range))
                        })
                        .unwrap_or_else(SynOrErr::ParseResUnrecoverable),
                ),
                CommandArgType::CommandIdent => Some(
                    parse_command_ident(tokens, commands)
                        .map(|(s, range)| SynOrErr::Syn((Syn::Text(s), range)))
                        .unwrap_or_else(|range_err| {
                            let (err, range) = range_err.unwrap_or_else(|| {
                                (anyhow!("No text was given"), range.end + 1..range.end + 2)
                            });
                            SynOrErr::ParseRes(CommandParseResult::InvalidCommandIdent {
                                range,
                                err: err.to_string(),
                            })
                        }),
                ),
                CommandArgType::Commands => {
                    let mut cmds: Commands = Default::default();
                    while tokens.peek().is_some() {
                        if let Ok(cmd) =
                            match parse_command(tokens, commands, false, index_key_regex) {
                                Ok(cmd) => anyhow::Ok(cmd),
                                Err(err) => {
                                    return Some(SynOrErr::ParseResUnrecoverablePartial {
                                        res: err,
                                        finished_cmds: cmds,
                                    });
                                }
                            }
                        {
                            cmds.push(cmd)
                        }
                    }
                    if cmds.is_empty() {
                        return Some(SynOrErr::ParseRes(
                            CommandParseResult::InvalidCommandIdent {
                                range: tokens
                                    .cur_stack_end_range_plus_one()
                                    .unwrap_or_else(|| range.end + 1..range.end + 2),
                                err: "No text was given".to_string(),
                            },
                        ));
                    }
                    let range = cmds
                        .first()
                        .and_then(|first| cmds.last().map(|last| (first, last)))
                        .and_then(|(first, last)| {
                            last.args
                                .last()
                                .map(|(_, arg_range)| first.cmd_range.start..arg_range.end)
                        })
                        .unwrap_or_default();
                    Some(SynOrErr::Syn((Syn::Commands(cmds), range)))
                }
                CommandArgType::CommandDoubleArg => Some(
                    parse_command(tokens, commands, true, index_key_regex)
                        .map(|s| {
                            let cmd_range_end = s.cmd_range.end;
                            let range = s.cmd_range.start
                                ..s.args
                                    .last()
                                    .map(|(_, arg_range)| arg_range.end)
                                    .unwrap_or(cmd_range_end);
                            SynOrErr::Syn((Syn::Command(Box::new(s)), range))
                        })
                        .unwrap_or_else(SynOrErr::ParseResUnrecoverable),
                ),
                CommandArgType::Number => parse_number(tokens)
                    .ok()
                    .map(|(s, range)| SynOrErr::Syn((Syn::Number(s), range))),
                CommandArgType::Float => parse_float(tokens)
                    .ok()
                    .map(|(s, range)| SynOrErr::Syn((Syn::Float(s), range))),
                CommandArgType::Text => parse_text(tokens, is_last, &|_| Ok(()))
                    .ok()
                    .map(|(s, range)| SynOrErr::Syn((Syn::Text(s), range))),
                CommandArgType::JsonObjectLike => parse_json_like_obj(tokens)
                    .ok()
                    .map(|(s, range)| SynOrErr::Syn((Syn::JsonObjectLike(s), range))),
                CommandArgType::JsonArrayLike => parse_json_like_arr(tokens)
                    .ok()
                    .map(|(s, range)| SynOrErr::Syn((Syn::JsonArrayLike(s), range))),
                CommandArgType::TextFrom(texts) => {
                    // like text but with limited allowed texts
                    parse_text(tokens, is_last, &|str| {
                        texts
                            .iter()
                            .any(|s| s.as_str() == str)
                            .then_some(())
                            .ok_or_else(|| {
                                anyhow!(
                                    "text must be either of [{}]",
                                    texts
                                        .iter()
                                        .map(|s| s.as_str())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                )
                            })
                    })
                    .ok()
                    .map(|(s, range)| SynOrErr::Syn((Syn::Text(s), range)))
                }
                CommandArgType::TextArrayFrom { from, separator } => {
                    // like text but with limited allowed texts
                    parse_text(tokens, is_last, &|str| {
                        for slice in str.split(*separator) {
                            from.iter()
                                .any(|s| s.as_str() == slice)
                                .then_some(())
                                .ok_or_else(|| {
                                    anyhow!(
                                        "text must be either of [{}]",
                                        from.iter()
                                            .map(|s| s.as_str())
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    )
                                })?
                        }
                        Ok(())
                    })
                    .ok()
                    .map(|(s, range)| SynOrErr::Syn((Syn::Text(s), range)))
                }
            };
            let syn = syn();
            match syn {
                Some(syn) => match syn {
                    SynOrErr::Syn((syn, syn_range)) => {
                        range = syn_range.clone();
                        cmd.args.push((syn, syn_range));
                    }
                    SynOrErr::ParseRes(res) => {
                        if args_res.is_ok() {
                            args_res = Err(CommandParseResult::InvalidCommandArg {
                                partial_cmd: cmd.clone(),
                                range: res.range().clone(),
                                err: Box::new(res),
                            });
                        }
                    }
                    SynOrErr::ParseResUnrecoverable(res) => {
                        return Err(CommandParseResult::InvalidCommandArg {
                            partial_cmd: cmd.clone(),
                            range: res.range().clone(),
                            err: Box::new(res),
                        });
                    }
                    SynOrErr::ParseResUnrecoverablePartial { res, finished_cmds } => {
                        return Err(CommandParseResult::InvalidCommandsArg {
                            partial_cmd: cmd.clone(),
                            range: res.range().clone(),
                            err: Box::new(res),
                            full_arg_cmds: finished_cmds,
                        });
                    }
                },
                None => {
                    let token = tokens.next_token();
                    tokens.can_pop();
                    let (err, range) = token
                        .map(|(token, token_text, range)| {
                            (
                                anyhow!(
                                    "Expected {}, but found {} (\"{}\") instead",
                                    if let Some(user_ty) = arg.user_ty.clone() {
                                        user_ty.to_string()
                                    } else {
                                        arg.ty.human_readable()
                                    },
                                    token.human_readable(),
                                    token_text,
                                ),
                                range,
                            )
                        })
                        .unwrap_or((
                            anyhow!(
                                "{} is required as command argument, but not found.",
                                if let Some(user_ty) = arg.user_ty.clone() {
                                    user_ty.to_string()
                                } else {
                                    arg.ty.human_readable()
                                }
                            ),
                            range.end + 1..range.end + 2,
                        ));
                    if args_res.is_ok() {
                        args_res = Err(CommandParseResult::InvalidArg {
                            arg_index: if double_arg_mode {
                                arg_index / 2
                            } else {
                                arg_index
                            },
                            err: err.to_string(),
                            range,
                            partial_cmd: cmd.clone(),
                        });
                    }
                }
            }
        }

        args_res?;

        tokens.can_pop();
        Ok(cmd)
    } else {
        let peek_token = tokens.next_token().map(|(_, text, range)| (text, range));
        let (err, range) = if let Some((text, range)) = peek_token {
            (text, range)
        } else {
            (
                "No text was given".to_string(),
                tokens
                    .cur_stack_end_range_plus_one()
                    .unwrap_or_else(|| (0..0)),
            )
        };
        let res = Err(CommandParseResult::InvalidCommandIdent { range, err });

        tokens.can_pop();
        res
    }
}

fn generate_token_stack_entries(
    tokens: Vec<(Token, String, Range<usize>)>,
    full_text: &str,
    text_range_start: usize,
) -> Vec<TokenStackEntry> {
    let mut res: Vec<TokenStackEntry> = Default::default();

    let splits = tokens.split(|(token, _, _)| matches!(token, Token::Semicolon));

    for tokens in splits.rev() {
        if !tokens.is_empty() {
            let start_range = tokens.first().unwrap().2.start;
            let end_range = tokens.last().unwrap().2.end;

            res.push(TokenStackEntry {
                tokens: tokens
                    .iter()
                    .map(|(token, text, range)| {
                        (
                            *token,
                            text.clone(),
                            range.start - start_range..range.end - start_range,
                        )
                    })
                    .collect(),
                raw_str: full_text[start_range..end_range].to_string(),
                offset_in_parent: text_range_start + start_range,
            });
        }
    }

    res
}

/// This cache can be used if performance matters.
///
/// Creating this cache is free.
#[derive(Debug, Default)]
pub struct ParserCache {
    reg: RefCell<Option<regex::Regex>>,
}

pub fn parse<const S: usize>(
    raw: &str,
    commands: &HashMap<NetworkString<S>, Vec<CommandArg>>,
    state: &ParserCache,
) -> CommandsTyped {
    let mut reg = state.reg.borrow_mut();
    let reg = reg.get_or_insert_with(|| regex::Regex::new(r"\[([^\]]+)\]").unwrap());

    let (tokens, token_err) = tokenize(raw)
        .map(|tokens| (tokens, None))
        .unwrap_or_else(|(tokens, (err_str, err_range))| (tokens, Some((err_str, err_range))));

    let mut res: CommandsTyped = Default::default();

    let tokens = generate_token_stack_entries(tokens, raw, 0);
    let mut tokens = TokenStack { tokens };
    while tokens.peek().is_some() {
        match parse_command(&mut tokens, commands, false, reg) {
            Ok(cmd) => {
                res.push(CommandType::Full(cmd));
            }
            Err(cmd_err) => {
                res.push(CommandType::Partial(cmd_err));
            }
        }
    }

    if let (Some((err_token_text, err_range)), last_mut) = (token_err, res.last_mut()) {
        let err_token = || {
            super::tokenizer::token_err(&err_token_text).unwrap_or(anyhow!(err_token_text.clone()))
        };
        let cmd_partial = CommandType::Partial(CommandParseResult::Other {
            range: err_range.clone(),
            err: err_token().to_string(),
        });
        if let Some(CommandType::Partial(cmd)) = last_mut {
            match cmd {
                CommandParseResult::InvalidArg { err, range, .. } => {
                    *range = err_range;
                    *err = err_token().to_string();
                }
                CommandParseResult::InvalidCommandIdent { .. }
                | CommandParseResult::InvalidCommandArg { .. }
                | CommandParseResult::InvalidCommandsArg { .. }
                | CommandParseResult::InvalidQuoteParsing(_)
                | CommandParseResult::Other { .. } => {
                    *last_mut.unwrap() = cmd_partial;
                }
            }
        } else {
            res.push(cmd_partial);
        }
    }

    res
}

#[cfg(test)]
mod test {
    use crate::parser::{parse, CommandParseResult, CommandType, ParserCache, Syn};

    use super::{CommandArg, CommandArgType};

    #[test]
    fn console_tests() {
        let cache = ParserCache::default();
        let lex = parse::<65536>(
            "cl.map \"name with\\\" spaces\"",
            &vec![(
                "cl.map".try_into().unwrap(),
                vec![CommandArg {
                    ty: CommandArgType::Text,
                    user_ty: None,
                }],
            )]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        assert!(lex[0].unwrap_ref_full().args[0].0 == Syn::Text("name with\" spaces".to_string()));

        let lex = parse::<65536>(
            "bind b cl.map \"name with\\\" spaces\"",
            &vec![
                (
                    "bind".try_into().unwrap(),
                    vec![
                        CommandArg {
                            ty: CommandArgType::Text,
                            user_ty: None,
                        },
                        CommandArg {
                            ty: CommandArgType::Command,
                            user_ty: None,
                        },
                    ],
                ),
                (
                    "cl.map".try_into().unwrap(),
                    vec![CommandArg {
                        ty: CommandArgType::Text,
                        user_ty: None,
                    }],
                ),
            ]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        assert!(lex[0].unwrap_ref_full().args[0].0 == Syn::Text("b".to_string()));
        assert!(matches!(
            lex[0].unwrap_ref_full().args[1].0,
            Syn::Command(_)
        ));

        let lex = parse::<65536>(
            "bind b cl.map \"name with\\\" spaces\"",
            &vec![
                (
                    "bind".try_into().unwrap(),
                    vec![
                        CommandArg {
                            ty: CommandArgType::Text,
                            user_ty: None,
                        },
                        CommandArg {
                            ty: CommandArgType::Command,
                            user_ty: None,
                        },
                    ],
                ),
                (
                    "cl.map".try_into().unwrap(),
                    vec![CommandArg {
                        ty: CommandArgType::Text,
                        user_ty: None,
                    }],
                ),
            ]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        assert!(lex[0].unwrap_ref_full().args[0].0 == Syn::Text("b".to_string()));
        assert!(matches!(
            lex[0].unwrap_ref_full().args[1].0,
            Syn::Command(_)
        ));

        let lex = parse::<65536>(
            "bind b \"cl.map \\\"name with\\\\\\\" spaces\\\"; cl.rate 50;\"",
            &vec![
                (
                    "bind".try_into().unwrap(),
                    vec![
                        CommandArg {
                            ty: CommandArgType::Text,
                            user_ty: None,
                        },
                        CommandArg {
                            ty: CommandArgType::Commands,
                            user_ty: None,
                        },
                    ],
                ),
                (
                    "cl.map".try_into().unwrap(),
                    vec![CommandArg {
                        ty: CommandArgType::Text,
                        user_ty: None,
                    }],
                ),
                (
                    "cl.rate".try_into().unwrap(),
                    vec![CommandArg {
                        ty: CommandArgType::Number,
                        user_ty: None,
                    }],
                ),
            ]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        assert!(lex[0].unwrap_ref_full().args[0].0 == Syn::Text("b".to_string()));
        assert!(matches!(
            lex[0].unwrap_ref_full().args[1].0,
            Syn::Commands(_)
        ));

        let lex = parse::<65536>(
            "player.name \"name with\\\" spaces\" abc",
            &vec![(
                "player.name".try_into().unwrap(),
                vec![CommandArg {
                    ty: CommandArgType::Text,
                    user_ty: None,
                }],
            )]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        assert!(
            lex[0].unwrap_ref_full().args[0].0
                == Syn::Text("\"name with\\\" spaces\" abc".to_string())
        );

        let lex = parse::<65536>(
            "push players",
            &vec![
                (
                    "push".try_into().unwrap(),
                    vec![CommandArg {
                        ty: CommandArgType::CommandIdent,
                        user_ty: None,
                    }],
                ),
                ("players".try_into().unwrap(), vec![]),
            ]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        assert!(lex[0].unwrap_ref_full().args[0].0 == Syn::Text("players".to_string()));

        let lex = parse::<65536>(
            "toggle cl.map \"map1 \" \" map2\"",
            &vec![
                (
                    "toggle".try_into().unwrap(),
                    vec![CommandArg {
                        ty: CommandArgType::CommandDoubleArg,
                        user_ty: None,
                    }],
                ),
                (
                    "cl.map".try_into().unwrap(),
                    vec![CommandArg {
                        ty: CommandArgType::Text,
                        user_ty: None,
                    }],
                ),
            ]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        let cmds = lex;
        assert!(
            cmds[0].unwrap_ref_full().ident == "toggle" && {
                if let Syn::Command(cmd) = &cmds[0].unwrap_ref_full().args[0].0 {
                    cmd.args.len() == 2
                } else {
                    false
                }
            }
        );

        let lex = parse::<65536>(
            "cl.refresh_rate \"\" player \"\"; player \"\"",
            &vec![
                (
                    "cl.refresh_rate".try_into().unwrap(),
                    vec![CommandArg {
                        ty: CommandArgType::Number,
                        user_ty: None,
                    }],
                ),
                (
                    "player".try_into().unwrap(),
                    vec![CommandArg {
                        ty: CommandArgType::Text,
                        user_ty: None,
                    }],
                ),
            ]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        let cmds = lex;
        assert!(cmds.len() == 3);

        let lex = parse::<65536>(
            "bind space +jump",
            &vec![
                (
                    "bind".try_into().unwrap(),
                    vec![
                        CommandArg {
                            ty: CommandArgType::TextFrom(vec!["space".try_into().unwrap()]),
                            user_ty: None,
                        },
                        CommandArg {
                            ty: CommandArgType::Commands,
                            user_ty: None,
                        },
                    ],
                ),
                ("+jump".try_into().unwrap(), vec![]),
            ]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        let cmds = lex;
        assert!(cmds.len() == 1 && matches!(cmds[0], CommandType::Full(_)));

        let lex = parse::<65536>(
            "bind space+a +jump",
            &vec![
                (
                    "bind".try_into().unwrap(),
                    vec![
                        CommandArg {
                            ty: CommandArgType::TextArrayFrom {
                                from: vec!["space".try_into().unwrap(), "a".try_into().unwrap()],
                                separator: '+',
                            },
                            user_ty: None,
                        },
                        CommandArg {
                            ty: CommandArgType::Commands,
                            user_ty: None,
                        },
                    ],
                ),
                ("+jump".try_into().unwrap(), vec![]),
            ]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        let cmds = lex;
        assert!(cmds.len() == 1 && matches!(cmds[0], CommandType::Full(_)));
    }

    #[test]
    fn console_test_index() {
        let cache = ParserCache::default();
        let lex = parse::<65536>(
            "players[0] something",
            &vec![(
                "players$INDEX$".try_into().unwrap(),
                vec![CommandArg {
                    ty: CommandArgType::Text,
                    user_ty: None,
                }],
            )]
            .into_iter()
            .collect(),
            &cache,
        );
        dbg!(&lex);
        assert!(lex[0].unwrap_ref_full().args[0].0 == Syn::Text("something".to_string()));

        let lex = parse::<65536>(
            "players[0]",
            &vec![("players$INDEX$".try_into().unwrap(), vec![])]
                .into_iter()
                .collect(),
            &cache,
        );
        dbg!(&lex);
        assert!(matches!(lex[0], CommandType::Full(_)));

        let lex = parse::<65536>(
            "players[0][name] something",
            &vec![(
                "players$INDEX$$KEY$".try_into().unwrap(),
                vec![CommandArg {
                    ty: CommandArgType::Text,
                    user_ty: None,
                }],
            )]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        assert!(lex[0].unwrap_ref_full().args[0].0 == Syn::Text("something".to_string()));
    }

    #[test]
    fn err_console_tests() {
        let cache = ParserCache::default();
        let lex = parse::<65536>(
            "cl.map \"name with\\\" ",
            &vec![(
                "cl.map".try_into().unwrap(),
                vec![CommandArg {
                    ty: CommandArgType::Text,
                    user_ty: None,
                }],
            )]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        let cmds = lex;
        assert!(matches!(
            cmds[0].unwrap_ref_partial(),
            CommandParseResult::InvalidArg { .. }
        ));

        let lex = parse::<65536>(
            "toggle cl.map \"map1 \" map2\"",
            &vec![
                (
                    "toggle".try_into().unwrap(),
                    vec![CommandArg {
                        ty: CommandArgType::CommandDoubleArg,
                        user_ty: None,
                    }],
                ),
                (
                    "cl.map".try_into().unwrap(),
                    vec![CommandArg {
                        ty: CommandArgType::Text,
                        user_ty: None,
                    }],
                ),
            ]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        let cmds = lex;
        assert!(
            cmds.len() == 1
                && matches!(
                    cmds[0].unwrap_ref_partial(),
                    CommandParseResult::InvalidArg { .. }
                )
        );

        let lex = parse::<65536>(
            "bind space +jump",
            &vec![
                (
                    "bind".try_into().unwrap(),
                    vec![
                        CommandArg {
                            ty: CommandArgType::TextFrom(vec!["a".try_into().unwrap()]),
                            user_ty: None,
                        },
                        CommandArg {
                            ty: CommandArgType::Commands,
                            user_ty: None,
                        },
                    ],
                ),
                ("+jump".try_into().unwrap(), vec![]),
            ]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        let cmds = lex;
        assert!(
            cmds.len() == 1
                && matches!(
                    cmds[0].unwrap_ref_partial(),
                    CommandParseResult::InvalidArg { .. }
                )
        );

        let lex = parse::<65536>(
            "bind space+a +jump",
            &vec![
                (
                    "bind".try_into().unwrap(),
                    vec![
                        CommandArg {
                            ty: CommandArgType::TextArrayFrom {
                                from: vec!["b".try_into().unwrap(), "a".try_into().unwrap()],
                                separator: '+',
                            },
                            user_ty: None,
                        },
                        CommandArg {
                            ty: CommandArgType::Commands,
                            user_ty: None,
                        },
                    ],
                ),
                ("+jump".try_into().unwrap(), vec![]),
            ]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        let cmds = lex;
        assert!(
            cmds.len() == 1
                && matches!(
                    cmds[0].unwrap_ref_partial(),
                    CommandParseResult::InvalidArg { .. }
                )
        );

        let lex = parse::<65536>(
            "cl.refresh_rate \"\" player \"\"; player",
            &vec![
                (
                    "cl.refresh_rate".try_into().unwrap(),
                    vec![CommandArg {
                        ty: CommandArgType::Number,
                        user_ty: None,
                    }],
                ),
                (
                    "player".try_into().unwrap(),
                    vec![CommandArg {
                        ty: CommandArgType::Text,
                        user_ty: None,
                    }],
                ),
            ]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        let cmds = lex;
        assert!(
            cmds.len() == 3
                && if let CommandType::Partial(CommandParseResult::InvalidArg { range, .. }) =
                    &cmds[2]
                {
                    range.end <= "cl.refresh_rate \"\" player \"\"; player".len()
                } else {
                    false
                }
        );

        let lex = parse::<65536>(
            "cl.refresh_rate;player",
            &vec![
                (
                    "cl.refresh_rate".try_into().unwrap(),
                    vec![CommandArg {
                        ty: CommandArgType::Number,
                        user_ty: None,
                    }],
                ),
                (
                    "player".try_into().unwrap(),
                    vec![CommandArg {
                        ty: CommandArgType::Text,
                        user_ty: None,
                    }],
                ),
            ]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        assert!(lex.len() == 2);

        let lex = parse::<65536>(
            "player;player",
            &vec![(
                "player".try_into().unwrap(),
                vec![CommandArg {
                    ty: CommandArgType::Text,
                    user_ty: None,
                }],
            )]
            .into_iter()
            .collect(),
            &cache,
        );

        dbg!(&lex);
        assert!(lex.len() == 2);
    }
}
