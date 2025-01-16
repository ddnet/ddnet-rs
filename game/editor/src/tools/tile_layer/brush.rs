use std::{collections::HashSet, sync::Arc};

use client_containers::{container::ContainerKey, entities::EntitiesContainer};
use client_render_base::map::{
    map_buffered::{
        ClientMapBuffered, PhysicsTileLayerVisuals, TileLayerBufferedVisuals, TileLayerVisuals,
    },
    map_pipeline::{MapGraphics, TileLayerDrawInfo},
    render_tools::{CanvasType, RenderTools},
};
use egui::{pos2, Rect};
use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::{
        backend::backend::GraphicsBackendHandle,
        buffer_object::buffer_object::GraphicsBufferObjectHandle,
        canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
        texture::texture::TextureContainer2dArray,
    },
    utils::{render_blur, render_swapped_frame, DEFAULT_BLUR_MIX_LENGTH, DEFAULT_BLUR_RADIUS},
};
use graphics_types::rendering::{ColorRgba, State};
use hiarc::Hiarc;
use map::{
    map::groups::{
        layers::tiles::{
            MapTileLayerPhysicsTiles, MapTileLayerTiles, SpeedupTile, SwitchTile, TeleTile, Tile,
            TileBase, TileFlags, TuneTile,
        },
        MapGroupAttr,
    },
    types::NonZeroU16MinusOne,
};
use math::math::vector::{dvec2, ivec2, ubvec4, usvec2, vec2, vec4};
use pool::mt_datatypes::PoolVec;

use crate::{
    actions::actions::{
        ActTileLayerReplTilesBase, ActTileLayerReplaceTiles, ActTilePhysicsLayerReplTilesBase,
        ActTilePhysicsLayerReplaceTiles, EditorAction,
    },
    client::EditorClient,
    map::{EditorLayer, EditorLayerUnionRef, EditorMap, EditorMapInterface, EditorPhysicsLayer},
    map_tools::{
        finish_design_tile_layer_buffer, finish_physics_layer_buffer,
        upload_design_tile_layer_buffer, upload_physics_layer_buffer,
    },
    tools::utils::{
        render_checkerboard_background, render_filled_rect, render_filled_rect_from_state,
        render_rect, render_rect_from_state,
    },
    utils::{ui_pos_to_world_pos, UiCanvasSize},
};

use super::shared::TILE_VISUAL_SIZE;

// 20 ui pixels
const TILE_PICKER_VISUAL_SIZE: f32 = 30.0;

#[derive(Debug, Hiarc)]
pub enum BrushVisual {
    Design(TileLayerVisuals),
    Physics(PhysicsTileLayerVisuals),
}

#[derive(Debug, Hiarc)]
pub struct TileBrushTiles {
    pub tiles: MapTileLayerTiles,
    pub w: NonZeroU16MinusOne,
    pub h: NonZeroU16MinusOne,

    pub negative_offset: usvec2,
    pub negative_offsetf: dvec2,

    pub render: BrushVisual,
    pub map_render: MapGraphics,
    pub texture: TextureContainer2dArray,
}

#[derive(Debug, Hiarc)]
pub struct TileBrushTilePicker {
    pub render: TileLayerVisuals,
    pub map_render: MapGraphics,
}

