use client_types::console::entries_to_parser;
use command_parser::parser;
use egui::{epaint::Shadow, Color32, Frame, Pos2, Rect, Stroke, Style, UiBuilder, Vec2};
use egui_extras::{Size, StripBuilder};

use ui_base::{
    style::default_style,
    types::{UiRenderPipe, UiState},
};

use super::user_data::UserData;

fn console_style() -> Style {
    let mut style = default_style();
    style.visuals.extreme_bg_color = Color32::TRANSPARENT;
    style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    //style.visuals.widgets.inactive.fg_stroke = Stroke::NONE;
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    //style.visuals.widgets.hovered.fg_stroke = Stroke::NONE;
    style.visuals.widgets.active.bg_stroke = Stroke::NONE;
    //style.visuals.widgets.active.fg_stroke = Stroke::NONE;
    style.visuals.widgets.open.bg_stroke = Stroke::NONE;
    //style.visuals.widgets.open.fg_stroke = Stroke::NONE;
    //style.visuals.selection.stroke = Stroke::NONE;
    style.override_text_style = Some(egui::TextStyle::Monospace);
    style
}

/// square, fills most of the screen
pub fn render(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    bg_color: Color32,
) {
    ui.set_style(console_style());
    let width = ui.available_width();
    let height = ui.available_height() * 2.0 / 3.0;

    let res = Frame::default()
        .fill(bg_color)
        .shadow(Shadow {
            blur: 10,
            spread: 5,
            color: ui.style().visuals.window_shadow.color,
            ..Default::default()
        })
        .show(ui, |ui| {
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                ui_state.is_ui_open = false;
            }

            ui.style_mut().spacing.item_spacing.y = 0.0;
            let mut has_text_selection = false;
            ui.scope_builder(
                UiBuilder::new().max_rect(Rect::from_min_size(
                    Pos2::new(0.0, 0.0),
                    Vec2::new(width, height),
                )),
                |ui| {
                    StripBuilder::new(ui)
                        .size(Size::exact(0.0))
                        .size(Size::remainder())
                        .size(Size::exact(50.0))
                        .size(Size::exact(15.0))
                        .size(Size::exact(25.0))
                        .size(Size::exact(0.0))
                        .vertical(|mut strip| {
                            strip.empty();
                            strip.cell(|ui| {
                                ui.style_mut().wrap_mode = None;
                                super::console_list::render(ui, pipe, &mut has_text_selection);
                            });

                            let msg = pipe.user_data.msg.clone();

                            let cmds = parser::parse(
                                &msg,
                                &entries_to_parser(pipe.user_data.entries),
                                pipe.user_data.cache,
                            );

                            strip.cell(|ui| {
                                ui.style_mut().wrap_mode = None;
                                super::suggestions::render(ui, ui_state, pipe, &cmds);
                            });
                            strip.cell(|ui| {
                                ui.style_mut().wrap_mode = None;
                                super::input::render(ui, pipe, ui_state, has_text_selection, &cmds);
                            });
                            strip.cell(|ui| {
                                ui.style_mut().wrap_mode = None;
                                super::input_err::render(ui, pipe.user_data.msg, &cmds);
                            });
                            strip.empty();
                        });
                },
            );
        });
    ui_state.add_blur_rect(res.response.rect, 0.0);
}
