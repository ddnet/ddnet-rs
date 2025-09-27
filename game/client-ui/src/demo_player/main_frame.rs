use std::time::Duration;

use base::duration_ext::DurationToRaceStr;
use egui::{
    Align2, Button, Color32, CornerRadius, FontId, Frame, Grid, Layout, Rect, Shadow, Stroke,
    TopBottomPanel, Vec2, Window,
};

use tracing::instrument;
use ui_base::{
    types::{UiRenderPipe, UiState},
    utils::{add_horizontal_margins, text_sized},
};

use crate::demo_player::user_data::{DemoViewerEvent, DemoViewerEventExport};

use super::user_data::UserData;

/// not required
#[instrument(level = "trace", skip_all)]
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    let res = TopBottomPanel::bottom("demo-main")
        .exact_height(40.0)
        .frame(
            Frame::NONE
                .shadow(Shadow::NONE)
                .stroke(Stroke::NONE)
                .fill(Color32::from_black_alpha(80)),
        )
        .show_separator_line(false)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            ui.set_clip_rect(ui.ctx().screen_rect());
            ui.style_mut().spacing.item_spacing.y = 0.0;
            let mut rect = ui.available_rect_before_wrap();
            ui.add_space(10.0);

            rect.set_height(10.0);
            let state = &mut *pipe.user_data.state;
            if let Some(pointer_pos) = ui.input(|i| {
                ((state.pointer_on_timeline && i.pointer.primary_down())
                    || !state.pointer_on_timeline && i.pointer.primary_pressed())
                .then_some(
                    i.pointer
                        .latest_pos()
                        .or(i.pointer.hover_pos())
                        .or(i.pointer.interact_pos())
                        .and_then(|p| (state.pointer_on_timeline || rect.contains(p)).then_some(p)),
                )
                .flatten()
            }) {
                state.pointer_on_timeline = true;
                pipe.user_data.events.push(DemoViewerEvent::SkipTo {
                    time: Duration::from_secs_f64(
                        (pipe.user_data.max_duration.as_secs_f64()
                            * (pointer_pos.x as f64 / rect.width() as f64))
                            .clamp(0.0, f64::MAX),
                    ),
                });
            } else {
                state.pointer_on_timeline = false;
            }

            if let Some(pointer_pos) = ui.input(|i| {
                i.pointer
                    .hover_pos()
                    .and_then(|p| (!state.pointer_on_timeline && rect.contains(p)).then_some(p))
            }) {
                let time = Duration::from_secs_f64(
                    (pipe.user_data.max_duration.as_secs_f64()
                        * (pointer_pos.x as f64 / rect.width() as f64))
                        .clamp(0.0, f64::MAX),
                );

                let canvas = ui.ctx().screen_rect();
                let canvas_width = canvas.width();
                let canvas_height = canvas.height();
                let ratio = canvas_width / canvas_height;

                let width = (100.0 * ratio).clamp(1.0, canvas_width);
                let height = 100.0f32.clamp(1.0, canvas_height);

                let ppp = ui.ctx().pixels_per_point();
                let rect = Rect::from_center_size(
                    egui::pos2(
                        pointer_pos.x.clamp(width / 2.0, canvas_width - width / 2.0),
                        (rect.min.y - height / 2.0)
                            .clamp(height / 2.0, canvas_height - height / 2.0),
                    ),
                    egui::vec2(width, height),
                )
                .translate(egui::vec2(0.0, -25.0));

                ui.painter().rect_stroke(
                    rect,
                    CornerRadius::default(),
                    Stroke::new(1.0, Color32::from_white_alpha(100)),
                    egui::StrokeKind::Inside,
                );

                let time_str = time.to_race_string();

                ui.painter().text(
                    rect.center_bottom() + egui::vec2(0.0, 6.0),
                    Align2::CENTER_TOP,
                    time_str,
                    FontId::proportional(12.0),
                    Color32::WHITE,
                );

                pipe.user_data.events.push(DemoViewerEvent::PreviewAt {
                    time,
                    rect: {
                        let mut rect = rect;
                        rect.min.x *= ppp;
                        rect.min.y *= ppp;
                        rect.max.x *= ppp;
                        rect.max.y *= ppp;
                        rect
                    },
                });
            }

            ui.painter()
                .rect_filled(rect, CornerRadius::default(), Color32::from_white_alpha(50));
            let len_rect = rect;
            rect.set_width(
                rect.width()
                    * (pipe.user_data.cur_duration.as_secs_f32()
                        / pipe.user_data.max_duration.as_secs_f32().max(0.0001))
                    .clamp(0.0, 1.0),
            );
            ui.painter().rect_filled(
                rect,
                CornerRadius::default(),
                Color32::from_rgba_unmultiplied(150, 0, 0, 150),
            );
            let state = &mut *pipe.user_data.state;
            let draw_export_rect = |at: Option<Duration>| {
                if let Some(at) = at {
                    let at = (at.as_secs_f32()
                        / pipe.user_data.max_duration.as_secs_f32().max(0.0001))
                    .clamp(0.0, 1.0);
                    let rect = Rect::from_center_size(
                        egui::pos2(
                            rect.left_center().x + len_rect.width() * at,
                            rect.left_center().y,
                        ),
                        egui::vec2(2.0, rect.height()),
                    );
                    ui.painter().rect_filled(
                        rect,
                        CornerRadius::default(),
                        Color32::from_rgba_unmultiplied(0, 0, 150, 255),
                    );
                }
            };
            draw_export_rect(state.left);
            draw_export_rect(state.right);
            if let Some((left, right)) = state.left.zip(state.right) {
                let at = (left.as_secs_f32()
                    / pipe.user_data.max_duration.as_secs_f32().max(0.0001))
                .clamp(0.0, 1.0);
                let until = (right.as_secs_f32()
                    / pipe.user_data.max_duration.as_secs_f32().max(0.0001))
                .clamp(0.0, 1.0);
                let width = (until - at) * len_rect.width();
                let rect = Rect::from_center_size(
                    egui::pos2(
                        rect.left_center().x + len_rect.width() * at + width / 2.0,
                        rect.left_center().y,
                    ),
                    egui::vec2(width, rect.height()),
                );
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::default(),
                    Color32::from_rgba_unmultiplied(0, 0, 150, 150),
                );
            }

            const FONT_SIZE: f32 = 20.0;
            add_horizontal_margins(ui, |ui| {
                let rect = ui.available_rect_before_wrap();
                ui.horizontal_centered(|ui| {
                    let style = ui.style_mut();
                    style.visuals.widgets.inactive.fg_stroke.color = Color32::WHITE;
                    style.visuals.widgets.hovered.fg_stroke.color = Color32::LIGHT_GRAY;
                    style.visuals.widgets.active.fg_stroke.color = Color32::LIGHT_YELLOW;
                    style.visuals.button_frame = false;
                    ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_width(rect.width() - 150.0);

                        let btn = ui.add_sized(
                            egui::vec2(FONT_SIZE, rect.height()),
                            Button::new(if *pipe.user_data.is_paused {
                                text_sized("\u{f04b}", FONT_SIZE)
                            } else {
                                text_sized("\u{f04c}", FONT_SIZE)
                            }),
                        );
                        if btn.clicked() {
                            pipe.user_data.events.push(DemoViewerEvent::ResumeToggle);
                        }

                        ui.add_space(15.0);

                        // backward-fast, stop, forward-fast
                        if ui.button(text_sized("\u{f049}", FONT_SIZE)).clicked() {
                            pipe.user_data.events.push(DemoViewerEvent::BackwardFast);
                        }
                        if ui.button(text_sized("\u{f04d}", FONT_SIZE)).clicked() {
                            pipe.user_data.events.push(DemoViewerEvent::Stop);
                        }
                        if ui.button(text_sized("\u{f050}", FONT_SIZE)).clicked() {
                            pipe.user_data.events.push(DemoViewerEvent::ForwardFast);
                        }

                        ui.add_space(15.0);

                        // backward-step, forward-step
                        if ui.button(text_sized("\u{f048}", FONT_SIZE)).clicked() {
                            pipe.user_data.events.push(DemoViewerEvent::BackwardStep);
                        }
                        if ui.button(text_sized("\u{f051}", FONT_SIZE)).clicked() {
                            pipe.user_data.events.push(DemoViewerEvent::ForwardStep);
                        }

                        ui.add_space(15.0);

                        // backward, forward
                        if ui.button(text_sized("\u{f04a}", FONT_SIZE)).clicked() {
                            pipe.user_data.events.push(DemoViewerEvent::Backward);
                        }
                        if ui.button(text_sized("\u{f04e}", FONT_SIZE)).clicked() {
                            pipe.user_data.events.push(DemoViewerEvent::Forward);
                        }

                        ui.add_space(15.0);

                        if ui.button(text_sized("\u{f068}", FONT_SIZE)).clicked() {
                            pipe.user_data.events.push(DemoViewerEvent::SpeedSlower);
                        }
                        if ui
                            .button(format!(
                                "{:\u{2007}^7.2}",
                                pipe.user_data.speed.to_num::<f64>()
                            ))
                            .clicked()
                        {
                            pipe.user_data.events.push(DemoViewerEvent::SpeedReset);
                        }
                        if ui.button(text_sized("\u{2b}", FONT_SIZE)).clicked() {
                            pipe.user_data.events.push(DemoViewerEvent::SpeedFaster);
                        }

                        ui.add_space(15.0);
                        ui.colored_label(Color32::WHITE, pipe.user_data.name);
                    });
                    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                        // exit
                        if ui.button(text_sized("\u{f00d}", FONT_SIZE)).clicked() {
                            pipe.user_data.events.push(DemoViewerEvent::Close);
                        }

                        ui.add_space(15.0);

                        // left bracket, right bracket, share (in reverse order)
                        let state = &mut *pipe.user_data.state;
                        if ui
                            .add_enabled(
                                state.left.is_some() && state.right.is_some(),
                                Button::new(text_sized("\u{f08e}", FONT_SIZE)),
                            )
                            .clicked()
                        {
                            state.export = Some(DemoViewerEventExport {
                                left: state.left.unwrap_or_default(),
                                right: state.right.unwrap_or_default(),
                                name: pipe.user_data.name.to_string(),
                                remove_chat: false,
                            });
                        }
                        if ui.button(text_sized("\u{f090}", FONT_SIZE)).clicked() {
                            state.right = Some(*pipe.user_data.cur_duration);
                        }
                        if ui.button(text_sized("\u{f08b}", FONT_SIZE)).clicked() {
                            state.left = Some(*pipe.user_data.cur_duration);
                        }

                        if state.export.is_some() {
                            Window::new("Export demo")
                                .anchor(Align2::CENTER_CENTER, Vec2::default())
                                .show(ui.ctx(), |ui| {
                                    Grid::new("export-demo-grid").num_columns(2).show(ui, |ui| {
                                        if let Some(data) = state.export.as_mut() {
                                            ui.label("Cutted from - to:");
                                            ui.label(format!(
                                                "{} - {}",
                                                data.left.to_race_string(),
                                                data.right.to_race_string()
                                            ));
                                            ui.end_row();

                                            ui.label("Cut length:");
                                            ui.label(
                                                data.right
                                                    .saturating_sub(data.left)
                                                    .to_race_string(),
                                            );
                                            ui.end_row();

                                            ui.label("New name:");
                                            ui.text_edit_singleline(&mut data.name);
                                            ui.end_row();

                                            ui.label("Remove chat:");
                                            ui.checkbox(&mut data.remove_chat, "");
                                            ui.end_row();

                                            let cur_data = data.clone();
                                            if ui.button("Abort").clicked() {
                                                state.export.take();
                                                state.left.take();
                                                state.right.take();
                                            }
                                            if ui.button("Export").clicked() {
                                                pipe.user_data
                                                    .events
                                                    .push(DemoViewerEvent::Export(cur_data));

                                                state.export.take();
                                                state.left.take();
                                                state.right.take();
                                            }
                                            ui.end_row();
                                        }
                                    });
                                });
                        }
                    });
                });
            });
        });
    ui_state.add_blur_rect(res.response.rect, 0.0);
}
