use egui::{scroll_area::ScrollBarVisibility, Color32, Frame, Key, Layout, ScrollArea, TextEdit};
use egui_extras::{Size, StripBuilder};
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    map::EditorChatState,
    ui::user_data::{EditorUiEvent, UserDataWithTab},
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    let map = &mut pipe.user_data.editor_tab.map;
    if pipe
        .user_data
        .cur_hotkey_events
        .remove(&crate::hotkeys::EditorHotkeyEvent::Chat)
    {
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

        let mut close_chat = None;

        let res = panel.show_inside(ui, |ui| {
            StripBuilder::new(ui)
                .size(Size::remainder())
                .size(Size::exact(30.0))
                .cell_layout(Layout::top_down(egui::Align::Min).with_cross_justify(true))
                .vertical(|mut strip| {
                    strip.cell(|ui| {
                        ScrollArea::vertical()
                            .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                // small workaround to align text to bottom
                                ui.add_space(ui.available_height());
                                for (author, msg) in
                                    pipe.user_data.editor_tab.client.msgs.iter().rev()
                                {
                                    Frame::default()
                                        .fill(Color32::from_black_alpha(150))
                                        .inner_margin(10.0)
                                        .corner_radius(5.0)
                                        .show(ui, |ui| {
                                            ui.label(author);
                                            ui.colored_label(Color32::WHITE, msg);
                                        });
                                    ui.add_space(10.0);
                                }
                            });
                    });
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        let is_enter = ui.input(|i| i.key_pressed(Key::Enter));
                        let pointer_action = ui.input(|i| {
                            i.pointer.any_down()
                                || i.pointer.any_pressed()
                                || i.pointer.any_released()
                        });
                        let inp = ui.add(TextEdit::singleline(&mut chat_state.msg));
                        if inp.lost_focus() && !pointer_action {
                            close_chat =
                                Some(is_enter.then(|| std::mem::take(&mut chat_state.msg)));
                        } else if !pointer_action {
                            inp.request_focus();
                        }
                    });
                });
        });

        if let Some(msg) = close_chat {
            if let Some(msg) = msg {
                pipe.user_data.ui_events.push(EditorUiEvent::Chat { msg });
            }

            map.user.ui_values.chat_panel_open = None;
        }

        Some(res)
    };

    if let Some(res) = res {
        ui_state.add_blur_rect(res.response.rect, 0.0);
    }
}
