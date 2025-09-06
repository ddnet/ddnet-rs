use egui::{Rect, Shape, epaint::RectShape};
use egui_extras::{Column, TableBuilder};
use game_base::server_browser::ServerBrowserServer;
use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
};

use super::list::entry::EntryData;

/// table header + server list
pub fn render(
    ui: &mut egui::Ui,
    full_rect: &Rect,
    pipe: &mut UiRenderPipe<EntryData>,
    ui_state: &mut UiState,
    cur_server: &ServerBrowserServer,
) {
    ui.push_id("player-list", |ui| {
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.painter().add(Shape::Rect(RectShape::filled(
                ui.available_rect_before_wrap(),
                0.0,
                bg_frame_color(),
            )));
            let height = ui.available_height();
            ui.style_mut().spacing.scroll.floating = false;
            let table = TableBuilder::new(ui)
                .min_scrolled_height(10.0)
                .max_scroll_height(height)
                .column(Column::remainder().clip(true))
                .column(Column::remainder().clip(true))
                .column(Column::remainder().clip(true))
                .column(Column::remainder().clip(true))
                .column(Column::remainder().clip(true))
                .resizable(false)
                .striped(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
            table.body(|body| {
                super::list::frame::render(body, full_rect, pipe, ui_state, cur_server);
            });
        });
    });
}
