use tracing::instrument;
use ui_base::types::{UiRenderPipe, UiState};

use crate::main_menu::{
    constants::{
        MENU_COMMUNITY_PREFIX, MENU_EXPLORE_COMMUNITIES_NAME, MENU_FAVORITES_NAME,
        MENU_INTERNET_NAME, MENU_LAN_NAME,
    },
    user_data::UserData,
};

/// big box, rounded edges
#[instrument(level = "trace", skip_all)]
pub fn render(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    cur_page: &str,
) {
    if cur_page.is_empty()
        || cur_page == MENU_INTERNET_NAME
        || cur_page == MENU_LAN_NAME
        || cur_page == MENU_FAVORITES_NAME
        || cur_page.starts_with(MENU_COMMUNITY_PREFIX)
    {
        super::browser::main_frame::render(ui, pipe, ui_state, cur_page);
    } else if cur_page == MENU_EXPLORE_COMMUNITIES_NAME {
        super::super::communities::main_frame::render(ui, pipe, ui_state);
    }
}
