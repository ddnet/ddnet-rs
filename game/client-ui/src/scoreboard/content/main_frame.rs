use std::iter::Peekable;

use base::{duration_ext::DurationToRaceStr, linked_hash_map_view::FxLinkedHashMap};
use egui::{CornerRadius, Rect, Shape, epaint::RectShape};
use egui_extras::{Size, StripBuilder};

use game_interface::types::{
    id_types::{CharacterId, StageId},
    render::{
        character::CharacterInfo,
        scoreboard::{
            ScoreboardGameType, ScoreboardGameTypeOptions, ScoreboardScoreType, ScoreboardStageInfo,
        },
    },
};
use tracing::instrument;
use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
};

use crate::scoreboard::user_data::UserData;

use super::{list::player_list::entry::RenderPlayer, topbar::TopBarTypes};

#[instrument(level = "trace", skip_all)]
fn render_scoreboard_frame<'a>(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    full_ui_rect: Rect,
    topbar_type: TopBarTypes,
    rounding: CornerRadius,
    character_infos: &FxLinkedHashMap<CharacterId, CharacterInfo>,
    players: &mut Peekable<impl Iterator<Item = RenderPlayer<'a>>>,
    player_count: usize,
    stages: &FxLinkedHashMap<StageId, ScoreboardStageInfo>,
    top_label: &str,
    top_label_opposite: &str,
    bottom_label_left: &str,
    bottom_label_right: &str,
) {
    ui.painter().add(Shape::Rect(RectShape::filled(
        ui.available_rect_before_wrap(),
        rounding,
        bg_frame_color(),
    )));
    ui_state.add_blur_rect(ui.available_rect_before_wrap(), rounding);
    StripBuilder::new(ui)
        .size(Size::exact(30.0))
        .size(Size::exact(0.0))
        .size(Size::remainder())
        .size(Size::exact(0.0))
        .size(Size::exact(13.0))
        .size(Size::exact(2.0))
        .vertical(|mut strip| {
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                super::topbar::render(ui, topbar_type, rounding, top_label, top_label_opposite);
            });
            strip.empty();
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                super::list::list::render(
                    ui,
                    pipe,
                    ui_state,
                    &full_ui_rect,
                    character_infos,
                    players,
                    player_count,
                    stages,
                );
            });
            strip.empty();
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                super::footer::render(ui, (bottom_label_left, bottom_label_right));
            });
            strip.empty();
        });
}

