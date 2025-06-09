use egui::{Align2, Color32, FontId, Modal, ModifierNames, Window};
use ui_base::types::{UiRenderPipe, UiState};

use crate::network::{NetworkClientState, NetworkState};

use super::{
    dotted_rect::draw_dotted_rect,
    user_data::{UserData, UserDataWithTab},
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    super::mapper_cursors::main_frame::render(
        ui,
        pipe.user_data.canvas_handle,
        &mut pipe.user_data.editor_tabs,
    );

    super::top_menu::menu::render(ui, ui_state, pipe);
    super::top_tabs::main_frame::render(ui, pipe, ui_state);

    // groups & layers attr
    if let Some(tab) = pipe.user_data.editor_tabs.active_tab() {
        let mut user_data = UserDataWithTab {
            ui_events: pipe.user_data.ui_events,
            config: pipe.user_data.config,
            canvas_handle: pipe.user_data.canvas_handle,
            stream_handle: pipe.user_data.stream_handle,
            editor_tab: tab,
            tools: pipe.user_data.tools,
            pointer_is_used: pipe.user_data.pointer_is_used,
            io: pipe.user_data.io,
            tp: pipe.user_data.tp,
            editor_options: pipe.user_data.editor_options,
            auto_mapper: pipe.user_data.auto_mapper,
            graphics_mt: pipe.user_data.graphics_mt,
            shader_storage_handle: pipe.user_data.shader_storage_handle,
            buffer_object_handle: pipe.user_data.buffer_object_handle,
            backend_handle: pipe.user_data.backend_handle,
            quad_tile_images_container: pipe.user_data.quad_tile_images_container,
            sound_images_container: pipe.user_data.sound_images_container,
            container_scene: pipe.user_data.container_scene,

            hotkeys: pipe.user_data.hotkeys,
            cur_hotkey_events: pipe.user_data.cur_hotkey_events,
            cached_binds_per_event: pipe.user_data.cached_binds_per_event,
        };
        let mut pipe = UiRenderPipe {
            cur_time: pipe.cur_time,
            user_data: &mut user_data,
        };
        super::left_panel::panel::render(ui, &mut pipe, ui_state);
        super::top_toolbar::toolbar::render(ui, &mut pipe, ui_state);
        super::bottom_panel::panel::render(ui, &mut pipe, ui_state);
        super::animation_panel::panel::render(ui, &mut pipe, ui_state);
        super::server_settings::panel::render(ui, &mut pipe, ui_state);
        super::server_config_variables::panel::render(ui, &mut pipe, ui_state);
        super::group_and_layer::group_props::render(ui, &mut pipe, ui_state);
        super::group_and_layer::layer_props::render(ui, &mut pipe, ui_state);
        super::group_and_layer::quad_props::render(ui, &mut pipe, ui_state);
        super::group_and_layer::sound_props::render(ui, &mut pipe, ui_state);

        super::chat_panel::panel::render(ui, &mut pipe, ui_state);
        super::assets_store_panel::panel::render(ui, &mut pipe, ui_state);

        super::tool_overlays::tile_brush::render(ui, &mut pipe);

        super::hotkey_panel::panel::render(ui, &mut pipe);

        if let NetworkState::Client(state) = tab.client.net_state() {
            match state {
                NetworkClientState::Connecting(to) => {
                    Window::new("Network")
                        .anchor(Align2::CENTER_CENTER, (0.0, 0.0))
                        .show(ui.ctx(), |ui| {
                            ui.label(format!("Connecting: {to}."));
                            ui.label("The client will try to connect for around 2 minutes before timing out.");
                        });
                }
                NetworkClientState::Connected => {
                    if tab.client.is_likely_distconnected() {
                        Window::new("Network")
                            .anchor(Align2::CENTER_CENTER, (0.0, 0.0))
                            .show(ui.ctx(), |ui| {
                                ui.label("The server did not respond in the last few seconds.");
                                ui.label("The connection might be dead.");
                                ui.label("Timeout happens after around 2 minutes.");
                            });
                    }
                }
                NetworkClientState::Disconnected(reason) => {
                    Window::new("Network")
                        .anchor(Align2::CENTER_CENTER, (0.0, 0.0))
                        .show(ui.ctx(), |ui| {
                            ui.label(
                                "Disconnected. You can still save the map, \
                                but not edit it anymore.",
                            );
                            ui.label(format!("Reason: {reason}"));
                        });
                }
                NetworkClientState::Err(reason) => {
                    Window::new("Network")
                        .anchor(Align2::CENTER_CENTER, (0.0, 0.0))
                        .show(ui.ctx(), |ui| {
                            ui.label(format!("Error: {reason}"));
                        });
                }
            }
        }
    }

    super::close_modal::render(ui, pipe);

    *pipe.user_data.pointer_is_used |= ui.memory(|i| i.any_popup_open());

    *pipe.user_data.unused_rect = Some(ui.available_rect_before_wrap());
    if *pipe.user_data.pointer_is_used {
        *pipe.user_data.unused_rect = None;
    }

    *pipe.user_data.input_state = Some(ui.ctx().input(|inp| inp.clone()));
    *pipe.user_data.canvas_size = Some(ui.ctx().input(|inp| inp.screen_rect()));

    if let Some(hovered_file) = pipe.user_data.hovered_file.as_ref() {
        Modal::new("hovered-file-drag-zones".into()).show(ui.ctx(), |ui| {
            ui.set_width(ui.ctx().screen_rect().width());
            ui.set_height(ui.ctx().screen_rect().height());
            let ext = hovered_file
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            let drop_areas = match ext {
                "map" => vec!["Drop the legacy map file here to open a new tab."],
                "twmap" => vec!["Drop the map file here to open a new tab."],
                "png" => vec![
                    "Drop the quad texture here to add it to the current map.",
                    "Drop the tile texture here to add it to the current map.",
                ],
                "ogg" => vec!["Drop the sound here to add it to the current map."],
                _ => Default::default(),
            };

            if drop_areas.len() == 1 {
                draw_dotted_rect(
                    ui,
                    ui.ctx().screen_rect().expand(-50.0),
                    10.0,
                    Color32::WHITE,
                );
                ui.painter().text(
                    ui.ctx().screen_rect().center(),
                    Align2::CENTER_CENTER,
                    drop_areas[0],
                    FontId::proportional(30.0),
                    Color32::WHITE,
                );
            } else if drop_areas.len() == 2 {
                let pointer = ui
                    .input(|i| {
                        i.pointer
                            .hover_pos()
                            .or(i.pointer.interact_pos())
                            .or(i.pointer.latest_pos())
                    })
                    .unwrap_or(*pipe.user_data.current_client_pointer_pos);
                let left_active = pointer.x < ui.ctx().screen_rect().width() / 2.0;
                let (left_color, right_color) = if left_active {
                    (Color32::LIGHT_BLUE, Color32::WHITE)
                } else {
                    (Color32::WHITE, Color32::LIGHT_BLUE)
                };

                let mut rect = ui.ctx().screen_rect();
                rect.set_width(rect.width() / 2.0);
                draw_dotted_rect(ui, rect.expand(-50.0), 10.0, left_color);
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    drop_areas[0],
                    FontId::proportional(25.0),
                    left_color,
                );

                let mut rect = ui.ctx().screen_rect();
                rect = rect.translate((rect.width() / 2.0, 0.0).into());
                rect.set_width(rect.width() / 2.0);
                draw_dotted_rect(ui, rect.expand(-50.0), 10.0, right_color);
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    drop_areas[1],
                    FontId::proportional(25.0),
                    right_color,
                );
            }
        });

        *pipe.user_data.pointer_is_used = true;
    }

    // clear and handle new hotkeys
    *pipe.user_data.cached_binds_per_event = None;
    pipe.user_data.cur_hotkey_events.clear();
    let mut binds_sorted: Vec<_> = pipe
        .user_data
        .hotkeys
        .binds
        .iter()
        .map(|(s, ev)| (ModifierNames::NAMES.format(&s.modifiers, false), s, ev))
        .collect();
    binds_sorted.sort_by(|(s1, _, _), (s2, _, _)| s1.cmp(s2).reverse());
    for (_, shortcut, hotkey) in binds_sorted {
        if ui.input_mut(|i| i.consume_shortcut(shortcut)) {
            pipe.user_data.cur_hotkey_events.insert(*hotkey);
        }
    }
}
