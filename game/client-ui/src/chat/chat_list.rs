use std::time::Duration;

use client_types::chat::ServerMsg;
use egui::Layout;

use ui_base::types::{UiRenderPipe, UiState};

use super::user_data::UserData;

/// frame for the chat entries
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    ui.with_layout(Layout::bottom_up(egui::Align::Min), |ui| {
        // active input comes first (most bottom)
        super::input::render(ui, ui_state, pipe);

        for msg in pipe.user_data.entries.iter() {
            let time_diff = if pipe.user_data.show_chat_history {
                Duration::ZERO
            } else {
                pipe.cur_time.saturating_sub(msg.add_time)
            };
            if time_diff < Duration::from_secs(10) {
                let chat_fade = if time_diff >= Duration::from_secs(9) {
                    // re-render while opacity changes
                    ui.ctx().request_repaint();
                    1.0 - (time_diff.as_secs_f32() - 9.0)
                } else {
                    // re-render if opacity will change
                    ui.ctx()
                        .request_repaint_after(Duration::from_secs(9) - time_diff);
                    1.0
                };
                ui.set_opacity(chat_fade);
                match &msg.msg {
                    ServerMsg::Chat(msg) => {
                        super::chat_entry::render(ui, pipe, ui_state, msg);
                    }
                    ServerMsg::System(msg) => {
                        super::system_entry::render(ui, pipe, ui_state, msg);
                    }
                };
            }
        }
    });
}
