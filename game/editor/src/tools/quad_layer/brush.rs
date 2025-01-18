use std::{cell::Cell, time::Duration};

use client_render_base::map::{
    map::RenderMap,
    map_buffered::QuadLayerVisuals,
    map_pipeline::{MapGraphics, QuadRenderInfo},
    render_tools::{CanvasType, RenderTools},
};
use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::{
        backend::backend::GraphicsBackendHandle,
        buffer_object::buffer_object::{BufferObject, GraphicsBufferObjectHandle},
        canvas::canvas::GraphicsCanvasHandle,
        stream::stream::{GraphicsStreamHandle, StreamedUniforms},
        texture::texture::TextureContainer,
    },
};
use graphics_types::rendering::State;
use hiarc::{hi_closure, Hiarc};
use map::{map::groups::layers::design::Quad, skeleton::animations::AnimationsSkeleton};
use math::math::vector::{ffixed, ubvec4, vec2};

use crate::{
    actions::actions::{
        ActChangeQuadAttr, ActQuadLayerAddQuads, ActQuadLayerAddRemQuads, EditorAction,
    },
    client::EditorClient,
    map::{EditorLayer, EditorLayerUnionRef, EditorMap, EditorMapInterface},
    map_tools::{finish_design_quad_layer_buffer, upload_design_quad_layer_buffer},
    tools::{
        quad_layer::shared::QUAD_POINT_RADIUS,
        shared::{in_radius, rotate},
        utils::render_rect,
    },
    utils::{ui_pos_to_world_pos, UiCanvasSize},
};

use super::shared::{render_quad_points, QuadPointerDownPoint};

#[derive(Debug, Hiarc)]
pub struct QuadBrushQuads {
    pub quads: Vec<Quad>,
    pub w: f32,
    pub h: f32,

    pub render: QuadLayerVisuals,
    pub map_render: MapGraphics,
    pub texture: TextureContainer,
}

#[derive(Debug, Hiarc)]
pub struct QuadSelection {
    pub is_background: bool,
    pub group: usize,
    pub layer: usize,
    pub quad_index: usize,
    pub quad: Quad,
    pub point: QuadPointerDownPoint,
    pub cursor_in_world_pos: Option<vec2>,
}

#[derive(Debug, Hiarc)]
pub enum QuadPointerDownState {
    None,
    /// quad corner/center point
    Point(QuadPointerDownPoint),
    /// selection of quads
    Selection(vec2),
}

impl QuadPointerDownState {
    pub fn is_selection(&self) -> bool {
        matches!(self, Self::Selection(_))
    }
}

/// quad brushes are relative to where the mouse selected them
#[derive(Debug, Hiarc)]
pub struct QuadBrush {
    pub brush: Option<QuadBrushQuads>,

    /// this is the last quad selected (clicked on the corner selectors), this can be used
    /// for the animation to know the current quad
    pub last_selection: Option<QuadSelection>,
    pub last_active: Option<QuadSelection>,

    pub pointer_down_state: QuadPointerDownState,
}

impl Default for QuadBrush {
    fn default() -> Self {
        Self::new()
    }
}

impl QuadBrush {
    pub fn new() -> Self {
        Self {
            brush: Default::default(),
            last_selection: None,
            last_active: None,
            pointer_down_state: QuadPointerDownState::None,
        }
    }

