use std::collections::BTreeMap;

use game_interface::types::resource_key::{NetworkResourceKey, ResourceKey};
use math::math::vector::vec2;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{main_menu::user_data::UserData, utils::render_texture_for_ui};

pub fn theme_list(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    let entries = pipe.user_data.theme_container.entries_index();
    let entries_sorted = entries.into_iter().collect::<BTreeMap<_, _>>();
    let setting = &mut pipe.user_data.config.game.menu.background_map;
    let search_str = pipe
        .user_data
        .config
        .engine
        .ui
        .path
        .query
        .entry("theme-search".to_string())
        .or_default();
    let mut next_name = None;
    super::super::list::list::render(
        ui,
        entries_sorted.iter().map(|(name, &ty)| (name.as_str(), ty)),
        50.0,
        |_, name| {
            let valid: Result<NetworkResourceKey<32>, _> = name.try_into();
            valid.map(|_| ()).map_err(|err| err.into())
        },
        |_, name| setting == name,
        |s| s,
        |ui, _, name, pos, asset_size| {
            let key: ResourceKey = name.try_into().unwrap_or_default();
            let theme = pipe.user_data.theme_container.get_or_default(&key);
            render_texture_for_ui(
                pipe.user_data.stream_handle,
                pipe.user_data.canvas_handle,
                &theme.thumbnail,
                ui,
                ui_state,
                ui.ctx().screen_rect(),
                Some(ui.clip_rect()),
                pos,
                vec2::new(asset_size, asset_size / 2.0),
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
        *setting = next_name;
    }
}
