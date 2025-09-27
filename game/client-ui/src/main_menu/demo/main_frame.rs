use std::path::PathBuf;

use base::duration_ext::DurationToRaceStr;
use egui::{Align2, ComboBox, DragValue, Grid, Vec2};
use egui_extras::{Size, StripBuilder};
use tracing::instrument;
use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
    utils::add_horizontal_margins,
};

use crate::{
    events::UiEvent,
    main_menu::{constants::MENU_DEMO_NAME, user_data::UserData},
};

fn record_settings(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>) {
    egui::Window::new("Export demo to video")
        .anchor(Align2::CENTER_CENTER, Vec2::default())
        .show(ui.ctx(), |ui| {
            let config = &mut pipe.user_data.config;
            let file_name = config
                .engine
                .ui
                .path
                .query
                .entry("video-file-name".to_string())
                .or_default();
            Grid::new("record-settings").num_columns(2).show(ui, |ui| {
                let config = &mut config.game;
                ui.label("Name:");
                ui.text_edit_singleline(file_name);
                ui.end_row();

                ui.label("Fps:");
                ui.add(DragValue::new(&mut config.cl.recorder.fps).update_while_editing(false));
                ui.end_row();

                ui.label("Width:");
                ui.add(DragValue::new(&mut config.cl.recorder.width).update_while_editing(false));
                ui.end_row();

                ui.label("Height:");
                ui.add(DragValue::new(&mut config.cl.recorder.height).update_while_editing(false));
                ui.end_row();

                ui.label("Pixels per point (similar to DPI):");
                ui.add(
                    DragValue::new(&mut config.cl.recorder.pixels_per_point)
                        .update_while_editing(false),
                );
                ui.end_row();

                ui.label("Crf (0 = lossless, 51 = worst):");
                ui.add(
                    DragValue::new(&mut config.cl.recorder.crf)
                        .update_while_editing(false)
                        .range(0..=51),
                );
                ui.end_row();

                ui.label("Hardware acceleration (GPU):");
                ComboBox::new("hw_accel_combobox", "")
                    .selected_text(&config.cl.recorder.hw_accel)
                    .show_ui(ui, |ui| {
                        if ui.button("None").clicked() {
                            config.cl.recorder.hw_accel = "".to_string();
                        }
                        if ui.button("VAAPI (Linux)").clicked() {
                            config.cl.recorder.hw_accel = "vaapi".to_string();
                        }
                        if ui.button("Cuda (NVIDIA)").clicked() {
                            config.cl.recorder.hw_accel = "cuda".to_string();
                        }
                        if ui.button("AMF (AMD on Windows)").clicked() {
                            config.cl.recorder.hw_accel = "amf".to_string();
                        }
                    });
                ui.end_row();
            });
            let video_name = file_name.clone();
            ui.horizontal(|ui| {
                if ui.button("Abort").clicked() {
                    config.path().query.remove("recorder-clicked");
                }
                if ui.button("Ok").clicked() {
                    let cur_path: String = config.storage("demo-path");
                    let cur_path: PathBuf = cur_path.into();
                    let name: String = config.storage("selected-demo");

                    let demo_path = cur_path.join(name);

                    pipe.user_data.events.push(UiEvent::EncodeDemoToVideo {
                        name: demo_path,
                        video_name,
                    });
                    config.path().query.remove("recorder-clicked");
                }
            });
        });
}

