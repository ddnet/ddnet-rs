use std::{collections::BTreeMap, time::Duration};

use client_render_base::map::render_tools::{CanvasType, RenderTools};
use graphics::handles::{
    canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
};
use graphics_types::rendering::State;
use hiarc::Hiarc;
use map::map::{
    animations::{AnimPointColor, AnimPointCurveType, AnimPointPos},
    groups::layers::design::Quad,
};
use math::math::vector::{dvec2, ffixed, fvec3, nfvec4, ubvec4, vec2};

use crate::{
    map::{EditorLayer, EditorLayerQuad, EditorLayerUnionRef, EditorMap, EditorMapInterface},
    tools::{shared::in_radius, utils::render_rect},
    utils::{ui_pos_to_world_pos, UiCanvasSize},
};

use super::shared::{render_quad_points, QuadPointerDownPoint, QUAD_POINT_RADIUS};

#[derive(Debug, Hiarc)]
pub enum QuadPointerDownState {
    None,
    /// quad corner/center point
    Point {
        point: QuadPointerDownPoint,
        pos: vec2,
    },
    /// selection of quads
    Selection(vec2),
}

impl QuadPointerDownState {
    pub fn is_selection(&self) -> bool {
        matches!(self, Self::Selection(_))
    }
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

#[derive(Debug, Hiarc)]
pub struct QuadSelection {
    pub range: Option<QuadSelectionQuads>,
    pub pos_offset: dvec2,

    pub pointer_down_state: QuadPointerDownState,

    /// to be used to alter the animation using quad properties
    pub anim_point_pos: AnimPointPos,
    /// to be used to alter the animation using quad properties
    pub anim_point_color: AnimPointColor,
}

impl Default for QuadSelection {
    fn default() -> Self {
        Self::new()
    }
}

impl QuadSelection {
    pub fn new() -> Self {
        Self {
            pointer_down_state: QuadPointerDownState::None,
            pos_offset: dvec2::default(),
            range: None,

            anim_point_pos: AnimPointPos {
                time: Duration::ZERO,
                curve_type: AnimPointCurveType::Linear,
                value: fvec3::default(),
            },
            anim_point_color: AnimPointColor {
                time: Duration::ZERO,
                curve_type: AnimPointCurveType::Linear,
                value: nfvec4::default(),
            },
        }
    }

    fn handle_brush_select(
        &mut self,
        ui_canvas: &UiCanvasSize,
        canvas_handle: &GraphicsCanvasHandle,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
    ) {
        let layer = map.active_layer();
        let (offset, parallax) = if let Some(layer) = &layer {
            layer.get_offset_and_parallax()
        } else {
            Default::default()
        };
        let Some(EditorLayerUnionRef::Design {
            layer: EditorLayer::Quad(layer),
            ..
        }) = layer
        else {
            return;
        };

        let pointer_cur = vec2::new(current_pointer_pos.x, current_pointer_pos.y);

        let vec2 {
            x: mut x1,
            y: mut y1,
        } = ui_pos_to_world_pos(
            canvas_handle,
            ui_canvas,
            map.groups.user.zoom,
            vec2::new(pointer_cur.x, pointer_cur.y),
            map.groups.user.pos.x,
            map.groups.user.pos.y,
            offset.x,
            offset.y,
            parallax.x,
            parallax.y,
            map.groups.user.parallax_aware_zoom,
        );

        // check if selection phase ended
        if let QuadPointerDownState::Selection(pointer_down) = &self.pointer_down_state {
            // find current layer
            let vec2 {
                x: mut x0,
                y: mut y0,
            } = pointer_down;

            if x0 > x1 {
                std::mem::swap(&mut x0, &mut x1);
            }
            if y0 > y1 {
                std::mem::swap(&mut y0, &mut y1);
            }

            // check if any quads are in the selection
            let mut quads: BTreeMap<usize, Quad> = Default::default();

            for (q, quad) in layer.layer.quads.iter().enumerate() {
                let points = super::shared::get_quad_points_animated(quad, map, map.user.time);

                if super::shared::in_box(&points[0], x0, y0, x1, y1)
                    || super::shared::in_box(&points[1], x0, y0, x1, y1)
                    || super::shared::in_box(&points[2], x0, y0, x1, y1)
                    || super::shared::in_box(&points[3], x0, y0, x1, y1)
                    || super::shared::in_box(&points[4], x0, y0, x1, y1)
                {
                    quads.insert(q, *quad);
                }
            }

            // if there is an selection, apply that
            if !quads.is_empty() {
                self.range = Some(QuadSelectionQuads {
                    quads,
                    x: x0,
                    y: y0,
                    w: x1 - x0,
                    h: y1 - y0,

                    point: None,
                });
            } else {
                self.range = None;
            }

            if !latest_pointer.primary_down() {
                self.pointer_down_state = QuadPointerDownState::None;
            }
        } else {
            let pointer_cur = vec2::new(current_pointer_pos.x, current_pointer_pos.y);
            let pos = ui_pos_to_world_pos(
                canvas_handle,
                ui_canvas,
                map.groups.user.zoom,
                vec2::new(pointer_cur.x, pointer_cur.y),
                map.groups.user.pos.x,
                map.groups.user.pos.y,
                offset.x,
                offset.y,
                parallax.x,
                parallax.y,
                map.groups.user.parallax_aware_zoom,
            );
            self.pointer_down_state = QuadPointerDownState::Selection(pos);
        }
    }

