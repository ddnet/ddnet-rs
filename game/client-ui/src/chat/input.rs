use std::borrow::Borrow;

use egui::{
    scroll_area::ScrollBarVisibility, text::LayoutJob, Color32, Frame, Margin, ScrollArea, Shadow,
    TextFormat,
};
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use game_interface::types::render::character::TeeEye;
use math::math::vector::vec2;
use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
    utils::add_margins,
};

use crate::utils::render_tee_for_ui;

use super::user_data::{ChatEvent, ChatMode, UserData};

const SKIN_SIZE: f32 = 20.0;

/// chat input
fn render_inner(ui: &mut egui::Ui, ui_state: &mut UiState, pipe: &mut UiRenderPipe<UserData>) {
    let (is_escape, is_tab, is_enter, is_backspace) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::Escape),
            i.key_pressed(egui::Key::Tab),
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Backspace),
        )
    });

    // Some whisper related stuff
    if matches!(pipe.user_data.mode, ChatMode::Whisper(_)) {
        // in case backspace is pressed and the current msg is empty we allow
        // the user to reenter a new name
        if is_backspace && pipe.user_data.msg.is_empty() {
            *pipe.user_data.cur_whisper_player_id = None;
            pipe.user_data.mode = ChatMode::Whisper(None);
        }
    }
    if let ChatMode::Whisper(None) = &mut pipe.user_data.mode {
        // if whisper is empty, try to set from previous state
        pipe.user_data.mode = ChatMode::Whisper(*pipe.user_data.cur_whisper_player_id);
    }

    let to = ui
        .allocate_ui(egui::vec2(ui.available_width(), 30.0), |ui| {
            ui.horizontal_centered(|ui| {
                let (mode_name, to) = match pipe.user_data.mode {
                    ChatMode::Global => ("All", None),
                    ChatMode::Team => ("Team", None),
                    ChatMode::Whisper(player_id) => ("To", {
                        player_id
                            .and_then(|player_id| {
                                (!pipe.user_data.local_character_ids.contains(&player_id))
                                    .then_some(player_id)
                            })
                            .and_then(|player_id| pipe.user_data.character_infos.get(&player_id))
                    }),
                };
                let rect = ui.label(mode_name).rect;
                if let Some(to) = to {
                    let x = ui.style().spacing.item_spacing.x;
                    ui.style_mut().spacing.item_spacing.x = 0.0;
                    ui.add_space(SKIN_SIZE);
                    ui.style_mut().spacing.item_spacing.x = x;

                    render_tee_for_ui(
                        pipe.user_data.canvas_handle,
                        pipe.user_data.skin_container,
                        pipe.user_data.render_tee,
                        ui,
                        ui_state,
                        ui.ctx().screen_rect(),
                        None,
                        to.info.skin.borrow(),
                        Some(&to.info.skin_info),
                        vec2::new(
                            rect.max.x + ui.style().spacing.item_spacing.x + SKIN_SIZE / 2.0,
                            rect.right_center().y,
                        ),
                        SKIN_SIZE,
                        TeeEye::Happy,
                    );
                    ui.label(to.info.name.as_str());
                }
                ui.label(":");

                // If no use was selected for a whisper, then make a prompt to find one
                let unfinished_whisper =
                    to.is_none() && matches!(pipe.user_data.mode, ChatMode::Whisper(_));
                let label = if unfinished_whisper {
                    ui.text_edit_singleline(pipe.user_data.find_player_prompt)
                } else {
                    ui.text_edit_singleline(pipe.user_data.msg)
                };
                // handled later
                if !unfinished_whisper {
                    if label.lost_focus() {
                        if is_escape || (!is_tab && is_enter) {
                            pipe.user_data.chat_events.push(ChatEvent::ChatClosed);
                        }
                        if (matches!(pipe.user_data.mode, ChatMode::Whisper(Some(_)))
                            || !matches!(pipe.user_data.mode, ChatMode::Whisper(_)))
                            && !pipe.user_data.msg.is_empty()
                            && !is_escape
                        {
                            pipe.user_data.chat_events.push(ChatEvent::MsgSend {
                                msg: pipe.user_data.msg.clone(),
                                mode: pipe.user_data.mode,
                            });
                        }
                    } else {
                        pipe.user_data.chat_events.push(ChatEvent::CurMsg {
                            msg: pipe.user_data.msg.clone(),
                            mode: pipe.user_data.mode,
                        });
                    }
                }
                label.request_focus();

                to
            })
            .inner
        })
        .inner;

    let unfinished_whisper = to.is_none() && matches!(pipe.user_data.mode, ChatMode::Whisper(_));
    if let Some(whispered_id) = unfinished_whisper.then_some(&mut *pipe.user_data.find_player_id) {
        let matcher = SkimMatcherV2::default();
        let matches: Vec<_> = pipe
            .user_data
            .character_infos
            .iter()
            .filter(|(id, _)| !pipe.user_data.local_character_ids.contains(id))
            .map(|(id, c)| {
                (
                    id,
                    c,
                    c.info.name.len() as i64,
                    matcher.fuzzy_indices(&c.info.name, pipe.user_data.find_player_prompt),
                )
            })
            .filter(|(_, _, _, m)| m.is_some())
            .map(|(id, c, len, m)| (id, c, len, m.unwrap()))
            .collect();
        let shadow_color = ui.style().visuals.window_shadow.color;
        ui.add_space(5.0);
        ui.allocate_ui(egui::vec2(ui.available_width(), 45.0), |ui| {
            ui.style_mut().visuals.clip_rect_margin = 6.0;
            ScrollArea::horizontal()
                .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.horizontal_centered(|ui| {
                        for (&char_id, msg_char, _, (_, matching_char_indices)) in &matches {
                            let (bg_color_text, match_color, default_color, margin, shadow) =
                                if *whispered_id == Some(char_id) {
                                    (
                                        Color32::from_rgba_unmultiplied(140, 140, 140, 15),
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

                            let msg_chars = msg_char.info.name.as_str().chars().enumerate();
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
                                    let rect = ui.available_rect_before_wrap();
                                    ui.horizontal(|ui| {
                                        ui.add_space(SKIN_SIZE);
                                        render_tee_for_ui(
                                            pipe.user_data.canvas_handle,
                                            pipe.user_data.skin_container,
                                            pipe.user_data.render_tee,
                                            ui,
                                            ui_state,
                                            ui.ctx().screen_rect(),
                                            None,
                                            msg_char.info.skin.borrow(),
                                            Some(&msg_char.info.skin_info),
                                            vec2::new(
                                                rect.left() + SKIN_SIZE / 2.0,
                                                rect.left_center().y,
                                            ),
                                            SKIN_SIZE,
                                            TeeEye::Happy,
                                        );

                                        ui.label(text_label);
                                    });
                                });
                            if *whispered_id == Some(char_id) {
                                label.response.scroll_to_me(Some(egui::Align::Max));
                            }
                        }
                    });
                });

            ui.colored_label(
                Color32::YELLOW,
                "Press tab to switch the player. Press enter or space to select the player.",
            )
        });
        if is_tab {
            // chain here so we can simply call it.next()
            let mut it = matches
                .iter()
                .map(|(&id, _, _, _)| id)
                .chain(matches.iter().map(|(&id, _, _, _)| id).take(1))
                .skip_while(|id| Some(*id) != *whispered_id);
            // this would be current selection
            it.next();
            if let Some(next_id) = it.next() {
                *whispered_id = Some(next_id);
            }
            if whispered_id.is_none() {
                *whispered_id = matches.iter().map(|(&id, _, _, _)| id).next();
            }
        } else if let Some(whisper_id) = is_enter.then_some(*whispered_id).and_then(|find_id| {
            matches
                .iter()
                .any(|(&id, _, _, _)| Some(id) == find_id)
                .then_some(find_id)
        }) {
            pipe.user_data.mode = ChatMode::Whisper(whisper_id);
            *pipe.user_data.cur_whisper_player_id = whisper_id;
        }
        pipe.user_data.chat_events.push(ChatEvent::CurMsg {
            msg: pipe.user_data.msg.clone(),
            mode: pipe.user_data.mode,
        });

        if is_escape {
            pipe.user_data.chat_events.push(ChatEvent::ChatClosed);
        }
    }
}

pub fn render(ui: &mut egui::Ui, ui_state: &mut UiState, pipe: &mut UiRenderPipe<UserData>) {
    if pipe.user_data.is_input_active {
        ui.allocate_ui(
            egui::vec2(
                ui.available_width(),
                if matches!(pipe.user_data.mode, ChatMode::Whisper(_)) {
                    80.0
                } else {
                    30.0
                },
            ),
            |ui| {
                Frame::NONE
                    .corner_radius(5.0)
                    .fill(bg_frame_color())
                    .show(ui, |ui| {
                        add_margins(ui, |ui| {
                            render_inner(ui, ui_state, pipe);
                        });
                    });
            },
        );
    }
}
