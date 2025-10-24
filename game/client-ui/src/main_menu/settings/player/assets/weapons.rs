use std::collections::BTreeMap;

use game_interface::types::{
    character_info::MAX_ASSET_NAME_LEN, emoticons::IntoEnumIterator,
    resource_key::NetworkResourceKey, weapons::WeaponType,
};
use math::math::vector::vec2;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{main_menu::user_data::UserData, utils::render_weapon_for_ui};

pub fn weapon_list(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    profile_index: usize,
) {
    let entries = pipe.user_data.weapons_container.entries_index();
    let entries_sorted = entries.into_iter().collect::<BTreeMap<_, _>>();
    let player = &mut pipe.user_data.config.game.players[profile_index];
    let weapons_search = pipe
        .user_data
        .config
        .engine
        .ui
        .path
        .query
        .entry("weapons-search".to_string())
        .or_default();
    let mut next_name = None;
    super::super::super::list::list::render(
        ui,
        entries_sorted.iter().map(|(name, &ty)| (name.as_str(), ty)),
        150.0,
        |_, name| {
            let wpn_valid: Result<NetworkResourceKey<MAX_ASSET_NAME_LEN>, _> = name.try_into();
            wpn_valid.map(|_| ()).map_err(|err| err.into())
        },
        |_, name| player.weapon == name,
        |s| s,
        |ui, _, name, pos, asset_size| {
            let weapon_size = asset_size / 6.0;
            let pos = pos - vec2::new(asset_size / 4.0, weapon_size);
            for (index, weapon) in WeaponType::iter().enumerate() {
                let x = (index % 2) as f32;
                let y = (index / 2) as f32;
                render_weapon_for_ui(
                    pipe.user_data.canvas_handle,
                    pipe.user_data.weapons_container,
                    pipe.user_data.toolkit_render,
                    ui,
                    ui_state,
                    ui.ctx().screen_rect(),
                    Some(ui.clip_rect()),
                    &name.try_into().unwrap_or_default(),
                    weapon,
                    pos + vec2::new(x * weapon_size * 3.0, y * weapon_size * 1.5),
                    weapon_size,
                );
            }
        },
        |_, name| {
            next_name = Some(name.to_string());
        },
        |_, _| None,
        weapons_search,
        |_| {},
    );
    if let Some(next_name) = next_name.take() {
        player.weapon = next_name;
        pipe.user_data
            .player_settings_sync
            .set_player_info_changed();
    }
}
