use egui::{Layout, Sense, Shape, epaint::RectShape};
use egui_extras::{Column, Size, StripBuilder, TableBuilder};

use ui_base::{style::bg_frame_color, types::UiRenderPipe};

use crate::main_menu::user_data::UserData;

/// table header + server list
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, cur_page: &str) {
    StripBuilder::new(ui)
        .size(Size::exact(0.0))
        .size(Size::remainder())
        .size(Size::exact(0.0))
        .clip(true)
        .horizontal(|mut strip| {
            strip.empty();
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                ui.painter().add(Shape::Rect(RectShape::filled(
                    ui.available_rect_before_wrap(),
                    0.0,
                    bg_frame_color(),
                )));
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    let height = ui.available_height();
                    let width = ui.available_width();
                    ui.push_id(format!("server-browser-server-list{width}"), |ui| {
                        ui.style_mut().spacing.scroll.floating = false;
                        let table = TableBuilder::new(ui)
                            .min_scrolled_height(50.0)
                            .max_scroll_height(height)
                            .column(Column::exact(30.0))
                            .column(Column::remainder().clip(true))
                            .column(Column::auto().clip(true))
                            .column(Column::remainder().clip(true))
                            .column(Column::exact(70.0))
                            .column(Column::exact(50.0))
                            .resizable(false)
                            .striped(true)
                            .sense(Sense::click())
                            .cell_layout(Layout::left_to_right(egui::Align::Center));
                        table
                            .header(20.0, |mut header| {
                                super::header::render(&mut header, pipe.user_data.config);
                            })
                            .body(|body| {
                                super::server_list::frame::render(body, pipe, cur_page);
                            });
                    });
                });
            });
            strip.empty();
        });
}
