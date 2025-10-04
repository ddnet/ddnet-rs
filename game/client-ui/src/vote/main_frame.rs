use std::time::Duration;

use client_containers::container::ContainerKey;
use egui::{
    Align2, Color32, CornerRadius, FontId, Frame, Grid, Rect, RichText, Shadow, Stroke, UiBuilder,
    pos2, vec2,
};
use game_interface::{types::render::character::TeeEye, votes::Voted};
use math::math::vector::vec2;
use tracing::instrument;
use ui_base::{
    types::{UiRenderPipe, UiState},
    utils::get_margin,
};

use crate::{
    ingame_menu::call_vote::map::stars_text,
    utils::{render_tee_for_ui, render_texture_for_ui},
    vote::user_data::VoteRenderData,
};

use super::user_data::{UserData, VoteRenderType};

/// not required
#[instrument(level = "trace", skip_all)]
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    let full_rect = ui.ctx().screen_rect();
    let mut rect = ui.ctx().screen_rect();

    // 15% + some etra offset for the hud
    let x_offset = 10.0;
    let y_offset = 15.0 * rect.height() / 100.0 + 50.0;

    let max_width = 300.0;

    rect.set_left(x_offset);
    rect.set_top(y_offset);
    rect.set_width(max_width);

    ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
        let vote_rect = ui.available_rect_before_wrap();
        let style = ui.style();
        Frame::group(style)
            .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 15))
            .stroke(Stroke::NONE)
            .shadow(Shadow {
                color: style.visuals.window_shadow.color,
                spread: (style.spacing.item_spacing.y / 2.0) as u8,
                blur: 5,
                ..Default::default()
            })
            .corner_radius(5.0)
            .inner_margin(get_margin(ui))
            .show(ui, |ui| {
                ui.set_min_width(vote_rect.width());
                ui.set_width(vote_rect.width());
                let vote = &pipe.user_data.vote_data;

                fn render_header(ui: &mut egui::Ui, text: &str, remaining_time: &Duration) {
                    const HEADER_SIZE: f32 = 20.0;
                    let rect = ui.available_rect_before_wrap();
                    ui.painter().text(
                        rect.min,
                        Align2::LEFT_TOP,
                        text,
                        FontId::proportional(HEADER_SIZE),
                        Color32::WHITE,
                    );
                    let mut pos = rect.right_top();
                    pos.y += HEADER_SIZE / 4.0;
                    ui.painter().text(
                        pos,
                        Align2::RIGHT_TOP,
                        format!("Ends in: {:.2}s", remaining_time.as_secs_f32()),
                        FontId::proportional(HEADER_SIZE / 2.0),
                        Color32::WHITE,
                    );
                    ui.add_space(HEADER_SIZE);
                }

                fn render_footer(ui: &mut egui::Ui, vote: &VoteRenderData, vote_rect: &Rect) {
                    // extra margin
                    ui.add_space(5.0);

                    const VOTE_BAR_HEIGHT: f32 = 15.0;

                    let max = vote.data.allowed_to_vote_count.max(1);
                    let yes_perc = vote.data.yes_votes.clamp(0, max) as f32 / max as f32;
                    let no_perc = vote.data.no_votes.clamp(0, max) as f32 / max as f32;

                    let ui_rect = ui.available_rect_before_wrap();
                    let result_y = ui_rect.center_top().y;
                    let rect = Rect::from_center_size(
                        ui_rect.center_top(),
                        vec2(vote_rect.width(), VOTE_BAR_HEIGHT),
                    );
                    ui.painter().rect_filled(
                        rect,
                        CornerRadius::same(5),
                        Color32::from_black_alpha(50),
                    );

                    if no_perc > 0.0 {
                        // no
                        let no_size = vote_rect.width() * no_perc;
                        let mut at = ui_rect.right_top();
                        at.x -= no_size / 2.0;
                        at.y = result_y;
                        let rect = Rect::from_center_size(at, vec2(no_size, VOTE_BAR_HEIGHT));
                        ui.painter().rect_filled(
                            rect,
                            CornerRadius {
                                ne: 5,
                                se: 5,
                                ..Default::default()
                            },
                            Color32::RED,
                        );
                        at = rect.left_center();
                        let galley = ui.painter().layout_no_wrap(
                            format!("{:.1}%", no_perc * 100.0),
                            FontId::default(),
                            Color32::WHITE,
                        );
                        let size = galley.size();
                        ui.painter().galley(
                            if size.x + 10.0 < rect.width() {
                                at + egui::vec2(5.0, -size.y / 2.0)
                            } else {
                                rect.right_center()
                                    - egui::vec2(size.x, size.y / 2.0)
                                    - egui::vec2(5.0, 0.0)
                            },
                            galley,
                            Color32::WHITE,
                        );
                    }

                    if yes_perc > 0.0 {
                        // yes
                        let yes_size = vote_rect.width() * yes_perc;
                        let mut at = ui_rect.left_top();
                        at.x += yes_size / 2.0;
                        at.y = result_y;
                        let rect = Rect::from_center_size(at, vec2(yes_size, VOTE_BAR_HEIGHT));
                        ui.painter().rect_filled(
                            rect,
                            CornerRadius {
                                nw: 5,
                                sw: 5,
                                ..Default::default()
                            },
                            Color32::GREEN,
                        );
                        at = rect.right_center();
                        let galley = ui.painter().layout_no_wrap(
                            format!("{:.1}%", yes_perc * 100.0),
                            FontId::default(),
                            Color32::DARK_GRAY,
                        );
                        let size = galley.size();
                        ui.painter().galley(
                            if size.x + 10.0 < rect.width() {
                                at - egui::vec2(size.x, size.y / 2.0) - egui::vec2(5.0, 0.0)
                            } else {
                                rect.left_center() + egui::vec2(5.0, -size.y / 2.0)
                            },
                            galley,
                            Color32::DARK_GRAY,
                        );
                    }

                    ui.add_space(VOTE_BAR_HEIGHT);

                    let rect = ui.available_rect_before_wrap();
                    ui.painter().text(
                        rect.left_top(),
                        Align2::LEFT_TOP,
                        "f3 - vote yes",
                        FontId::default(),
                        if matches!(vote.voted, Some(Voted::Yes)) {
                            Color32::LIGHT_GREEN
                        } else {
                            Color32::from_rgb(240, 255, 240)
                        },
                    );
                    ui.painter().text(
                        rect.right_top(),
                        Align2::RIGHT_TOP,
                        "f4 - vote no",
                        FontId::default(),
                        if matches!(vote.voted, Some(Voted::No)) {
                            Color32::LIGHT_RED
                        } else {
                            Color32::from_rgb(255, 240, 240)
                        },
                    );
                    ui.add_space(14.0);
                }

                let content_size: f32 = 90.0
                    + if pipe.user_data.player_vote_miniscreen {
                        100.0
                    } else {
                        0.0
                    };

                const FONT_SIZE: f32 = 14.0;
                match vote.ty {
                    VoteRenderType::Map { key, map } => {
                        render_header(ui, "Map vote", vote.remaining_time);

                        ui.add_space(8.0);
                        Grid::new("map-vote-grid").num_columns(2).show(ui, |ui| {
                            ui.label(
                                RichText::new("Category:")
                                    .font(FontId::proportional(FONT_SIZE))
                                    .color(Color32::WHITE),
                            );
                            ui.label(
                                RichText::new(key.category.as_str())
                                    .font(FontId::proportional(FONT_SIZE))
                                    .color(Color32::WHITE),
                            );
                            ui.end_row();

                            ui.label(
                                RichText::new("Map:")
                                    .font(FontId::proportional(FONT_SIZE))
                                    .color(Color32::WHITE),
                            );
                            ui.label(
                                RichText::new(key.map.name.as_str())
                                    .font(FontId::proportional(FONT_SIZE))
                                    .color(Color32::WHITE),
                            );
                            ui.end_row();
                        });
                        ui.add_space(8.0);

                        let thumbnails = &mut *pipe.user_data.map_vote_thumbnail_container;
                        if let Some(hash) = &map.thumbnail_resource {
                            let key = ContainerKey {
                                name: "map".try_into().unwrap(),
                                hash: Some(*hash),
                            };
                            let thumbnail_loaded = thumbnails.contains_key(&key);
                            let thumbnail = thumbnails.get_or_default(&key);
                            if thumbnail_loaded {
                                // add map thumbnail preview
                                let mut rect = ui.available_rect_before_wrap();
                                rect.set_height(180.0);
                                ui.add_space(rect.height());

                                let width = thumbnail.width as f32;
                                let height = thumbnail.height as f32;
                                let draw_width = rect.width();
                                let draw_height = rect.height();
                                let w_scale = draw_width / width;
                                let h_scale = draw_height / height;
                                let scale = w_scale.min(h_scale).min(1.0);
                                let center = rect.center();
                                render_texture_for_ui(
                                    pipe.user_data.stream_handle,
                                    pipe.user_data.canvas_handle,
                                    &thumbnail.thumbnail,
                                    ui,
                                    ui_state,
                                    ui.ctx().screen_rect(),
                                    Some(ui.clip_rect()),
                                    vec2::new(center.x, center.y),
                                    vec2::new(width * scale, height * scale),
                                    None,
                                );

                                ui.painter().rect_stroke(
                                    Rect::from_center_size(
                                        center,
                                        egui::vec2(width * scale, height * scale),
                                    ),
                                    0,
                                    Stroke::new(2.0, Color32::GRAY),
                                    egui::StrokeKind::Inside,
                                );

                                ui.add_space(8.0);
                            }
                        }

                        render_footer(ui, vote, &vote_rect);
                    }
                    VoteRenderType::RandomUnfinishedMap { key } => {
                        render_header(ui, "Random unfinished map vote", vote.remaining_time);
                        ui.add_space(8.0);

                        Grid::new("random-unfinished-map-vote-grid")
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("Category:")
                                        .font(FontId::proportional(FONT_SIZE))
                                        .color(Color32::WHITE),
                                );
                                ui.label(
                                    RichText::new(key.category.as_str())
                                        .font(FontId::proportional(FONT_SIZE))
                                        .color(Color32::WHITE),
                                );
                                ui.end_row();

                                if let Some(difficulty) = key.difficulty {
                                    ui.label(
                                        RichText::new("Difficulty:")
                                            .font(FontId::proportional(FONT_SIZE))
                                            .color(Color32::WHITE),
                                    );
                                    ui.label(
                                        RichText::new(stars_text(difficulty))
                                            .font(FontId::proportional(FONT_SIZE))
                                            .color(Color32::WHITE),
                                    );
                                    ui.end_row();
                                }
                            });

                        ui.add_space(8.0);
                        render_footer(ui, vote, &vote_rect);
                    }
                    VoteRenderType::PlayerVoteSpec(player)
                    | VoteRenderType::PlayerVoteKick(player) => {
                        let is_kick = matches!(vote.ty, VoteRenderType::PlayerVoteKick(_));

                        render_header(
                            ui,
                            &format!("{} vote", if is_kick { "Kick" } else { "Spec" }),
                            vote.remaining_time,
                        );

                        let player_vote_miniscreen = pipe.user_data.player_vote_miniscreen;

                        let mut rect = ui.available_rect_before_wrap();
                        if player_vote_miniscreen {
                            let spacing_y = ui.style().spacing.item_spacing.y;
                            ui.style_mut().spacing.item_spacing.y = 0.0;
                            let label_rect = ui
                                .vertical_centered(|ui| {
                                    ui.add_space(5.0);
                                    ui.label(
                                        RichText::new(player.name)
                                            .font(FontId::proportional(20.0))
                                            .color(Color32::WHITE),
                                    )
                                })
                                .response
                                .rect;
                            let miniscreen_height = content_size
                                - (label_rect.height() + ui.style().spacing.item_spacing.y);
                            rect = ui.available_rect_before_wrap();
                            rect.set_height(miniscreen_height);
                            rect = rect.expand2(egui::vec2(0.0, -10.0));

                            ui.painter().rect_stroke(
                                rect,
                                0.0,
                                Stroke::new(2.0, Color32::GRAY),
                                egui::StrokeKind::Inside,
                            );

                            ui.add_space(miniscreen_height);
                            ui.style_mut().spacing.item_spacing.y = spacing_y;
                        } else {
                            render_tee_for_ui(
                                pipe.user_data.canvas_handle,
                                pipe.user_data.skin_container,
                                pipe.user_data.render_tee,
                                ui,
                                ui_state,
                                full_rect,
                                None,
                                player.skin,
                                Some(player.skin_info),
                                vec2::new(
                                    rect.min.x + content_size / 2.0,
                                    rect.min.y + content_size / 2.0,
                                ),
                                content_size,
                                TeeEye::Blink,
                            );
                            ui.painter().text(
                                pos2(rect.min.x + content_size, rect.min.y + content_size / 2.0),
                                Align2::LEFT_CENTER,
                                player.name,
                                FontId::proportional(22.0),
                                Color32::WHITE,
                            );
                            ui.add_space(content_size);
                        }

                        {
                            let mut player_rect = rect;
                            let ppp = ui.ctx().pixels_per_point();
                            player_rect.min.x *= ppp;
                            player_rect.min.y *= ppp;
                            player_rect.max.x *= ppp;
                            player_rect.max.y *= ppp;
                            *pipe.user_data.player_vote_rect = Some(player_rect);
                        }

                        ui.colored_label(Color32::WHITE, format!("Reason: {}", player.reason));

                        render_footer(ui, vote, &vote_rect);
                    }
                    VoteRenderType::Misc { key, .. } => {
                        render_header(ui, &key.vote_key.display_name, vote.remaining_time);

                        if !key.vote_key.description.is_empty() {
                            ui.add_space(10.0);
                            ui.label(key.vote_key.description.as_str());
                        }

                        ui.add_space(10.0);

                        render_footer(ui, vote, &vote_rect);
                    }
                }
            });
    });
}
