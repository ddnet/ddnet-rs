use std::borrow::Borrow;

use egui::{vec2, Align2, Frame, ScrollArea, Vec2, Window};

use game_interface::types::render::character::TeeEye;
use math::math::vector::vec2;
use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
    utils::add_margins,
};

use crate::utils::render_tee_for_ui;

use super::user_data::{SpectatorSelectionEvent, UserData};

/// not required
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    ui.style_mut().animation_time = 0.0;
    ui.add_space(5.0);
    ui.set_clip_rect(ui.available_rect_before_wrap());

    let res = Window::new("")
        .resizable(false)
        .title_bar(false)
        .frame(Frame::default().fill(bg_frame_color()).corner_radius(5.0))
        .anchor(Align2::CENTER_CENTER, Vec2::new(0.0, 0.0))
        .fixed_size(vec2(300.0, 400.0))
        .show(ui.ctx(), |ui| {
            add_margins(ui, |ui| {
                ui.style_mut().visuals.clip_rect_margin = 6.0;
                ScrollArea::vertical().show(ui, |ui| {
                    if ui.button("Free view").clicked() {
                        pipe.user_data
                            .events
                            .push_back(SpectatorSelectionEvent::FreeView);
                    }
                    for (id, character) in pipe
                        .user_data
                        .character_infos
                        .iter()
                        .filter(|(_, char)| char.stage_id.is_some())
                    {
                        let mut render_rect = ui.available_rect_before_wrap();
                        render_rect.set_height(64.0);

                        render_tee_for_ui(
                            pipe.user_data.canvas_handle,
                            pipe.user_data.skin_container,
                            pipe.user_data.skin_renderer,
                            ui,
                            ui_state,
                            render_rect,
                            Some(ui.available_rect_before_wrap()),
                            character.info.skin.borrow(),
                            Some(&character.skin_info),
                            vec2::new(
                                render_rect.left_center().x + 32.0,
                                render_rect.left_center().y,
                            ),
                            64.0,
                            TeeEye::Normal,
                        );

                        if ui.button(character.info.name.as_str()).clicked() {
                            pipe.user_data
                                .events
                                .push_back(SpectatorSelectionEvent::Selected([*id].into()));
                        }
                    }
                });
            });
        });
    if let Some(res) = res {
        ui_state.add_blur_rect(res.response.rect, 5.0);
    }
}
