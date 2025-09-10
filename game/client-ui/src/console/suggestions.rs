use client_types::console::ConsoleEntry;
use command_parser::parser::CommandsTyped;
use config::traits::ConfigInterface;
use egui::{
    Color32, FontId, Frame, Margin, ScrollArea, TextFormat, UiBuilder, epaint::Shadow,
    scroll_area::ScrollBarVisibility, text::LayoutJob,
};
use ui_base::types::{UiRenderPipe, UiState};

use super::{
    user_data::UserData,
    utils::{MatchedType, find_matches},
};

pub fn render(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    pipe: &mut UiRenderPipe<UserData>,
    cmds: &CommandsTyped,
) {
    if !pipe.user_data.msg.is_empty() {
        // add suggestions
        let (found_entries, list_entries, _) = find_matches(
            cmds,
            *pipe.user_data.cursor,
            pipe.user_data.entries,
            pipe.user_data.msg,
            pipe.user_data.custom_matches,
        );

        let mut rect = ui.available_rect_before_wrap();
        rect.min.x += 5.0;
        rect.max.x -= 5.0;
        let shadow_color = ui.style().visuals.window_shadow.color;
        ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
            ui.vertical(|ui| {
                let found_entries_is_empty = found_entries.is_empty();

                ScrollArea::horizontal()
                    .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (entry_index, (_, matching_char_indices)) in found_entries {
                                let (bg_color_text, match_color, default_color, margin, shadow) =
                                    if *pipe.user_data.select_index == Some(entry_index.index()) {
                                        (
                                            Color32::from_rgba_unmultiplied(140, 140, 140, 100),
                                            Color32::from_rgb(180, 180, 255),
                                            Color32::from_rgb(255, 255, 255),
                                            Margin::symmetric(5, 5),
                                            Shadow {
                                                blur: 10,
                                                spread: 1,
                                                color: shadow_color,
                                                ..Default::default()
                                            },
                                        )
                                    } else {
                                        (
                                            Color32::TRANSPARENT,
                                            Color32::from_rgb(180, 180, 255),
                                            if ui.visuals().dark_mode {
                                                Color32::WHITE
                                            } else {
                                                Color32::DARK_GRAY
                                            },
                                            Margin::symmetric(5, 5),
                                            Shadow::NONE,
                                        )
                                    };

                                let shorted_path = match entry_index {
                                    MatchedType::Entry(index) => {
                                        match &pipe.user_data.entries[index] {
                                            ConsoleEntry::Var(v) => v
                                                .full_name
                                                .replace("$KEY$", "[key]")
                                                .replace("$INDEX$", "[index]"),
                                            ConsoleEntry::Cmd(c) => c.name.clone(),
                                        }
                                    }
                                    MatchedType::ArgList(index)
                                    | MatchedType::CustomList { index, .. } => {
                                        list_entries.as_ref().unwrap()[index].clone()
                                    }
                                };
                                let msg_chars = shorted_path.chars().enumerate();
                                let mut text_label = LayoutJob::default();
                                for (i, msg_char) in msg_chars {
                                    if matching_char_indices.contains(&i) {
                                        text_label.append(
                                            &msg_char.to_string(),
                                            0.0,
                                            TextFormat {
                                                color: match_color,
                                                ..Default::default()
                                            },
                                        );
                                    } else {
                                        text_label.append(
                                            &msg_char.to_string(),
                                            0.0,
                                            TextFormat {
                                                color: default_color,
                                                ..Default::default()
                                            },
                                        );
                                    }
                                }
                                let label = Frame::default()
                                    .fill(bg_color_text)
                                    .corner_radius(5.0)
                                    .inner_margin(margin)
                                    .shadow(shadow)
                                    .show(ui, |ui| {
                                        ui.label(text_label);

                                        if let MatchedType::CustomList { custom_ty, .. } =
                                            &entry_index
                                        {
                                            (pipe.user_data.render_custom_matches)(
                                                custom_ty,
                                                &shorted_path,
                                                ui,
                                                ui_state,
                                                pipe.user_data.skin_container,
                                                pipe.user_data.render_tee,
                                            )
                                        }
                                    });
                                if *pipe.user_data.select_index == Some(entry_index.index()) {
                                    label.response.scroll_to_me(Some(egui::Align::Max));
                                }
                            }
                        });
                    });

                let selected_index = *pipe.user_data.select_index;

                if list_entries.is_none() {
                    let selected_entry = (!found_entries_is_empty)
                        .then_some(
                            selected_index.and_then(|index| pipe.user_data.entries.get(index)),
                        )
                        .flatten();
                    if let Some(selected_entry) = selected_entry {
                        let mut job = LayoutJob::default();
                        let font_size = 9.0;
                        match selected_entry {
                            ConsoleEntry::Var(v) => {
                                let config = &mut *pipe.user_data.config;
                                let val = config
                                    .engine
                                    .try_set_from_str(
                                        v.full_name.clone(),
                                        None,
                                        None,
                                        None,
                                        Default::default(),
                                    )
                                    .map(Some)
                                    .unwrap_or_else(|_| {
                                        config
                                            .game
                                            .try_set_from_str(
                                                v.full_name.clone(),
                                                None,
                                                None,
                                                None,
                                                Default::default(),
                                            )
                                            .map(Some)
                                            .ok()
                                            .flatten()
                                    });

                                if let Some(mut val) = val {
                                    if val.len() > 42 {
                                        val.truncate(39);
                                        val = format!("{val}...");
                                    }
                                    job.append(
                                        "current value: ",
                                        0.0,
                                        TextFormat {
                                            font_id: FontId::monospace(font_size),
                                            color: Color32::WHITE,
                                            ..Default::default()
                                        },
                                    );
                                    job.append(
                                        &val,
                                        0.0,
                                        TextFormat {
                                            font_id: FontId::monospace(font_size),
                                            color: Color32::WHITE,
                                            background: Color32::DARK_GRAY,
                                            ..Default::default()
                                        },
                                    );
                                    job.append(
                                        ", ",
                                        0.0,
                                        TextFormat {
                                            font_id: FontId::monospace(font_size),
                                            color: Color32::WHITE,
                                            ..Default::default()
                                        },
                                    );
                                }

                                job.append(
                                    &format!(
                                        "{}{}",
                                        if !v.usage.is_empty() {
                                            format!("usage: {}, ", v.usage)
                                        } else {
                                            "".into()
                                        },
                                        if !v.description.is_empty() {
                                            format!("description: {}", v.description)
                                        } else {
                                            "".into()
                                        }
                                    ),
                                    0.0,
                                    TextFormat {
                                        font_id: FontId::monospace(font_size),
                                        color: Color32::WHITE,
                                        ..Default::default()
                                    },
                                );
                            }
                            ConsoleEntry::Cmd(cmd) => {
                                job.append(
                                    &format!("usage: {}", cmd.usage),
                                    0.0,
                                    TextFormat {
                                        font_id: FontId::monospace(font_size),
                                        color: Color32::WHITE,
                                        ..Default::default()
                                    },
                                );
                            }
                        }
                        ui.add_space(3.0);
                        ui.label(job);
                    }
                }
            });
        });
    }
}
