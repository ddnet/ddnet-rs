use egui::{Color32, Layout, RichText, epaint::RectShape};

use tracing::instrument;
use ui_base::utils::add_horizontal_margins;

/// can contain various information
/// depends on the modification
/// map name, scorelimit, round
#[instrument(level = "trace", skip_all)]
pub fn render(ui: &mut egui::Ui, bottom_labels: (&str, &str)) {
    ui.painter().add(RectShape::filled(
        ui.available_rect_before_wrap(),
        0.0,
        Color32::from_rgba_unmultiplied(70, 70, 70, 255),
    ));
    const FONT_SIZE: f32 = 10.0;
    add_horizontal_margins(ui, |ui| {
        let (left_label, right_label) = bottom_labels;
        ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
            ui.label(RichText::new(left_label).size(FONT_SIZE));
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(right_label).size(FONT_SIZE));
            });
        });
    });
}
