use std::collections::BTreeMap;

use game_interface::types::{
    character_info::MAX_ASSET_NAME_LEN,
    resource_key::{NetworkResourceKey, ResourceKey},
};
use graphics::handles::texture::texture::TextureContainer;
use math::math::vector::vec2;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{main_menu::user_data::UserData, utils::render_texture_for_ui};

pub fn ctf_list(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    profile_index: usize,
) {
    let entries = pipe.user_data.ctf_container.entries_index();
    let entries_sorted = entries.into_iter().collect::<BTreeMap<_, _>>();
    let player = &mut pipe.user_data.config.game.players[profile_index];
    let search_str = pipe
        .user_data
        .config
        .engine
        .ui
        .path
        .query
        .entry("ctf-search".to_string())
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
        |_, name| player.ctf == name,
        |ui, _, name, pos, asset_size| {
            let item_size = asset_size / 2.0;
            let pos = pos + vec2::new(item_size / 2.0 - (asset_size / 2.0), 0.0);
            let key: ResourceKey = name.try_into().unwrap_or_default();
            let ctf = pipe.user_data.ctf_container.get_or_default(&key);

            let mut render_texture = |texture: &TextureContainer, index: usize| {
                let x = (index % 2) as f32;
                render_texture_for_ui(
                    pipe.user_data.stream_handle,
                    pipe.user_data.canvas_handle,
                    texture,
                    ui,
                    ui_state,
                    ui.ctx().screen_rect(),
                    Some(ui.clip_rect()),
                    pos + vec2::new(x * item_size, 0.0),
                    vec2::new(item_size, asset_size),
                    None,
                );
            };
            let mut index = 0;

            render_texture(&ctf.flag_blue, index);
            index += 1;
            render_texture(&ctf.flag_red, index);
            //index += 1;
        },
        |_, name| {
            next_name = Some(name.to_string());
        },
        |_, _| None,
        search_str,
        |_| {},
    );
    if let Some(next_name) = next_name.take() {
        player.ctf = next_name;
        pipe.user_data
            .player_settings_sync
            .set_player_info_changed();
    }
}
