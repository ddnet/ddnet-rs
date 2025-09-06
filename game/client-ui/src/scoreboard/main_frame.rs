use egui::{Pos2, Rect, Vec2};

use ui_base::types::{UiRenderPipe, UiState};

use super::user_data::UserData;

/// big square, rounded edges
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    let max_width = 1750.0;

    // max possible rect
    let available_rect = ui.available_rect_before_wrap();
    let full_width = available_rect.width();
    let full_height = available_rect.height();

    let allowed_width_no_spec = full_width.clamp(0.0, max_width); // clamp to something sane

    // scoreboard get's 2 / 3 of the space minus some space for the score, a win message etc.
    let offset_y = 50.0;
    let allowed_height_no_spec = (full_height * 2.0 / 3.0) - offset_y;

    // normal centering
    let offset_x = (full_width - allowed_width_no_spec) / 2.0;

    let no_spec_rect = Rect::from_min_size(
        Pos2::new(offset_x, offset_y),
        Vec2::new(allowed_width_no_spec, allowed_height_no_spec),
    );

    let players_res = ui.scope_builder(egui::UiBuilder::new().max_rect(no_spec_rect), |ui| {
        super::content::main_frame::render_players(ui, pipe, ui_state, available_rect)
    });

    let spacing_x = ui.style().spacing.item_spacing.x;
    let extra_offset_x = 10.0;
    let offset_x = full_width / 2.0 + spacing_x / 2.0;
    let allowed_width_spec = full_width / 2.0 - extra_offset_x - spacing_x;

    let allowed_width_spec = allowed_width_spec.clamp(0.0, players_res.inner); // clamp to size of other boards

    let extra_offset_y = 10.0;
    let offset_y = full_height * 2.0 / 3.0 + extra_offset_y;
    let allowed_height_spec = full_height * 1.0 / 3.0 - extra_offset_y * 2.0;

    let spec_rect = Rect::from_min_size(
        Pos2::new(offset_x, offset_y),
        Vec2::new(allowed_width_spec, allowed_height_spec),
    );

    ui.scope_builder(egui::UiBuilder::new().max_rect(spec_rect), |ui| {
        super::content::main_frame::render_spectators(ui, pipe, ui_state, available_rect)
    });
}
