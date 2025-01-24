use std::{
    sync::{atomic::AtomicU64, Arc},
    time::Duration,
};

use egui::{Color32, FontId};
use egui_extras::StripBuilder;
use fixed::{types::extra::U16, FixedI64};

use graphics::{
    graphics::graphics::Graphics,
    handles::{
        backend::backend::GraphicsBackendHandle, canvas::canvas::GraphicsCanvasHandle,
        stream::stream::GraphicsStreamHandle, texture::texture::GraphicsTextureHandle,
    },
};

use prediction_timer::prediction_timing::PredictionTimer;
use ui_base::{
    types::UiRenderPipe,
    ui::{UiContainer, UiCreator},
    ui_render::render_ui,
};

use math::math::{blend, vector::luffixed};

use base::system::{self, SystemTimeInterface};

use crate::game::data::NetworkByteStats;

pub struct DebugHudData {
    texture_memory_usage: Arc<AtomicU64>,
    buffer_memory_usage: Arc<AtomicU64>,
    stream_memory_usage: Arc<AtomicU64>,
    staging_memory_usage: Arc<AtomicU64>,
}

impl DebugHudData {
    pub fn new(
        texture_memory_usage: Arc<AtomicU64>,
        buffer_memory_usage: Arc<AtomicU64>,
        stream_memory_usage: Arc<AtomicU64>,
        staging_memory_usage: Arc<AtomicU64>,
    ) -> Self {
        Self {
            texture_memory_usage,
            buffer_memory_usage,
            stream_memory_usage,
            staging_memory_usage,
        }
    }

