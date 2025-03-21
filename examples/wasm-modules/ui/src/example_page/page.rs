use ui_base::types::{UiRenderPipe, UiState};
use ui_generic::traits::UiPageInterface;

pub struct ExamplePage {}

impl Default for ExamplePage {
    fn default() -> Self {
        Self::new()
    }
}

impl ExamplePage {
    pub fn new() -> Self {
        Self {}
    }

    fn render_impl(&mut self, ui: &mut egui::Ui) {
        super::main_frame::render(ui)
    }
}

impl UiPageInterface<()> for ExamplePage {
    fn render(&mut self, ui: &mut egui::Ui, _pipe: &mut UiRenderPipe<()>, _ui_state: &mut UiState) {
        self.render_impl(ui)
    }
}
