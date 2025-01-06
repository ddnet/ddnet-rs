use std::{sync::Arc, time::Duration};

use base::system::{System, SystemTimeInterface};
use egui::{Color32, WidgetText};
use egui_notify::{Toast, Toasts};
use graphics::{
    graphics::graphics::Graphics,
    handles::{
        backend::backend::GraphicsBackendHandle, canvas::canvas::GraphicsCanvasHandle,
        stream::stream::GraphicsStreamHandle, texture::texture::GraphicsTextureHandle,
    },
};
use ui_base::{
    types::UiRenderPipe,
    ui::{UiContainer, UiCreator},
    ui_render::render_ui,
};
use ui_generic::{generic_ui_renderer, traits::UiPageInterface};

/// Notifications, e.g. popups, for warnings, errors or similar events.
pub struct ClientNotifications {
    pub ui: UiContainer,

    sys: Arc<dyn SystemTimeInterface>,

    toasts: Toasts,

    pub backend_handle: GraphicsBackendHandle,
    pub canvas_handle: GraphicsCanvasHandle,
    pub stream_handle: GraphicsStreamHandle,
    pub texture_handle: GraphicsTextureHandle,
}

impl ClientNotifications {
    pub fn new(graphics: &Graphics, sys: &System, creator: &UiCreator) -> Self {
        let mut ui = UiContainer::new(creator);
        ui.set_main_panel_color(&Color32::TRANSPARENT);
        ui.ui_state.is_ui_open = false;
        Self {
            ui,
            sys: sys.time.clone(),

            toasts: Toasts::new().with_anchor(egui_notify::Anchor::BottomRight),

            backend_handle: graphics.backend_handle.clone(),
            canvas_handle: graphics.canvas_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),
            texture_handle: graphics.texture_handle.clone(),
        }
    }

    pub fn render(&mut self) {
        if self.toasts.is_empty() {
            return;
        }
        generic_ui_renderer::render(
            &self.backend_handle,
            &self.texture_handle,
            &self.stream_handle,
            &self.canvas_handle,
            &mut self.ui,
            {
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

                &mut Render
            },
            &mut UiRenderPipe::new(self.sys.time_get(), &mut &mut self.toasts),
            Default::default(),
        );
        let window_width = self.canvas_handle.window_width();
        let window_height = self.canvas_handle.window_height();
        let window_pixels_per_point = self.canvas_handle.window_pixels_per_point();
        let (screen_rect, full_output, zoom_level) = self.ui.render(
            window_width,
            window_height,
            window_pixels_per_point,
            |ui, _, _| {
                self.toasts.show(ui.ctx());
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
        self.truncate();
    }

    fn truncate(&mut self) {
        // how many toasts there should be visible at most at once
        if self.toasts.len() > 10 {
            self.toasts.dismiss_oldest_toast();
        }
    }

    pub fn add_info(&mut self, text: impl Into<WidgetText>, duration: Duration) {
        // upper limit in case of abuse
        if self.toasts.len() >= 1000 {
            return;
        }
        self.toasts.info(text).duration(Some(duration));
        self.truncate();
    }

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
