use std::collections::BTreeMap;

use game_interface::types::{character_info::MAX_ASSET_NAME_LEN, resource_key::NetworkResourceKey};
use math::math::vector::vec2;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{main_menu::user_data::UserData, utils::render_hook_for_ui};

pub fn hook_list(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    profile_index: usize,
) {
    let entries = pipe.user_data.hook_container.entries_index();
    let entries_sorted = entries.into_iter().collect::<BTreeMap<_, _>>();
    let player = &mut pipe.user_data.config.game.players[profile_index];
    let search_str = pipe
        .user_data
        .config
        .engine
        .ui
        .path
        .query
        .entry("hooks-search".to_string())
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
        |_, name| player.hook == name,
        |s| s,
        |ui, _, name, pos, asset_size| {
            let hook_size = asset_size / 2.0;
            let pos = pos - vec2::new(hook_size * 1.25, 0.0);
            render_hook_for_ui(
                pipe.user_data.canvas_handle,
                pipe.user_data.hook_container,
                pipe.user_data.toolkit_render,
                ui,
                ui_state,
                ui.ctx().screen_rect(),
                Some(ui.clip_rect()),
                &name.try_into().unwrap_or_default(),
                pos,
                hook_size,
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
        player.hook = next_name;
        pipe.user_data
            .player_settings_sync
            .set_player_info_changed();
    }
}
