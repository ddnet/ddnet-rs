use std::net::SocketAddr;

use config::types::ConfRgb;
use egui::{Color32, ComboBox, CornerRadius, DragValue, Layout, TextEdit, Window};
use egui_extras::{Size, StripBuilder};
use game_config::config::ConfigDummyScreenAnchor;
use game_interface::types::render::character::{PlayerCameraMode, PlayerIngameMode};
use math::math::vector::ubvec4;
use ui_base::{
    style::topbar_secondary_buttons,
    types::{UiRenderPipe, UiState},
};

use crate::{events::UiEvent, ingame_menu::user_data::UserData};

pub fn render(ui: &mut egui::Ui, ui_state: &mut UiState, pipe: &mut UiRenderPipe<UserData>) {
    let config = &mut pipe.user_data.browser_menu.config;
    let mut frame_rect = ui.available_rect_before_wrap();
    frame_rect.set_height(18.0);

    ui.painter().rect_filled(
        frame_rect,
        CornerRadius::default(),
        Color32::from_black_alpha(75),
    );
    ui_state.add_blur_rect(frame_rect, 0.0);
    let players_connected = pipe.user_data.browser_menu.client_info.local_player_count();
    pipe.user_data
        .browser_menu
        .client_info
        .request_active_client_info();
    let active_client_info = pipe.user_data.browser_menu.client_info.active_client_info();
    let options = pipe.user_data.game_server_info.server_options();
    ui.set_style(topbar_secondary_buttons());
    StripBuilder::new(ui)
        .size(Size::relative(0.3))
        .size(Size::remainder())
        .horizontal(|mut strip| {
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                ui.horizontal(|ui| {
                    if !matches!(active_client_info.ingame_mode, PlayerIngameMode::Spectator) {
                        if ui.button("Spectate").clicked() {
                            pipe.user_data
                                .browser_menu
                                .events
                                .push(UiEvent::JoinSpectators);
                        }
                        if ui.button("Kill").clicked() {
                            pipe.user_data.browser_menu.events.push(UiEvent::Kill);
                        }
                        let is_ingame_cam =
                            matches!(active_client_info.camera_mode, PlayerCameraMode::Default);
                        if is_ingame_cam && ui.button("Pause").clicked() {
                            pipe.user_data
                                .browser_menu
                                .events
                                .push(UiEvent::SwitchToFreeCam);
                        } else if !is_ingame_cam && ui.button("Unpause").clicked() {
                            pipe.user_data
                                .browser_menu
                                .events
                                .push(UiEvent::SwitchToDefaultCam);
                        }
                    } else if (options.allow_stages || !options.use_vanilla_sides)
                        && ui.button("Join game").clicked()
                    {
                        pipe.user_data.browser_menu.events.push(UiEvent::JoinGame);
                    }
                    if options.allow_stages {
                        ui.horizontal(|ui| {
                            ui.style_mut().spacing.item_spacing.x = 0.0;

                            ui.menu_button("Team", |ui| {
                                let team = &mut config.game.cl.team;

                                if ui.button("Create & join new team").clicked() {
                                    pipe.user_data
                                        .browser_menu
                                        .events
                                        .push(UiEvent::JoinOwnTeam {
                                            name: team.name.to_string(),
                                            color: ubvec4::new(
                                                team.color.r,
                                                team.color.g,
                                                team.color.b,
                                                255,
                                            ),
                                        });
                                }
                                if ui.button("Join other team").clicked() {
                                    // Show an overview over all teams
                                    config
                                        .path()
                                        .add_query(("team_select".to_string(), "1".to_string()));
                                }
                                if ui.button("Join default team").clicked() {
                                    pipe.user_data
                                        .browser_menu
                                        .events
                                        .push(UiEvent::JoinDefaultTeam);
                                }
                            });

                            if ui.button("\u{f013}").clicked() {
                                // Settings like team color and name
                                config
                                    .path()
                                    .add_query(("team_settings".to_string(), "1".to_string()));
                            }
                        });
                    }

                    if options.use_vanilla_sides {
                        ui.menu_button("Pick side", |ui| {
                            if ui.button("Red side").clicked() {
                                pipe.user_data
                                    .browser_menu
                                    .events
                                    .push(UiEvent::JoinVanillaSide { is_red_side: true });
                            }
                            if ui.button("Blue side").clicked() {
                                pipe.user_data
                                    .browser_menu
                                    .events
                                    .push(UiEvent::JoinVanillaSide { is_red_side: false });
                            }
                        });
                    }
                });

                let show_dummy_settings = config
                    .path()
                    .query
                    .get("team_settings")
                    .map(|v| v == "1")
                    .unwrap_or_default();
                if show_dummy_settings {
                    let mut open = show_dummy_settings;
                    Window::new("Team settings")
                        .open(&mut open)
                        .collapsible(false)
                        .show(ui.ctx(), |ui| {
                            let team = &mut config.game.cl.team;

                            ui.label("Name:");
                            if ui
                                .add(TextEdit::singleline(&mut team.name).char_limit(24))
                                .changed()
                            {
                                pipe.user_data
                                    .browser_menu
                                    .player_settings_sync
                                    .set_team_settings_changed();
                            }
                            ui.label("Color:");
                            let mut colors = [team.color.r, team.color.g, team.color.b];
                            if ui.color_edit_button_srgb(&mut colors).changed() {
                                pipe.user_data
                                    .browser_menu
                                    .player_settings_sync
                                    .set_team_settings_changed();
                            }
                            team.color = ConfRgb {
                                r: colors[0],
                                g: colors[1],
                                b: colors[2],
                            };
                        });
                    if !open {
                        config.path().query.remove("team_settings");
                    }
                }

                let show_select_team = config
                    .path()
                    .query
                    .get("team_select")
                    .map(|v| v == "1")
                    .unwrap_or_default();
                if show_select_team {
                    let mut open = show_select_team;
                    Window::new("Team select")
                        .open(&mut open)
                        .collapsible(false)
                        .show(ui.ctx(), |ui| {
                            let team_selected = config
                                .path()
                                .query
                                .entry("team_selected".to_string())
                                .or_default();
                            ComboBox::new("team_select_combobox", "")
                                .selected_text(if team_selected.is_empty() {
                                    "Default".to_string()
                                } else {
                                    active_client_info
                                        .stage_names
                                        .get(team_selected)
                                        .cloned()
                                        .unwrap_or_else(|| "Team does not exist".to_string())
                                })
                                .show_ui(ui, |ui| {
                                    for stage_name in active_client_info.stage_names.iter() {
                                        if ui
                                            .button(if stage_name.is_empty() {
                                                "Default"
                                            } else {
                                                stage_name
                                            })
                                            .clicked()
                                        {
                                            *team_selected = stage_name.clone();
                                        }
                                    }
                                });

                            if ui.button("Join").clicked() {
                                pipe.user_data
                                    .browser_menu
                                    .events
                                    .push(UiEvent::JoinOtherTeam(team_selected.clone()));
                            }
                        });
                    if !open {
                        config.path().query.remove("team_select");
                        config.path().query.remove("team_selected");
                    }
                }
            });
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                ui.with_layout(
                    Layout::right_to_left(egui::Align::Min).with_main_wrap(true),
                    |ui| {
                        if ui.button("Disconnect").clicked() {
                            pipe.user_data.browser_menu.events.push(UiEvent::Disconnect);
                            config.path().route("");
                        }
                        if config.engine.dbg.app {
                            if ui.button("(dbg) reconnect").clicked() {
                                pipe.user_data.browser_menu.events.push(UiEvent::Disconnect);
                                pipe.user_data.browser_menu.events.push(UiEvent::Connect {
                                    addr: config.storage_opt("server-addr").unwrap_or_else(|| {
                                        SocketAddr::V4("127.0.0.1".parse().unwrap())
                                    }),
                                    cert_hash: config.storage("server-cert"),

                                    rcon_secret: config.storage("rcon-secret"),
                                    can_start_internal_server: true,
                                    can_connect_internal_server: true,
                                });
                            }
                            if ui
                                .button(format!(
                                    "(dbg) connect dummy ({})",
                                    players_connected.saturating_sub(1)
                                ))
                                .clicked()
                            {
                                pipe.user_data
                                    .browser_menu
                                    .events
                                    .push(UiEvent::ConnectLocalPlayer { as_dummy: true });
                            }
                            if ui.button("(dbg) disconnect dummy").clicked() {
                                pipe.user_data
                                    .browser_menu
                                    .events
                                    .push(UiEvent::DisconnectLocalPlayer);
                            }
                        } else {
                            ui.horizontal(|ui| {
                                ui.style_mut().spacing.item_spacing.x = 0.0;

                                ui.menu_button("\u{f013}", |ui| {
                                    if ui.button("Dummy settings").clicked() {
                                        // settings like if a mini screen of the dummy should show up
                                        // and how big this screen should be etc.
                                        config.path().add_query((
                                            "dummy_settings".to_string(),
                                            "1".to_string(),
                                        ));
                                    }
                                });

                                // dummy settings
                                let show_dummy_settings = config
                                    .path()
                                    .query
                                    .get("dummy_settings")
                                    .map(|v| v == "1")
                                    .unwrap_or_default();
                                if show_dummy_settings {
                                    let mut open = show_dummy_settings;
                                    Window::new("Dummy settings")
                                        .open(&mut open)
                                        .collapsible(false)
                                        .show(ui.ctx(), |ui| {
                                            let dummy = &mut config.game.cl.dummy;
                                            ui.checkbox(
                                                &mut dummy.mini_screen,
                                                "Show dummy in mini screen.",
                                            );
                                            ui.label("Sceen width:");
                                            ui.add(
                                                DragValue::new(&mut dummy.screen_width)
                                                    .update_while_editing(false)
                                                    .range(1..=100),
                                            );
                                            ui.label("Sceen height:");
                                            ui.add(
                                                DragValue::new(&mut dummy.screen_height)
                                                    .update_while_editing(false)
                                                    .range(1..=100),
                                            );

                                            let anchors = [
                                                "Top left",
                                                "Top right",
                                                "Bottom left",
                                                "Bottom right",
                                            ];
                                            egui::ComboBox::new("select-anchor", "")
                                                .selected_text(
                                                    anchors[dummy.screen_anchor as usize],
                                                )
                                                .show_ui(ui, |ui| {
                                                    let mut btn =
                                                        |anchor: ConfigDummyScreenAnchor| {
                                                            if ui
                                                                .button(anchors[anchor as usize])
                                                                .clicked()
                                                            {
                                                                dummy.screen_anchor = anchor;
                                                            }
                                                        };
                                                    btn(ConfigDummyScreenAnchor::TopLeft);
                                                    btn(ConfigDummyScreenAnchor::TopRight);
                                                    btn(ConfigDummyScreenAnchor::BottomLeft);
                                                    btn(ConfigDummyScreenAnchor::BottomRight);
                                                });
                                        });
                                    if !open {
                                        config.path().query.remove("dummy_settings");
                                    }
                                }

                                if players_connected > 1 {
                                    if ui.button("Disconnect dummy").clicked() {
                                        pipe.user_data
                                            .browser_menu
                                            .events
                                            .push(UiEvent::DisconnectLocalPlayer);
                                    }
                                } else if ui.button("Connect dummy").clicked() {
                                    pipe.user_data
                                        .browser_menu
                                        .events
                                        .push(UiEvent::ConnectLocalPlayer { as_dummy: true });
                                }
                            });
                        }

                        let path = config.path();
                        let is_recording = path.query.contains_key("demo-record-manual");
                        if !is_recording && ui.button("Record demo").clicked() {
                            path.query.insert("demo-record-manual".into(), "1".into());
                            pipe.user_data.browser_menu.events.push(UiEvent::RecordDemo);
                        }
                        if is_recording && ui.button("Stop record").clicked() {
                            path.query.remove("demo-record-manual");
                            pipe.user_data
                                .browser_menu
                                .events
                                .push(UiEvent::StopRecordDemo);
                        }
                        if ui.button("Instant replay").clicked() {
                            pipe.user_data
                                .browser_menu
                                .events
                                .push(UiEvent::InstantReplay);
                        }
                    },
                );
            });
        });
}
