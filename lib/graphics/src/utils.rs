use graphics_types::rendering::{
    BlendType, ColorMaskMode, RenderMode, State, StencilMode, WrapType,
};
use hiarc::hi_closure;
use math::math::vector::{vec2, vec4};

use crate::handles::{
    backend::backend::GraphicsBackendHandle,
    canvas::canvas::GraphicsCanvasHandle,
    stream::stream::{GraphicsStreamHandle, QuadStreamHandle},
    stream_types::StreamedQuad,
    texture::texture::TextureType,
};

pub const DEFAULT_BLUR_RADIUS: f32 = 13.0;
pub const DEFAULT_BLUR_MIX_LENGTH: f32 = 8.0;

fn render_blur_impl(
    backend_handle: &GraphicsBackendHandle,
    stream_handle: &GraphicsStreamHandle,
    is_hori: bool,
    blur_radius: f32,
    blur_mix_length: f32,
    blur_color: &vec4,
    is_first: bool,
) {
    let is_last_iter = blur_mix_length <= 1.0;

    let mut state = State::new();
    state.map_canvas(0.0, 0.0, 1.0, 1.0);
    state.set_stencil_mode(StencilMode::StencilPassed);
    state.wrap(WrapType::Clamp);
    state.blend(BlendType::None);
    if is_first {
        state.set_color_mask(ColorMaskMode::WriteColorOnly);
    }
    stream_handle.stream_quads(
        hi_closure!([is_hori: bool, blur_radius: f32, blur_mix_length: f32, is_last_iter: bool, blur_color: &vec4], |mut stream_handle: QuadStreamHandle<'_>| -> () {
            stream_handle.set_color_attachment_texture();
            stream_handle.set_render_mode(RenderMode::Blur {
                blur_radius,
                scale: if is_hori {
                    vec2::new(1.0, 0.0) * blur_mix_length
                } else {
                    vec2::new(0.0, 1.0) * blur_mix_length
                },
                blur_color: if !is_hori && is_last_iter {
                    *blur_color
                } else {
                    vec4::new(1.0, 1.0, 1.0, 0.0)
                },
            });
            stream_handle.add_vertices(
                StreamedQuad::default()
                    .from_pos_and_size(vec2::new(0.0, 0.0), vec2::new(1.0, 1.0))
                    .tex_free_form(
                        vec2::new(0.0, 0.0),
                        vec2::new(1.0, 0.0),
                        vec2::new(1.0, 1.0),
                        vec2::new(0.0, 1.0),
                    )
                    .colorf(vec4::new(1.0, 1.0, 1.0, 1.0))
                    .into(),
            );
        }),
        state,
    );

    state.map_canvas(0.0, 0.0, 1.0, 1.0);
    state.set_stencil_mode(StencilMode::StencilNotPassed {
        clear_stencil: false,
    });
    state.wrap(WrapType::Clamp);
    state.blend(BlendType::None);
    if is_first {
        state.set_color_mask(ColorMaskMode::WriteColorOnly);
    }
    stream_handle.render_quads(
        &[StreamedQuad::default()
            .from_pos_and_size(vec2::new(0.0, 0.0), vec2::new(1.0, 1.0))
            .tex_free_form(
                vec2::new(0.0, 0.0),
                vec2::new(1.0, 0.0),
                vec2::new(1.0, 1.0),
                vec2::new(0.0, 1.0),
            )
            .colorf(vec4::new(1.0, 1.0, 1.0, 1.0))],
        state,
        TextureType::ColorAttachmentOfPreviousPass,
    );

    backend_handle.next_switch_pass();

    if is_hori {
        render_blur_impl(
            backend_handle,
            stream_handle,
            false,
            blur_radius,
            (blur_mix_length - 1.0).max(1.0),
            blur_color,
            false,
        );
    } else if blur_mix_length > 1.0 {
        render_blur_impl(
            backend_handle,
            stream_handle,
            true,
            blur_radius,
            blur_mix_length - 1.0,
            blur_color,
            false,
        );
    }
}

pub fn render_blur(
    backend_handle: &GraphicsBackendHandle,
    stream_handle: &GraphicsStreamHandle,
    canvas_handle: &GraphicsCanvasHandle,
    is_hori: bool,
    blur_radius: f32,
    blur_mix_length: f32,
    blur_color: &vec4,
) {
    let dynamic_viewport = canvas_handle.dynamic_viewport();
    canvas_handle.reset_window_viewport();
    render_blur_impl(
        backend_handle,
        stream_handle,
        is_hori,
        blur_radius,
        blur_mix_length,
        blur_color,
        true,
    );
    if let Some(dynamic_viewport) = dynamic_viewport {
        canvas_handle.update_window_viewport(
            dynamic_viewport.x,
            dynamic_viewport.y,
            dynamic_viewport.width,
            dynamic_viewport.height,
        );
    }
}

pub fn render_swapped_frame(
    canvas_handle: &GraphicsCanvasHandle,
    stream_handle: &GraphicsStreamHandle,
) {
    let dynamic_viewport = canvas_handle.dynamic_viewport();
    canvas_handle.reset_window_viewport();

    let mut state = State::new();
    state.map_canvas(0.0, 0.0, 1.0, 1.0);
    state.set_stencil_mode(StencilMode::StencilNotPassed {
        clear_stencil: true,
    });
    state.wrap(WrapType::Clamp);
    state.blend(BlendType::None);

    stream_handle.render_quads(
        &[StreamedQuad::default()
            .from_pos_and_size(vec2::new(0.0, 0.0), vec2::new(1.0, 1.0))
            .tex_free_form(
                vec2::new(0.0, 0.0),
                vec2::new(1.0, 0.0),
                vec2::new(1.0, 1.0),
                vec2::new(0.0, 1.0),
            )
            .colorf(vec4::new(1.0, 1.0, 1.0, 1.0))],
        state,
        TextureType::ColorAttachmentOfPreviousPass,
    );

    if let Some(dynamic_viewport) = dynamic_viewport {
        canvas_handle.update_window_viewport(
            dynamic_viewport.x,
            dynamic_viewport.y,
            dynamic_viewport.width,
            dynamic_viewport.height,
        );
    }
}
