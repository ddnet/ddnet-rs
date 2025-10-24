use std::collections::BTreeMap;

use base::{hash::decode_hash, reduced_ascii_str::ReducedAsciiString};
use client_containers::container::ContainerItemIndexType;
use game_interface::types::resource_key::ResourceKey;
use math::math::vector::vec2;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    main_menu::{communities::IconUrlHash, user_data::UserData},
    thumbnail_container::Thumbnail,
    utils::render_texture_for_ui,
};

pub fn community_list(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
) {
    let communities = &pipe.user_data.ddnet_info.communities;
    let entries_sorted = communities.iter().collect::<BTreeMap<_, _>>();
    let setting = &mut pipe.user_data.config.game.menu.background_map;
    let search_str = pipe
        .user_data
        .config
        .engine
        .ui
        .path
        .query
        .entry("community-explore-search".to_string())
        .or_default();
    let mut next_name = None;
    super::super::settings::list::list::render(
        ui,
        entries_sorted
            .keys()
            .map(|name| (name.as_str(), ContainerItemIndexType::Disk)),
        100.0,
        |_, _| Ok(()),
        |_, name| setting == name,
        |name| communities.get(&name).unwrap().name.clone(),
        |ui, _, name, pos, asset_size| {
            let entry = communities.get(name).unwrap();
            let key = ResourceKey {
                name: ReducedAsciiString::from_str_lossy(&entry.id),
                hash: if let IconUrlHash::Blake3 { blake3: hash } = &entry.icon.hash {
                    decode_hash(hash)
                } else {
                    None
                },
            };
            let Thumbnail {
                thumbnail,
                width,
                height,
            } = pipe.user_data.icons.get_or_default(&key);
            let (ratio_w, ratio_h) = if *width >= *height {
                (1.0, *width as f32 / *height as f32)
            } else {
                (*height as f32 / *width as f32, 1.0)
            };

            render_texture_for_ui(
                pipe.user_data.stream_handle,
                pipe.user_data.canvas_handle,
                thumbnail,
                ui,
                ui_state,
                ui.ctx().screen_rect(),
                Some(ui.clip_rect()),
                pos,
                vec2::new(asset_size / ratio_w, asset_size / ratio_h),
                None,
            );
        },
        |_, name| {
            next_name = Some(name.to_string());
        },
        |v, _| {
            pipe.user_data
                .ddnet_info
                .communities
                .get(v)
                .map(|c| c.name.clone().into())
        },
        search_str,
        |_| {},
    );
    if let Some(next_name) = next_name.take() {
        *setting = next_name;
    }
}
