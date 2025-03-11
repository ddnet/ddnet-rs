use egui::{vec2, Align2, Frame, Vec2, Window};

use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
    utils::add_margins,
};

use super::user_data::UserData;

/// not required
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    ui.style_mut().animation_time = 0.0;
    ui.set_clip_rect(ui.available_rect_before_wrap());

    let res = Window::new("")
        .resizable(false)
        .title_bar(false)
        .frame(Frame::default().fill(bg_frame_color()).corner_radius(5.0))
        .anchor(Align2::CENTER_CENTER, Vec2::new(0.0, 0.0))
        .fixed_size(vec2(300.0, 400.0))
        .show(ui.ctx(), |ui| {
            ui.style_mut().spacing.item_spacing.y = 0.0;
            add_margins(ui, |ui| {
                let mut cache = egui_commonmark::CommonMarkCache::default();
                egui_commonmark::CommonMarkViewer::new().show(ui, &mut cache, pipe.user_data.msg);
            });
        });
    if let Some(res) = res {
        ui_state.add_blur_rect(res.response.rect, 5.0);
    }
}
