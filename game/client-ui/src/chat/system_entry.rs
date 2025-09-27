use client_types::chat::MsgSystem;
use egui::{Align, Color32, FontId, Layout, RichText, Stroke, Vec2, text::LayoutJob};
use game_interface::types::render::character::TeeEye;
use math::math::vector::vec2;
use tracing::instrument;
use ui_base::types::{UiRenderPipe, UiState};

use crate::utils::render_tee_for_ui;

use super::{
    shared::{MARGIN, MARGIN_FROM_TEE, TEE_SIZE, entry_frame},
    user_data::UserData,
};

/// one chat entry
#[instrument(level = "trace", skip_all)]
pub fn render(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    msg: &MsgSystem,
) {
    entry_frame(ui, Stroke::NONE, |ui| {
        ui.add_space(MARGIN);
        let response = ui.horizontal(|ui| {
            ui.add_space(MARGIN);
            if msg.front_skin.is_some() {
                ui.add_space(TEE_SIZE + MARGIN_FROM_TEE);
            } else {
                ui.add_space(2.0);
            }
            ui.style_mut().spacing.item_spacing.x = 4.0;
            ui.style_mut().spacing.item_spacing.y = 0.0;
            ui.with_layout(Layout::bottom_up(egui::Align::Min), |ui| {
                let color = Color32::from_rgba_unmultiplied(255, 238, 0, 255);
                ui.add_space(2.0);
                ui.label(RichText::new(&msg.msg).color(color));
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), 12.0),
                    Layout::left_to_right(Align::Max),
                    |ui| {
                        let text_format = egui::TextFormat {
                            line_height: Some(12.0),
                            font_id: FontId::proportional(10.0),
                            valign: Align::BOTTOM,
                            color,
                            ..Default::default()
                        };
                        let job = LayoutJob::single_section("System".to_string(), text_format);
                        ui.label(job);
                    },
                );
                ui.add_space(2.0);
            });
            if msg.end_skin.is_some() {
                ui.add_space(TEE_SIZE + MARGIN_FROM_TEE);
            }
            ui.add_space(MARGIN);
        });
        ui.add_space(MARGIN);

        let rect = response.response.rect;

        if let Some(skin) = &msg.front_skin {
            render_tee_for_ui(
                pipe.user_data.canvas_handle,
                pipe.user_data.skin_container,
                pipe.user_data.render_tee,
                ui,
                ui_state,
                ui.ctx().screen_rect(),
                Some(ui.clip_rect()),
                &skin.skin_name,
                Some(&skin.skin_info),
                vec2::new(
                    rect.min.x + MARGIN + TEE_SIZE / 2.0,
                    rect.min.y + TEE_SIZE / 2.0 + 3.0,
                ),
                TEE_SIZE,
                TeeEye::Normal,
            );
        }
        if let Some(skin) = &msg.end_skin {
            render_tee_for_ui(
                pipe.user_data.canvas_handle,
                pipe.user_data.skin_container,
                pipe.user_data.render_tee,
                ui,
                ui_state,
                ui.ctx().screen_rect(),
                Some(ui.clip_rect()),
                &skin.skin_name,
                Some(&skin.skin_info),
                vec2::new(
                    rect.max.x - (MARGIN + TEE_SIZE / 2.0),
                    rect.min.y + TEE_SIZE / 2.0 + 3.0,
                ),
                TEE_SIZE,
                TeeEye::Normal,
            );
        }
    });
}
