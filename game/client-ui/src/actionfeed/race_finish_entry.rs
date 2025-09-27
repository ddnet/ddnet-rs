use std::time::Duration;

use base::duration_ext::DurationToRaceStr;
use client_types::actionfeed::ActionPlayer;
use egui::{Color32, Layout, Rect};
use game_interface::types::render::character::TeeEye;
use math::math::vector::vec2;
use tracing::instrument;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{actionfeed::shared::entry_frame, utils::render_tee_for_ui};

use super::user_data::{RenderTeeInfo, UserData};

/// one actionfeed entry
#[instrument(level = "trace", skip_all)]
pub fn render(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    players: &[ActionPlayer],
    display_str: &str,
    finish_time: &Duration,
    full_rect: &Rect,
) {
    entry_frame(ui, |ui| {
        let tee_size = 20.0;
        let margin_from_tee = 2.0;

        let render_tees = &mut *pipe.user_data.render_tee_helper;
        render_tees.clear();

        ui.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
            ui.style_mut().spacing.item_spacing.x = 4.0;
            ui.style_mut().spacing.item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.colored_label(Color32::WHITE, display_str);
                let display_tee_space = tee_size * 0.75;
                for player in players {
                    ui.add_space(display_tee_space + margin_from_tee);
                    let rect = ui.available_rect_before_wrap();
                    render_tees.push(RenderTeeInfo {
                        skin: player.skin.clone(),
                        skin_info: player.skin_info,
                        pos: vec2::new(
                            rect.max.x + display_tee_space / 2.0 + margin_from_tee / 2.0,
                            rect.min.y + rect.height() / 2.0,
                        ),
                    });
                }
                ui.add_space(display_tee_space * 0.25);
                ui.colored_label(Color32::WHITE, finish_time.to_race_string());
                ui.colored_label(Color32::WHITE, "\u{f11e}");
            });
        });

        for render_tee in render_tees {
            render_tee_for_ui(
                pipe.user_data.canvas_handle,
                pipe.user_data.skin_container,
                pipe.user_data.render_tee,
                ui,
                ui_state,
                *full_rect,
                Some(ui.clip_rect()),
                &render_tee.skin,
                Some(&render_tee.skin_info),
                render_tee.pos,
                tee_size,
                TeeEye::Normal,
            );
        }
    });
}