#[instrument(level = "trace", skip_all)]
pub fn render(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    pipe: &mut UiRenderPipe<UserData>,
    cur_page: &str,
) {
    if cur_page == MENU_DEMO_NAME {
        let w = ui.available_width();
        let margin = ui.style().spacing.item_spacing.x;
        let width_info = 300.0;
        let max_width = 800.0;
        let width = (w - width_info - margin).clamp(100.0, max_width);
        StripBuilder::new(ui)
            .size(Size::remainder())
            .size(Size::exact(width))
            .size(Size::exact(width_info))
            .size(Size::remainder())
            .horizontal(|mut strip| {
                strip.empty();
                strip.cell(|ui| {
                    ui.style_mut().wrap_mode = None;
                    let res = egui::Frame::NONE
                        .fill(bg_frame_color())
                        .corner_radius(5.0)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.set_height(ui.available_height());

                            add_horizontal_margins(ui, |ui| {
                                StripBuilder::new(ui)
                                    .size(Size::exact(30.0))
                                    .size(Size::remainder())
                                    .size(Size::exact(30.0))
                                    .vertical(|mut strip| {
                                        strip.cell(|ui| {
                                            ui.style_mut().wrap_mode = None;
                                            super::search::render(ui, pipe);
                                        });
                                        strip.cell(|ui| {
                                            ui.style_mut().wrap_mode = None;
                                            super::list::render(ui, pipe);
                                        });
                                        strip.cell(|ui| {
                                            ui.style_mut().wrap_mode = None;
                                            ui.horizontal(|ui| {
                                                if ui.button("Play").clicked() {
                                                    let cur_path: String =
                                                        pipe.user_data.config.storage("demo-path");
                                                    let cur_path: PathBuf = cur_path.into();
                                                    let name: String = pipe
                                                        .user_data
                                                        .config
                                                        .storage("selected-demo");

                                                    let new_path = cur_path.join(name);
                                                    pipe.user_data
                                                        .events
                                                        .push(UiEvent::PlayDemo { name: new_path });
                                                }
                                                if pipe.user_data.features.demo_to_video
                                                    && ui.button("Record").clicked()
                                                {
                                                    pipe.user_data.config.path().query.insert(
                                                        "recorder-clicked".to_string(),
                                                        "true".to_string(),
                                                    );
                                                }

                                                if pipe
                                                    .user_data
                                                    .config
                                                    .path()
                                                    .query
                                                    .contains_key("recorder-clicked")
                                                {
                                                    let name: String = pipe
                                                        .user_data
                                                        .config
                                                        .storage("selected-demo");
                                                    let file_name = pipe
                                                        .user_data
                                                        .config
                                                        .path()
                                                        .query
                                                        .entry("video-file-name".to_string())
                                                        .or_default();
                                                    let name: PathBuf = name.into();
                                                    *file_name = name
                                                        .file_stem()
                                                        .map(|s| s.to_string_lossy().to_string())
                                                        .unwrap_or_default();
                                                    record_settings(ui, pipe);
                                                }
                                            });
                                        });
                                    });
                            });
                        });
                    ui_state.add_blur_rect(res.response.rect, 5.0);
                });
                strip.cell(|ui| {
                    ui.style_mut().wrap_mode = None;
                    let res = egui::Frame::NONE
                        .fill(bg_frame_color())
                        .corner_radius(5.0)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.set_height(ui.available_height());

                            StripBuilder::new(ui)
                                .size(Size::exact(30.0))
                                .size(Size::remainder())
                                .vertical(|mut strip| {
                                    strip.cell(|ui| {
                                        ui.style_mut().wrap_mode = None;
                                        ui.centered_and_justified(|ui| {
                                            ui.label("\u{f05a} Demo information");
                                        });
                                    });
                                    strip.cell(|ui| {
                                        ui.style_mut().wrap_mode = None;
                                        if let Some((header, header_ext)) = pipe.user_data.demo_info
                                        {
                                            Grid::new("demo-info").num_columns(2).show(ui, |ui| {
                                                ui.label("Map:");
                                                ui.label(header_ext.map.as_str());
                                                ui.end_row();
                                                ui.label("Length:");
                                                ui.label(header.len.to_race_string());
                                                ui.end_row();
                                            });
                                        }
                                    });
                                });
                        });
                    ui_state.add_blur_rect(res.response.rect, 5.0);
                });
                strip.empty();
            });
    }
}
