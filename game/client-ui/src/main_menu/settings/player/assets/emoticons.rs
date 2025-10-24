use std::collections::BTreeMap;

use game_interface::types::{
    character_info::MAX_ASSET_NAME_LEN,
    emoticons::{EmoticonType, IntoEnumIterator},
    resource_key::NetworkResourceKey,
};
use math::math::vector::vec2;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{main_menu::user_data::UserData, utils::render_emoticon_for_ui};

pub fn emoticons_list(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    profile_index: usize,
) {
    let entries = pipe.user_data.emoticons_container.entries_index();
    let entries_sorted = entries.into_iter().collect::<BTreeMap<_, _>>();
    let player = &mut pipe.user_data.config.game.players[profile_index];
    let search_str = pipe
        .user_data
        .config
        .engine
        .ui
        .path
        .query
        .entry("emoticons-search".to_string())
        .or_default();
    let mut next_name = None;
    super::super::super::list::list::render(
        ui,
        entries_sorted.iter().map(|(name, &ty)| (name.as_str(), ty)),
        150.0,
        |_, name| {
            let valid: Result<NetworkResourceKey<MAX_ASSET_NAME_LEN>, _> = name.try_into();
            valid.map(|_| ()).map_err(|err| err.into())
        },
        |_, name| player.emoticons == name,
        |s| s,
        |ui, _, name, pos, asset_size| {
            let emoticons_size = asset_size / 4.0;
            let pos = pos
                + vec2::new(
                    emoticons_size / 2.0 - (asset_size / 2.0),
                    emoticons_size / 2.0 - (asset_size / 2.0),
                );
            for (index, emoticons) in EmoticonType::iter().enumerate() {
                let x = (index % 4) as f32;
                let y = (index / 4) as f32;
                render_emoticon_for_ui(
                    pipe.user_data.stream_handle,
                    pipe.user_data.canvas_handle,
                    pipe.user_data.emoticons_container,
                    ui,
                    ui_state,
                    ui.ctx().screen_rect(),
                    Some(ui.clip_rect()),
                    &name.try_into().unwrap_or_default(),
                    pos + vec2::new(x * emoticons_size, y * emoticons_size),
                    emoticons_size,
                    emoticons,
                );
            }
        },
        |_, name| {
            next_name = Some(name.to_string());
        },
        |_, _| None,
        search_str,
        |_| {},
    );
    if let Some(next_name) = next_name.take() {
        player.emoticons = next_name;
        pipe.user_data
            .player_settings_sync
            .set_player_info_changed();
    }
}
