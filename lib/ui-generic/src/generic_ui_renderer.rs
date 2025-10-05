use std::time::Duration;

use crate::traits::UiPageInterface;
use egui::{Color32, Rect, Stroke};
use graphics::{
    handles::{
        backend::backend::GraphicsBackendHandle, canvas::canvas::GraphicsCanvasHandle,
        stream::stream::GraphicsStreamHandle, texture::texture::GraphicsTextureHandle,
    },
    utils::{
        DEFAULT_BLUR_MIX_LENGTH, DEFAULT_BLUR_RADIUS, render_blur, render_glass, render_glass_rest,
        render_swapped_frame,
    },
};
use math::math::vector::{vec2, vec4};
use tracing::instrument;
use ui_base::{
    types::{BlurShape, GlassShape, UiRenderPipe, UiState},
    ui::UiContainer,
    ui_render::render_ui,
};

#[instrument(level = "trace", skip_all)]
fn render_impl<U>(
    canvas_handle: &GraphicsCanvasHandle,
    ui: &mut UiContainer,
    mut ui_render: impl FnMut(&mut egui::Ui, &mut UiRenderPipe<U>, &mut UiState),

    pipe: &mut UiRenderPipe<U>,
    inp: egui::RawInput,
    as_stencil: bool,
) -> (egui::Rect, egui::FullOutput, f32) {
    let canvas_width = canvas_handle.canvas_width();
    let canvas_height = canvas_handle.canvas_height();
    let pixels_per_point = canvas_handle.pixels_per_point();

    ui.render(
        canvas_width,
        canvas_height,
        pixels_per_point,
        |ui, inner_pipe, ui_state| {
            ui_render(ui, inner_pipe, ui_state);
        },
        pipe,
        inp,
        as_stencil,
    )
}

#[instrument(level = "trace", skip_all)]
pub fn render_blur_if_needed(
    backend_handle: &GraphicsBackendHandle,
    texture_handle: &GraphicsTextureHandle,
    stream_handle: &GraphicsStreamHandle,
    canvas_handle: &GraphicsCanvasHandle,
    ui: &mut UiContainer,
) {
    // check if blur is needed
    if !ui.ui_state.blur_shapes.is_empty() {
        let (screen_rect, full_output, zoom_level) = render_impl(
            canvas_handle,
            ui,
            |ui, _, ui_state| {
                for blur_shape in ui_state.blur_shapes.drain(..) {
                    match blur_shape {
                        BlurShape::Rect(blur_rect) => {
                            ui.painter().rect(
                                blur_rect.rect,
                                blur_rect.rounding,
                                blur_rect.color,
                                Stroke::NONE,
                                egui::StrokeKind::Inside,
                            );
                        }
                        BlurShape::Circle(blur_circle) => {
                            ui.painter().circle(
                                blur_circle.center,
                                blur_circle.radius,
                                blur_circle.color,
                                Stroke::NONE,
                            );
                        }
                    }
                }
            },
            &mut UiRenderPipe {
                cur_time: Duration::ZERO,
                user_data: &mut (),
            },
            egui::RawInput::default(),
            true,
        );
        backend_handle.next_switch_pass();
        let _ = render_ui(
            ui,
            full_output,
            &screen_rect,
            zoom_level,
            backend_handle,
            texture_handle,
            stream_handle,
            true,
        );
        render_blur(
            backend_handle,
            stream_handle,
            canvas_handle,
            true,
            DEFAULT_BLUR_RADIUS,
            DEFAULT_BLUR_MIX_LENGTH,
            &vec4::new(1.0, 1.0, 1.0, 0.15),
        );
        render_swapped_frame(canvas_handle, stream_handle);
    }
}

