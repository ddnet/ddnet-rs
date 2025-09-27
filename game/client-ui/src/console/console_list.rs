use egui::Layout;

use tracing::instrument;
use ui_base::types::UiRenderPipe;

use super::user_data::UserData;

/// frame for the console entries
#[instrument(level = "trace", skip_all)]
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, has_text_selection: &mut bool) {
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.with_layout(
                Layout::left_to_right(egui::Align::Min)
                    .with_main_justify(true)
                    .with_main_align(egui::Align::Min),
                |ui| {
                    let mut text_as_str = pipe.user_data.msgs.as_str();
                    let text = egui::TextEdit::multiline(&mut text_as_str)
                        .id_source("console-text-output")
                        .frame(false)
                        .show(ui);
                    *has_text_selection = text.cursor_range.is_some_and(|range| !range.is_empty());
                },
            );
        });
}