    pub fn render_stats(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut UiRenderPipe<Option<DebugHudRenderPipe<'_>>>,
    ) {
        StripBuilder::new(ui)
            .size(egui_extras::Size::remainder())
            .size(egui_extras::Size::exact(100.0))
            .horizontal(|mut strip| {
                strip.cell(|_| {});
                strip.cell(|ui| {
                    ui.style_mut().wrap_mode = None;
                    ui.painter()
                        .rect_filled(ui.available_rect_before_wrap(), 5.0, Color32::BLACK);

                    if let Some(dbg) = pipe.user_data {
                        let timing = dbg.prediction_timer.snapshot();
                        ui.label("Network");
                        ui.label("Ping (ms):");
                        ui.colored_label(
                            Color32::from_rgb(255, 0, 255),
                            format!(
                                "{:.2}",
                                dbg.prediction_timer.ping_average().as_micros() as f64 / 1000.0
                            ),
                        );
                        ui.label("Min-Ping (ms):");
                        ui.colored_label(
                            Color32::from_rgb(255, 0, 255),
                            format!(
                                "{:.2}",
                                dbg.prediction_timer.ping_min().as_micros() as f64 / 1000.0
                            ),
                        );
                        ui.label("Max-Ping (ms):");
                        ui.colored_label(
                            Color32::from_rgb(255, 0, 255),
                            format!(
                                "{:.2}",
                                dbg.prediction_timer.ping_max().as_micros() as f64 / 1000.0
                            ),
                        );
                        ui.label("Ping jitter (ms):");
                        ui.colored_label(
                            Color32::from_rgb(255, 0, 255),
                            format!(
                                "{:.2}",
                                dbg.prediction_timer.jitter_range.as_micros() as f64 / 1000.0
                            ),
                        );
                        ui.label("Max frame time (ms):");
                        ui.colored_label(
                            Color32::from_rgb(255, 0, 255),
                            format!(
                                "{:.2}",
                                dbg.prediction_timer.max_frametime().as_micros() as f64 / 1000.0
                            ),
                        );
                        ui.label("Prediction adjustment smooth (ms):");
                        ui.colored_label(
                            Color32::from_rgb(255, 0, 255),
                            format!("{:.4}", timing.smooth_adjustment_time * 1000.0),
                        );
                        ui.label("Ingame time (ms):");
                        ui.colored_label(
                            Color32::from_rgb(255, 0, 255),
                            format!("{:.2}", dbg.ingame_timer.as_micros() as f64 / 1000.0),
                        );
                        ui.label("Packet loss (sending) %:");
                        ui.colored_label(
                            Color32::from_rgb(255, 0, 255),
                            format!("{:.2}", dbg.prediction_timer.packet_loss() * 100.0),
                        );

                        ui.label("Sent Kibit/s:");
                        ui.colored_label(
                            Color32::from_rgb(255, 0, 255),
                            format!(
                                "{:.2}",
                                (dbg.byte_stats.bytes_per_sec_sent * luffixed::from_num(8))
                                    / luffixed::from_num(1024)
                            ),
                        );
                        ui.label("Recv Kibit/s:");
                        ui.colored_label(
                            Color32::from_rgb(255, 0, 255),
                            format!(
                                "{:.2}",
                                (dbg.byte_stats.bytes_per_sec_recv * luffixed::from_num(8))
                                    / luffixed::from_num(1024)
                            ),
                        );
                    }

                    ui.label("Graphics");
                    ui.label("Texture usage MiB:");
                    ui.colored_label(
                        Color32::from_rgb(255, 0, 255),
                        format!(
                            "{:.6}",
                            (self
                                .texture_memory_usage
                                .load(std::sync::atomic::Ordering::Relaxed)
                                as f64
                                / 1024.0
                                / 1024.0)
                        ),
                    );

                    ui.label("Buffer usage MiB:");
                    ui.colored_label(
                        Color32::from_rgb(255, 0, 255),
                        format!(
                            "{:.6}",
                            (self
                                .buffer_memory_usage
                                .load(std::sync::atomic::Ordering::Relaxed)
                                as f64
                                / 1024.0
                                / 1024.0)
                        ),
                    );

                    ui.label("Stream usage MiB:");
                    ui.colored_label(
                        Color32::from_rgb(255, 0, 255),
                        format!(
                            "{:.6}",
                            (self
                                .stream_memory_usage
                                .load(std::sync::atomic::Ordering::Relaxed)
                                as f64
                                / 1024.0
                                / 1024.0)
                        ),
                    );

                    ui.label("Staging usage MiB:");
                    ui.colored_label(
                        Color32::from_rgb(255, 0, 255),
                        format!(
                            "{:.6}",
                            (self
                                .staging_memory_usage
                                .load(std::sync::atomic::Ordering::Relaxed)
                                as f64
                                / 1024.0
                                / 1024.0)
                        ),
                    );
                })
            });
    }
}

pub struct DebugHudRenderPipe<'a> {
    pub prediction_timer: &'a PredictionTimer,
    pub byte_stats: &'a NetworkByteStats,
    pub ingame_timer: &'a Duration,
}

pub struct ClientStatsData {
    last_frame_time: Duration,
    fps: FixedI64<U16>,
}

impl ClientStatsData {
    pub fn new(sys: &system::System) -> Self {
        Self {
            fps: FixedI64::from_num(60.0),
            last_frame_time: sys.time_get(),
        }
    }

    pub fn render_fps(&mut self, ui: &mut egui::Ui, pipe: &mut UiRenderPipe<()>, bottom: bool) {
        let cur_time = pipe.cur_time;
        let time_diff = cur_time - self.last_frame_time;
        self.last_frame_time = cur_time;

        self.fps = blend(
            &self.fps,
            &(FixedI64::from_num(Duration::from_secs(1).as_nanos())
                / FixedI64::from_num(time_diff.as_nanos())),
            FixedI64::from_num(1),
            FixedI64::from_num(0.1),
        );

        let (pos, anchor) = if bottom {
            (
                ui.ctx().screen_rect().right_bottom(),
                egui::Align2::RIGHT_BOTTOM,
            )
        } else {
            (ui.ctx().screen_rect().right_top(), egui::Align2::RIGHT_TOP)
        };

        ui.painter().text(
            pos,
            anchor,
            format!("FPS: {}", self.fps.floor()),
            FontId::proportional(12.0),
            Color32::from_rgb(255, 0, 255),
        );
    }
}

