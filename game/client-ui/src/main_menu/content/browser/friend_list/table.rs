use egui::{Shape, epaint::RectShape};
use egui_extras::{Column, TableBuilder};
use game_base::browser_favorite_player::FavoritePlayers;
use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
};

use crate::main_menu::user_data::UserData;

/// table header + server list
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    let favorites = pipe
        .user_data
        .config
        .storage::<FavoritePlayers>("favorite-players");
    ui.push_id("friend-list", |ui| {
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.painter().add(Shape::Rect(RectShape::filled(
                ui.available_rect_before_wrap(),
                0.0,
                bg_frame_color(),
            )));
            let height = ui.available_height();
            ui.style_mut().spacing.scroll.floating = false;
            let table = TableBuilder::new(ui)
                .min_scrolled_height(50.0)
                .max_scroll_height(height)
                .column(Column::remainder().clip(true))
                .column(Column::remainder().clip(true))
                .column(Column::remainder().clip(true))
                .resizable(false)
                .striped(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
            table.header(0.0, |_| {}).body(|body| {
                super::list::frame::render(body, pipe, ui_state, &favorites);
            });
        });
    });
}