    fn handle_brush_select(
        &mut self,
        ui_canvas: &UiCanvasSize,
        graphics_mt: &GraphicsMultiThreaded,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        backend_handle: &GraphicsBackendHandle,
        canvas_handle: &GraphicsCanvasHandle,
        map: &EditorMap,
        fake_texture: &TextureContainer,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
        latest_modifiers: &egui::Modifiers,
        client: &mut EditorClient,
    ) {
        let layer = map.active_layer();
        let (offset, parallax) = if let Some(layer) = &layer {
            layer.get_offset_and_parallax()
        } else {
            Default::default()
        };
        let Some(EditorLayerUnionRef::Design {
            layer: EditorLayer::Quad(layer),
            group_index,
            is_background,
            layer_index,
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
        );

        // if pointer was already down
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
            let mut quads: Vec<Quad> = Default::default();

            for quad in &layer.layer.quads {
                let points = super::shared::get_quad_points_animated(quad, map, map.user.time);

                if super::shared::in_box(&points[0], x0, y0, x1, y1)
                    || super::shared::in_box(&points[1], x0, y0, x1, y1)
                    || super::shared::in_box(&points[2], x0, y0, x1, y1)
                    || super::shared::in_box(&points[3], x0, y0, x1, y1)
                    || super::shared::in_box(&points[4], x0, y0, x1, y1)
                {
                    quads.push(quad.clone());
                }
            }

            // if there is an selection, apply that
            if !quads.is_empty() {
                let pointer_down = vec2::new(x0, y0);

                let x = -pointer_down.x;
                let y = -pointer_down.y;

                for quad in &mut quads {
                    for point in &mut quad.points {
                        point.x += ffixed::from_num(x);
                        point.y += ffixed::from_num(y);
                    }
                }

                let buffer =
                    upload_design_quad_layer_buffer(graphics_mt, &layer.layer.attr, &quads);
                let render =
                    finish_design_quad_layer_buffer(buffer_object_handle, backend_handle, buffer);
                self.brush = Some(QuadBrushQuads {
                    quads,
                    w: x1 - x0,
                    h: y1 - y0,
                    render,
                    map_render: MapGraphics::new(backend_handle),
                    texture: layer
                        .layer
                        .attr
                        .image
                        .map(|img| map.resources.images[img].user.user.clone())
                        .unwrap_or_else(|| fake_texture.clone()),
                });
            }

            if !latest_pointer.primary_down() {
                self.pointer_down_state = QuadPointerDownState::None;
            }
        } else {
            // check if the pointer clicked on one of the quad corner/center points
            let mut clicked_quad_point = false;
            if latest_pointer.primary_pressed() || latest_pointer.secondary_pressed() {
                for (q, quad) in layer.layer.quads.iter().enumerate() {
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
                        self.pointer_down_state = QuadPointerDownState::Point(down_point);
                        *if latest_pointer.primary_pressed() {
                            &mut self.last_active
                        } else {
                            &mut self.last_selection
                        } = Some(QuadSelection {
                            is_background,
                            group: group_index,
                            layer: layer_index,
                            quad_index: q,
                            quad: quad.clone(),
                            point: down_point,
                            cursor_in_world_pos: None,
                        });

                        break;
                    }
                }
            }
            // else check if the pointer is down now
            if !clicked_quad_point && latest_pointer.primary_pressed() && self.last_active.is_none()
            {
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
                );
                self.pointer_down_state = QuadPointerDownState::Selection(pos);
            }
            if !clicked_quad_point && latest_pointer.primary_pressed() {
                self.last_active = None;
            }
            if latest_pointer.primary_down() && self.last_active.is_some() {
                let last_active = self.last_active.as_mut().unwrap();
                if let Some(edit_quad) = layer.layer.quads.get(last_active.quad_index) {
                    let p = match last_active.point {
                        QuadPointerDownPoint::Center => 4,
                        QuadPointerDownPoint::Corner(index) => index,
                    };

                    let quad = &mut last_active.quad;

                    if matches!(last_active.point, QuadPointerDownPoint::Center)
                        && latest_modifiers.ctrl
                    {
                        if let Some(cursor_pos) = &last_active.cursor_in_world_pos {
                            // handle rotation
                            let diff = vec2::new(x1, y1) - vec2::new(cursor_pos.x, cursor_pos.y);
                            let diff = diff.x;

                            let (points, center) = quad.points.split_at_mut(4);
                            rotate(&center[0], ffixed::from_num(diff), points);
                        }
                    } else {
                        // handle position
                        let old_x = quad.points[p].x;
                        let old_y = quad.points[p].y;
                        quad.points[p].x = ffixed::from_num(x1);
                        quad.points[p].y = ffixed::from_num(y1);

                        if matches!(last_active.point, QuadPointerDownPoint::Center)
                            && !latest_modifiers.shift
                        {
                            // move other points too (because shift is not pressed to only move center)
                            let diff_x = quad.points[p].x - old_x;
                            let diff_y = quad.points[p].y - old_y;

                            for i in 0..4 {
                                quad.points[i].x += diff_x;
                                quad.points[i].y += diff_y;
                            }
                        }
                    }

                    if *quad != *edit_quad {
                        let index = last_active.quad_index;
                        client.execute(
                            EditorAction::ChangeQuadAttr(Box::new(ActChangeQuadAttr {
                                is_background,
                                group_index,
                                layer_index,
                                old_attr: edit_quad.clone(),
                                new_attr: quad.clone(),

                                index,
                            })),
                            Some(&format!(
                                "change-quad-attr-{is_background}-{group_index}-{layer_index}-{index}"
                            )),
                        );
                    }
                }

                last_active.cursor_in_world_pos = Some(vec2::new(x1, y1));
            }
        }
    }

    pub fn handle_brush_draw(
        &mut self,
        ui_canvas: &UiCanvasSize,
        canvas_handle: &GraphicsCanvasHandle,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
        client: &mut EditorClient,
    ) {
        let layer = map.active_layer().unwrap();
        let (offset, parallax) = layer.get_offset_and_parallax();

        // reset brush
        if latest_pointer.secondary_pressed() {
            self.brush = None;
        }
        // apply brush
        else {
            let brush = self.brush.as_ref().unwrap();

            if latest_pointer.primary_pressed() {
                let pos = current_pointer_pos;

                let pos = vec2::new(pos.x, pos.y);

                let vec2 { x, y } = ui_pos_to_world_pos(
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
                );

                let mut quads = brush.quads.clone();
                for quad in &mut quads {
                    for point in &mut quad.points {
                        point.x += ffixed::from_num(x);
                        point.y += ffixed::from_num(y);
                    }
                }

                if let Some((action, group_indentifier)) = if let EditorLayerUnionRef::Design {
                    layer: EditorLayer::Quad(layer),
                    layer_index,
                    is_background,
                    group_index,
                    ..
                } = layer
                {
                    Some((
                        EditorAction::QuadLayerAddQuads(ActQuadLayerAddQuads {
                            base: ActQuadLayerAddRemQuads {
                                is_background,
                                group_index,
                                layer_index,
                                index: layer.layer.quads.len(),
                                quads,
                            },
                        }),
                        format!("quad-brush design {}", layer_index),
                    ))
                } else {
                    None
                } {
                    client.execute(action, Some(&group_indentifier));
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
        ui_canvas: &UiCanvasSize,
        canvas_handle: &GraphicsCanvasHandle,
        stream_handle: &GraphicsStreamHandle,
        map: &EditorMap,
        current_pointer_pos: &egui::Pos2,
    ) {
        let layer = map.active_layer();
        let (offset, parallax) = if let Some(layer) = &layer {
            layer.get_offset_and_parallax()
        } else {
            Default::default()
        };

        let brush = self.brush.as_ref().unwrap();

        let pos = current_pointer_pos;
        let pos_on_map = ui_pos_to_world_pos(
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
        );
        let pos = pos_on_map;
        let pos = egui::pos2(pos.x, pos.y);

        let mut state = State::new();

        RenderTools::map_canvas_of_group(
            CanvasType::Handle(canvas_handle),
            &mut state,
            map.groups.user.pos.x,
            map.groups.user.pos.y,
            layer.map(|layer| layer.get_or_fake_group_attr()).as_ref(),
            map.groups.user.zoom,
        );
        let center = -pos_on_map;
        state.canvas_br.x += center.x;
        state.canvas_br.y += center.y;
        state.canvas_tl.x += center.x;
        state.canvas_tl.y += center.y;
        if let Some(buffer_object_index) = &brush.render.buffer_object_index {
            let quads = &brush.quads;
            let cur_time = &map.get_time();
            let cur_anim_time = &map.animation_time();
            let cur_quad_offset_cell = Cell::new(0);
            let cur_quad_offset = &cur_quad_offset_cell;
            let animations = &map.animations;
            stream_handle.fill_uniform_instance(
                hi_closure!(
                    <AN, AS>,
                    [
                    cur_time: &Duration,
                    cur_anim_time: &Duration,
                    cur_quad_offset: &Cell<usize>,
                    animations: &AnimationsSkeleton<AN, AS>,
                    quads: &Vec<Quad>,
                ], |stream_handle: StreamedUniforms<
                    '_,
                    QuadRenderInfo,
                >|
                 -> () {
                    RenderMap::prepare_quad_rendering(
                        stream_handle,
                        cur_time,
                        cur_anim_time,
                        cur_quad_offset,
                        animations,
                        quads
                    );
                }),
                hi_closure!([
                    brush: &QuadBrushQuads,
                    state: State,
                    buffer_object_index: &BufferObject,
                    cur_quad_offset: &Cell<usize>,
                ], |instance: usize, count: usize| -> () {
                    brush.map_render.render_quad_layer(
                        &state,
                        (&brush.texture).into(),
                        buffer_object_index,
                        instance,
                        count,
                        cur_quad_offset.get(),
                    );
                }),
            );
        }

        let brush_size = vec2::new(brush.w, brush.h);
        let rect =
            egui::Rect::from_min_max(pos, egui::pos2(pos.x + brush_size.x, pos.y + brush_size.y));

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

    pub fn update(
        &mut self,
        ui_canvas: &UiCanvasSize,
        graphics_mt: &GraphicsMultiThreaded,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        backend_handle: &GraphicsBackendHandle,
        canvas_handle: &GraphicsCanvasHandle,
        map: &EditorMap,
        fake_texture: &TextureContainer,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
        latest_modifiers: &egui::Modifiers,
        client: &mut EditorClient,
    ) {
        let layer = map.active_layer();
        if !layer.as_ref().is_some_and(|layer| layer.is_quad_layer()) {
            return;
        }

        if self.brush.is_none() || self.pointer_down_state.is_selection() {
            self.handle_brush_select(
                ui_canvas,
                graphics_mt,
                buffer_object_handle,
                backend_handle,
                canvas_handle,
                map,
                fake_texture,
                latest_pointer,
                current_pointer_pos,
                latest_modifiers,
                client,
            );
        } else {
            self.handle_brush_draw(
                ui_canvas,
                canvas_handle,
                map,
                latest_pointer,
                current_pointer_pos,
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
            true,
        );

        if self.brush.is_none() || self.pointer_down_state.is_selection() {
            self.render_selection(
                ui_canvas,
                canvas_handle,
                stream_handle,
                map,
                latest_pointer,
                current_pointer_pos,
            );
        } else {
            self.render_brush(
                ui_canvas,
                canvas_handle,
                stream_handle,
                map,
                current_pointer_pos,
            );
        }
    }
}
