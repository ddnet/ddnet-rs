use egui::{Stroke, epaint::Shadow};
use game_config::config::Config;
use ui_generic::traits::UiPageInterface;
use ui_wasm_manager::UiWasmManagerErrorPageErr;

pub struct Error404Page {
    err: UiWasmManagerErrorPageErr,
}

impl Error404Page {
    pub fn new(err: UiWasmManagerErrorPageErr) -> Self {
        Self { err }
    }
}

impl UiPageInterface<Config> for Error404Page {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut ui_base::types::UiRenderPipe<Config>,
        _ui_state: &mut ui_base::types::UiState,
    ) {
        let style = ui.style();
        egui::Frame::group(style)
            .fill(style.visuals.window_fill)
            .stroke(Stroke::NONE)
            .shadow(Shadow {
                color: style.visuals.window_shadow.color,
                spread: (style.spacing.item_spacing.y / 2.0) as u8,
                blur: 5,
                ..Default::default()
            })
            .show(ui, |ui| {
                ui.label(format!("Error 404 not found: {}", self.err.get()));
                if ui.button("return").clicked() {
                    pipe.user_data.engine.ui.path.route("");
                }
            });
    }
}
