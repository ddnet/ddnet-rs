use egui::{epaint::Shadow, Color32, Stroke};

/// Since egui's frame's inner margin makes trouble,
/// use this instead.
pub const MARGIN: f32 = 5.0;
pub const TEE_SIZE: f32 = 25.0;
pub const MARGIN_FROM_TEE: f32 = 5.0;
pub fn entry_frame(ui: &mut egui::Ui, stroke: Stroke, f: impl FnOnce(&mut egui::Ui)) {
    let color_frame = Color32::from_rgba_unmultiplied(0, 0, 0, 15);

    let style = ui.style();
    egui::Frame::default()
        .fill(color_frame)
        .stroke(stroke)
        .corner_radius(5.0)
        .shadow(Shadow {
            color: style.visuals.window_shadow.color,
            spread: (style.spacing.item_spacing.y / 2.0) as u8,
            blur: 5,
            ..Default::default()
        })
        .show(ui, f);
}
