use std::collections::{BTreeMap, HashSet};

use camera::CameraInterface;
use graphics::handles::{
    canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
};
use graphics_types::rendering::State;
use hiarc::Hiarc;
use map::map::groups::layers::design::Quad;
use math::math::vector::{dvec2, ffixed, ubvec4, vec2};

use crate::{
    actions::actions::{ActChangeQuadAttr, EditorAction},
    client::EditorClient,
    map::{EditorLayer, EditorLayerUnionRef, EditorMap, EditorMapInterface},
    tools::{
        shared::{align_pos, in_radius, rotate},
        utils::render_rect,
    },
    utils::{UiCanvasSize, ui_pos_to_world_pos, ui_pos_to_world_pos_and_world_height},
};

use super::shared::{
    QUAD_POINT_RADIUS_FACTOR, QuadPointerDownPoint, QuadSelectionQuads, render_quad_points,
};

#[derive(Debug, Hiarc)]
pub enum QuadPointerDownState {
    None,
    /// quad corner/center point
    Point {
        point: QuadPointerDownPoint,
        cursor_in_world_pos: vec2,
        cursor_corner_offset: vec2,
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
pub struct QuadSelection {
    pub range: Option<QuadSelectionQuads>,
    pub pos_offset: dvec2,

    pub pointer_down_state: QuadPointerDownState,
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
        }
    }

