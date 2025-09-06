use egui::{Pos2, Rect, UiBuilder, Vec2};

use ui_base::{
    types::{UiRenderPipe, UiState},
    utils::{add_margins, get_margin},
};

use super::user_data::UserData;

/// not required
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    // last 1/2 is for action feed
    let x_offset = ui.available_width() * 1.0 / 2.0;

    let margin = (15.0 - get_margin(ui)).max(0.0);
    let y_offset = margin;

    let width = (ui.available_width() * 1.0 / 2.0) - margin;
    let height = (ui.available_height() / 2.0) - y_offset;

    let render_rect = Rect::from_min_size(Pos2::new(x_offset, y_offset), Vec2::new(width, height));

    let full_rect = ui.available_rect_before_wrap();

    ui.scope_builder(UiBuilder::new().max_rect(render_rect), |ui| {
        ui.set_clip_rect(ui.available_rect_before_wrap());
        add_margins(ui, |ui| {
            super::feed_list::render(ui, pipe, ui_state, &full_rect)
        });
    });
}
