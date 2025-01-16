use egui::Layout;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    map::EditorChatState,
    ui::user_data::{EditorUiEvent, UserDataWithTab},
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    let map = &mut pipe.user_data.editor_tab.map;
    if ui.input(|i| i.modifiers.shift && i.key_pressed(egui::Key::Enter)) {
        map.user.ui_values.chat_panel_open = Some(EditorChatState::default());
    }

    let Some(chat_state) = &mut map.user.ui_values.chat_panel_open else {
        return;
    };

    let res = {
        let mut panel = egui::SidePanel::right("chat_panel")
            .resizable(true)
            .width_range(300.0..=600.0);
        panel = panel.default_width(500.0);

        let mut send_chat = None;

        let res = panel.show_inside(ui, |ui| {
            ui.with_layout(Layout::bottom_up(egui::Align::Min), |ui| {
                let inp = ui.text_edit_singleline(&mut chat_state.msg);
                if inp.lost_focus() {
                    send_chat = Some(std::mem::take(&mut chat_state.msg));
                } else {
                    inp.request_focus();
                }

                for (author, msg) in pipe.user_data.editor_tab.client.msgs.iter() {
                    ui.label(msg);
                    ui.label(format!("{author}:"));
                    ui.add_space(10.0);
                }
            })
        });

        if let Some(msg) = send_chat {
            pipe.user_data.ui_events.push(EditorUiEvent::Chat { msg });

            map.user.ui_values.chat_panel_open = None;
        }

        Some(res)
    };

    if let Some(res) = res {
        ui_state.add_blur_rect(res.response.rect, 0.0);
    }
}
