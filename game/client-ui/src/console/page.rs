use egui::Color32;
use ui_base::types::{UiRenderPipe, UiState};
use ui_generic::traits::UiPageInterface;

use super::{main_frame, user_data::UserData};

pub struct ConsoleUi {
    bg_color: Color32,
}

impl ConsoleUi {
    pub fn new(bg_color: Color32) -> Self {
        Self { bg_color }
    }
}

impl<'a> UiPageInterface<UserData<'a>> for ConsoleUi {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut UiRenderPipe<UserData<'a>>,
        ui_state: &mut UiState,
    ) {
        main_frame::render(ui, pipe, ui_state, self.bg_color)
    }
}
