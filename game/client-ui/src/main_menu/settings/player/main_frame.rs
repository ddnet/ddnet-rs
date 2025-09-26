use client_containers::container::ContainerItemIndexType;
use egui::{Align2, Color32, FontId};
use game_config::config::ConfigPlayer;
use game_interface::types::render::character::TeeEye;
use tracing::instrument;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    main_menu::{settings::constants::SETTINGS_SUB_UI_PAGE_QUERY, user_data::UserData},
    utils::render_tee_for_ui,
};

#[instrument(level = "trace", skip_all)]
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    let cur_sub = pipe
        .user_data
        .config
        .engine
        .ui
        .path
        .query
        .get(SETTINGS_SUB_UI_PAGE_QUERY)
        .map(|path| path.as_ref())
        .unwrap_or("")
        .to_string();

    match cur_sub.as_str() {
        "Tee" => {
            super::tee::main_frame::render(ui, pipe, ui_state);
        }
        "Misc" => {
            super::misc::main_frame::render(ui, pipe, ui_state);
        }
        "Assets" => {
            super::assets::main_frame::render(ui, pipe, ui_state);
        }
        #[cfg(feature = "binds")]
        "Controls" => {
            super::controls::main_frame::render(ui, pipe);
        }
        // Player page directly is selected.
        _ => {
            ui.label("Player profiles");

            let config = &mut pipe.user_data.config.game;
            let path = &mut pipe.user_data.config.engine.ui.path;

            if let Some(player) = config.players.get(config.profiles.main as usize) {
                ui.label(format!("Current player's profile: {}", player.name));
            }
            if let Some(player) = config.players.get(config.profiles.dummy.index as usize) {
                ui.label(format!("Current dummy's profile: {}", player.name));
            }

            ui.checkbox(
                &mut config.profiles.dummy.copy_assets_from_main,
                "Dummy copies assets settings from player's profile",
            );
            ui.checkbox(
                &mut config.profiles.dummy.copy_binds_from_main,
                "Dummy copies binds/controls from player's profile",
            );

            let mut next_profile_selected = None;
            let mut next_player_profile = false;
            let mut next_dummy_profile = false;
            let mut delete_profile = false;
            let mut add_profile = false;

            let cur_profile = path.query.get("selected-profile").cloned();
            let search = path.query.entry("profile-search".to_string()).or_default();
            super::super::list::list::render(
                ui,
                config
                    .players
                    .iter()
                    .map(|p| (p.name.as_str(), ContainerItemIndexType::Disk)),
                100.0,
                |_, _| Ok(()),
                |index, _| {
                    cur_profile
                        .as_ref()
                        .is_some_and(|profile_index| (index as u64).to_string().eq(profile_index))
                },
                |ui, index, _, pos, skin_size| {
                    let skin_name = &config.players[index].skin.name;
                    let skin_info = (&config.players[index].skin).into();
                    render_tee_for_ui(
                        pipe.user_data.canvas_handle,
                        pipe.user_data.skin_container,
                        pipe.user_data.render_tee,
                        ui,
                        ui_state,
                        ui.ctx().screen_rect(),
                        Some(ui.clip_rect()),
                        &skin_name.as_str().try_into().unwrap_or_default(),
                        Some(&skin_info),
                        pos,
                        skin_size,
                        TeeEye::Normal,
                    );

                    let is_main = index == config.profiles.main as usize;
                    if is_main {
                        ui.painter().text(
                            egui::pos2(
                                pos.x + skin_size / 2.0 - 1.5,
                                pos.y - skin_size / 2.0 + 5.0,
                            ),
                            Align2::RIGHT_TOP,
                            "\u{f007}",
                            FontId::proportional(12.0),
                            Color32::WHITE,
                        );
                    }
                    if index == config.profiles.dummy.index as usize {
                        ui.painter().text(
                            egui::pos2(
                                pos.x + skin_size / 2.0,
                                pos.y - skin_size / 2.0 + 5.0 + if is_main { 13.0 } else { 0.0 },
                            ),
                            Align2::RIGHT_TOP,
                            "\u{f544}",
                            FontId::proportional(12.0),
                            Color32::WHITE,
                        );
                    }
                },
                |index, _| {
                    next_profile_selected = Some((index as u64).to_string());
                },
                |_, _| None,
                search,
                |ui| {
                    if ui.button("\u{f2ed}").clicked() {
                        delete_profile = true;
                    }
                    if ui.button("\u{f0fe}").clicked() {
                        add_profile = true;
                    }

                    ui.add_space(20.0);

                    if ui.button("As dummy").clicked() {
                        next_dummy_profile = true;
                    }
                    if ui.button("As player").clicked() {
                        next_player_profile = true;
                    }
                },
            );

            if let Some(next_profile_selected) = next_profile_selected {
                *path
                    .query
                    .entry("selected-profile".to_string())
                    .or_default() = next_profile_selected;
            }

            if let Some(selected_profile) = path
                .query
                .get("selected-profile")
                .and_then(|profile| profile.parse::<u64>().ok())
            {
                if (selected_profile as usize) < config.players.len() {
                    if next_dummy_profile {
                        config.profiles.dummy.index = selected_profile;
                        pipe.user_data
                            .player_settings_sync
                            .set_player_info_changed();
                    }
                    if next_player_profile {
                        config.profiles.main = selected_profile;
                        pipe.user_data
                            .player_settings_sync
                            .set_player_info_changed();
                    }
                }

                // delete the profile
                if delete_profile
                    && (selected_profile as usize) < config.players.len()
                    && config.players.len() >= 2
                {
                    config.players.remove(selected_profile as usize);

                    if config.profiles.dummy.index >= selected_profile {
                        config.profiles.dummy.index -= 1;
                    }
                    if config.profiles.main >= selected_profile {
                        config.profiles.main -= 1;
                    }
                }
            }
            if add_profile {
                let new_tee_count = config
                    .players
                    .iter()
                    .filter(|p| p.name.to_lowercase().starts_with("new tee"))
                    .count();
                config.players.push(if new_tee_count > 0 {
                    ConfigPlayer::new(&format!("new tee ({new_tee_count})"))
                } else {
                    ConfigPlayer::new("new tee")
                });
            }
        }
    }
}
