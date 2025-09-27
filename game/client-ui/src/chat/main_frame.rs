use egui::{Pos2, Rect, UiBuilder, Vec2};

use tracing::instrument;
use ui_base::{
    types::{UiRenderPipe, UiState},
    utils::{add_margins, get_margin},
};

use super::user_data::UserData;

/// not required
#[instrument(level = "trace", skip_all)]
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    let margin = (15.0 - get_margin(ui)).max(0.0);
    let x_offset = margin;

    let width = (ui.available_width() / 2.0).min(500.0) - x_offset;
    let (y_offset, height) = if !pipe.user_data.show_chat_history {
        (
            // chat renders in the lower 1/3 of the ui height
            ui.available_height() * 2.0 / 3.0,
            (ui.available_height() * 1.0 / 3.0) - margin,
        )
    } else {
        (
            // chat renders in the lower 2/3 of the ui height
            ui.available_height() * 1.0 / 3.0,
            (ui.available_height() * 2.0 / 3.0) - margin,
        )
    };

    let render_rect = Rect::from_min_size(Pos2::new(x_offset, y_offset), Vec2::new(width, height));

    ui.scope_builder(UiBuilder::new().max_rect(render_rect), |ui| {
        ui.set_clip_rect(ui.available_rect_before_wrap());
        add_margins(ui, |ui| super::chat_list::render(ui, pipe, ui_state));
    });
}
