use egui::{Frame, Grid};
use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
    utils::get_margin,
};

use crate::ingame_menu::user_data::UserData;

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    let res = Frame::default()
        .fill(bg_frame_color())
        .corner_radius(5.0)
        .inner_margin(get_margin(ui))
        .show(ui, |ui| {
            let game_info = pipe.user_data.game_server_info.game_info();
            Grid::new("server-info-grid").num_columns(2).show(ui, |ui| {
                ui.label("Map:");
                ui.label(&game_info.map_name);
            });
        });
    ui_state.add_blur_rect(res.response.rect, 5.0);
}
