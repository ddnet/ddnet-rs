use egui::{Color32, Stroke, Style, Visuals};

pub fn bg_frame_color() -> Color32 {
    Color32::from_black_alpha(100)
}

pub fn default_style() -> Style {
    let mut visuals = Visuals::dark();
    let clr = visuals.extreme_bg_color.to_srgba_unmultiplied();
    visuals.extreme_bg_color = Color32::from_rgba_unmultiplied(clr[0], clr[1], clr[2], 180);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(200, 200, 200));
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(200, 200, 200));
    visuals.clip_rect_margin = 0.0;
    visuals.slider_trailing_fill = true;
    Style {
        visuals,
        ..Default::default()
    }
}

pub fn topbar_buttons() -> Style {
    let mut style = default_style();
    style.visuals.widgets.inactive.corner_radius = 0.0.into();
    style.visuals.widgets.inactive.bg_fill = bg_frame_color();
    style.visuals.widgets.inactive.weak_bg_fill = bg_frame_color();

    style.visuals.widgets.hovered.corner_radius = 0.0.into();
    style.visuals.widgets.hovered.bg_fill = bg_frame_color();
    style.visuals.widgets.hovered.weak_bg_fill = bg_frame_color();
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;

    style.visuals.widgets.active.corner_radius = 0.0.into();
    style.visuals.widgets.active.bg_fill = bg_frame_color();
    style.visuals.widgets.active.weak_bg_fill = bg_frame_color();
    style.visuals.widgets.active.bg_stroke = Stroke::NONE;

    style.visuals.button_frame = false;

    style
}

pub fn topbar_secondary_buttons() -> Style {
    let mut style = default_style();
    style.visuals.widgets.inactive.corner_radius = 0.0.into();
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgba_unmultiplied(0, 0, 0, 75);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgba_unmultiplied(0, 0, 0, 75);
    style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;

    style.visuals.widgets.hovered.corner_radius = 0.0.into();
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgba_unmultiplied(100, 100, 100, 75);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgba_unmultiplied(0, 0, 0, 75);
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    style.visuals.widgets.hovered.expansion = 0.0;

    style.visuals.widgets.active.corner_radius = 0.0.into();
    style.visuals.widgets.active.bg_fill = Color32::from_rgba_unmultiplied(0, 0, 0, 75);
    style.visuals.widgets.active.weak_bg_fill = Color32::from_rgba_unmultiplied(0, 0, 0, 75);
    style.visuals.widgets.active.bg_stroke = Stroke::NONE;

    style
}