    fn handle_brush_select(
        &mut self,
        ui_canvas: &UiCanvasSize,
        canvas_handle: &GraphicsCanvasHandle,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        latest_modifiers: &egui::Modifiers,
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
        let is_primary_allowed_down = !latest_modifiers.ctrl && latest_pointer.primary_down();

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
            let &vec2 {
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
                let points =
                    super::shared::get_quad_points_animated(quad, map, map.user.render_time());

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

            if !is_primary_allowed_down {
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
        map: &mut EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
        latest_modifiers: &egui::Modifiers,
        latest_keys_down: &HashSet<egui::Key>,
        client: &EditorClient,
    ) {
        let layer = map.active_layer();
        let (offset, parallax) = if let Some(layer) = &layer {
            layer.get_offset_and_parallax()
        } else {
            Default::default()
        };
        let Some(EditorLayerUnionRef::Design {
            layer: EditorLayer::Quad(layer),
            is_background,
            group_index,
            layer_index,
            ..
        }) = layer
        else {
            return;
        };
        let range = self.range.as_mut().unwrap();

        let pointer_cur = vec2::new(current_pointer_pos.x, current_pointer_pos.y);

        let is_primary_allowed_down = !latest_modifiers.ctrl && latest_pointer.primary_down();
        let is_primary_allowed_pressed = !latest_modifiers.ctrl && latest_pointer.primary_pressed();
        let is_primary_allowed_released =
            !latest_modifiers.ctrl && latest_pointer.primary_released();

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
            cursor_in_world_pos,
            cursor_corner_offset,
        }) = is_primary_allowed_down.then_some(&mut self.pointer_down_state)
        {
            let align_pos = |pos: vec2| align_pos(map, latest_modifiers, pos);

            let new_pos = vec2::new(x, y);
            let aligned_pos = align_pos(new_pos);
            let new_pos = if let Some(aligned_pos) = aligned_pos {
                aligned_pos + *cursor_corner_offset
            } else {
                new_pos
            };

            let x_diff = new_pos.x - cursor_in_world_pos.x;
            let y_diff = new_pos.y - cursor_in_world_pos.y;
            // for rotation
            let diff = x_diff;

            *cursor_in_world_pos = new_pos;

            if let Some(range) = &mut self.range {
                let quads = range.indices_checked(layer);
                let pos_anim = quads.values().next().and_then(|q| q.pos_anim);

                let alter_anim_point = map.user.change_animations()
                    && pos_anim.is_some_and(|a| quads.values().all(|q| q.pos_anim == Some(a)));
                if alter_anim_point {
                    if latest_keys_down.contains(&egui::Key::R) {
                        if let Some(pos) = &mut map.animations.user.active_anim_points.pos {
                            pos.value.z += ffixed::from_num(diff);
                        }
                    } else if let Some(pos) = &mut map.animations.user.active_anim_points.pos {
                        pos.value.x += ffixed::from_num(x_diff);
                        pos.value.y += ffixed::from_num(y_diff);
                    }
                } else {
                    quads.into_iter().for_each(|(index, q)| {
                        let old = *q;

                        if latest_keys_down.contains(&egui::Key::R) {
                            // handle rotation
                            let (points, center) = q.points.split_at_mut(4);

                            rotate(&center[0], ffixed::from_num(diff), points);
                        } else {
                            let diff_x = ffixed::from_num(x_diff);
                            let diff_y = ffixed::from_num(y_diff);
                            q.points[4].x += diff_x;
                            q.points[4].y += diff_y;

                            if !latest_modifiers.shift {
                                // move other points too (because shift is not pressed to only move center)
                                for i in 0..4 {
                                    q.points[i].x += diff_x;
                                    q.points[i].y += diff_y;
                                }
                            }
                        }

                        if old != *q {
                            client.execute(
                                EditorAction::ChangeQuadAttr(Box::new(ActChangeQuadAttr {
                                    is_background,
                                    group_index,
                                    layer_index,
                                    old_attr: old,
                                    new_attr: *q,

                                    index,
                                })),
                                Some(&format!(
                                    "change-quad-attr-\
                                    {is_background}-{group_index}-{layer_index}-{index}"
                                )),
                            );
                        }
                    });

                    // move the selection, small visual upgrade
                    if !latest_keys_down.contains(&egui::Key::R) {
                        range.x += x_diff;
                        range.y += y_diff;
                    }
                }
            }
        } else {
            // check if the pointer clicked on one of the quad corner/center points
            let mut clicked_quad_point = false;
            if is_primary_allowed_pressed || latest_pointer.secondary_pressed() {
                for quad in layer.layer.quads.iter() {
                    let points =
                        super::shared::get_quad_points_animated(quad, map, map.user.render_time());

                    let pointer_cur = vec2::new(current_pointer_pos.x, current_pointer_pos.y);

                    let (pointer_cur, h) = ui_pos_to_world_pos_and_world_height(
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

                    let h = h / canvas_handle.canvas_height() as f32;
                    let radius = QUAD_POINT_RADIUS_FACTOR * h;
                    let mut p = [false; 5];
                    p.iter_mut().enumerate().for_each(|(index, p)| {
                        *p = in_radius(&points[index], &pointer_cur, radius)
                    });
                    // for now only respect the center point.
                    if p[4] {
                        let index = 4;
                        // pointer is in a drag mode
                        clicked_quad_point = true;
                        let down_point = if index == 4 {
                            QuadPointerDownPoint::Center
                        } else {
                            QuadPointerDownPoint::Corner(index)
                        };
                        let quad_pos =
                            vec2::new(points[index].x.to_num(), points[index].y.to_num());
                        let cursor = vec2::new(x, y);
                        if is_primary_allowed_pressed {
                            self.pointer_down_state = QuadPointerDownState::Point {
                                point: down_point,
                                cursor_in_world_pos: cursor,
                                cursor_corner_offset: cursor - quad_pos,
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

            if is_primary_allowed_released {
                self.pointer_down_state = QuadPointerDownState::None;
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
        latest_modifiers: &egui::Modifiers,
        current_pointer_pos: &egui::Pos2,
    ) {
        let layer = map.active_layer();
        let (offset, parallax) = if let Some(layer) = &layer {
            layer.get_offset_and_parallax()
        } else {
            Default::default()
        };
        let is_primary_allowed_down = !latest_modifiers.ctrl && latest_pointer.primary_down();
        // if pointer was already down
        if let QuadPointerDownState::Selection(pointer_down) = &self.pointer_down_state
            && is_primary_allowed_down
        {
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

        let group_attr = layer.map(|layer| layer.get_or_fake_group_attr());
        map.game_camera()
            .project(canvas_handle, &mut state, group_attr.as_ref());

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
        map: &mut EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
        latest_modifiers: &egui::Modifiers,
        latest_keys_down: &HashSet<egui::Key>,
        client: &EditorClient,
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
                latest_modifiers,
                current_pointer_pos,
            );
        } else if self.range.is_some() {
            self.handle_selected(
                ui_canvas,
                canvas_handle,
                map,
                latest_pointer,
                current_pointer_pos,
                latest_modifiers,
                latest_keys_down,
                client,
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
        latest_modifiers: &egui::Modifiers,
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
                latest_modifiers,
                current_pointer_pos,
            );
        } else {
            self.render_brush(canvas_handle, stream_handle, map);
        }
    }
}
