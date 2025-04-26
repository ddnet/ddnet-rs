use client_ui::events::{UiEvent, UiEvents};
use egui::TextEdit;
use game_config::config::Config;
use ui_generic::traits::UiPageInterface;

pub struct PasswordConnectPage {
    events: UiEvents,

    password: String,
}

impl PasswordConnectPage {
    pub fn new(events: UiEvents) -> Self {
        Self {
            events,
            password: Default::default(),
        }
    }
}

impl UiPageInterface<Config> for PasswordConnectPage {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        _pipe: &mut ui_base::types::UiRenderPipe<Config>,
        _ui_state: &mut ui_base::types::UiState,
    ) {
        egui::Window::new("")
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .min_width(700.0)
            .anchor(egui::Align2::CENTER_CENTER, (0.0, 0.0))
            .show(ui.ctx(), |ui| {
                ui.label("The server requires you to enter a password to connect:");

                ui.add(
                    TextEdit::singleline(&mut self.password)
                        .password(true)
                        .char_limit(1024),
                );

                ui.horizontal(|ui| {
                    if ui.button("Connect").clicked() && !self.password.is_empty() {
                        self.events
                            .push(UiEvent::PasswordEntered(Some(self.password.clone())));
                    }
                    if ui.button("Cancel").clicked() && !self.password.is_empty() {
                        self.events.push(UiEvent::PasswordEntered(None));
                    }
                });
            });
    }

    fn mount(&mut self) {
        self.password.clear();
    }
}
