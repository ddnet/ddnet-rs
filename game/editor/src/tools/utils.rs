use camera::CameraInterface;
use egui::Color32;
use graphics::handles::{
    canvas::canvas::GraphicsCanvasHandle,
    stream::stream::GraphicsStreamHandle,
    stream_types::{StreamedLine, StreamedQuad},
    texture::texture::TextureType,
};
use graphics_types::rendering::{ColorMaskMode, State, StencilMode};
use map::map::groups::MapGroupAttr;
use math::math::vector::{ffixed, fvec2, ubvec4, vec2};

use crate::map::{EditorMap, EditorMapInterface};

pub fn render_rect_from_state(
    stream_handle: &GraphicsStreamHandle,
    state: State,
    rect: egui::Rect,
    color: ubvec4,
) {
    let line = StreamedLine::new().with_color(color);

    let line1 = line.from_pos([
        vec2::new(rect.min.x, rect.min.y),
        vec2::new(rect.max.x, rect.min.y),
    ]);
    let line2 = line.from_pos([
        vec2::new(rect.min.x, rect.min.y),
        vec2::new(rect.min.x, rect.max.y),
    ]);
    let line3 = line.from_pos([
        vec2::new(rect.max.x, rect.min.y),
        vec2::new(rect.max.x, rect.max.y),
    ]);
    let line4 = line.from_pos([
        vec2::new(rect.min.x, rect.max.y),
        vec2::new(rect.max.x, rect.max.y),
    ]);
    stream_handle.render_lines(&[line1, line2, line3, line4], state);
}

pub fn render_rect_state(
    canvas_handle: &GraphicsCanvasHandle,
    map: &EditorMap,
    parallax: &vec2,
    offset: &vec2,
) -> State {
    let mut state = State::new();
    map.game_camera().project(
        canvas_handle,
        &mut state,
        Some(&MapGroupAttr {
            offset: fvec2::new(ffixed::from_num(offset.x), ffixed::from_num(offset.y)),
            parallax: fvec2::new(ffixed::from_num(parallax.x), ffixed::from_num(parallax.y)),
            clipping: None,
        }),
    );
    state
}

pub fn render_rect(
    canvas_handle: &GraphicsCanvasHandle,
    stream_handle: &GraphicsStreamHandle,
    map: &EditorMap,
    rect: egui::Rect,
    color: ubvec4,
    parallax: &vec2,
    offset: &vec2,
) {
    let state = render_rect_state(canvas_handle, map, parallax, offset);

    render_rect_from_state(stream_handle, state, rect, color)
}

pub fn render_checkerboard_background(
    stream_handle: &GraphicsStreamHandle,
    render_rect: egui::Rect,
    state: &State,
) {
    const FIELD_SIZE: f32 = 15.0;

    let cols = (render_rect.width() / FIELD_SIZE).ceil() as i32;
    let rows = (render_rect.height() / FIELD_SIZE).ceil() as i32;

    let color1 = ubvec4::new(180, 180, 180, 255);
    let color2 = ubvec4::new(140, 140, 140, 255);

    for row in 0..rows {
        for col in 0..cols {
            let x = render_rect.min.x + (col as f32 * FIELD_SIZE);
            let y = render_rect.min.y + (row as f32 * FIELD_SIZE);

            let checker_rect = egui::Rect::from_min_size(
                egui::pos2(x, y),
                egui::vec2(
                    FIELD_SIZE.min(render_rect.max.x - x),
                    FIELD_SIZE.min(render_rect.max.y - y),
                ),
            );

            let color = if (row + col) % 2 == 0 { color1 } else { color2 };

            render_filled_rect_from_state(stream_handle, checker_rect, color, *state, false);
        }
    }
}

pub fn render_checkerboard_ui(ui: &mut egui::Ui, render_rect: egui::Rect, field_size: f32) {
    let cols = (render_rect.width() / field_size).ceil() as i32;
    let rows = (render_rect.height() / field_size).ceil() as i32;

    let color1 = Color32::from_rgba_unmultiplied(180, 180, 180, 255);
    let color2 = Color32::from_rgba_unmultiplied(140, 140, 140, 255);

    for row in 0..rows {
        for col in 0..cols {
            let x = col as f32 * field_size;
            let y = row as f32 * field_size;

            let checker_rect = egui::Rect::from_min_size(
                render_rect.min + egui::vec2(x, y),
                egui::vec2(
                    field_size.min(render_rect.max.x - x),
                    field_size.min(render_rect.max.y - y),
                ),
            );

            let color = if (row + col) % 2 == 0 { color1 } else { color2 };

            ui.painter().rect_filled(checker_rect, 0.0, color);
        }
    }
}

pub fn render_filled_rect_from_state(
    stream_handle: &GraphicsStreamHandle,
    rect: egui::Rect,
    color: ubvec4,
    mut state: State,
    as_stencil: bool,
) {
    state.set_stencil_mode(if as_stencil {
        StencilMode::FillStencil
    } else {
        StencilMode::None
    });
    state.set_color_mask(if as_stencil {
        ColorMaskMode::WriteAlphaOnly
    } else {
        ColorMaskMode::WriteAll
    });

    let pos = rect.min;
    let size = rect.size();
    stream_handle.render_quads(
        &[StreamedQuad::default()
            .from_pos_and_size(vec2::new(pos.x, pos.y), vec2::new(size.x, size.y))
            .tex_free_form(
                vec2::new(0.0, 0.0),
                vec2::new(1.0, 0.0),
                vec2::new(1.0, 1.0),
                vec2::new(0.0, 1.0),
            )
            .color(color)],
        state,
        TextureType::None,
    );
}

pub fn render_filled_rect(
    canvas_handle: &GraphicsCanvasHandle,
    stream_handle: &GraphicsStreamHandle,
    map: &EditorMap,
    rect: egui::Rect,
    color: ubvec4,
    parallax: &vec2,
    offset: &vec2,
    as_stencil: bool,
) {
    let state = render_rect_state(canvas_handle, map, parallax, offset);
    render_filled_rect_from_state(stream_handle, rect, color, state, as_stencil)
}
