use std::collections::BTreeMap;

use client_containers::container::ContainerItemIndexType;
use egui::Layout;
use game_interface::types::{
    character_info::{MAX_CHARACTER_CLAN_LEN, MAX_CHARACTER_NAME_LEN, MAX_FLAG_NAME_LEN},
    resource_key::NetworkResourceKey,
};
use ui_base::{
    components::clearable_edit_field::clearable_edit_field,
    types::{UiRenderPipe, UiState},
};

use crate::{
    main_menu::{settings::player::profile_selector::profile_selector, user_data::UserData},
    utils::render_flag_for_ui,
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    ui.with_layout(Layout::top_down(egui::Align::Min), |ui| {
        let config = &mut pipe.user_data.config.game;

        let profile_index = profile_selector(
            ui,
            "misc-profile-selection",
            config,
            &mut pipe.user_data.config.engine,
        );
        ui.add_space(5.0);
        let player = &mut config.players[profile_index as usize];
        egui::Grid::new("misc-name-clan-editbox")
            .spacing([2.0, 4.0])
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Name");
                if clearable_edit_field(
                    ui,
                    &mut player.name,
                    Some(200.0),
                    Some(MAX_CHARACTER_NAME_LEN),
                )
                .is_some_and(|i| i.changed())
                {
                    pipe.user_data
                        .player_settings_sync
                        .set_player_info_changed();
                }
                ui.end_row();
                ui.label("Clan");
                if clearable_edit_field(
                    ui,
                    &mut player.clan,
                    Some(200.0),
                    Some(MAX_CHARACTER_CLAN_LEN),
                )
                .is_some_and(|i| i.changed())
                {
                    pipe.user_data
                        .player_settings_sync
                        .set_player_info_changed();
                }
                ui.end_row();
            });

        let default_key = pipe.user_data.flags_container.default_key.clone();
        let entries = pipe.user_data.flags_container.get_or_default(&default_key);

        let entries_sorted = entries
            .flags
            .keys()
            .map(|flag| (flag.to_string(), ContainerItemIndexType::Disk))
            .collect::<BTreeMap<_, _>>();
        let player = &mut pipe.user_data.config.game.players[profile_index as usize];
        let flag_search = pipe
            .user_data
            .config
            .engine
            .ui
            .path
            .query
            .entry("flag-search".to_string())
            .or_default();
        let mut next_name = None;
        super::super::super::list::list::render(
            ui,
            entries_sorted.iter().map(|(name, &ty)| (name.as_str(), ty)),
            50.0,
            |_, name| {
                let flag_valid: Result<NetworkResourceKey<MAX_FLAG_NAME_LEN>, _> =
                    name.to_lowercase().replace("-", "_").as_str().try_into();
                flag_valid.map(|_| ()).map_err(|err| err.into())
            },
            |_, name| player.flag == name.to_lowercase().replace("-", "_"),
            |s| s,
            |ui, _, name, pos, flag_size| {
                render_flag_for_ui(
                    pipe.user_data.stream_handle,
                    pipe.user_data.canvas_handle,
                    pipe.user_data.flags_container,
                    ui,
                    ui_state,
                    ui.ctx().screen_rect(),
                    Some(ui.clip_rect()),
                    &default_key,
                    &name.to_lowercase().replace("-", "_"),
                    pos,
                    flag_size,
                );
            },
            |_, name| {
                next_name = Some(name.to_string());
            },
            |_, _| None,
            flag_search,
            |_| {},
        );
        if let Some(next_name) = next_name.take() {
            player.flag = next_name.to_lowercase().replace("-", "_");
            pipe.user_data
                .player_settings_sync
                .set_player_info_changed();
        }
    });
}
