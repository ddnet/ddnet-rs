use camera::{Camera, CameraInterface};
use egui::{vec2, Rect};
use graphics::handles::canvas::canvas::GraphicsCanvasHandle;
use graphics_types::rendering::State;
use map::map::groups::MapGroupAttr;
use math::math::vector::{ffixed, fvec2, vec2};

pub type UiCanvasSize = Rect;

pub fn ui_pos_to_world_pos_and_world_height(
    canvas_handle: &GraphicsCanvasHandle,
    ui_canvas: &UiCanvasSize,
    zoom: f32,
    inp: vec2,
    center_x: f32,
    center_y: f32,
    offset_x: f32,
    offset_y: f32,
    parallax_x: f32,
    parallax_y: f32,
    parallax_aware_zoom: bool,
) -> (vec2, f32) {
    let mut fake_state = State::new();
    Camera::new(
        vec2::new(center_x, center_y),
        zoom,
        None,
        parallax_aware_zoom,
    )
    .project(
        canvas_handle,
        &mut fake_state,
        Some(&MapGroupAttr {
            offset: fvec2::new(ffixed::from_num(offset_x), ffixed::from_num(offset_y)),
            parallax: fvec2::new(ffixed::from_num(parallax_x), ffixed::from_num(parallax_y)),
            clipping: None,
        }),
    );
    let (tl_x, tl_y, br_x, br_y) = fake_state.get_canvas_mapping();

    let x = inp.x;
    let y = inp.y;

    let size = ui_canvas
        .size()
        .clamp(vec2(0.01, 0.01), vec2(f32::MAX, f32::MAX));
    let x_ratio = x / size.x;
    let y_ratio = y / size.y;

    let x = tl_x + x_ratio * (br_x - tl_x);
    let y = tl_y + y_ratio * (br_y - tl_y);

    (vec2::new(x, y), br_y - tl_y)
}

pub fn ui_pos_to_world_pos(
    canvas_handle: &GraphicsCanvasHandle,
    ui_canvas: &UiCanvasSize,
    zoom: f32,
    inp: vec2,
    center_x: f32,
    center_y: f32,
    offset_x: f32,
    offset_y: f32,
    parallax_x: f32,
    parallax_y: f32,
    parallax_aware_zoom: bool,
) -> vec2 {
    ui_pos_to_world_pos_and_world_height(
        canvas_handle,
        ui_canvas,
        zoom,
        inp,
        center_x,
        center_y,
        offset_x,
        offset_y,
        parallax_x,
        parallax_y,
        parallax_aware_zoom,
    )
    .0
}
