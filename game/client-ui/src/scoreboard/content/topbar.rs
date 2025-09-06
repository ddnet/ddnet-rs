use egui::{Color32, CornerRadius, Layout, RichText, epaint::RectShape};

use ui_base::utils::add_horizontal_margins;

pub enum TopBarTypes {
    Neutral,
    Red,
    Blue,
    Spectator,
}

/// can contain various information
/// depends on the modification
/// map name, team name, differently colored
/// current team score, best player time
/// spectator info etc.
pub fn render(
    ui: &mut egui::Ui,
    ty: TopBarTypes,
    rounding: CornerRadius,
    label: &str,
    label_opposite: &str,
) {
    ui.painter().add(RectShape::filled(
        ui.available_rect_before_wrap(),
        rounding,
        match ty {
            TopBarTypes::Neutral => Color32::DARK_GRAY,
            TopBarTypes::Red => Color32::from_rgba_unmultiplied(150, 20, 6, 255),
            TopBarTypes::Blue => Color32::from_rgba_unmultiplied(18, 43, 136, 255),
            TopBarTypes::Spectator => Color32::from_rgb(80, 30, 100),
        },
    ));
    const FONT_SIZE: f32 = 18.0;
    match ty {
        TopBarTypes::Neutral | TopBarTypes::Red | TopBarTypes::Spectator => {
            ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
                add_horizontal_margins(ui, |ui| {
                    ui.label(RichText::new(label).size(FONT_SIZE).color(Color32::WHITE));

                    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(label_opposite)
                                .size(FONT_SIZE)
                                .color(Color32::WHITE),
                        );
                    });
                });
            });
        }
        TopBarTypes::Blue => {
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                add_horizontal_margins(ui, |ui| {
                    ui.label(RichText::new(label).size(FONT_SIZE).color(Color32::WHITE));

                    ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(label_opposite)
                                .size(FONT_SIZE)
                                .color(Color32::WHITE),
                        );
                    });
                });
            });
        }
    }
}
