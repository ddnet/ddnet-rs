use std::net::SocketAddr;

use client_ui::events::{UiEvent, UiEvents};
use game_config::config::Config;
use ui_generic::traits::UiPageInterface;

pub struct LegacyWarningPage {
    events: UiEvents,
}

impl LegacyWarningPage {
    pub fn new(events: UiEvents) -> Self {
        Self { events }
    }
}

impl UiPageInterface<Config> for LegacyWarningPage {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut ui_base::types::UiRenderPipe<Config>,
        _ui_state: &mut ui_base::types::UiState,
    ) {
        egui::Window::new("")
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .min_width(700.0)
            .anchor(egui::Align2::CENTER_CENTER, (0.0, 0.0))
            .show(ui.ctx(), |ui| {
                let server_addr_str = pipe.user_data.storage::<String>("server-addr");
                let server_addr: Result<SocketAddr, _> = server_addr_str.parse();
                if let Ok(server_addr) = server_addr {
                    ui.label(format!(
                        "You are connecting to a legacy server: {server_addr}",
                    ));

                    ui.label("There are some important things to know:");
                    ui.label(
                        "- Legacy server support is built on best effort base, \
                        not everything will work",
                    );
                    ui.label("- ... like seeing assets from other players");
                    ui.label("- ... accounts will not work");
                    ui.label("- ... encryption is disabled");
                    ui.label(
                        "- ... connecting might take long due to map downloads, \
                        not shown in UI",
                    );
                    ui.label(
                        "- ... and other features not supported \
                        by legacy ddnet client",
                    );
                    ui.label("- ... prediction is not good yet");

                    ui.add_space(10.0);
                    ui.label(
                        "Please keep these points in mind before \
                        requesting features or report bugs.",
                    );

                    ui.checkbox(
                        &mut pipe.user_data.game.cl.shown_legacy_server_warning,
                        "Don't show again.",
                    );

                    if ui.button("Ok").clicked() {
                        self.events.push(UiEvent::ConnectLegacy {
                            addr: server_addr,
                            can_show_warning: false,
                        });
                    }
                } else {
                    ui.label("Legacy server address invalid");
                    if ui.button("return").clicked() {
                        pipe.user_data.engine.ui.path.route("");
                    }
                }
            });
    }
}
