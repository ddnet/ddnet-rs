use egui::{Frame, Grid, Layout, Rect, RichText};
use egui_extras::{Size, StripBuilder};
use game_base::server_browser::ServerBrowserServer;

use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
};

use super::player_list::list::entry::EntryData;

/// big box, rounded edges
pub fn render(
    ui: &mut egui::Ui,
    full_rect: &Rect,
    pipe: &mut UiRenderPipe<EntryData>,
    ui_state: &mut UiState,
    cur_server: Option<&ServerBrowserServer>,
) {
    let res = Frame::default()
        .fill(bg_frame_color())
        .corner_radius(5.0)
        .show(ui, |ui| {
            let item_spacing = ui.style().spacing.item_spacing.x;
            StripBuilder::new(ui)
                .size(Size::exact(0.0))
                .size(Size::remainder())
                .size(Size::exact(0.0))
                .horizontal(|mut strip| {
                    strip.empty();
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        let server_details_height = 70.0;
                        StripBuilder::new(ui)
                            .size(Size::exact(0.0))
                            .size(Size::exact(server_details_height))
                            .size(Size::remainder())
                            .size(Size::exact(item_spacing))
                            .clip(true)
                            .vertical(|mut strip| {
                                strip.empty();
                                strip.cell(|ui| {
                                    ui.style_mut().wrap_mode = None;
                                    StripBuilder::new(ui)
                                        .size(Size::exact(30.0))
                                        .size(Size::remainder())
                                        .vertical(|mut strip| {
                                            strip.cell(|ui| {
                                                ui.style_mut().wrap_mode = None;
                                                ui.with_layout(
                                                    Layout::left_to_right(egui::Align::Center)
                                                        .with_main_align(egui::Align::Center)
                                                        .with_main_justify(true),
                                                    |ui| {
                                                        ui.label("\u{f05a} Server details");
                                                    },
                                                );
                                            });
                                            strip.cell(|ui| {
                                                ui.style_mut().wrap_mode = None;
                                                if let Some(cur_server) = cur_server {
                                                    Grid::new("server-details-short")
                                                        .num_columns(2)
                                                        .show(ui, |ui| {
                                                            ui.label(
                                                                RichText::new("Version:")
                                                                    .size(10.0),
                                                            );
                                                            ui.label(
                                                                RichText::new(
                                                                    cur_server
                                                                        .info
                                                                        .version
                                                                        .as_str(),
                                                                )
                                                                .size(10.0),
                                                            );
                                                            ui.end_row();
                                                            ui.label(
                                                                RichText::new("Game type:")
                                                                    .size(10.0),
                                                            );
                                                            ui.label(
                                                                RichText::new(
                                                                    cur_server
                                                                        .info
                                                                        .game_type
                                                                        .as_str(),
                                                                )
                                                                .size(10.0),
                                                            );
                                                            ui.end_row();
                                                        });
                                                } else {
                                                    ui.label("No server selected");
                                                }
                                            });
                                        });
                                });
                                strip.cell(|ui| {
                                    ui.style_mut().wrap_mode = None;
                                    if let Some(cur_server) = cur_server {
                                        super::player_list::table::render(
                                            ui, full_rect, pipe, ui_state, cur_server,
                                        );
                                    }
                                });
                                strip.empty();
                            });
                    });
                    strip.empty();
                });
        });
    ui_state.add_blur_rect(res.response.rect, 5.0);
}