pub struct ClientStatsRenderPipe<'a> {
    pub debug_hud: Option<DebugHudRenderPipe<'a>>,
    pub connection_issues: bool,
    pub force_bottom: bool,
    pub show_fps: bool,
}

/// This component collects various client statistics and displays them optionally
pub struct ClientStats {
    stats: ClientStatsData,
    dbg: DebugHudData,
    pub ui: UiContainer,

    sys: Arc<dyn SystemTimeInterface>,

    pub backend_handle: GraphicsBackendHandle,
    pub canvas_handle: GraphicsCanvasHandle,
    pub stream_handle: GraphicsStreamHandle,
    pub texture_handle: GraphicsTextureHandle,
}

impl ClientStats {
    pub fn new(
        graphics: &Graphics,
        sys: &system::System,
        texture_memory_usage: Arc<AtomicU64>,
        buffer_memory_usage: Arc<AtomicU64>,
        stream_memory_usage: Arc<AtomicU64>,
        staging_memory_usage: Arc<AtomicU64>,
        creator: &UiCreator,
    ) -> Self {
        let mut ui = UiContainer::new(creator);
        ui.set_main_panel_color(&Color32::TRANSPARENT);
        ui.ui_state.is_ui_open = false;
        Self {
            stats: ClientStatsData::new(sys),
            dbg: DebugHudData::new(
                texture_memory_usage,
                buffer_memory_usage,
                stream_memory_usage,
                staging_memory_usage,
            ),
            ui,
            sys: sys.time.clone(),
            backend_handle: graphics.backend_handle.clone(),
            canvas_handle: graphics.canvas_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),
            texture_handle: graphics.texture_handle.clone(),
        }
    }

    pub fn render_connection_issues(ui: &mut egui::Ui) {
        ui.painter().text(
            ui.ctx().screen_rect().center(),
            egui::Align2::CENTER_CENTER,
            "\u{f071} Connection to the server unstable or lost.",
            FontId::proportional(25.0),
            Color32::from_rgb(255, 255, 255),
        );
    }

    pub fn render(&mut self, pipe: &mut ClientStatsRenderPipe) {
        let dbg_hud_open = self.ui.ui_state.is_ui_open;
        if !dbg_hud_open && !pipe.show_fps && !pipe.connection_issues {
            return;
        }

        let window_width = self.canvas_handle.window_width();
        let window_height = self.canvas_handle.window_height();
        let window_pixels_per_point = self.canvas_handle.window_pixels_per_point();
        let (screen_rect, full_output, zoom_level) = self.ui.render(
            window_width,
            window_height,
            window_pixels_per_point,
            |ui, inner_pipe, _| {
                let game_active = pipe.debug_hud.is_some();
                if dbg_hud_open {
                    self.dbg.render_stats(
                        ui,
                        &mut UiRenderPipe {
                            cur_time: inner_pipe.cur_time,
                            user_data: &mut pipe.debug_hud,
                        },
                    );
                }
                if pipe.show_fps {
                    self.stats.render_fps(
                        ui,
                        inner_pipe,
                        dbg_hud_open || !game_active || pipe.force_bottom,
                    );
                }
                if pipe.connection_issues {
                    Self::render_connection_issues(ui);
                }
            },
            &mut UiRenderPipe::new(self.sys.time_get(), &mut ()),
            Default::default(),
            false,
        );
        render_ui(
            &mut self.ui,
            full_output,
            &screen_rect,
            zoom_level,
            &self.backend_handle,
            &self.texture_handle,
            &self.stream_handle,
            false,
        );
    }
}
