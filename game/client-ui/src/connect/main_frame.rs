use egui::{vec2, Align2, Color32, FontId, Frame, RichText, ScrollArea, Vec2, Window};

use game_base::connecting_log::ConnectModes;
use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
    utils::add_margins,
};

use crate::events::UiEvent;

use super::user_data::UserData;

pub fn render_modes(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>) {
    let log = &pipe.user_data.log;
    let mode = log.mode();
    if let Some(mode) = mode {
        match mode {
            ConnectModes::Connecting { addr } => {
                ui.vertical(|ui| {
                    ui.label(format!("Connecting to:\n{addr}"));
                    if ui.button("Cancel").clicked() {
                        pipe.user_data.events.push(UiEvent::Disconnect);
                        pipe.user_data.config.engine.ui.path.route("");
                    }
                });
            }
            ConnectModes::ConnectingErr { msg } => {
                ui.vertical(|ui| {
                    ui.label(format!(
                        "Connecting to {} failed:\n{msg}",
                        pipe.user_data.config.storage::<String>("server-addr")
                    ));
                    if ui.button("Return").clicked() {
                        pipe.user_data.events.push(UiEvent::Disconnect);
                        pipe.user_data.config.engine.ui.path.route("");
                    }
                });
            }
            ConnectModes::Queue { msg } => {
                ui.vertical(|ui| {
                    ui.label(format!(
                        "Connecting to {}",
                        pipe.user_data.config.storage::<String>("server-addr")
                    ));
                    ui.label(format!("Waiting in queue: {msg}"));
                    if ui.button("Cancel").clicked() {
                        pipe.user_data.events.push(UiEvent::Disconnect);
                        pipe.user_data.config.engine.ui.path.route("");
                    }
                });
            }
            ConnectModes::DisconnectErr { msg } => {
                ui.vertical(|ui| {
                    ui.label(format!(
                        "Connection to {} lost:\n{msg}",
                        pipe.user_data.config.storage::<String>("server-addr")
                    ));
                    if ui.button("Return").clicked() {
                        pipe.user_data.events.push(UiEvent::Disconnect);
                        pipe.user_data.config.engine.ui.path.route("");
                    }
                });
            }
        }
    }
}

/// top bar
/// big square, rounded edges
pub fn render(ui: &mut egui::Ui, ui_state: &mut UiState, pipe: &mut UiRenderPipe<UserData>) {
    let res = Window::new("")
        .resizable(false)
        .title_bar(false)
        .frame(Frame::default().fill(bg_frame_color()).corner_radius(5.0))
        .anchor(Align2::CENTER_CENTER, Vec2::new(0.0, 0.0))
        .default_size(vec2(200.0, 10.0))
        .show(ui.ctx(), |ui| {
            add_margins(ui, |ui| {
                ui.style_mut().visuals.clip_rect_margin = 6.0;
                render_modes(ui, pipe);
                let log = &pipe.user_data.log;
                let logs = log.logs();
                if !logs.is_empty() {
                    ui.separator();
                    Frame::new()
                        .fill(Color32::from_black_alpha(20))
                        .inner_margin(5)
                        .show(ui, |ui| {
                            ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                                for log in logs {
                                    ui.label(
                                        RichText::new(log)
                                            .font(FontId::proportional(10.0))
                                            .color(Color32::GRAY),
                                    );
                                }
                            });
                        });
                }
            });
        });
    if let Some(res) = res {
        ui_state.add_blur_rect(res.response.rect, 5.0);
    }
}
