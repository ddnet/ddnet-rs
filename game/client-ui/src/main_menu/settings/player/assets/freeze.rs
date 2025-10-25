use std::collections::BTreeMap;

use game_interface::types::{
    character_info::MAX_ASSET_NAME_LEN,
    resource_key::{NetworkResourceKey, ResourceKey},
};
use math::math::vector::vec2;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{main_menu::user_data::UserData, utils::render_texture_for_ui};

pub fn freeze_list(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    profile_index: usize,
) {
    let entries = pipe.user_data.freeze_container.entries_index();
    let entries_sorted = entries.into_iter().collect::<BTreeMap<_, _>>();
    let player = &mut pipe.user_data.config.game.players[profile_index];
    let search_str = pipe
        .user_data
        .config
        .engine
        .ui
        .path
        .query
        .entry("freeze-search".to_string())
        .or_default();
    let mut next_name = None;
    super::super::super::list::list::render(
        ui,
        entries_sorted.iter().map(|(name, &ty)| (name.as_str(), ty)),
        100.0,
        |_, name| {
            let valid: Result<NetworkResourceKey<MAX_ASSET_NAME_LEN>, _> = name.try_into();
            valid.map(|_| ()).map_err(|err| err.into())
        },
        |_, name| player.freeze == name,
        |s| s,
        |ui, _, name, pos, asset_size| {
            let key: ResourceKey = name.try_into().unwrap_or_default();
            let freeze = pipe.user_data.freeze_container.get_or_default(&key);
            let single_size = asset_size / 4.0;
            render_texture_for_ui(
                pipe.user_data.stream_handle,
                pipe.user_data.canvas_handle,
                &freeze.freeze_bar_full_left,
                ui,
                ui_state,
                ui.ctx().screen_rect(),
                Some(ui.clip_rect()),
                vec2::new(pos.x - single_size / 2.0 - single_size, pos.y),
                vec2::new(single_size, single_size),
                None,
            );
            render_texture_for_ui(
                pipe.user_data.stream_handle,
                pipe.user_data.canvas_handle,
                &freeze.freeze_bar_full,
                ui,
                ui_state,
                ui.ctx().screen_rect(),
                Some(ui.clip_rect()),
                vec2::new(pos.x - single_size / 2.0, pos.y),
                vec2::new(single_size, single_size),
                None,
            );
            render_texture_for_ui(
                pipe.user_data.stream_handle,
                pipe.user_data.canvas_handle,
                &freeze.freeze_bar_empty,
                ui,
                ui_state,
                ui.ctx().screen_rect(),
                Some(ui.clip_rect()),
                vec2::new(pos.x + single_size / 2.0, pos.y),
                vec2::new(single_size, single_size),
                None,
            );
            render_texture_for_ui(
                pipe.user_data.stream_handle,
                pipe.user_data.canvas_handle,
                &freeze.freeze_bar_empty_right,
                ui,
                ui_state,
                ui.ctx().screen_rect(),
                Some(ui.clip_rect()),
                vec2::new(pos.x + single_size / 2.0 + single_size, pos.y),
                vec2::new(single_size, single_size),
                None,
            );
        },
        |_, name| {
            next_name = Some(name.to_string());
        },
        |_, _| None,
        search_str,
        |_| {},
    );
    if let Some(next_name) = next_name.take() {
        player.freeze = next_name;
        pipe.user_data
            .player_settings_sync
            .set_player_info_changed();
    }
}
