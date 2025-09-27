use crate::main_menu::user_data::UserData;
use egui::Frame;
use egui_extras::{Size, StripBuilder};
use tracing::instrument;
use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
    utils::add_margins,
};

#[instrument(level = "trace", skip_all)]
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    let w = ui.available_width();
    let max_width = 800.0;
    let width = w.clamp(100.0, max_width);
    StripBuilder::new(ui)
        .size(Size::remainder())
        .size(Size::exact(width))
        .size(Size::remainder())
        .horizontal(|mut strip| {
            strip.empty();
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                let res = Frame::default()
                    .fill(bg_frame_color())
                    .corner_radius(5.0)
                    .show(ui, |ui| {
                        add_margins(ui, |ui| {
                            ui.style_mut().wrap_mode = None;

                            super::list::community_list(ui, pipe, ui_state);
                        });
                    });
                ui_state.add_blur_rect(res.response.rect, 5.0);
            });
            strip.empty();
        });
}
