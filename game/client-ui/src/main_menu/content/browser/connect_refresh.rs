use std::net::SocketAddr;

use egui::{Button, Color32};

use ui_base::{style::default_style, types::UiRenderPipe};

use crate::{events::UiEvent, main_menu::user_data::UserData};

/// connect & refresh button
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>) {
    ui.horizontal(|ui| {
        ui.set_clip_rect(ui.ctx().screen_rect());
        let server_addr_str = pipe.user_data.config.storage::<String>("server-addr");
        let server_addr: Result<SocketAddr, _> = server_addr_str.parse();

        let mut button_style = default_style();
        let btn_color = if server_addr.is_ok() {
            Color32::DARK_GREEN
        } else {
            Color32::DARK_RED
        };
        button_style.visuals.widgets.inactive.weak_bg_fill = btn_color;
        button_style.visuals.widgets.noninteractive.weak_bg_fill = btn_color;
        button_style.visuals.widgets.active.weak_bg_fill = btn_color;
        button_style.visuals.widgets.hovered.weak_bg_fill = btn_color;
        ui.set_style(button_style);

        let enter_clicked = ui.ctx().input(|i| i.key_pressed(egui::Key::Enter))
            && ui.ctx().memory(|m| m.focused().is_none());

        // connect
        if ui
            .add(Button::new("\u{f2f6}"))
            .on_hover_text(match &server_addr {
                Ok(addr) => format!("connect to {addr}"),
                Err(err) => format!("canno't connect to {server_addr_str}: {err}"),
            })
            .clicked()
            || enter_clicked
        {
            if let Ok(addr) = server_addr {
                let is_legacy_server: bool = pipe.user_data.config.storage("server-is-legacy");
                if is_legacy_server {
                    pipe.user_data.events.push(UiEvent::ConnectLegacy {
                        addr,
                        can_show_warning: true,
                    });
                } else {
                    pipe.user_data.events.push(UiEvent::Connect {
                        addr,
                        cert_hash: pipe.user_data.config.storage("server-cert"),
                        rcon_secret: pipe.user_data.config.storage("rcon-secret"),
                        can_start_internal_server: pipe
                            .user_data
                            .config
                            .storage("server-is-internal"),
                        can_connect_internal_server: pipe
                            .user_data
                            .config
                            .storage("server-is-internal"),
                    });
                }
            }
        }
    });
    // refresh
    if ui.button("\u{f2f9}").clicked() {
        pipe.user_data.main_menu.refresh();
        let profiles = pipe.user_data.profiles.clone();
        pipe.user_data.profile_tasks.user_interactions.push(
            pipe.user_data
                .io
                .rt
                .spawn(async move { profiles.user_interaction().await })
                .cancelable(),
        );
    }
    if pipe.user_data.config.engine.dbg.app && ui.button("legacy").clicked() {
        pipe.user_data
            .config
            .set_storage("server-addr", &"127.0.0.1:8303".to_string());
        pipe.user_data.events.push(UiEvent::ConnectLegacy {
            addr: "127.0.0.1:8303".parse().unwrap(),
            can_show_warning: true,
        });
    }
}
