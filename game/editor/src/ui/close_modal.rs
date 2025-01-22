use egui::Modal;
use ui_base::types::UiRenderPipe;

use super::user_data::{EditorModalDialogMode, EditorUiEvent, UserData};

pub fn render(ui: &egui::Ui, pipe: &mut UiRenderPipe<UserData>) {
    if let EditorModalDialogMode::CloseEditor = pipe.user_data.modal_dialog_mode {
        Modal::new("close-tab-confirm".into()).show(ui.ctx(), |ui| {
            ui.label("There are still unsaved tabs in the editor.");
            ui.horizontal(|ui| {
                if ui.button("Save all & close").clicked() {
                    pipe.user_data
                        .ui_events
                        .push(EditorUiEvent::SaveAllAndClose);
                    *pipe.user_data.modal_dialog_mode = EditorModalDialogMode::None;
                }
                if ui.button("Close without saving").clicked() {
                    pipe.user_data.ui_events.push(EditorUiEvent::ForceClose);
                    *pipe.user_data.modal_dialog_mode = EditorModalDialogMode::None;
                }
                if ui.button("Cancel").clicked() {
                    *pipe.user_data.modal_dialog_mode = EditorModalDialogMode::None;
                }
            });
        });
        *pipe.user_data.pointer_is_used = true;
    }
}
