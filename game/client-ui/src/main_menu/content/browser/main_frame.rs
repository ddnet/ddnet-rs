use egui::Frame;
use egui_extras::{Size, StripBuilder};

use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
};

use crate::main_menu::user_data::UserData;

use super::info_panel::player_list::list::entry::EntryData;

/// big box, rounded edges
pub fn render(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    cur_page: &str,
) {
    let w = ui.available_width();
    let margin = ui.style().spacing.item_spacing.x;
    let width_details = 300.0;
    let max_width_browser = 800.0;
    let width_browser = (w - width_details - margin).clamp(100.0, max_width_browser);
    StripBuilder::new(ui)
        .size(Size::remainder())
        .size(Size::exact(width_browser))
        .size(Size::exact(width_details))
        .size(Size::remainder())
        .horizontal(|mut strip| {
            strip.empty();
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;

                let rect = Frame::default()
                    .fill(bg_frame_color())
                    .corner_radius(5.0)
                    .show(ui, |ui| {
                        let filter_height = 30.0;
                        let bottom_bar_height = 30.0;
                        StripBuilder::new(ui)
                            .size(Size::exact(0.0))
                            .size(Size::exact(filter_height))
                            .size(Size::remainder())
                            .size(Size::exact(bottom_bar_height))
                            .size(Size::exact(0.0))
                            .clip(true)
                            .vertical(|mut strip| {
                                strip.empty();
                                strip.cell(|ui| {
                                    ui.style_mut().wrap_mode = None;
                                    super::filter::render(ui, pipe, ui_state);
                                });
                                strip.cell(|ui| {
                                    ui.style_mut().wrap_mode = None;
                                    super::list::list::render(ui, pipe, cur_page);
                                });
                                strip.cell(|ui| {
                                    ui.style_mut().wrap_mode = None;
                                    super::bottom_bar::render(ui, pipe);
                                });
                                strip.empty();
                            });
                    });
                ui_state.add_blur_rect(rect.response.rect, 5.0);
            });
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                StripBuilder::new(ui)
                    .size(Size::remainder())
                    .size(Size::exact(0.0))
                    .size(Size::remainder())
                    .vertical(|mut strip| {
                        strip.cell(|ui| {
                            ui.style_mut().wrap_mode = None;
                            let browser_data = &pipe.user_data.browser_data;
                            let server = browser_data
                                .find_str(&pipe.user_data.config.storage::<String>("server-addr"));
                            super::info_panel::main_frame::render(
                                ui,
                                &ui.ctx().screen_rect().clone(),
                                &mut UiRenderPipe {
                                    cur_time: pipe.cur_time,
                                    user_data: &mut EntryData {
                                        stream_handle: pipe.user_data.stream_handle,
                                        canvas_handle: pipe.user_data.canvas_handle,
                                        skin_container: pipe.user_data.skin_container,
                                        render_tee: pipe.user_data.render_tee,
                                        flags_container: pipe.user_data.flags_container,
                                    },
                                },
                                ui_state,
                                server.as_ref(),
                            );
                        });
                        strip.empty();
                        strip.cell(|ui| {
                            ui.style_mut().wrap_mode = None;
                            super::friend_list::main_frame::render(ui, pipe, ui_state);
                        });
                    });
            });
            strip.empty();
        });
}
