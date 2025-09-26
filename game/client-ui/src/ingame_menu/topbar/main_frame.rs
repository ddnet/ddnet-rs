use egui::Frame;

use tracing::instrument;
use ui_base::{
    components::menu_top_button::{MenuTopButtonProps, menu_top_button},
    style::{bg_frame_color, topbar_buttons},
    types::{UiRenderPipe, UiState},
    utils::add_horizontal_margins,
};

use crate::{
    ingame_menu::{constants::INGAME_MENU_UI_PAGE_QUERY, user_data::UserData},
    main_menu::topbar::main_frame::render_right_buttons,
};

/// main frame. full width
#[instrument(level = "trace", skip_all)]
pub fn render(ui: &mut egui::Ui, ui_state: &mut UiState, pipe: &mut UiRenderPipe<UserData>) {
    let res = Frame::default().fill(bg_frame_color()).show(ui, |ui| {
        add_horizontal_margins(ui, |ui| {
            ui.set_style(topbar_buttons());
            ui.horizontal(|ui| {
                let current_active = pipe
                    .user_data
                    .browser_menu
                    .config
                    .engine
                    .ui
                    .path
                    .query
                    .get(INGAME_MENU_UI_PAGE_QUERY)
                    .cloned();
                if menu_top_button(
                    ui,
                    |_, _| None,
                    MenuTopButtonProps::new("Server info", &current_active),
                )
                .clicked()
                {
                    pipe.user_data
                        .browser_menu
                        .config
                        .engine
                        .ui
                        .path
                        .add_query((
                            INGAME_MENU_UI_PAGE_QUERY.to_string(),
                            "Server info".to_string(),
                        ));
                }
                if menu_top_button(
                    ui,
                    |_, _| None,
                    MenuTopButtonProps::new("Players", &current_active),
                )
                .clicked()
                {
                    pipe.user_data
                        .browser_menu
                        .config
                        .engine
                        .ui
                        .path
                        .add_query((INGAME_MENU_UI_PAGE_QUERY.to_string(), "Players".to_string()));
                }

                let server_options = pipe.user_data.game_server_info.server_options();
                if server_options.use_account_name
                    && menu_top_button(
                        ui,
                        |_, _| None,
                        MenuTopButtonProps::new("Account", &current_active),
                    )
                    .clicked()
                {
                    pipe.user_data
                        .browser_menu
                        .config
                        .engine
                        .ui
                        .path
                        .add_query((INGAME_MENU_UI_PAGE_QUERY.to_string(), "Account".to_string()));
                }
                if server_options.ghosts
                    && menu_top_button(
                        ui,
                        |_, _| None,
                        MenuTopButtonProps::new("Ghost", &current_active),
                    )
                    .clicked()
                {
                    pipe.user_data
                        .browser_menu
                        .config
                        .engine
                        .ui
                        .path
                        .add_query((INGAME_MENU_UI_PAGE_QUERY.to_string(), "Ghost".to_string()));
                }
                if menu_top_button(
                    ui,
                    |_, _| None,
                    MenuTopButtonProps::new("Call vote", &current_active),
                )
                .clicked()
                {
                    pipe.user_data
                        .browser_menu
                        .config
                        .engine
                        .ui
                        .path
                        .add_query((
                            INGAME_MENU_UI_PAGE_QUERY.to_string(),
                            "Call vote".to_string(),
                        ));
                }
                render_right_buttons(
                    ui,
                    pipe.user_data.browser_menu.events,
                    pipe.user_data.browser_menu.config,
                    pipe.user_data.browser_menu.main_menu,
                    &current_active,
                    INGAME_MENU_UI_PAGE_QUERY,
                );
            });
        });
    });
    ui_state.add_blur_rect(res.response.rect, 0.0);
}
