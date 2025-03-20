use std::{collections::BTreeMap, time::Duration};

use client_render_base::map::render_tools::{CanvasType, RenderTools};
use graphics::handles::{
    canvas::canvas::GraphicsCanvasHandle,
    stream::stream::{GraphicsStreamHandle, QuadStreamHandle},
    stream_types::StreamedQuad,
};
use graphics_types::rendering::State;
use hiarc::{hi_closure, Hiarc};
use map::map::groups::layers::design::Quad;
use math::math::vector::{ffixed, fvec2, nffixed, nfvec4, ubvec4, vec2};

use crate::{
    map::{EditorLayer, EditorLayerQuad, EditorLayerUnionRef, EditorMap},
    tools::shared::{in_radius, rotate},
    utils::{ui_pos_to_world_pos, UiCanvasSize},
};

#[derive(Debug, Hiarc, Clone, Copy)]
pub enum QuadPointerDownPoint {
    Center,
    Corner(usize),
}

#[derive(Debug, Hiarc)]
pub struct QuadSelectionQuads {
    pub quads: BTreeMap<usize, Quad>,

    /// selection x offset
    pub x: f32,
    /// selection y offset
    pub y: f32,
    /// width of the selection
    pub w: f32,
    /// height of the selection
    pub h: f32,

    pub point: Option<QuadPointerDownPoint>,
}

impl QuadSelectionQuads {
    pub fn indices_checked(&mut self, layer: &EditorLayerQuad) -> BTreeMap<usize, &mut Quad> {
        while self
            .quads
            .last_key_value()
            .is_some_and(|(index, _)| *index >= layer.layer.quads.len())
        {
            self.quads.pop_last();
        }

        self.quads
            .iter_mut()
            .map(|(index, quad)| (*index, quad))
            .collect()
    }
}

pub fn in_box(pos: &fvec2, x0: f32, y0: f32, x1: f32, y1: f32) -> bool {
    pos.x.to_num::<f32>() >= x0
        && pos.x.to_num::<f32>() < x1
        && pos.y.to_num::<f32>() >= y0
        && pos.y.to_num::<f32>() < y1
}

pub fn get_quad_points_animated(quad: &Quad, map: &EditorMap, time: Duration) -> [fvec2; 5] {
    let mut points = quad.points;
    if let Some(pos_anim) = quad.pos_anim {
        let anim = &map.animations.pos[pos_anim];
        let anim_pos = RenderTools::render_eval_anim(
            anim.def.points.as_slice(),
            time::Duration::try_from(time).unwrap(),
        );
        let rot = anim_pos.z / ffixed::from_num(360.0) * ffixed::PI * ffixed::from_num(2.0);
        let center = points[4];

        rotate(&center, rot, &mut points);

        for point in points.iter_mut() {
            *point += fvec2::new(ffixed::from_num(anim_pos.x), ffixed::from_num(anim_pos.y));
        }
    }
    points
}

pub fn get_quad_points_color_animated(quad: &Quad, map: &EditorMap, time: Duration) -> [nfvec4; 4] {
    let mut color = quad.colors;
    if let Some(color_anim) = quad.color_anim {
        let anim = &map.animations.color[color_anim];
        let anim_color = RenderTools::render_eval_anim(
            anim.def.points.as_slice(),
            time::Duration::try_from(time).unwrap(),
        );

        for color in color.iter_mut() {
            color.x *= nffixed::from_num(anim_color.x);
            color.y *= nffixed::from_num(anim_color.y);
            color.z *= nffixed::from_num(anim_color.z);
            color.w *= nffixed::from_num(anim_color.w);
        }
    }
    color
}

pub const QUAD_POINT_RADIUS: f32 = 0.75;

pub fn render_quad_points(
    ui_canvas: &UiCanvasSize,
    layer: Option<EditorLayerUnionRef>,

    current_pointer_pos: &egui::Pos2,
    stream_handle: &GraphicsStreamHandle,
    canvas_handle: &GraphicsCanvasHandle,
    map: &EditorMap,
    render_corner_points: bool,
) {
    // render quad corner/center points
    if let Some(EditorLayerUnionRef::Design {
        layer: EditorLayer::Quad(layer),
        group,
        ..
    }) = layer
    {
        let (offset, parallax) = (group.attr.offset, group.attr.parallax);

        let pos = current_pointer_pos;

        let pos = vec2::new(pos.x, pos.y);

        let vec2 { x, y } = ui_pos_to_world_pos(
            canvas_handle,
            ui_canvas,
            map.groups.user.zoom,
            vec2::new(pos.x, pos.y),
            map.groups.user.pos.x,
            map.groups.user.pos.y,
            offset.x.to_num::<f32>(),
            offset.y.to_num::<f32>(),
            parallax.x.to_num::<f32>(),
            parallax.y.to_num::<f32>(),
            map.groups.user.parallax_aware_zoom,
        );
        for quad in &layer.layer.quads {
            let points = get_quad_points_animated(quad, map, map.user.time);

            let mut state = State::new();
            RenderTools::map_canvas_of_group(
                CanvasType::Handle(canvas_handle),
                &mut state,
                map.groups.user.pos.x,
                map.groups.user.pos.y,
                Some(&group.attr),
                map.groups.user.zoom,
                map.groups.user.parallax_aware_zoom,
            );
            stream_handle.render_quads(
                hi_closure!([points: [fvec2; 5], x: f32, y: f32, render_corner_points: bool], |mut stream_handle: QuadStreamHandle<'_>| -> () {
                    let point_size = QUAD_POINT_RADIUS * 0.7;
                    if render_corner_points {
                        for point in &points[0..4] {
                            let color = if in_radius(point, &vec2::new(x, y), QUAD_POINT_RADIUS) {
                                ubvec4::new(150, 150, 255, 255)
                            }
                            else {
                                ubvec4::new(0, 0, 255, 255)
                            };
                            stream_handle.add_vertices(
                                StreamedQuad::default().from_pos_and_size(
                                    vec2::new(point.x.to_num::<f32>() - point_size / 2.0, point.y.to_num::<f32>() - point_size / 2.0),
                                    vec2::new(point_size, point_size)
                                )
                                .color(color)
                                .into()
                            );
                        }
                    }
                    let color = if in_radius(&points[4], &vec2::new(x, y), QUAD_POINT_RADIUS) {
                        ubvec4::new(150, 255, 150, 255)
                    }
                    else {
                        ubvec4::new(0, 255, 0, 255)
                    };
                    stream_handle.add_vertices(
                        StreamedQuad::default().from_pos_and_size(
                            vec2::new(points[4].x.to_num::<f32>() - point_size / 2.0, points[4].y.to_num::<f32>() - point_size / 2.0),
                            vec2::new(point_size, point_size)
                        )
                        .color(color)
                        .into()
                    );
                }),
                state,
            );
        }
    }
}
