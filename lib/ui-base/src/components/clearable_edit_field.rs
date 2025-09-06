use egui::{Button, CornerRadius, FontId, Layout, Margin, Response, Stroke, TextEdit, vec2};
use egui_extras::{Size, StripBuilder};

pub fn clearable_edit_field(
    ui: &mut egui::Ui,
    text: &mut String,
    input_at_most_size: Option<f32>,
    max_chars: Option<usize>,
) -> Option<Response> {
    let style = ui.style_mut();
    let rounding = &style.visuals.widgets.inactive.corner_radius;
    let rounding = rounding.ne.max(rounding.nw);
    style.spacing.item_spacing = vec2(0.0, 0.0);
    let mut res = None;
    ui.horizontal(|ui| {
        StripBuilder::new(ui)
            .size(if let Some(input_at_most_size) = input_at_most_size {
                Size::remainder().at_most(input_at_most_size)
            } else {
                Size::remainder()
            })
            .size(Size::exact(20.0))
            .clip(true)
            .horizontal(|mut strip| {
                strip.cell(|ui| {
                    ui.style_mut().wrap_mode = None;
                    res = Some(
                        ui.with_layout(
                            Layout::left_to_right(egui::Align::Center)
                                .with_main_justify(true)
                                .with_cross_justify(true),
                            |ui| {
                                let style = ui.style_mut();
                                style.visuals.selection.stroke = Stroke::NONE;
                                let widgets = &mut style.visuals.widgets;
                                widgets.inactive.corner_radius = CornerRadius {
                                    nw: rounding,
                                    sw: rounding,
                                    ..Default::default()
                                };
                                widgets.inactive.expansion = 0.0;
                                widgets.inactive.bg_stroke = Stroke::NONE;
                                widgets.active.corner_radius = widgets.inactive.corner_radius;
                                widgets.active.expansion = widgets.inactive.expansion;
                                widgets.active.bg_stroke = widgets.inactive.bg_stroke;
                                widgets.hovered.corner_radius = widgets.inactive.corner_radius;
                                widgets.hovered.expansion = widgets.inactive.expansion;
                                widgets.hovered.bg_stroke = widgets.inactive.bg_stroke;
                                widgets.noninteractive.corner_radius =
                                    widgets.inactive.corner_radius;
                                widgets.noninteractive.expansion = widgets.inactive.expansion;
                                widgets.noninteractive.bg_stroke = widgets.inactive.bg_stroke;
                                widgets.open.corner_radius = widgets.inactive.corner_radius;
                                widgets.open.expansion = widgets.inactive.expansion;
                                widgets.open.bg_stroke = widgets.inactive.bg_stroke;
                                ui.add(
                                    TextEdit::singleline(text)
                                        .margin(Margin {
                                            left: 3,
                                            right: 3,
                                            top: 3,
                                            ..Margin::ZERO
                                        })
                                        .font(FontId::proportional(10.0))
                                        .char_limit(max_chars.unwrap_or(usize::MAX).max(1)),
                                )
                            },
                        )
                        .inner,
                    );
                });
                strip.cell(|ui| {
                    ui.style_mut().wrap_mode = None;
                    let style = ui.style_mut();
                    let widgets = &mut style.visuals.widgets;
                    widgets.inactive.corner_radius = CornerRadius {
                        ne: rounding,
                        se: rounding,
                        ..Default::default()
                    };
                    widgets.active.corner_radius = widgets.inactive.corner_radius;
                    widgets.active.expansion = widgets.inactive.expansion;
                    widgets.hovered.corner_radius = widgets.inactive.corner_radius;
                    widgets.hovered.expansion = widgets.inactive.expansion;
                    widgets.noninteractive.corner_radius = widgets.inactive.corner_radius;
                    widgets.noninteractive.expansion = widgets.inactive.expansion;
                    widgets.open.corner_radius = widgets.inactive.corner_radius;
                    widgets.noninteractive.expansion = widgets.inactive.expansion;
                    if ui
                        .add(Button::new("\u{f00d}").stroke(Stroke::NONE))
                        .clicked()
                    {
                        text.clear();
                        if let Some(res) = &mut res {
                            res.mark_changed();
                        }
                    }
                });
            });
    });
    res
}
