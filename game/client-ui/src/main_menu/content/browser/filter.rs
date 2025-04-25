use client_containers::container::ContainerItemIndexType;
use egui::{Align, Layout, Rect, UiBuilder};
use egui_extras::{Size, StripBuilder};

use game_base::server_browser::ServerFilter;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{main_menu::user_data::UserData, utils::render_flag_for_ui};

/// button & popover
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    let search_width = if ui.available_width() < 350.0 {
        150.0
    } else {
        250.0
    };
    let extra_space = 0.0;
    StripBuilder::new(ui)
        .size(Size::exact(extra_space))
        .size(Size::exact(30.0))
        .size(Size::remainder().at_least(search_width))
        .size(Size::exact(30.0))
        .size(Size::exact(extra_space))
        .horizontal(|mut strip| {
            strip.empty();
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    // hamburger menu
                    ui.menu_button("\u{f0c9}", |ui| {
                        if ui.button("Save current filter in tab").clicked() {
                            // TODO:
                        }
                    });
                });
            });
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                StripBuilder::new(ui)
                    .size(Size::remainder())
                    .size(Size::exact(search_width))
                    .size(Size::remainder())
                    .horizontal(|mut strip| {
                        strip.empty();
                        strip.cell(|ui| {
                            ui.style_mut().wrap_mode = None;
                            ui.with_layout(
                                Layout::left_to_right(Align::Center).with_main_justify(true),
                                |ui| {
                                    super::search::render(ui, pipe);
                                },
                            );
                        });
                        strip.empty();
                    });
            });
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // filter
                    ui.menu_button("\u{f0b0}", |ui| {
                        let config = &mut *pipe.user_data.config;
                        // filter window
                        let mut filter = config.storage::<ServerFilter>("browser_filter");
                        let prev_filter = filter.clone();
                        ui.checkbox(&mut filter.has_players, "Has players");
                        ui.checkbox(&mut filter.filter_full_servers, "Server not full");
                        ui.checkbox(&mut filter.fav_players_only, "Favorite players only");
                        ui.checkbox(&mut filter.no_password, "No password");
                        ui.checkbox(&mut filter.unfinished_maps, "Unfinished maps only");
                        ui.checkbox(&mut filter.hide_legacy_servers, "Hide legacy servers");
                        if filter != prev_filter {
                            config.set_storage("browser_filter", &filter);
                        }
                        // list countries and mod types
                        let left_top = ui.available_rect_before_wrap().left_top();
                        ui.allocate_new_ui(
                            UiBuilder::new().max_rect(Rect::from_min_max(
                                left_top,
                                left_top + egui::vec2(150.0, 150.0),
                            )),
                            |ui| {
                                let servers = &pipe.user_data.browser_data;
                                let server_locations = servers.locations();
                                super::super::super::settings::list::list::render(
                                    ui,
                                    server_locations
                                        .iter()
                                        .map(|s| (s.as_str(), ContainerItemIndexType::Disk)),
                                    20.0,
                                    |_, _| Ok(()),
                                    |_, _| true,
                                    |ui, _, name, pos, size| {
                                        let key =
                                            pipe.user_data.flags_container.default_key.clone();
                                        render_flag_for_ui(
                                            pipe.user_data.stream_handle,
                                            pipe.user_data.canvas_handle,
                                            pipe.user_data.flags_container,
                                            ui,
                                            ui_state,
                                            ui.ctx().screen_rect(),
                                            Some(ui.available_rect_before_wrap()),
                                            &key,
                                            &name.to_lowercase().replace("-", "_"),
                                            pos,
                                            size,
                                        );
                                    },
                                    |_, _| {},
                                    |_, _| None,
                                    &mut String::new(),
                                    |_| {},
                                );
                            },
                        );

                        if ui.button("Reset filter").clicked() {
                            config.rem_storage("browser_filter");
                        }
                    });
                });
            });
            strip.empty();
        });
}
