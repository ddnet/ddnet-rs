use egui_extras::{Size, StripBuilder};

use tracing::instrument;
use ui_base::{
    types::{UiRenderPipe, UiState},
    utils::add_margins,
};

use super::{
    constants::{INGAME_MENU_FALLBACK_QUERY, INGAME_MENU_UI_PAGE_QUERY},
    user_data::UserData,
};

pub fn render_centered_max_sized(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    f: impl FnOnce(&mut egui::Ui, &mut UiRenderPipe<UserData>, &mut UiState),
) {
    let w = ui.available_width();
    let max_width = 800.0;
    let width = w.clamp(100.0, max_width);
    StripBuilder::new(ui)
        .size(Size::remainder())
        .size(Size::exact(width))
        .size(Size::remainder())
        .horizontal(|mut strip| {
            strip.empty();
            strip.cell(|ui| f(ui, pipe, ui_state));
            strip.empty();
        });
}

fn render_content(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    ui_page_query_name: &str,
) {
    let style = ui.style_mut();
    let y = style.spacing.item_spacing.y;
    style.spacing.item_spacing.y = 0.0;
    StripBuilder::new(ui)
        .size(Size::exact(20.0))
        .size(Size::exact(18.0))
        .size(Size::remainder())
        .vertical(|mut strip| {
            strip.cell(|ui| {
                let style = ui.style_mut();
                style.spacing.item_spacing.y = y;
                style.wrap_mode = None;
                super::topbar::main_frame::render(ui, ui_state, pipe);
            });
            strip.cell(|ui| {
                let style = ui.style_mut();
                style.spacing.item_spacing.y = y;
                style.wrap_mode = None;
                super::game::main_frame::render(ui, ui_state, pipe);
            });
            strip.cell(|ui| {
                let style = ui.style_mut();
                style.spacing.item_spacing.y = y;
                style.wrap_mode = None;
                let current_active = pipe
                    .user_data
                    .browser_menu
                    .config
                    .engine
                    .ui
                    .path
                    .query
                    .get(ui_page_query_name)
                    .cloned()
                    .unwrap_or_else(|| INGAME_MENU_FALLBACK_QUERY.to_string());
                match current_active.as_str() {
                    "Server info" => {
                        add_margins(ui, |ui| {
                            render_centered_max_sized(ui, pipe, ui_state, |ui, pipe, ui_state| {
                                super::server_info::main_frame::render(ui, pipe, ui_state)
                            })
                        });
                    }
                    "Players" => {
                        add_margins(ui, |ui| {
                            render_centered_max_sized(ui, pipe, ui_state, |ui, pipe, ui_state| {
                                super::server_players::main_frame::render(ui, ui_state, pipe)
                            })
                        });
                    }
                    "Call vote" => {
                        add_margins(ui, |ui| {
                            render_centered_max_sized(
                                ui,
                                pipe,
                                ui_state,
                                super::call_vote::main_frame::render,
                            )
                        });
                    }
                    "Account" => {
                        add_margins(ui, |ui| {
                            render_centered_max_sized(ui, pipe, ui_state, |ui, pipe, ui_state| {
                                super::account::main_frame::render(ui, ui_state, pipe)
                            });
                        });
                    }
                    // Otherwise assume it's inside the server browser
                    _ => {
                        crate::main_menu::main_frame::render_content(
                            ui,
                            &mut UiRenderPipe {
                                cur_time: pipe.cur_time,
                                user_data: &mut crate::main_menu::user_data::UserData {
                                    browser_data: pipe.user_data.browser_menu.browser_data,
                                    ddnet_info: pipe.user_data.browser_menu.ddnet_info,
                                    icons: pipe.user_data.browser_menu.icons,

                                    demos: pipe.user_data.browser_menu.demos,
                                    demo_info: pipe.user_data.browser_menu.demo_info,
                                    server_info: pipe.user_data.browser_menu.server_info,
                                    render_options: pipe.user_data.browser_menu.render_options,
                                    main_menu: pipe.user_data.browser_menu.main_menu,
                                    config: pipe.user_data.browser_menu.config,
                                    events: pipe.user_data.browser_menu.events,
                                    client_info: pipe.user_data.browser_menu.client_info,

                                    graphics_mt: pipe.user_data.browser_menu.graphics_mt,
                                    backend_handle: pipe.user_data.browser_menu.backend_handle,
                                    shader_storage_handle: pipe
                                        .user_data
                                        .browser_menu
                                        .shader_storage_handle,
                                    buffer_object_handle: pipe
                                        .user_data
                                        .browser_menu
                                        .buffer_object_handle,
                                    stream_handle: pipe.user_data.browser_menu.stream_handle,
                                    canvas_handle: pipe.user_data.browser_menu.canvas_handle,
                                    texture_handle: pipe.user_data.browser_menu.texture_handle,

                                    render_tee: pipe.user_data.browser_menu.render_tee,
                                    flags_container: pipe.user_data.browser_menu.flags_container,
                                    skin_container: pipe.user_data.browser_menu.skin_container,
                                    toolkit_render: pipe.user_data.browser_menu.toolkit_render,
                                    weapons_container: pipe
                                        .user_data
                                        .browser_menu
                                        .weapons_container,
                                    hook_container: pipe.user_data.browser_menu.hook_container,
                                    entities_container: pipe
                                        .user_data
                                        .browser_menu
                                        .entities_container,
                                    freeze_container: pipe.user_data.browser_menu.freeze_container,
                                    emoticons_container: pipe
                                        .user_data
                                        .browser_menu
                                        .emoticons_container,
                                    particles_container: pipe
                                        .user_data
                                        .browser_menu
                                        .particles_container,
                                    ninja_container: pipe.user_data.browser_menu.ninja_container,
                                    game_container: pipe.user_data.browser_menu.game_container,
                                    hud_container: pipe.user_data.browser_menu.hud_container,
                                    ctf_container: pipe.user_data.browser_menu.ctf_container,
                                    theme_container: pipe.user_data.browser_menu.theme_container,

                                    map_render: pipe.user_data.browser_menu.map_render,
                                    tile_set_preview: pipe.user_data.browser_menu.tile_set_preview,

                                    spatial_chat: pipe.user_data.browser_menu.spatial_chat,
                                    player_settings_sync: pipe
                                        .user_data
                                        .browser_menu
                                        .player_settings_sync,

                                    profiles: pipe.user_data.browser_menu.profiles,
                                    profile_tasks: pipe.user_data.browser_menu.profile_tasks,
                                    io: pipe.user_data.browser_menu.io,
                                    monitors: pipe.user_data.browser_menu.monitors,

                                    console_entries: pipe.user_data.browser_menu.console_entries,
                                    parser_cache: pipe.user_data.browser_menu.parser_cache,

                                    raw_input: pipe.user_data.browser_menu.raw_input,
                                    features: pipe.user_data.browser_menu.features,
                                },
                            },
                            ui_state,
                            ui_page_query_name,
                        );
                    }
                }
            });
        });
}

#[instrument(level = "trace", skip_all)]
pub fn render<'a>(
    ui: &mut egui::Ui,
    pipe: &'a mut UiRenderPipe<'a, UserData<'a>>,
    ui_state: &mut UiState,
) {
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui_state.is_ui_open = false;
    }

    super::super::main_menu::main_frame::render_left_bar_and_content(
        ui,
        pipe,
        ui_state,
        INGAME_MENU_UI_PAGE_QUERY,
        INGAME_MENU_FALLBACK_QUERY,
        render_content,
    );
}
