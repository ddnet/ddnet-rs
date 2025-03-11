use egui::{Frame, Layout};
use egui_extras::{Size, StripBuilder};

use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
};

use crate::main_menu::user_data::UserData;

/// big box, rounded edges
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    let res = Frame::default()
        .fill(bg_frame_color())
        .corner_radius(5.0)
        .show(ui, |ui| {
            let item_spacing = ui.style().spacing.item_spacing.x;
            StripBuilder::new(ui)
                .size(Size::exact(0.0))
                .size(Size::remainder())
                .size(Size::exact(0.0))
                .horizontal(|mut strip| {
                    strip.empty();
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        StripBuilder::new(ui)
                            .size(Size::exact(item_spacing))
                            .size(Size::exact(15.0))
                            .size(Size::exact(item_spacing))
                            .size(Size::remainder())
                            .size(Size::exact(item_spacing))
                            .vertical(|mut strip| {
                                strip.empty();
                                strip.cell(|ui| {
                                    ui.style_mut().wrap_mode = None;
                                    ui.with_layout(
                                        Layout::left_to_right(egui::Align::Center)
                                            .with_main_align(egui::Align::Center)
                                            .with_main_justify(true),
                                        |ui| {
                                            ui.label("\u{e533} Friends & Favorites");
                                        },
                                    );
                                });
                                strip.empty();
                                strip.cell(|ui| {
                                    ui.style_mut().wrap_mode = None;
                                    super::table::render(ui, pipe, ui_state);
                                });
                                strip.empty();
                            });
                    });
                    strip.empty();
                });
        });
    ui_state.add_blur_rect(res.response.rect, 5.0);
}
