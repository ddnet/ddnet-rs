use egui::{Button, Color32, CornerRadius, Frame, Layout, Rect, Shape};
use egui_extras::{Size, StripBuilder};

use tracing::instrument;
use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
    utils::{add_margins, add_vertical_margins},
};

use crate::main_menu::{constants::MENU_SETTINGS_NAME, user_data::UserData};

use super::constants::{SETTINGS_SUB_UI_PAGE_QUERY, SETTINGS_UI_PAGE_QUERY};

fn render_nav(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    cur_sub: &str,
    cur_subsub: &str,
) {
    ui.style_mut().spacing.item_spacing.x = 0.0;
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.with_layout(
            Layout::top_down(egui::Align::Min).with_cross_justify(true),
            |ui| {
                let mut add_btn = |ui: &mut egui::Ui, s: &str, submenu: Option<&str>| {
                    let selected = (submenu.is_none() && cur_sub == s && cur_subsub.is_empty())
                        || (submenu.is_some() && cur_subsub == s);
                    let bg_idx = ui.painter().add(Shape::Noop);
                    let bgsub_idx = ui.painter().add(Shape::Noop);
                    let style = ui.style_mut();
                    let y = style.spacing.interact_size.y;
                    style.spacing.interact_size.y = 22.0;
                    let entry = style
                        .text_styles
                        .entry(egui::TextStyle::Button)
                        .or_default();
                    let size = entry.size;
                    if submenu.is_none() {
                        entry.size = 16.0;
                    }
                    let btn = ui.add(Button::new(s).frame(false));
                    let style = ui.style_mut();
                    let entry = style
                        .text_styles
                        .entry(egui::TextStyle::Button)
                        .or_default();
                    entry.size = size;
                    style.spacing.interact_size.y = y;
                    if btn.clicked() {
                        let path = &mut pipe.user_data.config.engine.ui.path;
                        if let Some(parent) = submenu {
                            path.add_query((
                                SETTINGS_UI_PAGE_QUERY.to_string(),
                                parent.to_string(),
                            ));
                            path.add_query((SETTINGS_SUB_UI_PAGE_QUERY.to_string(), s.to_string()));
                        } else {
                            path.add_query((SETTINGS_UI_PAGE_QUERY.to_string(), s.to_string()));
                            path.add_query((
                                SETTINGS_SUB_UI_PAGE_QUERY.to_string(),
                                "".to_string(),
                            ));
                        }
                    }

                    if submenu.is_some() {
                        let btn_rect = btn
                            .rect
                            .expand2(egui::vec2(6.0, 0.0))
                            .translate(egui::vec2(-6.0, 0.0));

                        ui.painter().set(
                            bgsub_idx,
                            Shape::rect_filled(
                                btn_rect,
                                CornerRadius::default(),
                                Color32::from_black_alpha(50),
                            ),
                        );
                    }

                    if selected {
                        let mut offset = btn.rect.left_center();
                        if submenu.is_some() {
                            offset.x -= 2.0;
                        }
                        ui.painter().rect_filled(
                            Rect::from_center_size(offset, egui::vec2(4.0, btn.rect.height()))
                                .translate(egui::vec2(-8.0, 0.0)),
                            CornerRadius::default(),
                            Color32::LIGHT_BLUE,
                        );
                        let mut btn_rect = btn
                            .rect
                            .expand2(egui::vec2(3.0, 0.0))
                            .translate(egui::vec2(-3.0, 0.0));

                        if submenu.is_some() {
                            btn_rect = btn_rect
                                .expand2(egui::vec2(2.0, 0.0))
                                .translate(egui::vec2(-2.0, 0.0));
                        }
                        let light_blue = Color32::from_rgba_unmultiplied(
                            Color32::LIGHT_BLUE.r(),
                            Color32::LIGHT_BLUE.g(),
                            Color32::LIGHT_BLUE.b(),
                            5,
                        );
                        ui.painter().set(
                            bg_idx,
                            Shape::rect_filled(btn_rect, CornerRadius::default(), light_blue),
                        );
                    }
                };

                add_btn(ui, "General", None);
                add_btn(ui, "Language", None);
                ui.add_space(10.0);

                let old_spacing_y =
                    std::mem::replace(&mut ui.style_mut().spacing.item_spacing.y, 0.0);
                add_btn(ui, "Player", None);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.with_layout(
                        Layout::top_down(egui::Align::Min).with_cross_justify(true),
                        |ui| {
                            add_btn(ui, "Tee", Some("Player"));
                            add_btn(ui, "Misc", Some("Player"));
                            add_btn(ui, "Assets", Some("Player"));
                            add_btn(ui, "Controls", Some("Player"));
                        },
                    );
                });
                ui.style_mut().spacing.item_spacing.y = old_spacing_y;

                ui.add_space(10.0);
                add_btn(ui, "Graphics", None);

                let old_spacing_y =
                    std::mem::replace(&mut ui.style_mut().spacing.item_spacing.y, 0.0);
                add_btn(ui, "Sound", None);
                if pipe.user_data.features.spatial_chat {
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.with_layout(
                            Layout::top_down(egui::Align::Min).with_cross_justify(true),
                            |ui| {
                                add_btn(ui, "Spatial Chat", Some("Sound"));
                            },
                        );
                    });
                }
                ui.style_mut().spacing.item_spacing.y = old_spacing_y;

                ui.add_space(10.0);
                // search icon
                add_btn(ui, "\u{1f50d} Settings", None);
            },
        );
    });
}

