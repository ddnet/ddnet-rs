use crate::main_menu::user_data::UserData;
use tracing::instrument;
use ui_base::types::{UiRenderPipe, UiState};

#[instrument(level = "trace", skip_all)]
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    super::list::lang_list(ui, pipe, ui_state)
}