#[instrument(level = "trace", skip_all)]
pub fn render_glass_if_needed(
    backend_handle: &GraphicsBackendHandle,
    texture_handle: &GraphicsTextureHandle,
    stream_handle: &GraphicsStreamHandle,
    canvas_handle: &GraphicsCanvasHandle,
    ui: &mut UiContainer,
) {
    if !ui.ui_state.glass_shapes.is_empty() {
        let glass_shapes = ui.ui_state.glass_shapes.clone();
        let (screen_rect, full_output, zoom_level) = render_impl(
            canvas_handle,
            ui,
            |ui, _, ui_state| {
                for glass_shape in ui_state.glass_shapes.drain(..) {
                    let GlassShape::Elipse(elipse) = glass_shape;

                    ui.painter().rect(
                        Rect::from_center_size(elipse.center, elipse.size - egui::vec2(2.0, 2.0)),
                        0.0,
                        Color32::WHITE,
                        Stroke::NONE,
                        egui::StrokeKind::Inside,
                    );
                }
            },
            &mut UiRenderPipe {
                cur_time: Duration::ZERO,
                user_data: &mut (),
            },
            egui::RawInput::default(),
            true,
        );
        backend_handle.next_switch_pass();
        let _ = render_ui(
            ui,
            full_output,
            &screen_rect,
            zoom_level,
            backend_handle,
            texture_handle,
            stream_handle,
            true,
        );
        for glass_shape in glass_shapes {
            let GlassShape::Elipse(elipse) = glass_shape;
            render_glass(
                stream_handle,
                vec2::new(screen_rect.min.x, screen_rect.min.y),
                vec2::new(screen_rect.width(), screen_rect.height()),
                vec2::new(elipse.center.x, elipse.center.y),
                vec2::new(elipse.size.x, elipse.size.y),
                elipse.power,
                vec4::new(
                    elipse.color.r() as f32 / 255.0,
                    elipse.color.g() as f32 / 255.0,
                    elipse.color.b() as f32 / 255.0,
                    elipse.color.a() as f32 / 255.0,
                ),
            );
        }
        render_glass_rest(backend_handle, stream_handle);
        render_swapped_frame(canvas_handle, stream_handle);
    }
}

#[allow(clippy::too_many_arguments)]
#[instrument(level = "trace", skip_all)]
pub fn render_ex<U>(
    backend_handle: &GraphicsBackendHandle,
    texture_handle: &GraphicsTextureHandle,
    stream_handle: &GraphicsStreamHandle,
    canvas_handle: &GraphicsCanvasHandle,
    ui: &mut UiContainer,
    ui_impl: &mut dyn UiPageInterface<U>,

    pipe: &mut UiRenderPipe<U>,

    inp: egui::RawInput,
    allows_blur: bool,
) -> egui::PlatformOutput {
    let (screen_rect, full_output, zoom_level) = render_impl(
        canvas_handle,
        ui,
        |ui, inner_pipe, ui_state| {
            ui_impl.render(ui, inner_pipe, ui_state);
        },
        pipe,
        inp,
        false,
    );
    if !allows_blur {
        ui.ui_state.blur_shapes.clear();
    }
    render_blur_if_needed(
        backend_handle,
        texture_handle,
        stream_handle,
        canvas_handle,
        ui,
    );
    let res = render_ui(
        ui,
        full_output,
        &screen_rect,
        zoom_level,
        backend_handle,
        texture_handle,
        stream_handle,
        false,
    );
    render_glass_if_needed(
        backend_handle,
        texture_handle,
        stream_handle,
        canvas_handle,
        ui,
    );
    res
}

#[allow(clippy::too_many_arguments)]
pub fn render<U>(
    backend_handle: &GraphicsBackendHandle,
    texture_handle: &GraphicsTextureHandle,
    stream_handle: &GraphicsStreamHandle,
    canvas_handle: &GraphicsCanvasHandle,
    ui: &mut UiContainer,
    ui_impl: &mut dyn UiPageInterface<U>,

    pipe: &mut UiRenderPipe<U>,

    inp: egui::RawInput,
) -> egui::PlatformOutput {
    render_ex(
        backend_handle,
        texture_handle,
        stream_handle,
        canvas_handle,
        ui,
        ui_impl,
        pipe,
        inp,
        true,
    )
}