/// big box, rounded edges
#[instrument(level = "trace", skip_all)]
pub fn render(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    cur_page: &str,
) {
    if cur_page == MENU_SETTINGS_NAME {
        let path = &mut pipe.user_data.config.engine.ui.path;
        let cur_sub = path
            .query
            .get(SETTINGS_UI_PAGE_QUERY)
            .map(|path| path.as_ref())
            .unwrap_or("")
            .to_string();

        let cur_subsub = path
            .query
            .get(SETTINGS_SUB_UI_PAGE_QUERY)
            .map(|path| path.as_ref())
            .unwrap_or("")
            .to_string();

        let w = ui.available_width();
        let margin = ui.style().spacing.item_spacing.x;
        let width_nav = 180.0;
        let max_width_settings = 800.0;
        let width_settings = (w - width_nav - margin).clamp(100.0, max_width_settings);
        StripBuilder::new(ui)
            .size(Size::remainder())
            .size(Size::exact(width_nav))
            .size(Size::exact(width_settings))
            .size(Size::remainder())
            .horizontal(|mut strip| {
                strip.empty();
                strip.cell(|ui| {
                    ui.style_mut().wrap_mode = None;
                    let res = Frame::default()
                        .fill(bg_frame_color())
                        .corner_radius(5.0)
                        .show(ui, |ui| {
                            add_vertical_margins(ui, |ui| {
                                ui.style_mut().wrap_mode = None;
                                render_nav(ui, pipe, &cur_sub, &cur_subsub);
                            });
                        });
                    ui_state.add_blur_rect(res.response.rect, 5.0);
                });
                strip.cell(|ui| {
                    ui.style_mut().wrap_mode = None;
                    let res = Frame::default()
                        .fill(bg_frame_color())
                        .corner_radius(5.0)
                        .show(ui, |ui| {
                            add_margins(ui, |ui| {
                                ui.style_mut().wrap_mode = None;
                                match cur_sub.as_str() {
                                    "Language" => {
                                        super::language::main_frame::render(ui, pipe, ui_state);
                                    }
                                    "Player" => {
                                        super::player::main_frame::render(ui, pipe, ui_state);
                                    }
                                    "Graphics" => {
                                        super::graphics::main_frame::render(ui, pipe);
                                    }
                                    "Sound" => {
                                        super::sound::main_frame::render(ui, pipe);
                                    }
                                    "\u{1f50d} Settings" => {
                                        super::search_settings::main_frame::render(ui, pipe);
                                    }
                                    // general is default
                                    _ => {
                                        super::general::main_frame::render(ui, pipe, ui_state);
                                    }
                                }
                            });
                        });
                    ui_state.add_blur_rect(res.response.rect, 5.0);
                });
                strip.empty();
            });
    }
}
