use graphics::handles::{
    canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
};
use hiarc::Hiarc;
use map::types::NonZeroU16MinusOne;
use math::math::vector::{ubvec4, vec2};

use crate::{
    map::{EditorMap, EditorMapInterface},
    tools::utils::render_rect,
    utils::{ui_pos_to_world_pos, UiCanvasSize},
};

use super::shared::TILE_VISUAL_SIZE;

#[derive(Debug, Hiarc)]
pub struct TileSelectionRange {
    pub x: u16,
    pub y: u16,
    pub w: NonZeroU16MinusOne,
    pub h: NonZeroU16MinusOne,
}

#[derive(Debug, Hiarc)]
pub struct TileBrushDownPos {
    pub world: vec2,
    pub ui: egui::Pos2,
}

#[derive(Debug, Hiarc)]
pub struct TileSelection {
    pub range: Option<TileSelectionRange>,

    pub pointer_down_state: Option<TileBrushDownPos>,
}

impl Default for TileSelection {
    fn default() -> Self {
        Self::new()
    }
}

impl TileSelection {
    pub fn new() -> Self {
        Self {
            range: Default::default(),
            pointer_down_state: Default::default(),
        }
    }

    pub fn handle_range_select(
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
        // if pointer was already down
        if let Some(TileBrushDownPos { world, .. }) = &self.pointer_down_state {
            // find current layer
            if let Some(layer) = layer {
                let (layer_width, layer_height) = layer.get_width_and_height();

                let pointer_cur = vec2::new(current_pointer_pos.x, current_pointer_pos.y);

                let vec2 {
                    x: mut x0,
                    y: mut y0,
                } = world;
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

                if x0 > x1 {
                    std::mem::swap(&mut x0, &mut x1);
                }
                if y0 > y1 {
                    std::mem::swap(&mut y0, &mut y1);
                }

                let tile_visual_size = 1.0;
                let x0 = (x0 / tile_visual_size).floor() as i32;
                let y0 = (y0 / tile_visual_size).floor() as i32;
                let x1 = (x1 / tile_visual_size).ceil() as i32;
                let y1 = (y1 / tile_visual_size).ceil() as i32;

                let x0 = x0.clamp(0, layer_width.get() as i32) as u16;
                let y0 = y0.clamp(0, layer_height.get() as i32) as u16;
                let x1 = x1.clamp(0, layer_width.get() as i32) as u16;
                let y1 = y1.clamp(0, layer_height.get() as i32) as u16;

                let count_x = x1 - x0;
                let count_y = y1 - y0;

                // if there is an selection, apply that
                if count_x as usize * count_y as usize > 0 {
                    self.range = Some(TileSelectionRange {
                        x: x0,
                        y: y0,
                        w: NonZeroU16MinusOne::new(count_x).unwrap(),
                        h: NonZeroU16MinusOne::new(count_y).unwrap(),
                    });
                }
            }

            if !latest_pointer.primary_down() {
                self.pointer_down_state = None;
            }
        } else {
            // else check if the pointer is down now
            if latest_pointer.primary_down() {
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
                self.pointer_down_state = Some(TileBrushDownPos {
                    world: pos,
                    ui: *current_pointer_pos,
                });
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
        if let Some(TileBrushDownPos { world, .. }) = &self.pointer_down_state {
            let pos = current_pointer_pos;
            if latest_pointer.primary_down() {
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

                let down_pos = world;

                let min_pos = vec2::new(pos.x.min(down_pos.x), pos.y.min(down_pos.y));
                let max_pos = vec2::new(pos.x.max(down_pos.x), pos.y.max(down_pos.y));

                let min_pos = vec2::new(
                    (min_pos.x / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE,
                    (min_pos.y / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE,
                );
                let max_pos = vec2::new(
                    (max_pos.x / TILE_VISUAL_SIZE).ceil() * TILE_VISUAL_SIZE,
                    (max_pos.y / TILE_VISUAL_SIZE).ceil() * TILE_VISUAL_SIZE,
                );

                let min_pos = egui::pos2(min_pos.x, min_pos.y);
                let max_pos = egui::pos2(max_pos.x, max_pos.y);

                let rect = egui::Rect::from_min_max(min_pos, max_pos);

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

    fn render_range(
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

        let range = self.range.as_ref().unwrap();

        let brush_pos = vec2::new(range.x as f32, range.y as f32);
        let brush_size = vec2::new(range.w.get() as f32, range.h.get() as f32) * 1.0;
        let rect = egui::Rect::from_min_max(
            egui::pos2(brush_pos.x, brush_pos.y),
            egui::pos2(brush_pos.x + brush_size.x, brush_pos.y + brush_size.y),
        );

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
        canvas_handle: &GraphicsCanvasHandle,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
    ) {
        let layer = map.active_layer();
        if !layer.as_ref().is_some_and(|layer| layer.is_tile_layer()) {
            return;
        }

        if self.range.is_none() || self.pointer_down_state.is_some() {
            self.handle_range_select(
                ui_canvas,
                canvas_handle,
                map,
                latest_pointer,
                current_pointer_pos,
            );
        } else {
            // reset selection
            if latest_pointer.button_down(egui::PointerButton::Secondary) {
                self.range = None;
            }
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
        if self.range.is_none() || self.pointer_down_state.is_some() {
            self.render_selection(
                ui_canvas,
                canvas_handle,
                stream_handle,
                map,
                latest_pointer,
                current_pointer_pos,
            );
        } else {
            self.render_range(canvas_handle, stream_handle, map);
        }
    }
}
