use std::{borrow::Borrow, time::Duration};

use base::duration_ext::DurationToRaceStr;
use egui::{
    Align2, Color32, CornerRadius, FontId, Frame, Layout, Margin, Rect, RichText, UiBuilder, Vec2,
    Window,
};

use egui_extras::{Size, StripBuilder};
use game_interface::types::{
    flag::FlagType,
    id_types::CharacterId,
    render::{
        character::TeeEye,
        game::{
            game_match::{MatchSide, MatchStandings},
            GameRenderInfo, MatchRoundGameOverWinner, MatchRoundTimeType,
        },
    },
};
use math::math::vector::vec2;
use ui_base::{
    better_frame::BetterFrame,
    types::{UiRenderPipe, UiState},
};

use crate::utils::{render_tee_for_ui, render_texture_for_ui};

use super::user_data::UserData;

/// not required
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    ui.style_mut().animation_time = 0.0;
    ui.add_space(5.0);

    let tick_time_nanos =
        Duration::from_secs(1).as_nanos() as u64 / pipe.user_data.ticks_per_second.get();
    let secs = *pipe.user_data.race_round_timer_counter / pipe.user_data.ticks_per_second.get();
    let nanos = (*pipe.user_data.race_round_timer_counter % pipe.user_data.ticks_per_second.get())
        * tick_time_nanos;
    let round_time = Duration::new(secs, nanos as u32);
    let time_str = round_time.to_race_string();
    let (time_str, time_str_color, balance_msg, is_game_over) = match pipe.user_data.game {
        Some(info) => match info {
            GameRenderInfo::Race {} => (time_str, Color32::WHITE, None, None),
            GameRenderInfo::Match {
                round_time_type,
                unbalanced,
                ..
            } => {
                let balance_msg = unbalanced.then(|| {
                    (
                        "Please balance the teams!".to_string(),
                        if (pipe.cur_time.subsec_millis()) < 500 {
                            Color32::LIGHT_YELLOW
                        } else {
                            Color32::YELLOW
                        },
                    )
                });
                match round_time_type {
                    MatchRoundTimeType::TimeLimit { ticks_left } => {
                        let secs = ticks_left / pipe.user_data.ticks_per_second.get();
                        let nanos =
                            (ticks_left % pipe.user_data.ticks_per_second.get()) * tick_time_nanos;
                        (
                            Duration::new(secs, nanos as u32).to_race_string(),
                            if secs < 10 {
                                if (nanos / 1000000) < 500 {
                                    Color32::LIGHT_RED
                                } else {
                                    Color32::RED
                                }
                            } else if secs < 15 {
                                Color32::LIGHT_RED
                            } else {
                                Color32::WHITE
                            },
                            balance_msg,
                            None,
                        )
                    }
                    MatchRoundTimeType::Normal => (time_str, Color32::WHITE, balance_msg, None),
                    MatchRoundTimeType::SuddenDeath => (
                        "Sudden Death".to_string(),
                        Color32::WHITE,
                        balance_msg,
                        None,
                    ),
                    MatchRoundTimeType::GameOver { winner, .. } => {
                        ("".into(), Color32::WHITE, None, Some(winner))
                    }
                }
            }
        },
        None => (time_str, Color32::WHITE, None, None),
    };

    let color_a =
        |color: Color32, a: u8| Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a);

    const ROUNDING: u8 = 5;
    const MARGIN: i8 = 3;
    let (max_height, rounding) = if is_game_over.is_some() {
        (40.0, CornerRadius::same(ROUNDING))
    } else {
        match pipe.user_data.game {
            Some(GameRenderInfo::Match {
                standings: MatchStandings::Solo { .. },
                ..
            }) => (60.0, CornerRadius::same(0)),
            Some(GameRenderInfo::Match {
                standings: MatchStandings::Sided { .. },
                ..
            }) => (
                40.0,
                CornerRadius {
                    ne: ROUNDING,
                    nw: ROUNDING,
                    ..Default::default()
                },
            ),
            Some(GameRenderInfo::Race { .. }) | None => (25.0, CornerRadius::same(ROUNDING)),
        }
    };

    enum Side {
        Left,
        Right,
        Bottom(Rect),
    }
    let render_side = |pipe: &mut UiRenderPipe<UserData>,
                       ui: &mut egui::Ui,
                       ui_state: &mut UiState,
                       side: Side| {
        let rect = ui.available_rect_before_wrap();
        match pipe.user_data.game {
            Some(GameRenderInfo::Match { standings, .. }) => {
                pub struct RenderCharacter {
                    pub character_id: CharacterId,
                    pub score: i64,
                }
                let mut render_char = |ui: &mut egui::Ui,
                                       render_character: Option<RenderCharacter>,
                                       flag: Option<FlagType>,
                                       left: bool| {
                    let rounding = if left {
                        CornerRadius {
                            sw: ROUNDING,
                            nw: ROUNDING,
                            ..Default::default()
                        }
                    } else {
                        CornerRadius {
                            ne: ROUNDING,
                            se: ROUNDING,
                            ..Default::default()
                        }
                    };

                    let mut rect = rect;
                    rect.set_width(100.0);
                    rect.set_height(60.0);
                    ui.style_mut().spacing.item_spacing.y = 0.0;
                    ui.allocate_new_ui(UiBuilder::new().max_rect(rect), |ui| {
                        Frame::default()
                            .corner_radius(rounding)
                            .fill(color_a(Color32::BLACK, 50))
                            .inner_margin(Margin::same(MARGIN))
                            .show(ui, |ui| {
                                ui.set_height(60.0);
                                ui.set_width(100.0);
                                let data = &mut *pipe.user_data;
                                if let Some((character, score)) =
                                    render_character.as_ref().and_then(|leading_character| {
                                        data.character_infos
                                            .get(&leading_character.character_id)
                                            .map(|c| (c, leading_character.score))
                                    })
                                {
                                    let rect = ui.available_rect_before_wrap();

                                    let tee_size = rect.width().min(rect.height()).min(30.0);

                                    if let Some(flag) = flag {
                                        let ctf =
                                            data.ctf_container.get_or_default(&character.info.ctf);
                                        render_texture_for_ui(
                                            data.stream_handle,
                                            data.canvas_handle,
                                            match flag {
                                                FlagType::Red => &ctf.flag_red,
                                                FlagType::Blue => &ctf.flag_blue,
                                            },
                                            ui,
                                            ui_state,
                                            ui.ctx().screen_rect(),
                                            Some(ui.clip_rect()),
                                            vec2::new(
                                                rect.center().x,
                                                rect.center().y - tee_size / 4.0,
                                            ),
                                            vec2::new(tee_size / 2.0, tee_size),
                                            None,
                                        );
                                    }

                                    render_tee_for_ui(
                                        data.canvas_handle,
                                        data.skin_container,
                                        data.skin_renderer,
                                        ui,
                                        ui_state,
                                        ui.ctx().screen_rect(),
                                        Some(rect),
                                        character.info.skin.borrow(),
                                        Some(&character.skin_info),
                                        vec2::new(rect.center().x, rect.center().y),
                                        tee_size,
                                        TeeEye::Normal,
                                    );
                                    StripBuilder::new(ui)
                                        .size(Size::remainder())
                                        .size(Size::exact(tee_size))
                                        .size(Size::remainder())
                                        .cell_layout(
                                            Layout::bottom_up(egui::Align::Center)
                                                .with_main_align(egui::Align::Max),
                                        )
                                        .vertical(|mut strip| {
                                            strip.cell(|ui| {
                                                ui.style_mut().wrap_mode = None;
                                                ui.colored_label(
                                                    Color32::WHITE,
                                                    character.info.name.as_str(),
                                                );
                                            });
                                            strip.empty();

                                            strip.cell(|ui| {
                                                ui.style_mut().wrap_mode = None;
                                                ui.with_layout(
                                                    Layout::bottom_up(egui::Align::Center)
                                                        .with_main_justify(false),
                                                    |ui| {
                                                        ui.colored_label(
                                                            Color32::WHITE,
                                                            format!("{}", score),
                                                        );
                                                    },
                                                );
                                            });
                                        });
                                }
                            });
                    });
                };
                if is_game_over.is_none() {
                    match standings {
                        MatchStandings::Solo { leading_characters } => {
                            if matches!(side, Side::Left) {
                                render_char(
                                    ui,
                                    leading_characters[0].map(|c| RenderCharacter {
                                        character_id: c.character_id,
                                        score: c.score,
                                    }),
                                    None,
                                    true,
                                );
                                true
                            } else if matches!(side, Side::Right) {
                                render_char(
                                    ui,
                                    leading_characters[1].map(|c| RenderCharacter {
                                        character_id: c.character_id,
                                        score: c.score,
                                    }),
                                    None,
                                    false,
                                );
                                true
                            } else {
                                false
                            }
                        }
                        MatchStandings::Sided {
                            score_red,
                            score_blue,
                            flag_carrier_red,
                            flag_carrier_blue,
                        } => {
                            let has_carrier =
                                flag_carrier_red.is_some() || flag_carrier_blue.is_some();
                            if let Side::Bottom(rect) = side {
                                // no spacing for points
                                ui.style_mut().spacing.item_spacing = Default::default();
                                ui.allocate_new_ui(
                                    UiBuilder::default().max_rect(
                                        rect.translate(egui::vec2(
                                            0.0,
                                            rect.height() + 2.0 * MARGIN as f32,
                                        ))
                                        .expand(MARGIN as f32),
                                    ),
                                    |ui| {
                                        StripBuilder::new(ui)
                                            .size(Size::remainder())
                                            .size(Size::remainder())
                                            .cell_layout(Layout::top_down(egui::Align::Center))
                                            .horizontal(|mut strip| {
                                                strip.cell(|ui| {
                                                    ui.style_mut().wrap_mode = None;
                                                    Frame::NONE
                                                        .fill(color_a(Color32::RED, 150))
                                                        .corner_radius(CornerRadius {
                                                            sw: ROUNDING,
                                                            ..Default::default()
                                                        })
                                                        .show(ui, |ui| {
                                                            ui.colored_label(
                                                                Color32::WHITE,
                                                                format!("{}", score_red),
                                                            );
                                                        });
                                                });
                                                strip.cell(|ui| {
                                                    ui.style_mut().wrap_mode = None;
                                                    Frame::NONE
                                                        .fill(color_a(Color32::BLUE, 150))
                                                        .corner_radius(CornerRadius {
                                                            se: ROUNDING,
                                                            ..Default::default()
                                                        })
                                                        .show(ui, |ui| {
                                                            ui.colored_label(
                                                                Color32::WHITE,
                                                                format!("{}", score_blue),
                                                            );
                                                        });
                                                });
                                            });
                                    },
                                );
                                true
                            } else if matches!(side, Side::Left) && has_carrier {
                                render_char(
                                    ui,
                                    flag_carrier_red.map(|c| RenderCharacter {
                                        character_id: c.character_id,
                                        score: c.score,
                                    }),
                                    Some(FlagType::Blue),
                                    true,
                                );
                                true
                            } else if matches!(side, Side::Right) && has_carrier {
                                render_char(
                                    ui,
                                    flag_carrier_blue.map(|c| RenderCharacter {
                                        character_id: c.character_id,
                                        score: c.score,
                                    }),
                                    Some(FlagType::Red),
                                    false,
                                );
                                true
                            } else {
                                false
                            }
                        }
                    }
                } else {
                    false
                }
            }
            Some(GameRenderInfo::Race { .. }) => false,
            None => {
                // don't render anything
                false
            }
        }
    };

    let res = Window::new("")
        .resizable(false)
        .title_bar(false)
        .frame(Frame::NONE)
        .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 5.0))
        .max_height(max_height)
        .show(ui.ctx(), |ui| {
            ui.set_clip_rect(ui.ctx().screen_rect());
            ui.style_mut().spacing.item_spacing.y = 0.0;
            let rect = ui
                .with_layout(
                    Layout::left_to_right(egui::Align::Center)
                        .with_main_justify(false)
                        .with_cross_justify(true),
                    |ui| {
                        let rendered = render_side(pipe, ui, ui_state, Side::Left);

                        let mut frame = Frame::default()
                            .corner_radius(rounding)
                            .inner_margin(Margin::same(MARGIN))
                            .fill(color_a(Color32::BLACK, 50))
                            .begin_better(ui);

                        if rendered {
                            frame.frame.corner_radius.nw = 0;
                            frame.frame.corner_radius.sw = 0;
                        }

                        if let Some(is_game_over) = is_game_over {
                            match is_game_over {
                                MatchRoundGameOverWinner::Characters(chars) => {
                                    frame.content_ui.horizontal(|ui| {
                                        ui.style_mut().spacing.item_spacing.x = 0.0;
                                        let rect = ui.available_rect_before_wrap();
                                        for (index, char) in chars.iter().enumerate() {
                                            const SKIN_RECT_SIZE: f32 = 50.0;
                                            ui.add_space(SKIN_RECT_SIZE);

                                            render_tee_for_ui(
                                                pipe.user_data.canvas_handle,
                                                pipe.user_data.skin_container,
                                                pipe.user_data.skin_renderer,
                                                ui,
                                                ui_state,
                                                ui.ctx().screen_rect(),
                                                Some(rect),
                                                (*char.skin).borrow(),
                                                Some(&char.skin_info),
                                                vec2::new(
                                                    ui.available_rect_before_wrap().min.x
                                                        - SKIN_RECT_SIZE / 2.0,
                                                    rect.center().y,
                                                ),
                                                SKIN_RECT_SIZE / 2.0,
                                                TeeEye::Normal,
                                            );

                                            ui.label(
                                                RichText::new(char.name.as_str())
                                                    .color(Color32::WHITE),
                                            );

                                            match (index + 2).cmp(&chars.len()) {
                                                std::cmp::Ordering::Less => {
                                                    ui.label(
                                                        RichText::new(", ").color(Color32::WHITE),
                                                    );
                                                }
                                                std::cmp::Ordering::Equal => {
                                                    ui.label(
                                                        RichText::new(" & ").color(Color32::WHITE),
                                                    );
                                                }
                                                std::cmp::Ordering::Greater => {
                                                    // can't happen
                                                }
                                            }
                                        }

                                        match chars.len().cmp(&1) {
                                            std::cmp::Ordering::Less => {
                                                // ignore
                                            }
                                            std::cmp::Ordering::Equal => {
                                                ui.label(
                                                    RichText::new(" wins!").color(Color32::WHITE),
                                                );
                                            }
                                            std::cmp::Ordering::Greater => {
                                                ui.label(
                                                    RichText::new(" win!").color(Color32::WHITE),
                                                );
                                            }
                                        }
                                    });
                                }
                                MatchRoundGameOverWinner::Side(side) => {
                                    frame.content_ui.label(
                                        RichText::new(format!(
                                            "{} wins!",
                                            match side {
                                                MatchSide::Red => "Red",
                                                MatchSide::Blue => "Blue",
                                            }
                                        ))
                                        .color(Color32::WHITE),
                                    );
                                }
                                MatchRoundGameOverWinner::SideNamed(name) => {
                                    frame.content_ui.label(
                                        RichText::new(format!("{} wins!", name.as_str()))
                                            .color(Color32::WHITE),
                                    );
                                }
                            }
                        } else {
                            frame.content_ui.label(
                                RichText::new(time_str)
                                    .font(FontId::proportional(20.0))
                                    .color(time_str_color),
                            );
                        };

                        frame.allocate_space(ui);
                        let rendered = render_side(pipe, ui, ui_state, Side::Right);

                        if rendered {
                            frame.frame.corner_radius.ne = 0;
                            frame.frame.corner_radius.se = 0;
                        }
                        frame.paint(ui)
                    },
                )
                .inner;
            render_side(pipe, ui, ui_state, Side::Bottom(rect));
        });

    if let Some((balance_msg, color)) = balance_msg {
        ui.allocate_new_ui(
            UiBuilder::default().max_rect(
                res.map(|r| {
                    ui.ctx()
                        .screen_rect()
                        .translate(egui::vec2(0.0, r.response.rect.height()))
                })
                .unwrap_or_else(|| ui.ctx().screen_rect()),
            ),
            |ui| {
                ui.with_layout(
                    Layout::left_to_right(egui::Align::Min)
                        .with_main_justify(true)
                        .with_main_align(egui::Align::Center),
                    |ui| {
                        ui.label(RichText::new(balance_msg).color(color));
                    },
                );
            },
        );
    }
}
