use std::borrow::Borrow;

use base::network_string::NetworkReducedAsciiString;
use client_containers::{flags::FlagsContainer, skins::SkinContainer};
use client_render_base::render::tee::RenderTee;
use egui::Rect;
use egui_extras::TableRow;
use game_base::server_browser::ServerBrowserPlayer;
use game_interface::types::character_info::MAX_FLAG_NAME_LEN;
use graphics::handles::{
    canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
};
use math::math::vector::vec2;
use ui_base::types::{UiRenderPipe, UiState};

use crate::utils::{render_flag_for_ui, render_tee_for_ui};

pub struct EntryData<'a> {
    pub stream_handle: &'a GraphicsStreamHandle,
    pub canvas_handle: &'a GraphicsCanvasHandle,
    pub skin_container: &'a mut SkinContainer,
    pub render_tee: &'a RenderTee,
    pub flags_container: &'a mut FlagsContainer,
}

/// single server list entry
pub fn render(
    mut row: TableRow<'_, '_>,
    full_rect: &Rect,
    pipe: &mut UiRenderPipe<EntryData>,
    ui_state: &mut UiState,
    player: &ServerBrowserPlayer,
) {
    row.col(|ui| {
        ui.label(player.score.as_str());
    });
    row.col(|ui| {
        let rect = ui.available_rect_before_wrap();
        let center = rect.center();
        render_tee_for_ui(
            pipe.user_data.canvas_handle,
            pipe.user_data.skin_container,
            pipe.user_data.render_tee,
            ui,
            ui_state,
            *full_rect,
            Some(ui.clip_rect()),
            player.skin.name.borrow(),
            Some(&player.skin.info),
            vec2::new(center.x, center.y),
            rect.width().min(rect.height()),
            player.skin.eye,
        );
    });
    row.col(|ui| {
        ui.label(player.name.as_str());
    });
    row.col(|ui| {
        ui.label(player.clan.as_str());
    });
    row.col(|ui| {
        let rect = ui.available_rect_before_wrap();
        let center = rect.center();
        let flag_name = <NetworkReducedAsciiString<MAX_FLAG_NAME_LEN>>::new(
            player.flag.to_lowercase().replace("-", "_").as_str(),
        )
        .or_else(|_| "default".try_into())
        .unwrap();

        let default_key = pipe.user_data.flags_container.default_key.clone();
        render_flag_for_ui(
            pipe.user_data.stream_handle,
            pipe.user_data.canvas_handle,
            pipe.user_data.flags_container,
            ui,
            ui_state,
            *full_rect,
            Some(ui.clip_rect()),
            &default_key,
            flag_name.as_str(),
            vec2::new(center.x, center.y),
            rect.width().min(rect.height() * 2.0),
        );
    });
}