impl TileBrushTilePicker {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        backend_handle: &GraphicsBackendHandle,
    ) -> Self {
        let map_render = MapGraphics::new(backend_handle);

        Self {
            render: ClientMapBuffered::tile_set_preview(
                graphics_mt,
                buffer_object_handle,
                backend_handle,
            ),
            map_render,
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct TileBrushDownPos {
    pub world: vec2,
    pub ui: egui::Pos2,
}

#[derive(Debug, Hiarc)]
pub struct TileBrush {
    pub brush: Option<TileBrushTiles>,

    pub tile_picker: TileBrushTilePicker,

    pub pointer_down_world_pos: Option<TileBrushDownPos>,
    pub shift_pointer_down_world_pos: Option<TileBrushDownPos>,
}

impl TileBrush {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        backend_handle: &GraphicsBackendHandle,
    ) -> Self {
        Self {
            brush: None,

            tile_picker: TileBrushTilePicker::new(
                graphics_mt,
                buffer_object_handle,
                backend_handle,
            ),

            pointer_down_world_pos: None,
            shift_pointer_down_world_pos: None,
        }
    }

    fn collect_tiles<T: Copy>(
        tiles: &[T],
        width: usize,
        x: usize,
        copy_width: usize,
        y: usize,
        copy_height: usize,
    ) -> Vec<T> {
        tiles
            .chunks_exact(width)
            .skip(y)
            .take(copy_height)
            .flat_map(|tiles| tiles[x..x + copy_width].to_vec())
            .collect()
    }

    pub fn create_brush_visual(
        tp: &Arc<rayon::ThreadPool>,
        graphics_mt: &GraphicsMultiThreaded,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        backend_handle: &GraphicsBackendHandle,
        w: NonZeroU16MinusOne,
        h: NonZeroU16MinusOne,
        tiles: &MapTileLayerTiles,
    ) -> BrushVisual {
        match &tiles {
            MapTileLayerTiles::Design(tiles) => BrushVisual::Design({
                let has_texture = true;
                let buffer = tp.install(|| {
                    upload_design_tile_layer_buffer(graphics_mt, tiles, w, h, has_texture)
                });
                finish_design_tile_layer_buffer(buffer_object_handle, backend_handle, buffer)
            }),
            MapTileLayerTiles::Physics(tiles) => BrushVisual::Physics({
                let buffer =
                    tp.install(|| upload_physics_layer_buffer(graphics_mt, w, h, tiles.as_ref()));
                finish_physics_layer_buffer(buffer_object_handle, backend_handle, buffer)
            }),
        }
    }

    fn selected_tiles_picker(pointer_rect: Rect, render_rect: Rect) -> (Vec<u8>, usize, usize) {
        let mut brush_width = 0;
        let mut brush_height = 0;
        let mut tile_indices: Vec<u8> = Default::default();
        // handle pointer position inside the available rect
        if pointer_rect.intersects(render_rect) {
            // determine tile
            let size_of_tile = render_rect.width() / 16.0;
            let x0 = pointer_rect.min.x.max(render_rect.min.x) - render_rect.min.x;
            let y0 = pointer_rect.min.y.max(render_rect.min.y) - render_rect.min.y;
            let mut x1 = pointer_rect.max.x.min(render_rect.max.x) - render_rect.min.x;
            let mut y1 = pointer_rect.max.y.min(render_rect.max.y) - render_rect.min.y;

            let x0 = (x0 / size_of_tile).rem_euclid(16.0) as usize;
            let y0 = (y0 / size_of_tile).rem_euclid(16.0) as usize;
            // edge cases (next_down not stabilized in rust)
            if (x1 - render_rect.max.x) < 0.1 {
                x1 -= 0.1
            }
            if (y1 - render_rect.max.y) < 0.1 {
                y1 -= 0.1
            }
            let x1 = (x1 / size_of_tile).rem_euclid(16.0) as usize;
            let y1 = (y1 / size_of_tile).rem_euclid(16.0) as usize;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let tile_index = (x + y * 16) as u8;
                    tile_indices.push(tile_index)
                }
            }

            brush_width = (x1 + 1) - x0;
            brush_height = (y1 + 1) - y0;
        }

        (tile_indices, brush_width, brush_height)
    }

    fn tile_picker_rect(available_rect: &egui::Rect) -> egui::Rect {
        let size = available_rect.width().min(available_rect.height());
        let x_mid = available_rect.min.x + available_rect.width() / 2.0;
        let y_mid = available_rect.min.y + available_rect.height() / 2.0;
        let size = size.min(TILE_VISUAL_SIZE * TILE_PICKER_VISUAL_SIZE * 16.0);

        egui::Rect::from_min_size(
            pos2(x_mid - size / 2.0, y_mid - size / 2.0),
            egui::vec2(size, size),
        )
    }

    pub fn handle_brush_select(
        &mut self,
        ui_canvas: &UiCanvasSize,
        tp: &Arc<rayon::ThreadPool>,
        graphics_mt: &GraphicsMultiThreaded,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        backend_handle: &GraphicsBackendHandle,
        canvas_handle: &GraphicsCanvasHandle,
        entities_container: &mut EntitiesContainer,
        fake_texture_2d_array: &TextureContainer2dArray,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        latest_keys_down: &HashSet<egui::Key>,
        current_pointer_pos: &egui::Pos2,
        available_rect: &egui::Rect,
    ) {
        let layer = map.active_layer();
        let (offset, parallax) = if let Some(layer) = &layer {
            layer.get_offset_and_parallax()
        } else {
            Default::default()
        };
        // if pointer was already down
        if let Some(TileBrushDownPos { world, ui }) = &self.pointer_down_world_pos {
            // find current layer
            if let Some(layer) = layer {
                // if space is hold down, pick from a tile selector
                if latest_keys_down.contains(&egui::Key::Space) {
                    let pointer_down = pos2(ui.x, ui.y);
                    let pointer_rect = egui::Rect::from_min_max(
                        current_pointer_pos.min(pointer_down),
                        current_pointer_pos.max(pointer_down),
                    );
                    let render_rect = Self::tile_picker_rect(available_rect);

                    let (tile_indices, brush_width, brush_height) =
                        Self::selected_tiles_picker(pointer_rect, render_rect);

                    if !tile_indices.is_empty() {
                        let physics_group_editor = &map.groups.physics.user;
                        let (tiles, texture) = match layer {
                            EditorLayerUnionRef::Physics { layer, .. } => (
                                MapTileLayerTiles::Physics(match layer {
                                    EditorPhysicsLayer::Arbitrary(_) => {
                                        panic!("not supported")
                                    }
                                    EditorPhysicsLayer::Game(_) => MapTileLayerPhysicsTiles::Game(
                                        tile_indices
                                            .into_iter()
                                            .map(|index| Tile {
                                                index,
                                                flags: TileFlags::empty(),
                                            })
                                            .collect(),
                                    ),
                                    EditorPhysicsLayer::Front(_) => {
                                        MapTileLayerPhysicsTiles::Front(
                                            tile_indices
                                                .into_iter()
                                                .map(|index| Tile {
                                                    index,
                                                    flags: TileFlags::empty(),
                                                })
                                                .collect(),
                                        )
                                    }
                                    EditorPhysicsLayer::Tele(_) => MapTileLayerPhysicsTiles::Tele(
                                        tile_indices
                                            .into_iter()
                                            .map(|index| TeleTile {
                                                base: TileBase {
                                                    index,
                                                    flags: TileFlags::empty(),
                                                },
                                                number: physics_group_editor.active_tele,
                                            })
                                            .collect(),
                                    ),
                                    EditorPhysicsLayer::Speedup(_) => {
                                        MapTileLayerPhysicsTiles::Speedup(
                                            tile_indices
                                                .into_iter()
                                                .map(|index| SpeedupTile {
                                                    base: TileBase {
                                                        index,
                                                        flags: TileFlags::empty(),
                                                    },
                                                    ..Default::default()
                                                })
                                                .collect(),
                                        )
                                    }
                                    EditorPhysicsLayer::Switch(_) => {
                                        MapTileLayerPhysicsTiles::Switch(
                                            tile_indices
                                                .into_iter()
                                                .map(|index| SwitchTile {
                                                    base: TileBase {
                                                        index,
                                                        flags: TileFlags::empty(),
                                                    },
                                                    number: physics_group_editor.active_switch,
                                                    ..Default::default()
                                                })
                                                .collect(),
                                        )
                                    }
                                    EditorPhysicsLayer::Tune(_) => MapTileLayerPhysicsTiles::Tune(
                                        tile_indices
                                            .into_iter()
                                            .map(|index| TuneTile {
                                                base: TileBase {
                                                    index,
                                                    flags: TileFlags::empty(),
                                                },
                                                number: physics_group_editor.active_tune_zone,
                                            })
                                            .collect(),
                                    ),
                                }),
                                entities_container
                                    .get_or_default::<ContainerKey>(&"default".try_into().unwrap())
                                    // TODO:
                                    .get_or_default("ddnet")
                                    .clone(),
                            ),
                            EditorLayerUnionRef::Design { layer, .. } => {
                                let EditorLayer::Tile(layer) = layer else {
                                    panic!(
                                    "this cannot happen, it was previously checked if tile layer"
                                )
                                };
                                (
                                    MapTileLayerTiles::Design(
                                        tile_indices
                                            .into_iter()
                                            .map(|index| Tile {
                                                index,
                                                flags: TileFlags::empty(),
                                            })
                                            .collect(),
                                    ),
                                    layer
                                        .layer
                                        .attr
                                        .image_array
                                        .as_ref()
                                        .map(|&image| {
                                            map.resources.image_arrays[image].user.user.clone()
                                        })
                                        .unwrap_or_else(|| fake_texture_2d_array.clone()),
                                )
                            }
                        };

                        let w = NonZeroU16MinusOne::new(brush_width as u16).unwrap();
                        let h = NonZeroU16MinusOne::new(brush_height as u16).unwrap();
                        let render = match &tiles {
                            MapTileLayerTiles::Design(tiles) => BrushVisual::Design({
                                let has_texture = true;
                                let buffer = tp.install(|| {
                                    upload_design_tile_layer_buffer(
                                        graphics_mt,
                                        tiles,
                                        w,
                                        h,
                                        has_texture,
                                    )
                                });
                                finish_design_tile_layer_buffer(
                                    buffer_object_handle,
                                    backend_handle,
                                    buffer,
                                )
                            }),
                            MapTileLayerTiles::Physics(tiles) => BrushVisual::Physics({
                                let buffer = tp.install(|| {
                                    upload_physics_layer_buffer(graphics_mt, w, h, tiles.as_ref())
                                });
                                finish_physics_layer_buffer(
                                    buffer_object_handle,
                                    backend_handle,
                                    buffer,
                                )
                            }),
                        };

                        self.brush = Some(TileBrushTiles {
                            tiles,
                            w,
                            h,
                            negative_offset: usvec2::new(0, 0),
                            negative_offsetf: dvec2::new(0.0, 0.0),
                            render,
                            map_render: MapGraphics::new(backend_handle),
                            texture,
                        });
                    }
                }
                // else select from existing tiles
                else {
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
                    );

                    let x_needs_offset = x0 < x1;
                    let y_needs_offset = y0 < y1;

                    if x0 > x1 {
                        std::mem::swap(&mut x0, &mut x1);
                    }
                    if y0 > y1 {
                        std::mem::swap(&mut y0, &mut y1);
                    }

                    let x0 = (x0 / TILE_VISUAL_SIZE).floor() as i32;
                    let y0 = (y0 / TILE_VISUAL_SIZE).floor() as i32;
                    let x1 = (x1 / TILE_VISUAL_SIZE).ceil() as i32;
                    let y1 = (y1 / TILE_VISUAL_SIZE).ceil() as i32;

                    let x0 = x0.clamp(0, layer_width.get() as i32) as u16;
                    let y0 = y0.clamp(0, layer_height.get() as i32) as u16;
                    let x1 = x1.clamp(0, layer_width.get() as i32) as u16;
                    let y1 = y1.clamp(0, layer_height.get() as i32) as u16;

                    let count_x = x1 - x0;
                    let count_y = y1 - y0;

                    // if there is an selection, apply that
                    if count_x as usize * count_y as usize > 0 {
                        let (tiles, texture) = match layer {
                            EditorLayerUnionRef::Physics { layer, .. } => (
                                MapTileLayerTiles::Physics(match layer {
                                    EditorPhysicsLayer::Arbitrary(_) => {
                                        panic!("not supported")
                                    }
                                    EditorPhysicsLayer::Game(layer) => {
                                        MapTileLayerPhysicsTiles::Game(Self::collect_tiles(
                                            &layer.layer.tiles,
                                            layer_width.get() as usize,
                                            x0 as usize,
                                            count_x as usize,
                                            y0 as usize,
                                            count_y as usize,
                                        ))
                                    }
                                    EditorPhysicsLayer::Front(layer) => {
                                        MapTileLayerPhysicsTiles::Front(Self::collect_tiles(
                                            &layer.layer.tiles,
                                            layer_width.get() as usize,
                                            x0 as usize,
                                            count_x as usize,
                                            y0 as usize,
                                            count_y as usize,
                                        ))
                                    }
                                    EditorPhysicsLayer::Tele(layer) => {
                                        MapTileLayerPhysicsTiles::Tele(Self::collect_tiles(
                                            &layer.layer.base.tiles,
                                            layer_width.get() as usize,
                                            x0 as usize,
                                            count_x as usize,
                                            y0 as usize,
                                            count_y as usize,
                                        ))
                                    }
                                    EditorPhysicsLayer::Speedup(layer) => {
                                        MapTileLayerPhysicsTiles::Speedup(Self::collect_tiles(
                                            &layer.layer.tiles,
                                            layer_width.get() as usize,
                                            x0 as usize,
                                            count_x as usize,
                                            y0 as usize,
                                            count_y as usize,
                                        ))
                                    }
                                    EditorPhysicsLayer::Switch(layer) => {
                                        MapTileLayerPhysicsTiles::Switch(Self::collect_tiles(
                                            &layer.layer.base.tiles,
                                            layer_width.get() as usize,
                                            x0 as usize,
                                            count_x as usize,
                                            y0 as usize,
                                            count_y as usize,
                                        ))
                                    }
                                    EditorPhysicsLayer::Tune(layer) => {
                                        MapTileLayerPhysicsTiles::Tune(Self::collect_tiles(
                                            &layer.layer.base.tiles,
                                            layer_width.get() as usize,
                                            x0 as usize,
                                            count_x as usize,
                                            y0 as usize,
                                            count_y as usize,
                                        ))
                                    }
                                }),
                                entities_container
                                    .get_or_default::<ContainerKey>(&"default".try_into().unwrap())
                                    // TODO:
                                    .get_or_default("ddnet")
                                    .clone(),
                            ),
                            EditorLayerUnionRef::Design { layer, .. } => {
                                let EditorLayer::Tile(layer) = layer else {
                                    panic!(
                                    "this cannot happen, it was previously checked if tile layer"
                                )
                                };
                                (
                                    MapTileLayerTiles::Design(Self::collect_tiles(
                                        &layer.layer.tiles,
                                        layer_width.get() as usize,
                                        x0 as usize,
                                        count_x as usize,
                                        y0 as usize,
                                        count_y as usize,
                                    )),
                                    layer
                                        .layer
                                        .attr
                                        .image_array
                                        .as_ref()
                                        .map(|&image| {
                                            map.resources.image_arrays[image].user.user.clone()
                                        })
                                        .unwrap_or_else(|| fake_texture_2d_array.clone()),
                                )
                            }
                        };

                        let w = NonZeroU16MinusOne::new(count_x).unwrap();
                        let h = NonZeroU16MinusOne::new(count_y).unwrap();
                        let render = Self::create_brush_visual(
                            tp,
                            graphics_mt,
                            buffer_object_handle,
                            backend_handle,
                            w,
                            h,
                            &tiles,
                        );

                        self.brush = Some(TileBrushTiles {
                            tiles,
                            w,
                            h,
                            negative_offset: usvec2::new(
                                x_needs_offset.then_some(count_x - 1).unwrap_or_default(),
                                y_needs_offset.then_some(count_y - 1).unwrap_or_default(),
                            ),
                            negative_offsetf: dvec2::new(
                                x_needs_offset.then_some(count_x as f64).unwrap_or_default(),
                                y_needs_offset.then_some(count_y as f64).unwrap_or_default(),
                            ),
                            render,
                            map_render: MapGraphics::new(backend_handle),
                            texture,
                        });
                    } else {
                        self.brush = None;
                    }
                }
            }

            if !latest_pointer.primary_down() {
                self.pointer_down_world_pos = None;
            }
        } else {
            // else check if the pointer is down now
            if latest_pointer.primary_pressed() {
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
                self.pointer_down_world_pos = Some(TileBrushDownPos {
                    world: pos,
                    ui: *current_pointer_pos,
                });
            }
        }
    }

    fn apply_brush_internal(
        layer: &EditorLayerUnionRef<'_>,
        brush: &TileBrushTiles,
        client: &mut EditorClient,
        x: i32,
        y: i32,
        brush_off_x: u16,
        brush_off_y: u16,
        max_brush_w: u16,
        max_brush_h: u16,
    ) {
        let (layer_width, layer_height) = layer.get_width_and_height();

        let mut brush_x = brush_off_x;
        let mut brush_y = brush_off_y;
        let mut brush_w = brush.w.get().min(max_brush_w);
        let mut brush_h = brush.h.get().min(max_brush_h);
        if x < 0 {
            let diff = x.abs().min(brush_w as i32) as u16;
            brush_w -= diff;
            brush_x += diff;
        }
        if y < 0 {
            let diff = y.abs().min(brush_h as i32) as u16;
            brush_h -= diff;
            brush_y += diff;
        }

        let x = x.clamp(0, layer_width.get() as i32 - 1) as u16;
        let y = y.clamp(0, layer_height.get() as i32 - 1) as u16;

        if x as i32 + brush_w as i32 >= layer_width.get() as i32 {
            brush_w -= ((x as i32 + brush_w as i32) - layer_width.get() as i32) as u16;
        }
        if y as i32 + brush_h as i32 >= layer_height.get() as i32 {
            brush_h -= ((y as i32 + brush_h as i32) - layer_height.get() as i32) as u16;
        }

        let brush_matches_layer = match layer {
            EditorLayerUnionRef::Physics { layer, .. } => match layer {
                EditorPhysicsLayer::Arbitrary(_) => matches!(
                    brush.tiles,
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Arbitrary(_))
                ),
                EditorPhysicsLayer::Game(_) => {
                    matches!(
                        brush.tiles,
                        MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Game(_))
                    )
                }
                EditorPhysicsLayer::Front(_) => matches!(
                    brush.tiles,
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Front(_))
                ),
                EditorPhysicsLayer::Tele(_) => matches!(
                    brush.tiles,
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tele(_))
                ),
                EditorPhysicsLayer::Speedup(_) => matches!(
                    brush.tiles,
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Speedup(_))
                ),
                EditorPhysicsLayer::Switch(_) => matches!(
                    brush.tiles,
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Switch(_))
                ),
                EditorPhysicsLayer::Tune(_) => matches!(
                    brush.tiles,
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tune(_))
                ),
            },
            EditorLayerUnionRef::Design { .. } => {
                matches!(brush.tiles, MapTileLayerTiles::Design(_))
            }
        };
        if brush_w > 0 && brush_h > 0 && brush_matches_layer {
            let (action, group_indentifier) = match layer {
                EditorLayerUnionRef::Physics {
                    layer,
                    group_attr,
                    layer_index,
                } => (
                    EditorAction::TilePhysicsLayerReplaceTiles(ActTilePhysicsLayerReplaceTiles {
                        base: ActTilePhysicsLayerReplTilesBase {
                            layer_index: *layer_index,
                            old_tiles: match layer {
                                EditorPhysicsLayer::Arbitrary(_) => {
                                    panic!("not implemented for arbitrary layer")
                                }
                                EditorPhysicsLayer::Game(layer) => {
                                    MapTileLayerPhysicsTiles::Game(Self::collect_tiles(
                                        &layer.layer.tiles,
                                        group_attr.width.get() as usize,
                                        x as usize,
                                        brush_w as usize,
                                        y as usize,
                                        brush_h as usize,
                                    ))
                                }
                                EditorPhysicsLayer::Front(layer) => {
                                    MapTileLayerPhysicsTiles::Front(Self::collect_tiles(
                                        &layer.layer.tiles,
                                        group_attr.width.get() as usize,
                                        x as usize,
                                        brush_w as usize,
                                        y as usize,
                                        brush_h as usize,
                                    ))
                                }
                                EditorPhysicsLayer::Tele(layer) => {
                                    MapTileLayerPhysicsTiles::Tele(Self::collect_tiles(
                                        &layer.layer.base.tiles,
                                        group_attr.width.get() as usize,
                                        x as usize,
                                        brush_w as usize,
                                        y as usize,
                                        brush_h as usize,
                                    ))
                                }
                                EditorPhysicsLayer::Speedup(layer) => {
                                    MapTileLayerPhysicsTiles::Speedup(Self::collect_tiles(
                                        &layer.layer.tiles,
                                        group_attr.width.get() as usize,
                                        x as usize,
                                        brush_w as usize,
                                        y as usize,
                                        brush_h as usize,
                                    ))
                                }
                                EditorPhysicsLayer::Switch(layer) => {
                                    MapTileLayerPhysicsTiles::Switch(Self::collect_tiles(
                                        &layer.layer.base.tiles,
                                        group_attr.width.get() as usize,
                                        x as usize,
                                        brush_w as usize,
                                        y as usize,
                                        brush_h as usize,
                                    ))
                                }
                                EditorPhysicsLayer::Tune(layer) => {
                                    MapTileLayerPhysicsTiles::Tune(Self::collect_tiles(
                                        &layer.layer.base.tiles,
                                        group_attr.width.get() as usize,
                                        x as usize,
                                        brush_w as usize,
                                        y as usize,
                                        brush_h as usize,
                                    ))
                                }
                            },
                            new_tiles: match &brush.tiles {
                                MapTileLayerTiles::Design(_) => todo!(
                                    "currently design tiles can't be pasted on a physics layer"
                                ),
                                MapTileLayerTiles::Physics(tiles) => match tiles {
                                    MapTileLayerPhysicsTiles::Arbitrary(_) => {
                                        panic!("this operation is not supported")
                                    }
                                    MapTileLayerPhysicsTiles::Game(tiles) => {
                                        MapTileLayerPhysicsTiles::Game(Self::collect_tiles(
                                            tiles,
                                            brush.w.get() as usize,
                                            brush_x as usize,
                                            brush_w as usize,
                                            brush_y as usize,
                                            brush_h as usize,
                                        ))
                                    }
                                    MapTileLayerPhysicsTiles::Front(tiles) => {
                                        MapTileLayerPhysicsTiles::Front(Self::collect_tiles(
                                            tiles,
                                            brush.w.get() as usize,
                                            brush_x as usize,
                                            brush_w as usize,
                                            brush_y as usize,
                                            brush_h as usize,
                                        ))
                                    }
                                    MapTileLayerPhysicsTiles::Tele(tiles) => {
                                        MapTileLayerPhysicsTiles::Tele(Self::collect_tiles(
                                            tiles,
                                            brush.w.get() as usize,
                                            brush_x as usize,
                                            brush_w as usize,
                                            brush_y as usize,
                                            brush_h as usize,
                                        ))
                                    }
                                    MapTileLayerPhysicsTiles::Speedup(tiles) => {
                                        MapTileLayerPhysicsTiles::Speedup(Self::collect_tiles(
                                            tiles,
                                            brush.w.get() as usize,
                                            brush_x as usize,
                                            brush_w as usize,
                                            brush_y as usize,
                                            brush_h as usize,
                                        ))
                                    }
                                    MapTileLayerPhysicsTiles::Switch(tiles) => {
                                        MapTileLayerPhysicsTiles::Switch(Self::collect_tiles(
                                            tiles,
                                            brush.w.get() as usize,
                                            brush_x as usize,
                                            brush_w as usize,
                                            brush_y as usize,
                                            brush_h as usize,
                                        ))
                                    }
                                    MapTileLayerPhysicsTiles::Tune(tiles) => {
                                        MapTileLayerPhysicsTiles::Tune(Self::collect_tiles(
                                            tiles,
                                            brush.w.get() as usize,
                                            brush_x as usize,
                                            brush_w as usize,
                                            brush_y as usize,
                                            brush_h as usize,
                                        ))
                                    }
                                },
                            },
                            x,
                            y,
                            w: NonZeroU16MinusOne::new(brush_w).unwrap(),
                            h: NonZeroU16MinusOne::new(brush_h).unwrap(),
                        },
                    }),
                    format!("tile-brush phy {}", layer_index),
                ),
                EditorLayerUnionRef::Design {
                    layer,
                    layer_index,
                    group_index,
                    is_background,
                    ..
                } => {
                    let EditorLayer::Tile(layer) = layer else {
                        panic!("not a tile layer")
                    };
                    (
                        EditorAction::TileLayerReplaceTiles(ActTileLayerReplaceTiles {
                            base: ActTileLayerReplTilesBase {
                                is_background: *is_background,
                                group_index: *group_index,
                                layer_index: *layer_index,
                                old_tiles: Self::collect_tiles(
                                    &layer.layer.tiles,
                                    layer.layer.attr.width.get() as usize,
                                    x as usize,
                                    brush_w as usize,
                                    y as usize,
                                    brush_h as usize,
                                ),
                                new_tiles: match &brush.tiles {
                                    MapTileLayerTiles::Design(tiles) => Self::collect_tiles(
                                        tiles,
                                        brush.w.get() as usize,
                                        brush_x as usize,
                                        brush_w as usize,
                                        brush_y as usize,
                                        brush_h as usize,
                                    ),
                                    MapTileLayerTiles::Physics(tiles) => match tiles {
                                        MapTileLayerPhysicsTiles::Arbitrary(_) => {
                                            panic!("this operation is not supported")
                                        }
                                        MapTileLayerPhysicsTiles::Game(tiles) => {
                                            Self::collect_tiles(
                                                tiles,
                                                brush.w.get() as usize,
                                                brush_x as usize,
                                                brush_w as usize,
                                                brush_y as usize,
                                                brush_h as usize,
                                            )
                                        }
                                        MapTileLayerPhysicsTiles::Front(tiles) => {
                                            Self::collect_tiles(
                                                tiles,
                                                brush.w.get() as usize,
                                                brush_x as usize,
                                                brush_w as usize,
                                                brush_y as usize,
                                                brush_h as usize,
                                            )
                                        }
                                        MapTileLayerPhysicsTiles::Tele(_) => todo!(),
                                        MapTileLayerPhysicsTiles::Speedup(_) => todo!(),
                                        MapTileLayerPhysicsTiles::Switch(_) => todo!(),
                                        MapTileLayerPhysicsTiles::Tune(_) => todo!(),
                                    },
                                },
                                x,
                                y,
                                w: NonZeroU16MinusOne::new(brush_w).unwrap(),
                                h: NonZeroU16MinusOne::new(brush_h).unwrap(),
                            },
                        }),
                        format!(
                            "tile-brush {}-{}-{}",
                            group_index, layer_index, is_background
                        ),
                    )
                }
            };
            client.execute(action, Some(&group_indentifier));
        }
    }

    fn apply_brush_repeating_internal(
        &self,
        brush: &TileBrushTiles,
        layer: EditorLayerUnionRef<'_>,
        center: ivec2,
        mut tile_offset: usvec2,
        width: NonZeroU16MinusOne,
        height: NonZeroU16MinusOne,
        client: &mut EditorClient,
    ) {
        if matches!(
            brush.render,
            BrushVisual::Design(TileLayerVisuals { .. })
                | BrushVisual::Physics(PhysicsTileLayerVisuals { .. })
        ) {
            let mut off_y = 0;
            let mut height = height.get();

            while height > 0 {
                let brush_h = (brush.h.get() - tile_offset.y).min(height);

                for y in 0..brush_h {
                    let mut off_x = 0;
                    let mut width = width.get();
                    let brush_y = tile_offset.y + y;
                    let mut tile_offset_x = tile_offset.x;
                    while width > 0 {
                        let brush_x = tile_offset_x;
                        let brush_w = (brush.w.get() - tile_offset_x).min(width);

                        Self::apply_brush_internal(
                            &layer,
                            brush,
                            client,
                            center.x + off_x,
                            center.y + off_y + y as i32,
                            brush_x,
                            brush_y,
                            brush_w,
                            1,
                        );

                        width -= brush_w;
                        tile_offset_x = 0;
                        off_x += brush_w as i32;
                    }
                }

                height -= brush_h;
                tile_offset.y = 0;
                off_y += brush_h as i32;
            }
        }
    }

    pub fn handle_brush_draw(
        &mut self,
        ui_canvas: &UiCanvasSize,
        canvas_handle: &GraphicsCanvasHandle,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        latest_modifiers: &egui::Modifiers,
        current_pointer_pos: &egui::Pos2,
        client: &mut EditorClient,
    ) {
        let layer = map.active_layer().unwrap();
        let (offset, parallax) = layer.get_offset_and_parallax();

        // reset brush
        if latest_pointer.secondary_pressed() {
            self.brush = None;
            self.shift_pointer_down_world_pos = None;
        } else if (latest_modifiers.shift
            && (!latest_pointer.primary_down() || latest_pointer.primary_pressed()))
            || self.shift_pointer_down_world_pos.is_some()
        {
            let brush = self.brush.as_ref().unwrap();

            if let Some(TileBrushDownPos { world, .. }) = &self.shift_pointer_down_world_pos {
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

                let pos_old = ivec2::new(
                    ((world.x / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE) as i32,
                    ((world.y / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE) as i32,
                );
                let pos_cur = ivec2::new(
                    ((pointer_cur.x / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE) as i32,
                    ((pointer_cur.y / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE) as i32,
                );
                let width = (pos_cur.x - pos_old.x).unsigned_abs() as u16 + 1;
                let height = (pos_cur.y - pos_old.y).unsigned_abs() as u16 + 1;
                let pos_min = ivec2::new(pos_cur.x.min(pos_old.x), pos_cur.y.min(pos_old.y));

                if !latest_pointer.primary_down() {
                    self.apply_brush_repeating_internal(
                        brush,
                        layer,
                        pos_min,
                        usvec2::new(
                            (pos_cur.x - pos_old.x)
                                .clamp(i32::MIN, 0)
                                .rem_euclid(brush.w.get() as i32)
                                as u16,
                            (pos_cur.y - pos_old.y)
                                .clamp(i32::MIN, 0)
                                .rem_euclid(brush.h.get() as i32)
                                as u16,
                        ),
                        NonZeroU16MinusOne::new(width).unwrap(),
                        NonZeroU16MinusOne::new(height).unwrap(),
                        client,
                    );
                    self.shift_pointer_down_world_pos = None;
                }
            } else if latest_pointer.primary_pressed() {
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
                self.shift_pointer_down_world_pos = Some(TileBrushDownPos {
                    world: pos,
                    ui: *current_pointer_pos,
                });
            }
        }
        // apply brush
        else {
            let brush = self.brush.as_ref().unwrap();

            if latest_pointer.primary_down() {
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

                let x = (x / TILE_VISUAL_SIZE).floor() as i32;
                let y = (y / TILE_VISUAL_SIZE).floor() as i32;

                let x = x - brush.negative_offset.x as i32;
                let y = y - brush.negative_offset.y as i32;

                Self::apply_brush_internal(
                    &layer,
                    brush,
                    client,
                    x,
                    y,
                    0,
                    0,
                    brush.w.get(),
                    brush.h.get(),
                );
            }
        }
    }

    fn render_selection(
        &self,
        ui_canvas: &UiCanvasSize,
        backend_handle: &GraphicsBackendHandle,
        canvas_handle: &GraphicsCanvasHandle,
        stream_handle: &GraphicsStreamHandle,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
    ) {
        // if pointer was already down
        if let Some(TileBrushDownPos { .. }) = &self.pointer_down_world_pos {
            if latest_pointer.primary_down() && self.brush.is_some() {
                self.render_brush(
                    ui_canvas,
                    backend_handle,
                    canvas_handle,
                    stream_handle,
                    map,
                    current_pointer_pos,
                    true,
                );
            }
        }
    }

    fn render_brush_repeating_internal(
        &self,
        brush: &TileBrushTiles,
        map: &EditorMap,
        canvas_handle: &GraphicsCanvasHandle,
        center: vec2,
        group_attr: Option<MapGroupAttr>,
        mut tile_offset: usvec2,
        width: NonZeroU16MinusOne,
        height: NonZeroU16MinusOne,
    ) {
        if let BrushVisual::Design(TileLayerVisuals {
            base:
                TileLayerBufferedVisuals {
                    base,
                    buffer_object: Some(buffer_object),
                },
            ..
        })
        | BrushVisual::Physics(PhysicsTileLayerVisuals {
            base:
                TileLayerVisuals {
                    base:
                        TileLayerBufferedVisuals {
                            base,
                            buffer_object: Some(buffer_object),
                        },
                    ..
                },
            ..
        }) = &brush.render
        {
            let mut off_y = 0.0;
            let mut height = height.get();

            while height > 0 {
                let brush_h = (brush.h.get() - tile_offset.y).min(height);

                for y in 0..brush_h {
                    let mut off_x = 0.0;
                    let mut width = width.get();
                    let brush_y = tile_offset.y + y;
                    let mut tile_offset_x = tile_offset.x;
                    while width > 0 {
                        let brush_x = tile_offset_x;
                        let brush_w = (brush.w.get() - tile_offset_x).min(width);

                        let quad_offset = base.tiles_of_layer
                            [brush_y as usize * brush.w.get() as usize + brush_x as usize]
                            .quad_offset();
                        let draw_count = brush_w as usize;
                        let mut state = State::new();
                        let pos_x = off_x - tile_offset_x as f32 * TILE_VISUAL_SIZE;
                        let pos_y = off_y - tile_offset.y as f32 * TILE_VISUAL_SIZE;
                        RenderTools::map_canvas_of_group(
                            CanvasType::Handle(canvas_handle),
                            &mut state,
                            map.groups.user.pos.x,
                            map.groups.user.pos.y,
                            group_attr.as_ref(),
                            map.groups.user.zoom,
                        );
                        state.canvas_br.x += center.x - pos_x;
                        state.canvas_br.y += center.y - pos_y;
                        state.canvas_tl.x += center.x - pos_x;
                        state.canvas_tl.y += center.y - pos_y;
                        brush.map_render.render_tile_layer(
                            &state,
                            (&brush.texture).into(),
                            buffer_object,
                            &ColorRgba::new(1.0, 1.0, 1.0, 1.0),
                            PoolVec::from_without_pool(vec![TileLayerDrawInfo {
                                quad_offset,
                                quad_count: draw_count,
                            }]),
                        );

                        width -= brush_w;
                        tile_offset_x = 0;
                        off_x += brush_w as f32 * TILE_VISUAL_SIZE;
                    }
                }

                height -= brush_h;
                tile_offset.y = 0;
                off_y += brush_h as f32 * TILE_VISUAL_SIZE;
            }
        }
    }

    fn render_brush_internal(
        &self,
        brush: &TileBrushTiles,
        map: &EditorMap,
        canvas_handle: &GraphicsCanvasHandle,
        center: vec2,
        group_attr: Option<MapGroupAttr>,
    ) {
        if let BrushVisual::Design(TileLayerVisuals {
            base:
                TileLayerBufferedVisuals {
                    buffer_object: Some(buffer_object_index),
                    ..
                },
            ..
        })
        | BrushVisual::Physics(PhysicsTileLayerVisuals {
            base:
                TileLayerVisuals {
                    base:
                        TileLayerBufferedVisuals {
                            buffer_object: Some(buffer_object_index),
                            ..
                        },
                    ..
                },
            ..
        }) = &brush.render
        {
            let mut state = State::new();
            RenderTools::map_canvas_of_group(
                CanvasType::Handle(canvas_handle),
                &mut state,
                map.groups.user.pos.x,
                map.groups.user.pos.y,
                group_attr.as_ref(),
                map.groups.user.zoom,
            );
            state.canvas_br.x += center.x;
            state.canvas_br.y += center.y;
            state.canvas_tl.x += center.x;
            state.canvas_tl.y += center.y;
            brush.map_render.render_tile_layer(
                &state,
                (&brush.texture).into(),
                buffer_object_index,
                &ColorRgba::new(1.0, 1.0, 1.0, 1.0),
                PoolVec::from_without_pool(vec![TileLayerDrawInfo {
                    quad_offset: 0,
                    quad_count: brush.w.get() as usize * brush.h.get() as usize,
                }]),
            );
        }
    }

    fn render_brush(
        &self,
        ui_canvas: &UiCanvasSize,
        backend_handle: &GraphicsBackendHandle,
        canvas_handle: &GraphicsCanvasHandle,
        stream_handle: &GraphicsStreamHandle,
        map: &EditorMap,
        current_pointer_pos: &egui::Pos2,
        clamp_pos: bool,
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
        let pos_on_map = vec2::new(
            (pos_on_map.x / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE,
            (pos_on_map.y / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE,
        );
        let pos = pos_on_map;
        let mut pos = vec2::new(
            pos.x - brush.negative_offset.x as f32 * TILE_VISUAL_SIZE,
            pos.y - brush.negative_offset.y as f32 * TILE_VISUAL_SIZE,
        );
        if clamp_pos {
            if let Some(layer) = &layer {
                let (w, h) = layer.get_width_and_height();
                pos = vec2::new(
                    pos.x
                        .clamp(0.0, (w.get() - brush.w.get()) as f32 * TILE_VISUAL_SIZE),
                    pos.y
                        .clamp(0.0, (h.get() - brush.h.get()) as f32 * TILE_VISUAL_SIZE),
                );
            }
        }
        let pos = egui::pos2(pos.x, pos.y);

        let brush_size = vec2::new(brush.w.get() as f32, brush.h.get() as f32) * TILE_VISUAL_SIZE;
        let rect =
            egui::Rect::from_min_max(pos, egui::pos2(pos.x + brush_size.x, pos.y + brush_size.y));

        if let Some(TileBrushDownPos { world, .. }) = &self.shift_pointer_down_world_pos {
            let pos_old = vec2::new(
                (world.x / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE,
                (world.y / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE,
            );
            let pos_cur = vec2::new(
                (pos_on_map.x / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE,
                (pos_on_map.y / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE,
            );
            let width = (pos_cur.x - pos_old.x).abs() as u16 + 1;
            let height = (pos_cur.y - pos_old.y).abs() as u16 + 1;
            let pos_min = vec2::new(pos_cur.x.min(pos_old.x), pos_cur.y.min(pos_old.y));

            let rect = egui::Rect::from_min_max(
                egui::pos2(pos_min.x, pos_min.y),
                egui::pos2(
                    pos_min.x + width as f32 * TILE_VISUAL_SIZE,
                    pos_min.y + height as f32 * TILE_VISUAL_SIZE,
                ),
            );

            backend_handle.next_switch_pass();
            render_filled_rect(
                canvas_handle,
                stream_handle,
                map,
                rect,
                ubvec4::new(255, 255, 255, 255),
                &parallax,
                &offset,
                true,
            );
            render_blur(
                backend_handle,
                stream_handle,
                canvas_handle,
                true,
                DEFAULT_BLUR_RADIUS,
                DEFAULT_BLUR_MIX_LENGTH,
                &vec4::new(1.0, 1.0, 1.0, 0.05),
            );
            render_swapped_frame(canvas_handle, stream_handle);

            self.render_brush_repeating_internal(
                brush,
                map,
                canvas_handle,
                //map.groups.user.pos - pos_min,
                -vec2::new(pos_min.x, pos_min.y),
                layer.map(|layer| layer.get_or_fake_group_attr()),
                usvec2::new(
                    (pos_cur.x - pos_old.x)
                        .clamp(f32::MIN, 0.0)
                        .rem_euclid(brush.w.get() as f32) as u16,
                    (pos_cur.y - pos_old.y)
                        .clamp(f32::MIN, 0.0)
                        .rem_euclid(brush.h.get() as f32) as u16,
                ),
                NonZeroU16MinusOne::new(width).unwrap(),
                NonZeroU16MinusOne::new(height).unwrap(),
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
        } else {
            backend_handle.next_switch_pass();
            render_filled_rect(
                canvas_handle,
                stream_handle,
                map,
                rect,
                ubvec4::new(255, 255, 255, 255),
                &parallax,
                &offset,
                true,
            );
            render_blur(
                backend_handle,
                stream_handle,
                canvas_handle,
                true,
                DEFAULT_BLUR_RADIUS,
                DEFAULT_BLUR_MIX_LENGTH,
                &vec4::new(1.0, 1.0, 1.0, 0.05),
            );
            render_swapped_frame(canvas_handle, stream_handle);

            self.render_brush_internal(
                brush,
                map,
                canvas_handle,
                -vec2::new(pos.x, pos.y),
                layer.map(|layer| layer.get_or_fake_group_attr()),
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
    }

    pub fn update(
        &mut self,
        ui_canvas: &UiCanvasSize,
        tp: &Arc<rayon::ThreadPool>,
        graphics_mt: &GraphicsMultiThreaded,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        backend_handle: &GraphicsBackendHandle,
        canvas_handle: &GraphicsCanvasHandle,
        entities_container: &mut EntitiesContainer,
        fake_texture_2d_array: &TextureContainer2dArray,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        latest_keys_down: &HashSet<egui::Key>,
        latest_modifiers: &egui::Modifiers,
        current_pointer_pos: &egui::Pos2,
        available_rect: &egui::Rect,
        client: &mut EditorClient,
    ) {
        let layer = map.active_layer();
        if !layer.as_ref().is_some_and(|layer| layer.is_tile_layer()) {
            return;
        }

        if self.brush.is_none()
            || self.pointer_down_world_pos.is_some()
            || latest_keys_down.contains(&egui::Key::Space)
        {
            self.handle_brush_select(
                ui_canvas,
                tp,
                graphics_mt,
                buffer_object_handle,
                backend_handle,
                canvas_handle,
                entities_container,
                fake_texture_2d_array,
                map,
                latest_pointer,
                latest_keys_down,
                current_pointer_pos,
                available_rect,
            );
        } else {
            self.handle_brush_draw(
                ui_canvas,
                canvas_handle,
                map,
                latest_pointer,
                latest_modifiers,
                current_pointer_pos,
                client,
            );
        }
    }

    pub fn render(
        &mut self,
        ui_canvas: &UiCanvasSize,
        backend_handle: &GraphicsBackendHandle,
        stream_handle: &GraphicsStreamHandle,
        canvas_handle: &GraphicsCanvasHandle,
        entities_container: &mut EntitiesContainer,
        fake_texture_2d_array: &TextureContainer2dArray,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        latest_keys_down: &HashSet<egui::Key>,
        current_pointer_pos: &egui::Pos2,
        available_rect: &egui::Rect,
    ) {
        let layer = map.active_layer();
        if !layer.as_ref().is_some_and(|layer| layer.is_tile_layer()) {
            return;
        }

        // render tile picker if needed
        if latest_keys_down.contains(&egui::Key::Space) {
            let render_rect = Self::tile_picker_rect(available_rect);
            let mut state = State::new();
            // render tiles
            // w or h doesn't matter bcs square
            let size = render_rect.width();
            let size_ratio_x = (TILE_VISUAL_SIZE * 16.0) / size;
            let size_ratio_y = (TILE_VISUAL_SIZE * 16.0) / size;
            let tl_x = -render_rect.min.x * size_ratio_x;
            let tl_y = -render_rect.min.y * size_ratio_y;

            // render filled rect as bg
            state.map_canvas(
                0.0,
                0.0,
                canvas_handle.canvas_width(),
                canvas_handle.canvas_height(),
            );
            render_checkerboard_background(stream_handle, render_rect, &state);

            state.map_canvas(
                tl_x,
                tl_y,
                tl_x + canvas_handle.canvas_width() * size_ratio_x,
                tl_y + canvas_handle.canvas_height() * size_ratio_y,
            );
            let texture = match layer.as_ref().unwrap() {
                EditorLayerUnionRef::Physics { .. } => {
                    entities_container
                        .get_or_default::<ContainerKey>(&"default".try_into().unwrap())
                        // TODO:
                        .get_or_default("ddnet")
                }
                EditorLayerUnionRef::Design { layer, .. } => match layer {
                    EditorLayer::Tile(layer) => layer
                        .layer
                        .attr
                        .image_array
                        .map(|i| &map.resources.image_arrays[i].user.user)
                        .unwrap_or_else(|| fake_texture_2d_array),
                    _ => panic!("this should have been prevented in logic before"),
                },
            };
            let color = ColorRgba::new(1.0, 1.0, 1.0, 1.0);
            let buffer_object = self.tile_picker.render.base.buffer_object.as_ref().unwrap();
            self.tile_picker.map_render.render_tile_layer(
                &state,
                texture.into(),
                buffer_object,
                &color,
                PoolVec::from_without_pool(vec![TileLayerDrawInfo {
                    quad_offset: 0,
                    quad_count: 16 * 16,
                }]),
            );

            if let Some(buffer_object) = map
                .user
                .options
                .show_tile_numbers
                .then_some(self.tile_picker.render.tile_num_buffer_object.as_ref())
                .flatten()
            {
                self.tile_picker.map_render.render_tile_layer(
                    &state,
                    (&entities_container
                        .get_or_default::<ContainerKey>(&"default".try_into().unwrap())
                        .text_overlay_bottom)
                        .into(),
                    buffer_object,
                    &color,
                    PoolVec::from_without_pool(vec![TileLayerDrawInfo {
                        quad_offset: 0,
                        quad_count: 16 * 16,
                    }]),
                );
            }
            // render rect border
            state.map_canvas(
                0.0,
                0.0,
                canvas_handle.canvas_width(),
                canvas_handle.canvas_height(),
            );

            render_rect_from_state(
                stream_handle,
                state,
                render_rect,
                ubvec4::new(0, 0, 255, 255),
            );

            if let Some(TileBrushDownPos { ui, .. }) = &self.pointer_down_world_pos {
                let pointer_down = pos2(ui.x, ui.y);
                let pointer_rect = egui::Rect::from_min_max(
                    current_pointer_pos.min(pointer_down),
                    current_pointer_pos.max(pointer_down),
                );
                let (tile_indices, brush_width, brush_height) =
                    Self::selected_tiles_picker(pointer_rect, render_rect);
                if let Some(&index) = tile_indices.first() {
                    let tile_size = render_rect.width() / 16.0;
                    let min = render_rect.min
                        + egui::vec2((index % 16) as f32, (index / 16) as f32) * tile_size;
                    render_filled_rect_from_state(
                        stream_handle,
                        egui::Rect::from_min_max(
                            min,
                            min + egui::vec2(
                                brush_width as f32 * tile_size,
                                brush_height as f32 * tile_size,
                            ),
                        ),
                        ubvec4::new(0, 255, 255, 50),
                        state,
                        false,
                    );
                }

                render_rect_from_state(
                    stream_handle,
                    state,
                    egui::Rect::from_min_max(
                        current_pointer_pos.min(*ui),
                        current_pointer_pos.max(*ui),
                    ),
                    ubvec4::new(0, 255, 255, 255),
                );
            }
        } else if self.brush.is_none() || self.pointer_down_world_pos.is_some() {
            self.render_selection(
                ui_canvas,
                backend_handle,
                canvas_handle,
                stream_handle,
                map,
                latest_pointer,
                current_pointer_pos,
            );
        } else {
            self.render_brush(
                ui_canvas,
                backend_handle,
                canvas_handle,
                stream_handle,
                map,
                current_pointer_pos,
                false,
            );
        }
    }
}
