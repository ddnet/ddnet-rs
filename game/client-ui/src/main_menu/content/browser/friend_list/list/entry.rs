use std::borrow::Borrow;

use egui_extras::TableRow;
use game_base::browser_favorite_player::FavoritePlayer;
use game_interface::types::render::character::TeeEye;
use math::math::vector::vec2;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    main_menu::user_data::UserData,
    utils::{render_flag_for_ui, render_tee_for_ui},
};

/// single server list entry
pub fn render(
    mut row: TableRow<'_, '_>,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    favorite: &FavoritePlayer,
) {
    row.col(|ui| {
        let rect = ui.available_rect_before_wrap();
        render_tee_for_ui(
            pipe.user_data.canvas_handle,
            pipe.user_data.skin_container,
            pipe.user_data.render_tee,
            ui,
            ui_state,
            ui.ctx().screen_rect(),
            Some(rect),
            favorite.skin.borrow(),
            Some(&favorite.skin_info),
            vec2::new(rect.center().x, rect.center().y),
            rect.width().min(rect.height()),
            TeeEye::Happy,
        );
    });
    row.col(|ui| {
        ui.label(favorite.name.as_str());
    });
    row.col(|ui| {
        ui.label(favorite.clan.as_str());
    });
    row.col(|ui| {
        let rect = ui.available_rect_before_wrap();
        let key = pipe.user_data.flags_container.default_key.clone();
        render_flag_for_ui(
            pipe.user_data.stream_handle,
            pipe.user_data.canvas_handle,
            pipe.user_data.flags_container,
            ui,
            ui_state,
            ui.ctx().screen_rect(),
            Some(rect),
            &key,
            &favorite.flag,
            vec2::new(rect.center().x, rect.center().y),
            rect.width().min(rect.height()),
        );
    });
}
