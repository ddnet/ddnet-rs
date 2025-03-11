use egui::{epaint::Shadow, Color32, Stroke};

pub fn entry_frame(ui: &mut egui::Ui, f: impl FnOnce(&mut egui::Ui)) {
    let color_frame = Color32::from_rgba_unmultiplied(0, 0, 0, 15);

    let style = ui.style();
    egui::Frame::group(style)
        .fill(color_frame)
        .stroke(Stroke::NONE)
        .shadow(Shadow {
            color: style.visuals.window_shadow.color,
            spread: (style.spacing.item_spacing.y / 2.0) as u8,
            blur: 5,
            ..Default::default()
        })
        .show(ui, f);
}
