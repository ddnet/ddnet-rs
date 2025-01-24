use egui::{Align2, Window};
use ui_base::types::{UiRenderPipe, UiState};

use crate::network::{NetworkClientState, NetworkState};

use super::user_data::{UserData, UserDataWithTab};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    super::mapper_cursors::main_frame::render(
        ui,
        pipe.user_data.canvas_handle,
        &mut pipe.user_data.editor_tabs,
    );

    super::top_menu::menu::render(ui, pipe);
    super::top_tabs::main_frame::render(ui, pipe);

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
            graphics_mt: pipe.user_data.graphics_mt,
            buffer_object_handle: pipe.user_data.buffer_object_handle,
            backend_handle: pipe.user_data.backend_handle,
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
        super::group_and_layer::group_props::render(ui, &mut pipe, ui_state);
        super::group_and_layer::layer_props::render(ui, &mut pipe, ui_state);
        super::group_and_layer::quad_props::render(ui, &mut pipe, ui_state);
        super::group_and_layer::sound_props::render(ui, &mut pipe, ui_state);

        super::chat_panel::panel::render(ui, &mut pipe, ui_state);

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
                            ui.label(format!("Reason: {}", reason));
                        });
                }
                NetworkClientState::Err(reason) => {
                    Window::new("Network")
                        .anchor(Align2::CENTER_CENTER, (0.0, 0.0))
                        .show(ui.ctx(), |ui| {
                            ui.label(format!("Error: {}", reason));
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
}
