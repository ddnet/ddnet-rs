use std::iter::Peekable;

use base::{duration_ext::DurationToRaceStr, linked_hash_map_view::FxLinkedHashMap};
use egui::{Color32, Layout, Rect, RichText, Shape, layers::ShapeIdx};
use egui_extras::{Size, StripBuilder};

use game_interface::types::{
    id_types::{CharacterId, StageId},
    render::{
        character::{CharacterInfo, TeeEye},
        scoreboard::{ScoreboardCharacterInfo, ScoreboardConnectionType, ScoreboardScoreType},
    },
};
use math::math::vector::vec2;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    scoreboard::{
        content::list::definitions::{
            TABLE_CONTENT_COLUMN_SPACING, TABLE_CONTENT_FONT_SIZES, TABLE_CONTENT_TEE_SIZES,
            TABLE_CONTENT_WIDTH, TABLE_NAME_COLUMN_INDEX,
        },
        user_data::UserData,
    },
    utils::{render_flag_for_ui, render_tee_for_ui},
};

pub type RenderPlayer<'a> = (Option<&'a StageId>, &'a ScoreboardCharacterInfo);

#[derive(Debug)]
pub struct FrameRect {
    pub rects: Vec<Rect>,
    pub shape_id: ShapeIdx,
}

/// single player entry
pub fn render<'a>(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    character_infos: &FxLinkedHashMap<CharacterId, CharacterInfo>,
    players: &mut Peekable<impl Iterator<Item = RenderPlayer<'a>>>,
    full_ui_rect: &Rect,
    font_size_index: usize,
    spacing_y: f32,
    frame_rect: &mut FxLinkedHashMap<StageId, FrameRect>,
) {
    let Some((stage, player)) = players.next() else {
        return;
    };
    let Some(char) = character_infos.get(&player.id) else {
        return;
    };
    let rect = ui.available_rect_before_wrap();
    if let Some(&stage_id) = stage {
        frame_rect
            .entry(stage_id)
            .or_insert_with_keep_order(|| FrameRect {
                rects: Default::default(),
                shape_id: ui.painter().add(Shape::Noop),
            })
            .rects
            .push(rect);
    }

    let font_size = TABLE_CONTENT_FONT_SIZES[font_size_index];
    let tee_size = TABLE_CONTENT_TEE_SIZES[font_size_index];

    ui.add_space(spacing_y / 2.0);

    let mut width_left = ui.available_width();
    let spacing_x = TABLE_CONTENT_COLUMN_SPACING[font_size_index];
    let style = ui.style_mut();
    style.spacing.item_spacing.x = spacing_x;
    style.visuals.override_text_color = Some(Color32::WHITE);

    let mut strip = StripBuilder::new(ui);
    let mut col_count = 0;
    while width_left > 0.0 {
        if col_count < TABLE_CONTENT_WIDTH[font_size_index].len() {
            let col_width = TABLE_CONTENT_WIDTH[font_size_index][col_count];
            if width_left >= col_width {
                width_left -= col_width + spacing_x;
                if col_count == TABLE_NAME_COLUMN_INDEX {
                    strip = strip.size(Size::remainder());
                } else {
                    strip = strip.size(Size::exact(col_width));
                }
                col_count += 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    strip = strip.clip(true);
    strip.horizontal(|mut strip| {
        for i in 0..col_count {
            match i {
                0 => {
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(match player.score {
                                    ScoreboardScoreType::Points(points) => points.to_string(),
                                    ScoreboardScoreType::RaceFinishTime(duration) => {
                                        duration.to_race_string()
                                    }
                                    ScoreboardScoreType::None => "".into(),
                                })
                                .size(font_size),
                            );
                        });
                    });
                }
                1 => {
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        let this_rect = ui.available_rect_before_wrap();
                        render_tee_for_ui(
                            pipe.user_data.canvas_handle,
                            pipe.user_data.skin_container,
                            pipe.user_data.render_tee,
                            ui,
                            ui_state,
                            *full_ui_rect,
                            Some(ui.clip_rect()),
                            &char.info.skin.clone().into(),
                            Some(&char.skin_info),
                            vec2::new(
                                this_rect.min.x + this_rect.width() / 2.0,
                                this_rect.min.y + this_rect.height() / 2.0,
                            ),
                            tee_size,
                            TeeEye::Normal,
                        );
                    });
                }
                2 => {
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(RichText::new(char.info.name.as_str()).size(font_size));
                        });
                    });
                }
                3 => {
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(RichText::new(char.info.clan.as_str()).size(font_size));
                        });
                    });
                }
                4 => {
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        let rect = ui.available_rect_before_wrap();
                        let center = rect.center();
                        let user_data = &mut pipe.user_data;
                        let default_key = user_data.flags_container.default_key.clone();
                        render_flag_for_ui(
                            user_data.stream_handle,
                            user_data.canvas_handle,
                            user_data.flags_container,
                            ui,
                            ui_state,
                            *full_ui_rect,
                            Some(rect),
                            &default_key,
                            &char.info.flag.to_lowercase().replace("-", "_"),
                            vec2::new(center.x, center.y),
                            rect.width().min(rect.height() * 2.0).min(25.0),
                        );
                    });
                }
                _ => {
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
                            match &player.ping {
                                ScoreboardConnectionType::Network(stats) => {
                                    ui.label(
                                        RichText::new(stats.ping.as_millis().to_string())
                                            .size(font_size),
                                    );
                                }
                                ScoreboardConnectionType::Bot => {
                                    ui.label(RichText::new("BOT").size(font_size));
                                }
                            }
                        });
                    });
                }
            }
        }
    });
    ui.add_space(spacing_y / 2.0);
}