/// big boxes, rounded edges
#[instrument(level = "trace", skip_all)]
pub fn render_players(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    full_ui_rect: Rect,
) -> f32 {
    let mut res = 0.0;
    let character_infos = pipe.user_data.character_infos;
    let scoreboard = &pipe.user_data.scoreboard;
    let options = &scoreboard.options;

    let own_character = character_infos.get(pipe.user_data.own_character_id);

    fn match_ty_str(ty: ScoreboardGameTypeOptions) -> (String, String) {
        match ty {
            ScoreboardGameTypeOptions::Match {
                score_limit,
                time_limit,
            } => (
                format!("Score limit: {score_limit}"),
                if let Some(time_limit) = time_limit {
                    format!("Time limit: {}", time_limit.to_race_string())
                } else {
                    "".to_string()
                },
            ),
            ScoreboardGameTypeOptions::Race { time_limit } => (
                if let Some(time_limit) = time_limit {
                    format!("Time limit: {}", time_limit.to_race_string())
                } else {
                    "".to_string()
                },
                "".to_string(),
            ),
        }
    }

    match &scoreboard.game {
        ScoreboardGameType::SidedPlay {
            red_stages,
            blue_stages,
            red_side_name,
            blue_side_name,
            ignore_stage,
            ..
        } => {
            StripBuilder::new(ui)
                .size(Size::exact(10.0))
                .size(Size::remainder())
                .size(Size::remainder())
                .size(Size::exact(10.0))
                .horizontal(|mut strip| {
                    strip.empty();
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        res = ui.available_width();
                        let rounding = CornerRadius {
                            nw: 5,
                            ..Default::default()
                        };
                        let player_count: usize =
                            red_stages.values().map(|s| s.characters.len()).sum();
                        let mut players = red_stages
                            .iter()
                            .flat_map(|(stage_id, stage)| {
                                stage.characters.iter().map(move |c| {
                                    ((ignore_stage != stage_id).then_some(stage_id), c)
                                })
                            })
                            .peekable();

                        let (bottom_label_left, bottom_label_right) = match_ty_str(options.ty);
                        render_scoreboard_frame(
                            ui,
                            pipe,
                            ui_state,
                            full_ui_rect,
                            TopBarTypes::Red,
                            rounding,
                            character_infos,
                            &mut players,
                            player_count,
                            red_stages,
                            red_side_name,
                            &if let Some(stage) = own_character
                                .and_then(|c| c.stage_id)
                                .and_then(|stage_id| red_stages.get(&stage_id))
                            {
                                match stage.score {
                                    ScoreboardScoreType::Points(points) => format!("{points}"),
                                    ScoreboardScoreType::RaceFinishTime(duration) => {
                                        duration.to_race_string()
                                    }
                                    ScoreboardScoreType::None => "".into(),
                                }
                            } else {
                                Default::default()
                            },
                            &bottom_label_left,
                            &bottom_label_right,
                        );
                    });
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        let rounding = CornerRadius {
                            ne: 5,
                            ..Default::default()
                        };
                        let player_count: usize =
                            blue_stages.values().map(|s| s.characters.len()).sum();
                        let mut players = blue_stages
                            .iter()
                            .flat_map(|(stage_id, stage)| {
                                stage.characters.iter().map(move |c| {
                                    ((ignore_stage != stage_id).then_some(stage_id), c)
                                })
                            })
                            .peekable();
                        render_scoreboard_frame(
                            ui,
                            pipe,
                            ui_state,
                            full_ui_rect,
                            TopBarTypes::Blue,
                            rounding,
                            character_infos,
                            &mut players,
                            player_count,
                            blue_stages,
                            blue_side_name,
                            &if let Some(stage) = own_character
                                .and_then(|c| c.stage_id)
                                .and_then(|stage_id| blue_stages.get(&stage_id))
                            {
                                match stage.score {
                                    ScoreboardScoreType::Points(points) => format!("{points}"),
                                    ScoreboardScoreType::RaceFinishTime(duration) => {
                                        duration.to_race_string()
                                    }
                                    ScoreboardScoreType::None => "".into(),
                                }
                            } else {
                                Default::default()
                            },
                            &format!("Map: {}", options.map_name.as_str()),
                            "",
                        );
                    });
                    strip.empty();
                });
        }
        ScoreboardGameType::SoloPlay {
            stages,
            ignore_stage,
            ..
        } => {
            res = ui.available_width();
            let mut strip = StripBuilder::new(ui);

            let player_count: usize = stages.values().map(|s| s.characters.len()).sum();
            let split_count = if player_count > 16 { 2 } else { 1 };

            strip = strip.size(Size::exact(10.0));
            for _ in 0..split_count {
                strip = strip.size(Size::remainder());
            }
            strip = strip.size(Size::exact(10.0));
            strip.horizontal(|mut strip| {
                strip.empty();
                for i in 0..split_count {
                    let rounding = if i == 0 {
                        if split_count == 1 {
                            CornerRadius {
                                nw: 5,
                                ne: 5,
                                ..Default::default()
                            }
                        } else {
                            CornerRadius {
                                nw: 5,
                                ..Default::default()
                            }
                        }
                    } else {
                        CornerRadius {
                            ne: 5,
                            ..Default::default()
                        }
                    };

                    let (players, player_count): (
                        Box<dyn Iterator<Item = RenderPlayer<'_>>>,
                        usize,
                    ) = if split_count > 1 {
                        if i == 0 {
                            (
                                Box::new(
                                    stages
                                        .iter()
                                        .flat_map(|(stage_id, stage)| {
                                            stage.characters.iter().map(move |c| {
                                                ((ignore_stage != stage_id).then_some(stage_id), c)
                                            })
                                        })
                                        .take(player_count / 2),
                                ),
                                player_count / 2,
                            )
                        } else {
                            (
                                Box::new(
                                    stages
                                        .iter()
                                        .flat_map(|(stage_id, stage)| {
                                            stage.characters.iter().map(move |c| {
                                                ((ignore_stage != stage_id).then_some(stage_id), c)
                                            })
                                        })
                                        .skip(player_count / 2),
                                ),
                                player_count - player_count / 2,
                            )
                        }
                    } else {
                        (
                            Box::new(stages.iter().flat_map(|(stage_id, stage)| {
                                stage.characters.iter().map(move |c| {
                                    ((ignore_stage != stage_id).then_some(stage_id), c)
                                })
                            })),
                            player_count,
                        )
                    };
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;

                        let (bottom_label_left, bottom_label_right) = match_ty_str(options.ty);
                        render_scoreboard_frame(
                            ui,
                            pipe,
                            ui_state,
                            full_ui_rect,
                            TopBarTypes::Neutral,
                            rounding,
                            character_infos,
                            &mut players.peekable(),
                            player_count,
                            stages,
                            &format!("Map: {}", options.map_name.as_str(),),
                            "",
                            &bottom_label_left,
                            &bottom_label_right,
                        );
                    });
                }
                strip.empty();
            });
        }
    }
    res
}

#[instrument(level = "trace", skip_all)]
pub fn render_spectators(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    full_ui_rect: Rect,
) {
    let character_infos = pipe.user_data.character_infos;
    let scoreboard = &pipe.user_data.scoreboard;
    let spectator_players = match &scoreboard.game {
        ScoreboardGameType::SidedPlay {
            spectator_players, ..
        } => spectator_players,
        ScoreboardGameType::SoloPlay {
            spectator_players, ..
        } => spectator_players,
    };
    if spectator_players.is_empty() {
        return;
    }

    let rounding = CornerRadius {
        ..Default::default()
    };

    let player_count: usize = spectator_players.len();
    let mut players = spectator_players.iter().map(|c| (None, c)).peekable();
    render_scoreboard_frame(
        ui,
        pipe,
        ui_state,
        full_ui_rect,
        TopBarTypes::Spectator,
        rounding,
        character_infos,
        &mut players,
        player_count,
        &Default::default(),
        "Spectators",
        "",
        "",
        "",
    );
}
