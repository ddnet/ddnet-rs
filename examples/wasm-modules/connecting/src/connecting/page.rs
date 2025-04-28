use client_ui::{connect::user_data::UserData, events::UiEvents};
use ui_base::types::{UiRenderPipe, UiState};
use ui_generic::traits::UiPageInterface;

pub struct Connecting {}

impl Default for Connecting {
    fn default() -> Self {
        Self::new()
    }
}

impl Connecting {
    pub fn new() -> Self {
        Self {}
    }

    fn render_impl(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut UiRenderPipe<()>,
        ui_state: &mut UiState,
    ) {
        client_ui::connect::main_frame::render(
            ui,
            ui_state,
            &mut UiRenderPipe {
                cur_time: pipe.cur_time,
                user_data: &mut UserData {
                    log: &Default::default(),
                    config: &mut Default::default(),
                    events: &UiEvents::new(),
                },
            },
        );
    }
}

impl UiPageInterface<()> for Connecting {
    fn render(&mut self, ui: &mut egui::Ui, pipe: &mut UiRenderPipe<()>, ui_state: &mut UiState) {
        self.render_impl(ui, pipe, ui_state)
    }
}
