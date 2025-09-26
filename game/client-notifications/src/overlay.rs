use std::time::Duration;

use base::steady_clock::SteadyClock;
use egui::{Color32, WidgetText};
use egui_notify::{Toast, Toasts};
use graphics::{
    graphics::graphics::Graphics,
    handles::{
        backend::backend::GraphicsBackendHandle, canvas::canvas::GraphicsCanvasHandle,
        stream::stream::GraphicsStreamHandle, texture::texture::GraphicsTextureHandle,
    },
};
use tracing::instrument;
use ui_base::{
    types::UiRenderPipe,
    ui::{UiContainer, UiCreator},
    ui_render::render_ui,
};
use ui_generic::{generic_ui_renderer, traits::UiPageInterface};

/// Notifications, e.g. popups, for warnings, errors or similar events.
pub struct ClientNotifications {
    pub ui: UiContainer,

    time: SteadyClock,

    toasts: Toasts,

    pub backend_handle: GraphicsBackendHandle,
    pub canvas_handle: GraphicsCanvasHandle,
    pub stream_handle: GraphicsStreamHandle,
    pub texture_handle: GraphicsTextureHandle,
}

impl ClientNotifications {
    pub fn new(graphics: &Graphics, time: &SteadyClock, creator: &UiCreator) -> Self {
        let mut ui = UiContainer::new(creator);
        ui.set_main_panel_color(&Color32::TRANSPARENT);
        ui.ui_state.is_ui_open = false;
        Self {
            ui,
            time: time.clone(),

            toasts: Toasts::new().with_anchor(egui_notify::Anchor::BottomRight),

            backend_handle: graphics.backend_handle.clone(),
            canvas_handle: graphics.canvas_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),
            texture_handle: graphics.texture_handle.clone(),
        }
    }

    #[instrument(level = "trace", skip_all)]
    pub fn render(&mut self) {
        if self.toasts.is_empty() {
            return;
        }

        struct Render;

        impl UiPageInterface<&mut Toasts> for Render {
            fn render(
                &mut self,
                ui: &mut egui::Ui,
                pipe: &mut UiRenderPipe<&mut Toasts>,
                _ui_state: &mut ui_base::types::UiState,
            ) {
                pipe.user_data.show(ui.ctx());
            }
        }
        generic_ui_renderer::render(
            &self.backend_handle,
            &self.texture_handle,
            &self.stream_handle,
            &self.canvas_handle,
            &mut self.ui,
            &mut Render,
            &mut UiRenderPipe::new(self.time.now(), &mut &mut self.toasts),
            Default::default(),
        );
        let canvas_width = self.canvas_handle.canvas_width();
        let canvas_height = self.canvas_handle.canvas_height();
        let pixels_per_point = self.canvas_handle.pixels_per_point();
        let (screen_rect, full_output, zoom_level) = self.ui.render(
            canvas_width,
            canvas_height,
            pixels_per_point,
            |ui, _, _| {
                self.toasts.show(ui.ctx());
            },
            &mut UiRenderPipe::new(self.time.now(), &mut ()),
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
        self.truncate();
    }

    fn truncate(&mut self) {
        // how many toasts there should be visible at most at once
        if self.toasts.len() > 10 {
            self.toasts.dismiss_oldest_toast();
        }
    }

    #[instrument(level = "trace", skip_all)]
    pub fn add_info(&mut self, text: impl Into<WidgetText>, duration: Duration) {
        // upper limit in case of abuse
        if self.toasts.len() >= 1000 {
            return;
        }
        self.toasts.info(text).duration(Some(duration));
        self.truncate();
    }

    #[instrument(level = "trace", skip_all)]
    pub fn add_warn(&mut self, text: impl Into<WidgetText>, duration: Duration) {
        // upper limit in case of abuse
        if self.toasts.len() >= 1000 {
            return;
        }
        let mut toast = Toast::warning(text);
        toast.duration(Some(duration));
        self.toasts.add(toast);
        self.truncate();
    }

    #[instrument(level = "trace", skip_all)]
    pub fn add_err(&mut self, text: impl Into<WidgetText>, duration: Duration) {
        // upper limit in case of abuse
        if self.toasts.len() >= 1000 {
            return;
        }
        let mut toast = Toast::error(text);
        toast.duration(Some(duration));
        self.toasts.add(toast);
        self.truncate();
    }
}