    fn handle_selected(
        &mut self,
        ui_canvas: &UiCanvasSize,
        canvas_handle: &GraphicsCanvasHandle,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
    ) {
        let layer = map.active_layer();
        let (offset, parallax) = if let Some(layer) = &layer {
            layer.get_offset_and_parallax()
        } else {
            Default::default()
        };
        let Some(EditorLayerUnionRef::Design {
            layer: EditorLayer::Quad(layer),
            ..
        }) = layer
        else {
            return;
        };
        let range = self.range.as_mut().unwrap();

        let pointer_cur = vec2::new(current_pointer_pos.x, current_pointer_pos.y);

        let vec2 { x, y } = ui_pos_to_world_pos(
            canvas_handle,
            ui_canvas,
            map.groups.user.zoom,
            vec2::new(pointer_cur.x, pointer_cur.y),
            map.groups.user.pos.x,
            map.groups.user.pos.y,
            offset.x,
            offset.y,
            parallax.x,
            parallax.y,
            map.groups.user.parallax_aware_zoom,
        );

        if let Some(QuadPointerDownState::Point {
            point: QuadPointerDownPoint::Center,
            pos,
        }) = latest_pointer
            .primary_down()
            .then_some(&mut self.pointer_down_state)
        {
            let x_diff = x - pos.x;
            let y_diff = y - pos.y;
            *pos = vec2::new(x, y);
            self.anim_point_pos.value.x += ffixed::from_num(x_diff);
            self.anim_point_pos.value.y += ffixed::from_num(y_diff);
        } else {
            // check if the pointer clicked on one of the quad corner/center points
            let mut clicked_quad_point = false;
            if latest_pointer.primary_pressed() || latest_pointer.secondary_pressed() {
                for quad in layer.layer.quads.iter() {
                    let points = super::shared::get_quad_points_animated(quad, map, map.user.time);

                    let pointer_cur = vec2::new(current_pointer_pos.x, current_pointer_pos.y);

                    let pointer_cur = ui_pos_to_world_pos(
                        canvas_handle,
                        ui_canvas,
                        map.groups.user.zoom,
                        vec2::new(pointer_cur.x, pointer_cur.y),
                        map.groups.user.pos.x,
                        map.groups.user.pos.y,
                        offset.x,
                        offset.y,
                        parallax.x,
                        parallax.y,
                        map.groups.user.parallax_aware_zoom,
                    );

                    let radius = QUAD_POINT_RADIUS;
                    let mut p = [false; 5];
                    p.iter_mut().enumerate().for_each(|(index, p)| {
                        *p = in_radius(&points[index], &pointer_cur, radius)
                    });
                    if let Some((index, _)) = p.iter().enumerate().find(|(_, &p)| p) {
                        // pointer is in a drag mode
                        clicked_quad_point = true;
                        let down_point = if index == 4 {
                            QuadPointerDownPoint::Center
                        } else {
                            QuadPointerDownPoint::Corner(index)
                        };
                        if latest_pointer.primary_pressed() {
                            self.pointer_down_state = QuadPointerDownState::Point {
                                point: down_point,
                                pos: vec2::new(x, y),
                            };
                        } else {
                            range.point = Some(down_point);
                        }

                        break;
                    }
                }

                if !clicked_quad_point && latest_pointer.secondary_pressed() {
                    self.range = None;
                    self.pointer_down_state = QuadPointerDownState::None;
                }
            }
        }
    }

