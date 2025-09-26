use std::borrow::Borrow;

use client_types::chat::ChatMsg;
use egui::{Align, Color32, FontId, Layout, Stroke, Vec2, text::LayoutJob};
use game_base::network::types::chat::NetChatMsgPlayerChannel;
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
    msg: &ChatMsg,
) {
    let (stroke, to) = match &msg.channel {
        NetChatMsgPlayerChannel::Global => (Stroke::NONE, None),
        NetChatMsgPlayerChannel::GameTeam => (Stroke::new(2.0, Color32::LIGHT_GREEN), None),
        NetChatMsgPlayerChannel::Whisper(to) => (Stroke::new(2.0, Color32::RED), Some(to)),
    };
    entry_frame(ui, stroke, |ui| {
        ui.add_space(MARGIN);
        let response = ui.horizontal(|ui| {
            ui.add_space(MARGIN);
            ui.add_space(TEE_SIZE + MARGIN_FROM_TEE);
            ui.style_mut().spacing.item_spacing.x = 4.0;
            ui.style_mut().spacing.item_spacing.y = 0.0;
            ui.with_layout(Layout::bottom_up(egui::Align::Min), |ui| {
                ui.add_space(2.0);
                let text_format = egui::TextFormat {
                    color: Color32::WHITE,
                    ..Default::default()
                };
                let job = LayoutJob::single_section(msg.msg.clone(), text_format);
                ui.label(job);
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), 14.0),
                    Layout::left_to_right(Align::Max),
                    |ui| {
                        let text_format = egui::TextFormat {
                            line_height: Some(14.0),
                            font_id: FontId::proportional(12.0),
                            valign: Align::BOTTOM,
                            color: Color32::WHITE,
                            ..Default::default()
                        };
                        let mut job = LayoutJob::single_section(msg.player.clone(), text_format);
                        let text_format_clan = egui::TextFormat {
                            line_height: Some(12.0),
                            font_id: FontId::proportional(10.0),
                            valign: Align::BOTTOM,
                            color: Color32::LIGHT_GRAY,
                            ..Default::default()
                        };
                        job.append(&msg.clan, 4.0, text_format_clan);
                        ui.label(job);

                        if let Some(to) = to {
                            ui.colored_label(Color32::WHITE, "to");
                            ui.colored_label(Color32::WHITE, to.name.as_str());

                            let rect = ui.available_rect_before_wrap();

                            const TEE_SIZE_MINI: f32 = 12.0;
                            ui.add_space(TEE_SIZE_MINI);

                            render_tee_for_ui(
                                pipe.user_data.canvas_handle,
                                pipe.user_data.skin_container,
                                pipe.user_data.render_tee,
                                ui,
                                ui_state,
                                ui.ctx().screen_rect(),
                                Some(ui.clip_rect()),
                                to.skin.borrow(),
                                Some(&to.skin_info),
                                vec2::new(rect.min.x + TEE_SIZE_MINI / 2.0, rect.left_center().y),
                                TEE_SIZE_MINI,
                                TeeEye::Normal,
                            );
                        }
                    },
                );
                ui.add_space(2.0);
            });
            ui.add_space(ui.available_width().min(4.0));
            ui.add_space(MARGIN);
        });
        ui.add_space(MARGIN);

        let rect = response.response.rect;

        render_tee_for_ui(
            pipe.user_data.canvas_handle,
            pipe.user_data.skin_container,
            pipe.user_data.render_tee,
            ui,
            ui_state,
            ui.ctx().screen_rect(),
            Some(ui.clip_rect()),
            &msg.skin_name,
            Some(&msg.skin_info),
            vec2::new(
                rect.min.x + MARGIN + TEE_SIZE / 2.0,
                rect.min.y + TEE_SIZE / 2.0 + 5.0,
            ),
            TEE_SIZE,
            TeeEye::Normal,
        );
    });
}
