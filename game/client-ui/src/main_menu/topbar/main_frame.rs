use egui::Grid;
use egui::{Frame, Layout};

use game_base::server_browser::{SortDir, TableSort};
use game_config::config::Config;
use ui_base::style::bg_frame_color;
use ui_base::types::{UiRenderPipe, UiState};
use ui_base::{
    components::menu_top_button::{menu_top_button_icon, MenuTopButtonProps},
    style::topbar_buttons,
    utils::add_horizontal_margins,
};

use crate::events::UiEvents;
use crate::main_menu::constants::{
    MENU_COMMUNITY_PREFIX, MENU_EXPLORE_COMMUNITIES_NAME, MENU_FAVORITES_NAME, MENU_INTERNET_NAME,
    MENU_LAN_NAME, MENU_PROFILE_NAME, MENU_SETTINGS_NAME,
};
use crate::main_menu::user_data::MainMenuInterface;
use crate::main_menu::user_data::UserData;

use crate::{
    events::UiEvent,
    main_menu::constants::{MENU_DEMO_NAME, MENU_QUIT_NAME},
};

pub fn render_right_buttons(
    ui: &mut egui::Ui,
    events: &UiEvents,
    config: &mut Config,
    main_menu: &mut dyn MainMenuInterface,
    current_active: &Option<String>,
    query_name: &str,
) {
    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
        if menu_top_button_icon(ui, MenuTopButtonProps::new(MENU_QUIT_NAME, current_active))
            .clicked()
        {
            events.push(UiEvent::Quit);
        }
        if menu_top_button_icon(ui, MenuTopButtonProps::new(MENU_DEMO_NAME, current_active))
            .clicked()
        {
            let mut demo_dir: String = config.storage("demo-path");
            if demo_dir.is_empty() {
                demo_dir = "demos".to_string();
                config.set_storage("demo-path", &demo_dir);
            }
            if config.storage_opt::<TableSort>("demo.sort").is_none() {
                config.set_storage(
                    "demo.sort",
                    &TableSort {
                        name: "Date".to_string(),
                        sort_dir: SortDir::Desc,
                    },
                )
            }
            main_menu.refresh_demo_list(demo_dir.as_ref());
            config
                .path()
                .add_query((query_name.to_string(), MENU_DEMO_NAME.to_string()));
        }
        if menu_top_button_icon(ui, MenuTopButtonProps::new("\u{f279}", current_active)).clicked() {
            events.push(UiEvent::StartEditor)
        }
    });
}

/// main frame. full width
pub fn render(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    pipe: &mut UiRenderPipe<UserData>,
    ui_page_query_name: &str,
) {
    let current_active = pipe
        .user_data
        .config
        .path()
        .query
        .get(ui_page_query_name)
        .cloned()
        .unwrap_or_default();

    let res = Frame::default().fill(bg_frame_color()).show(ui, |ui| {
        add_horizontal_margins(ui, |ui| {
            ui.set_style(topbar_buttons());
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                if !pipe.user_data.render_options.hide_buttons_icons {
                    render_right_buttons(
                        ui,
                        pipe.user_data.events,
                        pipe.user_data.config,
                        pipe.user_data.main_menu,
                        &Some(current_active.clone()),
                        ui_page_query_name,
                    );
                }

                ui.with_layout(
                    Layout::left_to_right(egui::Align::Center)
                        .with_main_justify(true)
                        .with_main_align(egui::Align::Center),
                    |ui| {
                        match current_active.as_str() {
                            MENU_INTERNET_NAME | "" => {
                                ui.label("Internet \u{f05a}").on_hover_text_at_pointer(
                                    "The internet tab \
                                            shows all servers.",
                                );
                            }
                            MENU_LAN_NAME => {
                                ui.label("LAN \u{f05a}").on_hover_text_at_pointer(
                                    "The LAN tab shows servers \
                                            in your local network.",
                                );
                            }
                            MENU_SETTINGS_NAME => {
                                ui.label("Settings \u{f05a}").on_hover_text_at_pointer(
                                    "Change the settings of your client here.",
                                );
                            }
                            MENU_PROFILE_NAME => {
                                ui.label("Profiles \u{f05a}").on_hover_text_at_pointer(
                                    "Here you can manage your accounts, \
                                            and select the current active one.",
                                );
                            }
                            MENU_FAVORITES_NAME => {
                                ui.label("Favorites \u{f05a}").on_hover_text_at_pointer(
                                    "The favorite tab shows servers \
                                            that you marked with a \u{f005}.",
                                );
                            }
                            MENU_EXPLORE_COMMUNITIES_NAME => {
                                ui.label("Explore communities \u{f05a}")
                                    .on_hover_text_at_pointer(
                                        "This tab shows an overview over \
                                            all existing communities.",
                                    );
                            }
                            x if x.starts_with(MENU_COMMUNITY_PREFIX) => {
                                // render community name and info
                                if let Some(community) = pipe
                                    .user_data
                                    .ddnet_info
                                    .communities
                                    .get(&x[MENU_COMMUNITY_PREFIX.len()..])
                                {
                                    ui.label(format!("{} \u{f05a}", community.name))
                                        .on_hover_ui_at_pointer(|ui| {
                                            Grid::new(format!("community-info-{}", community.id))
                                                .num_columns(2)
                                                .show(ui, |ui| {
                                                    ui.label("ID:");
                                                    ui.label(&community.id);
                                                    ui.end_row();

                                                    if !community.contact_urls.is_empty() {
                                                        ui.label("Contact:");
                                                    }

                                                    for (index, url) in
                                                        community.contact_urls.iter().enumerate()
                                                    {
                                                        if index != 0 {
                                                            ui.label("");
                                                        }
                                                        ui.hyperlink(url);
                                                        ui.end_row();
                                                    }
                                                });
                                        });
                                }
                            }
                            _ => {
                                // render nothing
                            }
                        }
                    },
                );
            });
        });
    });
    ui_state.add_blur_rect(res.response.rect, 0.0);
}