    fn render_selection(
        &self,
        ui_canvas: &UiCanvasSize,
        canvas_handle: &GraphicsCanvasHandle,
        stream_handle: &GraphicsStreamHandle,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
    ) {
        let layer = map.active_layer();
        let (offset, parallax) = if let Some(layer) = &layer {
            layer.get_offset_and_parallax()
        } else {
            Default::default()
        };
        // if pointer was already down
        if let QuadPointerDownState::Selection(pointer_down) = &self.pointer_down_state {
            if latest_pointer.primary_down() {
                let pos = current_pointer_pos;
                let pos = ui_pos_to_world_pos(
                    canvas_handle,
                    ui_canvas,
                    map.groups.user.zoom,
                    vec2::new(pos.x, pos.y),
                    map.groups.user.pos.x,
                    map.groups.user.pos.y,
                    offset.x,
                    offset.y,
                    parallax.x,
                    parallax.y,
                    map.groups.user.parallax_aware_zoom,
                );
                let pos = egui::pos2(pos.x, pos.y);

                let down_pos = pointer_down;
                let down_pos = egui::pos2(down_pos.x, down_pos.y);

                let rect = egui::Rect::from_min_max(pos, down_pos);

                render_rect(
                    canvas_handle,
                    stream_handle,
                    map,
                    rect,
                    ubvec4::new(255, 0, 0, 255),
                    &parallax,
                    &offset,
                );
            }
        }
    }
    fn render_brush(
        &self,
        canvas_handle: &GraphicsCanvasHandle,
        stream_handle: &GraphicsStreamHandle,
        map: &EditorMap,
    ) {
        let layer = map.active_layer();
        let (offset, parallax) = if let Some(layer) = &layer {
            layer.get_offset_and_parallax()
        } else {
            Default::default()
        };

        let mut state = State::new();

        let range = self.range.as_ref().unwrap();

        let (center, group_attr) = (
            map.groups.user.pos,
            layer.map(|layer| layer.get_or_fake_group_attr()),
        );
        RenderTools::map_canvas_of_group(
            CanvasType::Handle(canvas_handle),
            &mut state,
            center.x,
            center.y,
            group_attr.as_ref(),
            map.groups.user.zoom,
            map.groups.user.parallax_aware_zoom,
        );

        let range_size = vec2::new(range.w, range.h);
        let rect = egui::Rect::from_min_max(
            egui::pos2(range.x, range.y),
            egui::pos2(range.x + range_size.x, range.y + range_size.y),
        );

        render_rect(
            canvas_handle,
            stream_handle,
            map,
            rect,
            ubvec4::new(0, 0, 255, 255),
            &parallax,
            &offset,
        );
    }

    pub fn update(
        &mut self,
        ui_canvas: &UiCanvasSize,
        canvas_handle: &GraphicsCanvasHandle,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
    ) {
        let layer = map.active_layer();
        if !layer.as_ref().is_some_and(|layer| layer.is_quad_layer()) {
            return;
        }

        if self.range.is_none() || self.pointer_down_state.is_selection() {
            self.handle_brush_select(
                ui_canvas,
                canvas_handle,
                map,
                latest_pointer,
                current_pointer_pos,
            );
        } else if self.range.is_some() {
            self.handle_selected(
                ui_canvas,
                canvas_handle,
                map,
                latest_pointer,
                current_pointer_pos,
            );
        }
    }

    pub fn render(
        &mut self,
        ui_canvas: &UiCanvasSize,
        stream_handle: &GraphicsStreamHandle,
        canvas_handle: &GraphicsCanvasHandle,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
    ) {
        let layer = map.active_layer();
        if !layer.as_ref().is_some_and(|layer| layer.is_quad_layer()) {
            return;
        }

        render_quad_points(
            ui_canvas,
            layer,
            current_pointer_pos,
            stream_handle,
            canvas_handle,
            map,
            false,
        );

        if self.range.is_none() || self.pointer_down_state.is_selection() {
            self.render_selection(
                ui_canvas,
                canvas_handle,
                stream_handle,
                map,
                latest_pointer,
                current_pointer_pos,
            );
        } else {
            self.render_brush(canvas_handle, stream_handle, map);
        }
    }
}
