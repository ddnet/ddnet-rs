use std::collections::BTreeMap;

use client_containers::container::ContainerItemIndexType;
use math::math::vector::vec2;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{main_menu::user_data::UserData, utils::render_texture_for_ui};

use super::CommunityIcon;

pub fn community_list(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
) {
    let entries_sorted = pipe
        .user_data
        .ddnet_info
        .communities
        .iter()
        .collect::<BTreeMap<_, _>>();
    let setting = &mut pipe.user_data.config.game.cl.menu_background_map;
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
        |ui, _, name, pos, asset_size| {
            let Some(CommunityIcon::Icon {
                texture,
                width,
                height,
            }) = pipe.user_data.icons.get(name)
            else {
                return;
            };
            let (ratio_w, ratio_h) = if *width >= *height {
                (1.0, *width as f32 / *height as f32)
            } else {
                (*height as f32 / *width as f32, 1.0)
            };

            render_texture_for_ui(
                pipe.user_data.stream_handle,
                pipe.user_data.canvas_handle,
                texture,
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
