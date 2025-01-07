use std::ops::Range;

use client_types::console::{
    entries_to_parser, ConsoleEntry, ConsoleEntryCmd, ConsoleEntryVariable,
};
use command_parser::parser::{parse, Command, CommandParseResult, CommandType, CommandsTyped, Syn};
use egui::{
    text::{CCursor, LayoutJob},
    text_selection::CCursorRange,
    Color32, FontId, Id, Layout, RichText, TextFormat,
};

use ui_base::types::{UiRenderPipe, UiState};

use super::{
    user_data::UserData,
    utils::{find_matches, run_commands, MatchedType},
};

/// console input
pub fn render(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    has_text_selection: bool,
    cmds: &CommandsTyped,
) {
    let mouse_is_down = ui.input(|i| i.any_touches() || i.pointer.any_down());

    let msg_before_inp = pipe.user_data.msg.clone();
    let cursor_before_inp = *pipe.user_data.cursor;

    ui.style_mut().spacing.item_spacing.x = 0.0;
    ui.horizontal(|ui| {
        ui.add_space(5.0);
        ui.label(RichText::new(">").font(FontId::monospace(12.0)));
        ui.with_layout(
            Layout::left_to_right(egui::Align::Max).with_main_justify(true),
            |ui| {
                let inp_id = Id::new("console-input");

                let mut layouter = |ui: &egui::Ui, string: &str, _wrap_width: f32| {
                    let cmd = cmds.iter();
                    let mut layout_job = LayoutJob::default();
                    let mut last_range = 0;
                    let len = string.len();
                    fn get_range(
                        last_range: usize,
                        range: &Range<usize>,
                        len: usize,
                    ) -> Range<usize> {
                        let start_range = range.start.min(len).max(last_range);
                        start_range..range.end.min(len).max(last_range)
                    }
                    fn text_fmt_base() -> TextFormat {
                        TextFormat {
                            valign: egui::Align::Max,
                            font_id: FontId::monospace(12.0),
                            color: Color32::WHITE,
                            ..Default::default()
                        }
                    }
                    let colorize_semicolons = |layout_job: &mut LayoutJob, range: Range<usize>| {
                        let Some(s) = string.get(range) else {
                            return;
                        };
                        let splits = || s.split(";");
                        let split_count = splits().count();
                        if split_count > 1 {
                            for (index, split) in splits().enumerate() {
                                layout_job.append(split, 0.0, text_fmt_base());

                                if index + 1 != split_count {
                                    layout_job.append(";", 0.0, {
                                        let mut fmt = text_fmt_base();
                                        fmt.color = Color32::LIGHT_RED;
                                        fmt
                                    });
                                }
                            }
                        } else {
                            layout_job.append(s, 0.0, text_fmt_base());
                        }
                    };
                    for cmd in cmd {
                        match cmd {
                            CommandType::Full(cmd) => {
                                fn get_ranges(
                                    last_range: usize,
                                    cmd: &Command,
                                    len: usize,
                                ) -> Vec<Range<usize>> {
                                    let mut res = vec![get_range(last_range, &cmd.cmd_range, len)];
                                    for (arg, _) in &cmd.args {
                                        if let Syn::Command(cmd) = arg {
                                            res.append(&mut get_ranges(last_range, cmd, len));
                                        } else if let Syn::Commands(cmds) = arg {
                                            for cmd in cmds {
                                                res.append(&mut get_ranges(last_range, cmd, len));
                                            }
                                        }
                                    }
                                    res
                                }
                                let ranges = get_ranges(last_range, cmd, len);
                                for range in ranges {
                                    colorize_semicolons(&mut layout_job, last_range..range.start);
                                    layout_job.append(
                                        string.get(range.start..range.end).unwrap_or_default(),
                                        0.0,
                                        {
                                            let mut fmt = text_fmt_base();
                                            fmt.color = Color32::GOLD;
                                            fmt
                                        },
                                    );
                                    last_range = range.end;
                                }
                            }
                            CommandType::Partial(cmd) => {
                                type RangeColor = (Range<usize>, Color32);
                                fn get_range_color_err(
                                    last_range: usize,
                                    res: &CommandParseResult,
                                    len: usize,
                                ) -> (Vec<RangeColor>, Option<(Range<usize>, Color32)>)
                                {
                                    match res {
                                        CommandParseResult::InvalidArg {
                                            partial_cmd,
                                            range,
                                            ..
                                        } => (
                                            vec![(
                                                get_range(last_range, &partial_cmd.cmd_range, len),
                                                Color32::GOLD,
                                            )],
                                            Some((range.clone(), Color32::LIGHT_GRAY)),
                                        ),
                                        CommandParseResult::InvalidCommandArg {
                                            partial_cmd,
                                            range,
                                            err,
                                        } => {
                                            let (mut range_colors, err) =
                                                get_range_color_err(last_range, err, len);
                                            let mut range_colors_res = vec![(
                                                get_range(last_range, &partial_cmd.cmd_range, len),
                                                Color32::GOLD,
                                            )];
                                            range_colors_res.append(&mut range_colors);
                                            (
                                                range_colors_res,
                                                err.or(Some((range.clone(), Color32::LIGHT_GRAY))),
                                            )
                                        }
                                        CommandParseResult::InvalidCommandsArg {
                                            partial_cmd,
                                            range,
                                            err,
                                            full_arg_cmds,
                                        } => {
                                            let (mut range_colors, err) =
                                                get_range_color_err(last_range, err, len);
                                            let mut range_colors_res = vec![(
                                                get_range(last_range, &partial_cmd.cmd_range, len),
                                                Color32::GOLD,
                                            )];
                                            for cmd in full_arg_cmds {
                                                range_colors_res.push((
                                                    get_range(last_range, &cmd.cmd_range, len),
                                                    Color32::GOLD,
                                                ));
                                            }
                                            range_colors_res.append(&mut range_colors);
                                            (
                                                range_colors_res,
                                                err.or(Some((range.clone(), Color32::LIGHT_GRAY))),
                                            )
                                        }
                                        CommandParseResult::InvalidCommandIdent { .. }
                                        | CommandParseResult::InvalidQuoteParsing(_)
                                        | CommandParseResult::Other { .. } => (
                                            vec![(
                                                get_range(last_range, res.range(), len),
                                                Color32::LIGHT_GRAY,
                                            )],
                                            None,
                                        ),
                                    }
                                }
                                let (range_colors, err) = get_range_color_err(last_range, cmd, len);
                                for (range, color) in range_colors {
                                    colorize_semicolons(&mut layout_job, last_range..range.start);
                                    layout_job.append(
                                        string.get(range.start..range.end).unwrap_or_default(),
                                        0.0,
                                        {
                                            let mut fmt = text_fmt_base();
                                            fmt.color = color;
                                            fmt
                                        },
                                    );
                                    last_range = range.end;
                                }
                                if let Some((range, color)) = err {
                                    let range = get_range(last_range, &range, len);
                                    colorize_semicolons(&mut layout_job, last_range..range.start);
                                    layout_job.append(
                                        string.get(range.start..range.end).unwrap_or_default(),
                                        0.0,
                                        {
                                            let mut fmt = text_fmt_base();
                                            fmt.color = color;
                                            fmt
                                        },
                                    );
                                    last_range = range.end;
                                }
                            }
                        }
                    }
                    colorize_semicolons(&mut layout_job, last_range..string.len());
                    ui.fonts(|f| f.layout_job(layout_job))
                };
                let had_quote = pipe.user_data.msg.char_indices().any(|(index, c)| {
                    if c == '"' {
                        *pipe.user_data.cursor == index + 1
                    } else {
                        false
                    }
                });
                let mut label = egui::TextEdit::singleline(pipe.user_data.msg)
                    .font(FontId::monospace(12.0))
                    .id(inp_id)
                    .layouter(&mut layouter)
                    .frame(false)
                    .show(ui);
                *pipe.user_data.cursor = label
                    .state
                    .cursor
                    .char_range()
                    .map(|cursor| cursor.primary.index)
                    .unwrap_or_default();
                let has_quote = pipe.user_data.msg.char_indices().any(|(index, c)| {
                    if c == '"' {
                        *pipe.user_data.cursor == index + 1
                    } else {
                        false
                    }
                });
                if has_quote && !had_quote && label.response.changed() {
                    let byte_offset = pipe
                        .user_data
                        .msg
                        .char_indices()
                        .enumerate()
                        .find_map(|(index, (byte_offset, _))| {
                            (index == *pipe.user_data.cursor).then_some(byte_offset)
                        })
                        .unwrap_or(pipe.user_data.msg.len());
                    pipe.user_data.msg.insert(byte_offset, '"');
                }
                let (enter, tab, space, up, down, modifiers) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::Enter),
                        i.key_pressed(egui::Key::Tab),
                        i.key_pressed(egui::Key::Space),
                        i.key_pressed(egui::Key::ArrowUp),
                        i.key_pressed(egui::Key::ArrowDown),
                        i.modifiers,
                    )
                });

                if label.response.lost_focus() {
                    if enter && !pipe.user_data.msg.is_empty() {
                        // check if an entry was selected, execute that in that case
                        if let Some(index) = *pipe.user_data.select_index {
                            let (entries, list_entries, _) = find_matches(
                                cmds,
                                cursor_before_inp,
                                pipe.user_data.entries,
                                &msg_before_inp,
                                pipe.user_data.custom_matches,
                            );
                            let cur_entry = entries.iter().find(|(e, _)| e.index() == index);
                            if let Some((e, _)) = cur_entry {
                                match e {
                                    MatchedType::Entry(index) => {
                                        match &pipe.user_data.entries[*index] {
                                            ConsoleEntry::Var(ConsoleEntryVariable {
                                                full_name: name,
                                                ..
                                            })
                                            | ConsoleEntry::Cmd(ConsoleEntryCmd { name, .. }) => {
                                                *pipe.user_data.msg = name.clone();
                                            }
                                        }
                                    }
                                    MatchedType::ArgList(index)
                                    | MatchedType::CustomList { index, .. } => {
                                        *pipe.user_data.msg = list_entries.unwrap()[*index].clone();
                                    }
                                }
                            }
                            *pipe.user_data.select_index = None;
                        }

                        let cmds = parse(
                            &*pipe.user_data.msg,
                            &entries_to_parser(pipe.user_data.entries),
                            pipe.user_data.cache,
                        );
                        run_commands(
                            &cmds,
                            pipe.user_data.entries,
                            &mut pipe.user_data.config.engine,
                            &mut pipe.user_data.config.game,
                            pipe.user_data.msgs,
                            pipe.user_data.can_change_client_config,
                        );
                        pipe.user_data
                            .msg_history
                            .push_front(std::mem::take(pipe.user_data.msg));
                    } else if tab {
                        // nothing to do here
                    } else if label.response.changed() {
                        // reset entry index
                        *pipe.user_data.select_index = None;
                    }
                } else if space && pipe.user_data.select_index.is_some() {
                    let index = pipe.user_data.select_index.unwrap();
                    let (entries, list_entries, msg_range) = find_matches(
                        cmds,
                        cursor_before_inp,
                        pipe.user_data.entries,
                        &msg_before_inp,
                        pipe.user_data.custom_matches,
                    );
                    let cur_entry = entries.iter().find(|(e, _)| e.index() == index);
                    let mut cursor_next = None;
                    if let Some((e, _)) = cur_entry {
                        let text = match e {
                            MatchedType::Entry(index) => match &pipe.user_data.entries[*index] {
                                ConsoleEntry::Var(ConsoleEntryVariable {
                                    full_name: name, ..
                                })
                                | ConsoleEntry::Cmd(ConsoleEntryCmd { name, .. }) => name,
                            },
                            MatchedType::ArgList(index) | MatchedType::CustomList { index, .. } => {
                                &list_entries.as_ref().unwrap()[*index]
                            }
                        };

                        let msg = &mut *pipe.user_data.msg;

                        let name_len = text.chars().count();
                        let cur_cursor_start = msg
                            .char_indices()
                            .enumerate()
                            .find_map(|(index, (off, _))| (off == msg_range.start).then_some(index))
                            .unwrap_or_default();
                        *msg = msg_before_inp;

                        let mut repl_text = text.clone();

                        if msg.char_indices().all(|(index, c)| {
                            if index == msg_range.end {
                                !c.is_ascii_whitespace()
                            } else {
                                true
                            }
                        }) {
                            repl_text.push(' ');
                        }

                        msg.replace_range(msg_range, &repl_text);
                        cursor_next = Some((cur_cursor_start + name_len) + 1);
                    }

                    *pipe.user_data.select_index = None;
                    label.state.cursor.set_char_range(cursor_next.map(|index| {
                        CCursorRange::one(CCursor {
                            index,
                            ..Default::default()
                        })
                    }));
                    label.state.store(ui.ctx(), inp_id);
                } else if (up || down) && !pipe.user_data.msg_history.is_empty() {
                    let new_index = match pipe.user_data.msg_history_index.as_mut() {
                        Some(index) => {
                            if up {
                                *index = index.checked_add(1).unwrap_or_default()
                                    % pipe.user_data.msg_history.len();
                            } else if down {
                                *index = index
                                    .checked_add(pipe.user_data.msg_history.len())
                                    .unwrap_or_default()
                                    .saturating_sub(1)
                                    % pipe.user_data.msg_history.len();
                            }
                            *index
                        }
                        None => {
                            *pipe.user_data.msg_history_index = Some(0);
                            0
                        }
                    };
                    *pipe.user_data.msg = pipe.user_data.msg_history[new_index].clone();
                    let index = pipe.user_data.msg.chars().count();
                    label
                        .state
                        .cursor
                        .set_char_range(Some(CCursorRange::one(CCursor {
                            index,
                            ..Default::default()
                        })));
                    label.state.store(ui.ctx(), inp_id);
                } else if (!mouse_is_down && !has_text_selection) || ui_state.hint_had_input {
                    label.response.request_focus();
                }
                if label.response.changed() {
                    *pipe.user_data.msg_history_index = None;
                }
                if tab {
                    // select next entry
                    let (entries, _, _) = find_matches(
                        cmds,
                        *pipe.user_data.cursor,
                        pipe.user_data.entries,
                        pipe.user_data.msg,
                        pipe.user_data.custom_matches,
                    );
                    let it: Box<dyn Iterator<Item = _>> = if !modifiers.shift {
                        Box::new(entries.iter())
                    } else {
                        Box::new(entries.iter().rev())
                    };

                    let mut cur_entry = it
                        .skip_while(|(e, _)| {
                            if let Some(i) = pipe.user_data.select_index {
                                e.index() != *i
                            } else {
                                true
                            }
                        })
                        .peekable()
                        .map(|(e, _)| e.index());
                    // skip the found element
                    cur_entry.next();
                    if let Some(cur_entry) = cur_entry.next() {
                        *pipe.user_data.select_index = Some(cur_entry);
                    } else {
                        // try select first entry
                        let mut it: Box<dyn Iterator<Item = _>> = if !modifiers.shift {
                            Box::new(entries.iter())
                        } else {
                            Box::new(entries.iter().rev())
                        };
                        if let Some((cur_entry, _)) = it.next() {
                            *pipe.user_data.select_index = Some(cur_entry.index());
                        }
                    }
                }
            },
        );
    });
}
