use std::{cell::Cell, collections::HashSet, rc::Rc, sync::Arc};

use camera::CameraInterface;
use client_containers::{container::ContainerKey, entities::EntitiesContainer};
use client_render_base::map::{
    map_buffered::{
        ClientMapBuffered, PhysicsTileLayerVisuals, TileLayerBufferedVisualObjects,
        TileLayerBufferedVisuals, TileLayerVisuals,
    },
    map_pipeline::{MapGraphics, TileLayerDrawInfo},
};
use egui::{Rect, pos2};
use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::{
        backend::backend::GraphicsBackendHandle,
        buffer_object::buffer_object::GraphicsBufferObjectHandle,
        canvas::canvas::GraphicsCanvasHandle,
        shader_storage::shader_storage::GraphicsShaderStorageHandle,
        stream::stream::GraphicsStreamHandle, texture::texture::TextureContainer2dArray,
    },
    utils::{DEFAULT_BLUR_MIX_LENGTH, DEFAULT_BLUR_RADIUS, render_blur, render_swapped_frame},
};
use graphics_types::rendering::State;
use hiarc::Hiarc;
use legacy_map::mapdef_06::DdraceTileNum;
use map::{
    map::groups::{
        MapGroupAttr, MapGroupPhysicsAttr,
        layers::{
            physics::{MapLayerPhysics, MapLayerTilePhysicsBase},
            tiles::{
                MapTileLayerAttr, MapTileLayerPhysicsTiles, MapTileLayerTiles, SpeedupTile,
                SwitchTile, TeleTile, Tile, TileBase, TileFlags, TuneTile,
            },
        },
    },
    types::NonZeroU16MinusOne,
};
use math::math::vector::{dvec2, ivec2, ubvec4, usvec2, vec2, vec4};
use pool::mt_datatypes::PoolVec;
use rand::RngCore;

use crate::{
    actions::actions::{
        ActAddPhysicsTileLayer, ActAddRemPhysicsTileLayer, ActTileLayerReplTilesBase,
        ActTileLayerReplaceTiles, ActTilePhysicsLayerReplTilesBase,
        ActTilePhysicsLayerReplaceTiles, EditorAction, EditorActionGroup,
    },
    client::EditorClient,
    map::{EditorLayer, EditorLayerUnionRef, EditorMap, EditorMapInterface, EditorPhysicsLayer},
    map_tools::{
        finish_design_tile_layer_buffer, finish_physics_layer_buffer,
        upload_design_tile_layer_buffer, upload_physics_layer_buffer,
    },
    notifications::EditorNotification,
    physics_layers::{PhysicsLayerOverlayTexture, PhysicsLayerOverlaysDdnet},
    tools::utils::{
        render_checkerboard_background, render_filled_rect, render_filled_rect_from_state,
        render_rect, render_rect_from_state,
    },
    utils::{UiCanvasSize, ui_pos_to_world_pos},
};

use super::shared::{TILE_VISUAL_SIZE, get_animated_color};

// 20 ui pixels
const TILE_PICKER_VISUAL_SIZE: f32 = 30.0;

#[derive(Debug, Hiarc)]
pub enum BrushVisual {
    Design(TileLayerVisuals),
    Physics(PhysicsTileLayerVisuals),
}

#[derive(Debug, Hiarc, Clone, Copy, PartialEq, Eq)]
pub enum TileBrushLastApplyLayer {
    Physics {
        layer_index: usize,
    },
    Design {
        group_index: usize,
        layer_index: usize,
        is_background: bool,
    },
}

#[derive(Debug, Hiarc, Clone, Copy, PartialEq, Eq)]
pub struct TileBrushLastApply {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,

    pub layer: TileBrushLastApplyLayer,
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

    pub last_apply: Cell<Option<TileBrushLastApply>>,
}

#[derive(Debug, Hiarc)]
pub struct TileBrushLastFill {
    pub x: u16,
    pub y: u16,

    // the pointer pos used for the fill
    pub pointer_pos: usvec2,

    // the brush tiles used to create the fill
    pub brush_tiles: MapTileLayerTiles,

    pub render: TileBrushTiles,
}

#[derive(Debug, Hiarc)]
pub struct TileBrushTilePicker {
    pub render: TileLayerVisuals,
    pub map_render: MapGraphics,

    physics_overlay: Rc<PhysicsLayerOverlaysDdnet>,
}

impl TileBrushTilePicker {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        shader_storage_handle: &GraphicsShaderStorageHandle,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        backend_handle: &GraphicsBackendHandle,
        physics_overlay: Rc<PhysicsLayerOverlaysDdnet>,
    ) -> Self {
        let map_render = MapGraphics::new(backend_handle);

        Self {
            render: ClientMapBuffered::tile_set_preview(
                graphics_mt,
                shader_storage_handle,
                buffer_object_handle,
                backend_handle,
            ),
            map_render,
            physics_overlay,
        }
    }
}

#[derive(Debug, Hiarc, Clone, Copy)]
pub struct TileBrushDownPos {
    pub world: vec2,
    pub ui: egui::Pos2,
}

#[derive(Debug, Hiarc, Clone, Copy)]
pub struct TileBrushDown {
    pub pos: TileBrushDownPos,
    pub shift: bool,
}

#[derive(Debug, Hiarc)]
pub struct TileBrush {
    pub brush: Option<TileBrushTiles>,

    pub tile_picker: TileBrushTilePicker,

    pub pointer_down_world_pos: Option<TileBrushDown>,
    pub shift_pointer_down_world_pos: Option<TileBrushDownPos>,

    pub fill: Option<TileBrushLastFill>,

    /// Can the brush destroy existing tiles
    pub destructive: bool,
    /// Can place unused tiles
    pub allow_unused: bool,
    showed_unused_id: Option<u128>,

    /// Random id counted up, used for action identifiers
    pub brush_id_counter: u128,
}

impl TileBrush {
    pub fn new(
        graphics_mt: &GraphicsMultiThreaded,
        shader_storage_handle: &GraphicsShaderStorageHandle,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        backend_handle: &GraphicsBackendHandle,
        physics_overlay: &Rc<PhysicsLayerOverlaysDdnet>,
    ) -> Self {
        Self {
            brush: None,

            tile_picker: TileBrushTilePicker::new(
                graphics_mt,
                shader_storage_handle,
                buffer_object_handle,
                backend_handle,
                physics_overlay.clone(),
            ),

            pointer_down_world_pos: None,
            shift_pointer_down_world_pos: None,

            fill: Default::default(),

            destructive: true,
            allow_unused: false,
            showed_unused_id: None,

            brush_id_counter: ((rand::rng().next_u64() as u128) << 64)
                + rand::rng().next_u64() as u128,
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

    fn non_destructive_copy_ex<T: Copy + AsRef<TileBase>>(
        old_tiles: &[T],
        new_tiles: &mut [T],
        extra_check: impl Fn(&T, usize) -> bool,
    ) {
        new_tiles.iter_mut().enumerate().for_each(|(i, t)| {
            if old_tiles[i].as_ref().index != 0 || extra_check(t, i) {
                *t = old_tiles[i];
            }
        });
    }

    fn non_destructive_copy<T: Copy + AsRef<TileBase>>(old_tiles: &[T], new_tiles: &mut [T]) {
        Self::non_destructive_copy_ex(old_tiles, new_tiles, |_, _| false)
    }

    fn collect_phy_tiles(
        layer: &EditorPhysicsLayer,
        group_attr: &MapGroupPhysicsAttr,
        x: usize,
        copy_width: usize,
        y: usize,
        copy_height: usize,
    ) -> MapTileLayerPhysicsTiles {
        match layer {
            EditorPhysicsLayer::Arbitrary(_) => {
                panic!(
                    "not implemented for \
                    arbitrary layer"
                )
            }
            EditorPhysicsLayer::Game(layer) => MapTileLayerPhysicsTiles::Game(Self::collect_tiles(
                &layer.layer.tiles,
                group_attr.width.get() as usize,
                x,
                copy_width,
                y,
                copy_height,
            )),
            EditorPhysicsLayer::Front(layer) => {
                MapTileLayerPhysicsTiles::Front(Self::collect_tiles(
                    &layer.layer.tiles,
                    group_attr.width.get() as usize,
                    x,
                    copy_width,
                    y,
                    copy_height,
                ))
            }
            EditorPhysicsLayer::Tele(layer) => MapTileLayerPhysicsTiles::Tele(Self::collect_tiles(
                &layer.layer.base.tiles,
                group_attr.width.get() as usize,
                x,
                copy_width,
                y,
                copy_height,
            )),
            EditorPhysicsLayer::Speedup(layer) => {
                MapTileLayerPhysicsTiles::Speedup(Self::collect_tiles(
                    &layer.layer.tiles,
                    group_attr.width.get() as usize,
                    x,
                    copy_width,
                    y,
                    copy_height,
                ))
            }
            EditorPhysicsLayer::Switch(layer) => {
                MapTileLayerPhysicsTiles::Switch(Self::collect_tiles(
                    &layer.layer.base.tiles,
                    group_attr.width.get() as usize,
                    x,
                    copy_width,
                    y,
                    copy_height,
                ))
            }
            EditorPhysicsLayer::Tune(layer) => MapTileLayerPhysicsTiles::Tune(Self::collect_tiles(
                &layer.layer.base.tiles,
                group_attr.width.get() as usize,
                x,
                copy_width,
                y,
                copy_height,
            )),
        }
    }

    fn collect_phy_brush_tiles(
        brush: &TileBrushTiles,
        copy_x: usize,
        copy_width: usize,
        copy_y: usize,
        copy_height: usize,
        map: &EditorMap,
        old_tiles: &MapTileLayerPhysicsTiles,
        group_attr: &MapGroupPhysicsAttr,
        old_x: usize,
        old_y: usize,
        destructive: bool,
    ) -> MapTileLayerPhysicsTiles {
        match &brush.tiles {
            MapTileLayerTiles::Design(_) => {
                todo!("currently design tiles can't be pasted on a physics layer")
            }
            MapTileLayerTiles::Physics(tiles) => match tiles {
                MapTileLayerPhysicsTiles::Arbitrary(_) => {
                    panic!("this operation is not supported")
                }
                MapTileLayerPhysicsTiles::Game(tiles) => {
                    let mut tiles = Self::collect_tiles(
                        tiles,
                        brush.w.get() as usize,
                        copy_x,
                        copy_width,
                        copy_y,
                        copy_height,
                    );
                    // if non-destructive:
                    // for all tiles where the old tiles are non air, set new tiles to old
                    if !destructive {
                        let MapTileLayerPhysicsTiles::Game(old_tiles) = old_tiles else {
                            panic!("expected game tiles. code bug.");
                        };
                        let front_layer = map.groups.physics.layers.iter().find_map(|l| {
                            if let EditorPhysicsLayer::Front(l) = l {
                                Some(l)
                            } else {
                                None
                            }
                        });
                        Self::non_destructive_copy_ex(old_tiles, &mut tiles, |new_tile, index| {
                            let y = index / copy_width;
                            let x = index % copy_width;

                            let x = old_x + x;
                            let y = old_y + y;
                            let tile_index = y * group_attr.width.get() as usize + x;
                            front_layer.is_some_and(|l| {
                                new_tile.index == DdraceTileNum::ThroughCut as u8
                                    && l.layer.tiles[tile_index].index != 0
                            })
                        });
                    }
                    MapTileLayerPhysicsTiles::Game(tiles)
                }
                MapTileLayerPhysicsTiles::Front(tiles) => {
                    let mut tiles = Self::collect_tiles(
                        tiles,
                        brush.w.get() as usize,
                        copy_x,
                        copy_width,
                        copy_y,
                        copy_height,
                    );
                    // if non-destructive:
                    // for all tiles where the old tiles are non air, set new tiles to old
                    if !destructive {
                        let MapTileLayerPhysicsTiles::Front(old_tiles) = old_tiles else {
                            panic!("expected front tiles. code bug.");
                        };
                        let game_layer = map.groups.physics.layers.iter().find_map(|l| {
                            if let EditorPhysicsLayer::Game(l) = l {
                                Some(l)
                            } else {
                                None
                            }
                        });
                        Self::non_destructive_copy_ex(old_tiles, &mut tiles, |new_tile, index| {
                            let y = index / copy_width;
                            let x = index % copy_width;

                            let x = old_x + x;
                            let y = old_y + y;
                            let tile_index = y * group_attr.width.get() as usize + x;
                            game_layer.is_some_and(|l| {
                                new_tile.index == DdraceTileNum::ThroughCut as u8
                                    && l.layer.tiles[tile_index].index != 0
                            })
                        });
                        Self::non_destructive_copy(old_tiles, &mut tiles);
                    }
                    MapTileLayerPhysicsTiles::Front(tiles)
                }
                MapTileLayerPhysicsTiles::Tele(tiles) => {
                    let mut tiles = Self::collect_tiles(
                        tiles,
                        brush.w.get() as usize,
                        copy_x,
                        copy_width,
                        copy_y,
                        copy_height,
                    );
                    // if non-destructive:
                    // for all tiles where the old tiles are non air, set new tiles to old
                    if !destructive {
                        let MapTileLayerPhysicsTiles::Tele(old_tiles) = old_tiles else {
                            panic!("expected tele tiles. code bug.");
                        };
                        Self::non_destructive_copy(old_tiles, &mut tiles);
                    }
                    MapTileLayerPhysicsTiles::Tele(tiles)
                }
                MapTileLayerPhysicsTiles::Speedup(tiles) => {
                    let mut tiles = Self::collect_tiles(
                        tiles,
                        brush.w.get() as usize,
                        copy_x,
                        copy_width,
                        copy_y,
                        copy_height,
                    );
                    // if non-destructive:
                    // for all tiles where the old tiles are non air, set new tiles to old
                    if !destructive {
                        let MapTileLayerPhysicsTiles::Speedup(old_tiles) = old_tiles else {
                            panic!("expected speedup tiles. code bug.");
                        };
                        Self::non_destructive_copy(old_tiles, &mut tiles);
                    }
                    MapTileLayerPhysicsTiles::Speedup(tiles)
                }
                MapTileLayerPhysicsTiles::Switch(tiles) => {
                    let mut tiles = Self::collect_tiles(
                        tiles,
                        brush.w.get() as usize,
                        copy_x,
                        copy_width,
                        copy_y,
                        copy_height,
                    );
                    // if non-destructive:
                    // for all tiles where the old tiles are non air, set new tiles to old
                    if !destructive {
                        let MapTileLayerPhysicsTiles::Switch(old_tiles) = old_tiles else {
                            panic!("expected switch tiles. code bug.");
                        };
                        Self::non_destructive_copy(old_tiles, &mut tiles);
                    }
                    MapTileLayerPhysicsTiles::Switch(tiles)
                }
                MapTileLayerPhysicsTiles::Tune(tiles) => {
                    let mut tiles = Self::collect_tiles(
                        tiles,
                        brush.w.get() as usize,
                        copy_x,
                        copy_width,
                        copy_y,
                        copy_height,
                    );
                    // if non-destructive:
                    // for all tiles where the old tiles are non air, set new tiles to old
                    if !destructive {
                        let MapTileLayerPhysicsTiles::Tune(old_tiles) = old_tiles else {
                            panic!("expected tune tiles. code bug.");
                        };
                        Self::non_destructive_copy(old_tiles, &mut tiles);
                    }
                    MapTileLayerPhysicsTiles::Tune(tiles)
                }
            },
        }
    }

    fn hookthrough_cut(
        map: &EditorMap,
        group_attr: &MapGroupPhysicsAttr,
        old_tiles: &MapTileLayerPhysicsTiles,
        new_tiles: &mut MapTileLayerPhysicsTiles,
        x: u16,
        brush_w: u16,
        y: u16,
        brush_h: u16,
        actions: &mut Vec<EditorAction>,
        repeating_assume_front_layer_created: Option<&mut bool>,
    ) {
        // if front layer contains the through cut and we remove it,
        // always also clear the game layer
        if let (
            MapTileLayerPhysicsTiles::Front(front_old_tiles),
            MapTileLayerPhysicsTiles::Front(front_new_tiles),
        ) = (&old_tiles, &new_tiles)
        {
            let removes_cuts = front_old_tiles.iter().enumerate().any(|(index, t)| {
                t.index == DdraceTileNum::ThroughCut as u8
                    && front_new_tiles[index].index == DdraceTileNum::Air as u8
            });
            let adds_cuts = front_old_tiles.iter().enumerate().any(|(index, t)| {
                t.index != DdraceTileNum::ThroughCut as u8
                    && front_new_tiles[index].index == DdraceTileNum::ThroughCut as u8
            });
            if let Some((layer_index, layer)) = (removes_cuts || adds_cuts)
                .then(|| {
                    map.groups
                        .physics
                        .layers
                        .iter()
                        .enumerate()
                        .find(|(_, l)| matches!(l, EditorPhysicsLayer::Game(_)))
                })
                .flatten()
            {
                let old_tiles = Self::collect_phy_tiles(
                    layer,
                    group_attr,
                    x as usize,
                    brush_w as usize,
                    y as usize,
                    brush_h as usize,
                );
                let mut new_tiles = Self::collect_phy_tiles(
                    layer,
                    group_attr,
                    x as usize,
                    brush_w as usize,
                    y as usize,
                    brush_h as usize,
                );
                let MapTileLayerPhysicsTiles::Game(tiles) = &mut new_tiles else {
                    panic!("Expected game tiles, logic error.");
                };
                // replace all unhookable with air
                tiles.iter_mut().enumerate().for_each(|(index, t)| {
                    if t.index == DdraceTileNum::NoHook as u8
                        && front_old_tiles[index].index == DdraceTileNum::ThroughCut as u8
                        && front_new_tiles[index].index == DdraceTileNum::Air as u8
                    {
                        t.index = DdraceTileNum::Air as u8;
                        t.flags = TileFlags::empty();
                    } else if t.index != DdraceTileNum::NoHook as u8
                        && front_old_tiles[index].index != DdraceTileNum::ThroughCut as u8
                        && front_new_tiles[index].index == DdraceTileNum::ThroughCut as u8
                    {
                        t.index = DdraceTileNum::NoHook as u8;
                        t.flags = TileFlags::empty();
                    }
                });
                actions.push(EditorAction::TilePhysicsLayerReplaceTiles(
                    ActTilePhysicsLayerReplaceTiles {
                        base: ActTilePhysicsLayerReplTilesBase {
                            layer_index,
                            old_tiles,
                            new_tiles,
                            x,
                            y,
                            w: NonZeroU16MinusOne::new(brush_w).unwrap(),
                            h: NonZeroU16MinusOne::new(brush_h).unwrap(),
                        },
                    },
                ));
            }
        }

        let phy_tile_count = map.groups.physics.attr.width.get() as usize
            * map.groups.physics.attr.height.get() as usize;

        // if game layer contains the unhook & front layer
        // the through cut and the game tile is removed,
        // always also clear the front layer's tile.
        if let (
            MapTileLayerPhysicsTiles::Game(old_tiles_game),
            MapTileLayerPhysicsTiles::Game(new_tiles_game),
            Some((front_layer_index, front_old_tiles, front_new_tiles)),
        ) = (
            &old_tiles,
            &new_tiles,
            map.groups
                .physics
                .layers
                .iter()
                .enumerate()
                .find_map(|(index, l)| {
                    if let EditorPhysicsLayer::Front(_) = l {
                        let tiles = Self::collect_phy_tiles(
                            l,
                            group_attr,
                            x as usize,
                            brush_w as usize,
                            y as usize,
                            brush_h as usize,
                        );
                        Some((index, tiles.clone(), tiles))
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    if repeating_assume_front_layer_created
                        .as_deref()
                        .is_some_and(|b| *b)
                    {
                        // hack in a dummy front layer tiles
                        let tiles = MapTileLayerPhysicsTiles::Front(vec![
                            Default::default();
                            brush_w as usize
                                * brush_h as usize
                        ]);
                        Some((map.groups.physics.layers.len(), tiles.clone(), tiles))
                    } else {
                        None
                    }
                }),
        ) {
            let MapTileLayerPhysicsTiles::Front(flayer_tiles) = &front_old_tiles else {
                panic!("Expected front layer, check above logic code.")
            };
            let game_replaces_cut = old_tiles_game.iter().enumerate().any(|(index, t)| {
                t.index == DdraceTileNum::NoHook as u8
                    && flayer_tiles[index].index == DdraceTileNum::ThroughCut as u8
                    && new_tiles_game[index].index != DdraceTileNum::NoHook as u8
            });
            let game_contains_cut = new_tiles_game.iter().enumerate().any(|(index, t)| {
                t.index == DdraceTileNum::ThroughCut as u8
                    && flayer_tiles[index].index != DdraceTileNum::ThroughCut as u8
            });
            if let Some((layer_index, old_tiles, mut new_tiles)) = (game_replaces_cut
                || game_contains_cut)
                .then_some((front_layer_index, front_old_tiles, front_new_tiles))
            {
                let MapTileLayerPhysicsTiles::Front(tiles) = &mut new_tiles else {
                    panic!("Expected front tiles, logic error.");
                };
                // replace all through cut with air where game layer replaces unhook
                old_tiles_game.iter().enumerate().for_each(|(index, t)| {
                    if t.index == DdraceTileNum::NoHook as u8
                        && tiles[index].index == DdraceTileNum::ThroughCut as u8
                        && new_tiles_game[index].index != DdraceTileNum::NoHook as u8
                    {
                        let ftile = &mut tiles[index];

                        ftile.index = DdraceTileNum::Air as u8;
                        ftile.flags = TileFlags::empty();
                    }
                });
                // add through cuts where game tiles would place them
                new_tiles_game
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.index == DdraceTileNum::ThroughCut as u8)
                    .for_each(|(index, _)| {
                        tiles[index].index = DdraceTileNum::ThroughCut as u8;
                        tiles[index].flags = TileFlags::empty();
                    });
                actions.push(EditorAction::TilePhysicsLayerReplaceTiles(
                    ActTilePhysicsLayerReplaceTiles {
                        base: ActTilePhysicsLayerReplTilesBase {
                            layer_index,
                            old_tiles,
                            new_tiles,
                            x,
                            y,
                            w: NonZeroU16MinusOne::new(brush_w).unwrap(),
                            h: NonZeroU16MinusOne::new(brush_h).unwrap(),
                        },
                    },
                ));
            }
        }

        let ensure_front_layer = if let MapTileLayerPhysicsTiles::Game(tiles) = &new_tiles {
            tiles
                .iter()
                .any(|t| t.index == DdraceTileNum::ThroughCut as u8)
        } else {
            false
        };

        if ensure_front_layer
            && !map
                .groups
                .physics
                .layers
                .iter()
                .any(|l| matches!(l, EditorPhysicsLayer::Front(_)))
            && repeating_assume_front_layer_created
                .as_deref()
                .is_none_or(|b| !*b)
        {
            // add action for front layer creation
            let mut tiles: Vec<TileBase> = vec![Default::default(); phy_tile_count];
            let MapTileLayerPhysicsTiles::Game(new_tiles) = &new_tiles else {
                panic!("expected  game layer tiles")
            };
            // for every new tile that is through cut, add a through cut to front layer
            new_tiles.iter().enumerate().for_each(|(index, t)| {
                if t.index == DdraceTileNum::ThroughCut as u8 {
                    let w = group_attr.width.get() as usize;
                    let brush_w = brush_w as usize;
                    let index = (y as usize + index / brush_w) * w + x as usize + index % brush_w;
                    tiles[index].index = DdraceTileNum::ThroughCut as u8;
                }
            });
            actions.push(EditorAction::AddPhysicsTileLayer(ActAddPhysicsTileLayer {
                base: ActAddRemPhysicsTileLayer {
                    index: map.groups.physics.layers.len(),
                    layer: MapLayerPhysics::Front(MapLayerTilePhysicsBase { tiles }),
                },
            }));

            if let Some(repeating_assume_front_layer_created) = repeating_assume_front_layer_created
            {
                *repeating_assume_front_layer_created = true;
            }
        }

        // game layer will not contain through cuts, instead use
        // unhookable
        if let MapTileLayerPhysicsTiles::Game(new_tiles) = new_tiles {
            new_tiles.iter_mut().for_each(|t| {
                if t.index == DdraceTileNum::ThroughCut as u8 {
                    t.index = DdraceTileNum::NoHook as u8;
                }
            });
        }
    }

    fn brush_hookthrough_cut(
        map: &EditorMap,
        tiles: &mut MapTileLayerTiles,
        layer_width: u16,
        brush_w: u16,
        x: u16,
        y: u16,
    ) {
        // if game layer and selected tile is unhook and
        // the front layer has a through cut on that position
        // then it should instead be through cut tile
        if let (
            MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Game(tiles)),
            Some(EditorPhysicsLayer::Front(layer)),
        ) = (
            tiles,
            map.groups
                .physics
                .layers
                .iter()
                .find(|l| matches!(l, EditorPhysicsLayer::Front(_))),
        ) {
            tiles.iter_mut().enumerate().for_each(|(index, t)| {
                let w = layer_width as usize;
                let brush_w = brush_w as usize;
                let index = (y as usize + index / brush_w) * w + x as usize + index % brush_w;

                if t.index == DdraceTileNum::NoHook as u8
                    && layer.layer.tiles[index].index == DdraceTileNum::ThroughCut as u8
                {
                    t.index = DdraceTileNum::ThroughCut as u8;
                    t.flags = TileFlags::empty();
                }
            });
        }
    }

    pub fn create_brush_visual(
        tp: &Arc<rayon::ThreadPool>,
        graphics_mt: &GraphicsMultiThreaded,
        shader_storage_handle: &GraphicsShaderStorageHandle,
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
                    upload_design_tile_layer_buffer(graphics_mt, tiles, w, h, has_texture, true)
                });
                finish_design_tile_layer_buffer(
                    shader_storage_handle,
                    buffer_object_handle,
                    backend_handle,
                    buffer,
                )
            }),
            MapTileLayerTiles::Physics(tiles) => BrushVisual::Physics({
                let buffer = tp.install(|| {
                    upload_physics_layer_buffer(graphics_mt, w, h, tiles.as_ref(), true)
                });
                finish_physics_layer_buffer(
                    shader_storage_handle,
                    buffer_object_handle,
                    backend_handle,
                    buffer,
                )
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
        shader_storage_handle: &GraphicsShaderStorageHandle,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        backend_handle: &GraphicsBackendHandle,
        canvas_handle: &GraphicsCanvasHandle,
        entities_container: &mut EntitiesContainer,
        fake_texture_2d_array: &TextureContainer2dArray,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        latest_modifiers: &egui::Modifiers,
        latest_keys_down: &HashSet<egui::Key>,
        current_pointer_pos: &egui::Pos2,
        available_rect: &egui::Rect,
        client: &mut EditorClient,
    ) {
        let is_primary_allowed_down = !latest_modifiers.ctrl && latest_pointer.primary_down();
        let is_primary_allowed_pressed = !latest_modifiers.ctrl && latest_pointer.primary_pressed();
        fn has_unused_tiles(
            tile_picker: &TileBrushTilePicker,
            map: &EditorMap,
            tiles: &MapTileLayerTiles,
            layer: &EditorLayerUnionRef<'_>,
        ) -> bool {
            match &tiles {
                MapTileLayerTiles::Design(tiles) => {
                    // only if the design layer has a texture
                    let EditorLayerUnionRef::Design {
                        layer: EditorLayer::Tile(layer),
                        ..
                    } = layer
                    else {
                        panic!(
                            "this cannot happen, \
                    it was previously checked if tile layer"
                        );
                    };
                    if let Some(img) = layer
                        .layer
                        .attr
                        .image_array
                        .and_then(|i| map.resources.image_arrays.get(i))
                    {
                        let mut has_unused = false;
                        for tile in tiles.iter() {
                            let index = tile.index as usize;
                            if index != 0
                                && img.user.props.tile_non_fully_transparent_percentage[index] == 0
                            {
                                has_unused = true;
                                break;
                            }
                        }
                        has_unused
                    } else {
                        false
                    }
                }
                MapTileLayerTiles::Physics(tiles) => {
                    fn has_unused<T: AsRef<TileBase>>(
                        tiles: &[T],
                        tex: &PhysicsLayerOverlayTexture,
                    ) -> bool {
                        for tile in tiles.iter() {
                            let index = tile.as_ref().index as usize;
                            if index != 0 && !tex.non_fully_transparent[index] {
                                return true;
                            }
                        }
                        false
                    }
                    //
                    match tiles {
                        MapTileLayerPhysicsTiles::Arbitrary(_) => false,
                        MapTileLayerPhysicsTiles::Game(tiles) => {
                            has_unused(tiles, &tile_picker.physics_overlay.game)
                        }
                        MapTileLayerPhysicsTiles::Front(tiles) => {
                            has_unused(tiles, &tile_picker.physics_overlay.front)
                        }
                        MapTileLayerPhysicsTiles::Tele(tiles) => {
                            has_unused(tiles, &tile_picker.physics_overlay.tele)
                        }
                        MapTileLayerPhysicsTiles::Speedup(tiles) => {
                            has_unused(tiles, &tile_picker.physics_overlay.speedup)
                        }
                        MapTileLayerPhysicsTiles::Switch(tiles) => {
                            has_unused(tiles, &tile_picker.physics_overlay.switch)
                        }
                        MapTileLayerPhysicsTiles::Tune(tiles) => {
                            has_unused(tiles, &tile_picker.physics_overlay.tune)
                        }
                    }
                }
            }
        }

        let layer = map.active_layer();
        let (offset, parallax) = if let Some(layer) = &layer {
            layer.get_offset_and_parallax()
        } else {
            Default::default()
        };
        // if pointer was already down
        if let Some(TileBrushDown {
            pos: TileBrushDownPos { world, ui },
            ..
        }) = &self.pointer_down_world_pos
        {
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
                                    EditorPhysicsLayer::Speedup(layer) => {
                                        MapTileLayerPhysicsTiles::Speedup(
                                            tile_indices
                                                .into_iter()
                                                .map(|index| SpeedupTile {
                                                    base: TileBase {
                                                        index,
                                                        flags: TileFlags::empty(),
                                                    },
                                                    angle: layer.user.speedup_angle,
                                                    force: layer.user.speedup_force,
                                                    max_speed: layer.user.speedup_max_speed,
                                                })
                                                .collect(),
                                        )
                                    }
                                    EditorPhysicsLayer::Switch(layer) => {
                                        MapTileLayerPhysicsTiles::Switch(
                                            tile_indices
                                                .into_iter()
                                                .map(|index| SwitchTile {
                                                    base: TileBase {
                                                        index,
                                                        flags: TileFlags::empty(),
                                                    },
                                                    number: physics_group_editor.active_switch,
                                                    delay: layer.user.switch_delay,
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
                                {
                                    let physics = entities_container
                                        .get_or_default::<ContainerKey>(
                                            &"default".try_into().unwrap(),
                                        );
                                    if matches!(layer, EditorPhysicsLayer::Speedup(_)) {
                                        physics.speedup.clone()
                                    } else {
                                        physics
                                            // TODO:
                                            .get_or_default("ddnet")
                                            .clone()
                                    }
                                },
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

                        // check for unused tiles
                        let has_unused = !self.allow_unused
                            && has_unused_tiles(&self.tile_picker, map, &tiles, &layer);
                        if has_unused {
                            self.brush = None;
                            if self
                                .showed_unused_id
                                .is_none_or(|id| id != self.brush_id_counter)
                            {
                                client.notifications.push(EditorNotification::Error(
                                    "Cannot use unused tiles".to_string(),
                                ));
                                self.showed_unused_id = Some(self.brush_id_counter);
                            }
                        } else {
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
                                            true,
                                        )
                                    });
                                    finish_design_tile_layer_buffer(
                                        shader_storage_handle,
                                        buffer_object_handle,
                                        backend_handle,
                                        buffer,
                                    )
                                }),
                                MapTileLayerTiles::Physics(tiles) => BrushVisual::Physics({
                                    let buffer = tp.install(|| {
                                        upload_physics_layer_buffer(
                                            graphics_mt,
                                            w,
                                            h,
                                            tiles.as_ref(),
                                            true,
                                        )
                                    });
                                    finish_physics_layer_buffer(
                                        shader_storage_handle,
                                        buffer_object_handle,
                                        backend_handle,
                                        buffer,
                                    )
                                }),
                            };

                            self.brush_id_counter += 1;
                            self.brush = Some(TileBrushTiles {
                                tiles,
                                w,
                                h,
                                negative_offset: usvec2::new(0, 0),
                                negative_offsetf: dvec2::new(0.0, 0.0),
                                render,
                                map_render: MapGraphics::new(backend_handle),
                                texture,

                                last_apply: Default::default(),
                            });
                        }
                    }
                }
                // else select from existing tiles
                else {
                    let (layer_width, layer_height) = layer.get_width_and_height();

                    let pointer_cur = vec2::new(current_pointer_pos.x, current_pointer_pos.y);

                    let &vec2 {
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
                        let (mut tiles, texture) = match layer {
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
                                {
                                    let physics = entities_container
                                        .get_or_default::<ContainerKey>(
                                            &"default".try_into().unwrap(),
                                        );
                                    if matches!(layer, EditorPhysicsLayer::Speedup(_)) {
                                        physics.speedup.clone()
                                    } else {
                                        physics
                                            // TODO:
                                            .get_or_default("ddnet")
                                            .clone()
                                    }
                                },
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

                        Self::brush_hookthrough_cut(
                            map,
                            &mut tiles,
                            layer_width.get(),
                            count_x,
                            x0,
                            y0,
                        );

                        let has_unused = !self.allow_unused
                            && has_unused_tiles(&self.tile_picker, map, &tiles, &layer);

                        if has_unused {
                            self.brush = None;
                            if self
                                .showed_unused_id
                                .is_none_or(|id| id != self.brush_id_counter)
                            {
                                client.notifications.push(EditorNotification::Error(
                                    "Cannot use unused tiles".to_string(),
                                ));
                                self.showed_unused_id = Some(self.brush_id_counter);
                            }
                        } else {
                            let w = NonZeroU16MinusOne::new(count_x).unwrap();
                            let h = NonZeroU16MinusOne::new(count_y).unwrap();
                            let render = Self::create_brush_visual(
                                tp,
                                graphics_mt,
                                shader_storage_handle,
                                buffer_object_handle,
                                backend_handle,
                                w,
                                h,
                                &tiles,
                            );

                            self.brush_id_counter += 1;
                            self.brush = Some(TileBrushTiles {
                                tiles,
                                w,
                                h,
                                negative_offset: usvec2::new(
                                    if x_needs_offset {
                                        count_x - 1
                                    } else {
                                        Default::default()
                                    },
                                    if y_needs_offset {
                                        count_y - 1
                                    } else {
                                        Default::default()
                                    },
                                ),
                                negative_offsetf: dvec2::new(
                                    if x_needs_offset {
                                        count_x as f64
                                    } else {
                                        Default::default()
                                    },
                                    if y_needs_offset {
                                        count_y as f64
                                    } else {
                                        Default::default()
                                    },
                                ),
                                render,
                                map_render: MapGraphics::new(backend_handle),
                                texture,

                                last_apply: Default::default(),
                            });
                        }
                    } else {
                        self.brush = None;
                    }

                    if !is_primary_allowed_down {
                        // if shift, then delete the selection
                        if self.pointer_down_world_pos.is_some_and(|t| t.shift) {
                            if let Some(brush) = &self.brush {
                                let x = x0;
                                let y = y0;
                                let brush_w = brush.w.get();
                                let brush_h = brush.h.get();
                                let brush_len = brush_w as usize * brush_h as usize;
                                let (actions, group_indentifier) = match &layer {
                                    EditorLayerUnionRef::Physics {
                                        layer,
                                        group_attr,
                                        layer_index,
                                    } => {
                                        let old_tiles = Self::collect_phy_tiles(
                                            layer,
                                            group_attr,
                                            x as usize,
                                            brush_w as usize,
                                            y as usize,
                                            brush_h as usize,
                                        );
                                        let mut new_tiles = match &brush.tiles {
                                            MapTileLayerTiles::Design(_) => todo!(
                                                "currently design tiles can't \
                                                be pasted on a physics layer"
                                            ),
                                            MapTileLayerTiles::Physics(tiles) => match tiles {
                                                MapTileLayerPhysicsTiles::Arbitrary(_) => panic!(
                                                    "this operation is \
                                                     not supported"
                                                ),
                                                MapTileLayerPhysicsTiles::Game(_) => {
                                                    MapTileLayerPhysicsTiles::Game(
                                                        vec![Default::default(); brush_len],
                                                    )
                                                }
                                                MapTileLayerPhysicsTiles::Front(_) => {
                                                    MapTileLayerPhysicsTiles::Front(
                                                        vec![Default::default(); brush_len],
                                                    )
                                                }
                                                MapTileLayerPhysicsTiles::Tele(_) => {
                                                    MapTileLayerPhysicsTiles::Tele(
                                                        vec![Default::default(); brush_len],
                                                    )
                                                }
                                                MapTileLayerPhysicsTiles::Speedup(_) => {
                                                    MapTileLayerPhysicsTiles::Speedup(
                                                        vec![Default::default(); brush_len],
                                                    )
                                                }
                                                MapTileLayerPhysicsTiles::Switch(_) => {
                                                    MapTileLayerPhysicsTiles::Switch(
                                                        vec![Default::default(); brush_len],
                                                    )
                                                }
                                                MapTileLayerPhysicsTiles::Tune(_) => {
                                                    MapTileLayerPhysicsTiles::Tune(
                                                        vec![Default::default(); brush_len],
                                                    )
                                                }
                                            },
                                        };

                                        let mut actions: Vec<EditorAction> = Default::default();

                                        Self::hookthrough_cut(
                                            map,
                                            group_attr,
                                            &old_tiles,
                                            &mut new_tiles,
                                            x,
                                            brush_w,
                                            y,
                                            brush_h,
                                            &mut actions,
                                            None,
                                        );

                                        actions.push(EditorAction::TilePhysicsLayerReplaceTiles(
                                            ActTilePhysicsLayerReplaceTiles {
                                                base: ActTilePhysicsLayerReplTilesBase {
                                                    layer_index: *layer_index,
                                                    old_tiles,
                                                    new_tiles,
                                                    x,
                                                    y,
                                                    w: NonZeroU16MinusOne::new(brush_w).unwrap(),
                                                    h: NonZeroU16MinusOne::new(brush_h).unwrap(),
                                                },
                                            },
                                        ));

                                        (actions, format!("tile-brush phy {layer_index}"))
                                    }
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
                                            vec![EditorAction::TileLayerReplaceTiles(
                                                ActTileLayerReplaceTiles {
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
                                                        new_tiles: vec![
                                                            Default::default();
                                                            brush_w as usize
                                                                * brush_h as usize
                                                        ],
                                                        x,
                                                        y,
                                                        w: NonZeroU16MinusOne::new(brush_w)
                                                            .unwrap(),
                                                        h: NonZeroU16MinusOne::new(brush_h)
                                                            .unwrap(),
                                                    },
                                                },
                                            )],
                                            format!(
                                                "tile-brush {group_index}-{layer_index}-{is_background}"
                                            ),
                                        )
                                    }
                                };
                                client.execute_group(EditorActionGroup {
                                    actions,
                                    identifier: Some(format!(
                                        "{group_indentifier}-{}",
                                        self.brush_id_counter
                                    )),
                                });
                            }
                            self.brush = None;
                        }
                        self.pointer_down_world_pos = None;
                    }
                }
            }

            if !is_primary_allowed_down {
                self.pointer_down_world_pos = None;
            }
        } else {
            // else check if the pointer is down now
            if is_primary_allowed_pressed {
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
                self.pointer_down_world_pos = Some(TileBrushDown {
                    pos: TileBrushDownPos {
                        world: pos,
                        ui: *current_pointer_pos,
                    },
                    shift: latest_modifiers.shift,
                });
            }
        }
    }

    fn fill<T: AsRef<TileBase>>(
        tiles: &[T],
        w: NonZeroU16MinusOne,
        h: NonZeroU16MinusOne,
        x: u16,
        y: u16,
    ) -> (Vec<bool>, usvec2, usvec2) {
        let mut filled = vec![false; tiles.len()];
        let get_index = |x: u16, y: u16| y as usize * w.get() as usize + x as usize;
        // can only fill starting from a non filled position.
        let index = get_index(x, y);
        let search_tile = tiles[index].as_ref().index;
        filled[index] = true;
        let mut check_stack = vec![(x, y)];
        let mut min_x = x;
        let mut min_y = y;
        let mut max_x = x;
        let mut max_y = y;
        while let Some((x, y)) = check_stack.pop() {
            // check all neighbours of this tile
            let x = [
                x.checked_sub(1),
                Some(x),
                x.checked_add(1).and_then(|x| (x < w.get()).then_some(x)),
            ];
            let y = [
                y.checked_sub(1),
                Some(y),
                y.checked_add(1).and_then(|y| (y < h.get()).then_some(y)),
            ];
            for y in y {
                for x in x {
                    if let Some((x, y)) = x.zip(y) {
                        let index = get_index(x, y);
                        if tiles[index].as_ref().index == search_tile && !filled[index] {
                            filled[index] = true;
                            check_stack.push((x, y));

                            min_x = min_x.min(x);
                            min_y = min_y.min(y);
                            max_x = max_x.max(x);
                            max_y = max_y.max(y);
                        }
                    }
                }
            }
        }

        (filled, usvec2::new(min_x, min_y), usvec2::new(max_x, max_y))
    }

    fn fill_tiles<T: AsRef<TileBase> + Copy + Default + Eq>(
        tiles: &[T],
        brush_tiles: &[T],
        w: NonZeroU16MinusOne,
        h: NonZeroU16MinusOne,
        pos: &usvec2,
        // if false, then it's filled with the layer's tile at that pos
        fill_with_empty_tile: bool,
        to_tiles: impl Fn(Vec<T>) -> MapTileLayerTiles,
    ) -> (
        MapTileLayerTiles,
        MapTileLayerTiles,
        usvec2,
        NonZeroU16MinusOne,
        NonZeroU16MinusOne,
    ) {
        let (filled, min, max) = Self::fill(tiles, w, h, pos.x, pos.y);
        let width = NonZeroU16MinusOne::new((max.x - min.x) + 1).unwrap();
        let height = NonZeroU16MinusOne::new((max.y - min.y) + 1).unwrap();
        let mut old_tiles = vec![Default::default(); width.get() as usize * height.get() as usize];
        let mut new_tiles = vec![brush_tiles[0]; width.get() as usize * height.get() as usize];
        for y in 0..height.get() {
            for x in 0..width.get() {
                let index = (min.y + y) as usize * w.get() as usize + (min.x + x) as usize;
                let index_tile = y as usize * width.get() as usize + x as usize;
                if !filled[index] {
                    if fill_with_empty_tile {
                        new_tiles[index_tile] = Default::default();
                    } else {
                        new_tiles[index_tile] = tiles[index];
                    }
                }
                old_tiles[index_tile] = tiles[index];
            }
        }
        (to_tiles(old_tiles), to_tiles(new_tiles), min, width, height)
    }

    fn brush_tiles_match_layer(
        brush_tiles: &MapTileLayerTiles,
        layer: &EditorLayerUnionRef<'_>,
    ) -> bool {
        match layer {
            EditorLayerUnionRef::Physics { layer, .. } => match layer {
                EditorPhysicsLayer::Arbitrary(_) => matches!(
                    brush_tiles,
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Arbitrary(_))
                ),
                EditorPhysicsLayer::Game(_) => {
                    matches!(
                        brush_tiles,
                        MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Game(_))
                    )
                }
                EditorPhysicsLayer::Front(_) => matches!(
                    brush_tiles,
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Front(_))
                ),
                EditorPhysicsLayer::Tele(_) => matches!(
                    brush_tiles,
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tele(_))
                ),
                EditorPhysicsLayer::Speedup(_) => matches!(
                    brush_tiles,
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Speedup(_))
                ),
                EditorPhysicsLayer::Switch(_) => matches!(
                    brush_tiles,
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Switch(_))
                ),
                EditorPhysicsLayer::Tune(_) => matches!(
                    brush_tiles,
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tune(_))
                ),
            },
            EditorLayerUnionRef::Design { .. } => {
                matches!(brush_tiles, MapTileLayerTiles::Design(_))
            }
        }
    }

    fn phy_brush_actions(
        map: &EditorMap,
        group_attr: &MapGroupPhysicsAttr,
        old_tiles: MapTileLayerPhysicsTiles,
        mut new_tiles: MapTileLayerPhysicsTiles,
        x: u16,
        brush_w: u16,
        y: u16,
        brush_h: u16,
        layer_index: usize,
        repeating_assume_front_layer_created: Option<&mut bool>,
    ) -> Vec<EditorAction> {
        let mut actions: Vec<EditorAction> = Default::default();

        Self::hookthrough_cut(
            map,
            group_attr,
            &old_tiles,
            &mut new_tiles,
            x,
            brush_w,
            y,
            brush_h,
            &mut actions,
            repeating_assume_front_layer_created,
        );

        actions.push(EditorAction::TilePhysicsLayerReplaceTiles(
            ActTilePhysicsLayerReplaceTiles {
                base: ActTilePhysicsLayerReplTilesBase {
                    layer_index,
                    old_tiles,
                    new_tiles,
                    x,
                    y,
                    w: NonZeroU16MinusOne::new(brush_w).unwrap(),
                    h: NonZeroU16MinusOne::new(brush_h).unwrap(),
                },
            },
        ));
        actions
    }

    fn design_brush_actions(
        old_tiles: Vec<Tile>,
        new_tiles: Vec<Tile>,
        is_background: bool,
        group_index: usize,
        layer_index: usize,
        x: u16,
        y: u16,
        brush_w: u16,
        brush_h: u16,
    ) -> Vec<EditorAction> {
        vec![EditorAction::TileLayerReplaceTiles(
            ActTileLayerReplaceTiles {
                base: ActTileLayerReplTilesBase {
                    is_background,
                    group_index,
                    layer_index,
                    old_tiles,
                    new_tiles,
                    x,
                    y,
                    w: NonZeroU16MinusOne::new(brush_w).unwrap(),
                    h: NonZeroU16MinusOne::new(brush_h).unwrap(),
                },
            },
        )]
    }

    fn apply_brush_internal(
        brush_id_counter: u128,
        map: &EditorMap,
        layer: &EditorLayerUnionRef<'_>,
        brush: &TileBrushTiles,
        client: &mut EditorClient,
        x: i32,
        y: i32,
        brush_off_x: u16,
        brush_off_y: u16,
        max_brush_w: u16,
        max_brush_h: u16,
        repeating_assume_front_layer_created: Option<&mut bool>,
        destructive: bool,
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

        let brush_matches_layer = Self::brush_tiles_match_layer(&brush.tiles, layer);

        if brush_w > 0 && brush_h > 0 && brush_matches_layer {
            let (actions, group_indentifier, apply_layer) = match layer {
                EditorLayerUnionRef::Physics {
                    layer,
                    group_attr,
                    layer_index,
                } => {
                    let old_tiles = Self::collect_phy_tiles(
                        layer,
                        group_attr,
                        x as usize,
                        brush_w as usize,
                        y as usize,
                        brush_h as usize,
                    );
                    let new_tiles = Self::collect_phy_brush_tiles(
                        brush,
                        brush_x as usize,
                        brush_w as usize,
                        brush_y as usize,
                        brush_h as usize,
                        map,
                        &old_tiles,
                        group_attr,
                        x as usize,
                        y as usize,
                        destructive,
                    );

                    let actions = Self::phy_brush_actions(
                        map,
                        group_attr,
                        old_tiles,
                        new_tiles,
                        x,
                        brush_w,
                        y,
                        brush_h,
                        *layer_index,
                        repeating_assume_front_layer_created,
                    );

                    (
                        actions,
                        format!("tile-brush phy {layer_index}"),
                        TileBrushLastApplyLayer::Physics {
                            layer_index: *layer_index,
                        },
                    )
                }
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
                    let old_tiles = Self::collect_tiles(
                        &layer.layer.tiles,
                        layer.layer.attr.width.get() as usize,
                        x as usize,
                        brush_w as usize,
                        y as usize,
                        brush_h as usize,
                    );
                    let mut new_tiles = match &brush.tiles {
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
                            MapTileLayerPhysicsTiles::Game(tiles) => Self::collect_tiles(
                                tiles,
                                brush.w.get() as usize,
                                brush_x as usize,
                                brush_w as usize,
                                brush_y as usize,
                                brush_h as usize,
                            ),
                            MapTileLayerPhysicsTiles::Front(tiles) => Self::collect_tiles(
                                tiles,
                                brush.w.get() as usize,
                                brush_x as usize,
                                brush_w as usize,
                                brush_y as usize,
                                brush_h as usize,
                            ),
                            MapTileLayerPhysicsTiles::Tele(_) => todo!(),
                            MapTileLayerPhysicsTiles::Speedup(_) => todo!(),
                            MapTileLayerPhysicsTiles::Switch(_) => todo!(),
                            MapTileLayerPhysicsTiles::Tune(_) => todo!(),
                        },
                    };

                    // if non-destructive:
                    // for all tiles where the old tiles are non air, set new tiles to old
                    if !destructive {
                        Self::non_destructive_copy(&old_tiles, &mut new_tiles);
                    }
                    let actions = Self::design_brush_actions(
                        old_tiles,
                        new_tiles,
                        *is_background,
                        *group_index,
                        *layer_index,
                        x,
                        y,
                        brush_w,
                        brush_h,
                    );

                    (
                        actions,
                        format!("tile-brush {group_index}-{layer_index}-{is_background}"),
                        TileBrushLastApplyLayer::Design {
                            group_index: *group_index,
                            layer_index: *layer_index,
                            is_background: *is_background,
                        },
                    )
                }
            };

            let next_apply = TileBrushLastApply {
                x,
                y,
                w: brush_w,
                h: brush_h,
                layer: apply_layer,
            };
            let apply = brush.last_apply.get().is_none_or(|b| b != next_apply);
            if apply {
                brush.last_apply.set(Some(next_apply));
                client.execute_group(EditorActionGroup {
                    actions,
                    identifier: Some(format!("{group_indentifier}-{brush_id_counter}")),
                });
            }
        }
    }

    fn apply_brush_repeating_internal(
        &self,
        brush: &TileBrushTiles,
        map: &EditorMap,
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

            let mut repeating_assume_front_layer_created = false;

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
                            self.brush_id_counter,
                            map,
                            &layer,
                            brush,
                            client,
                            center.x + off_x,
                            center.y + off_y + y as i32,
                            brush_x,
                            brush_y,
                            brush_w,
                            1,
                            Some(&mut repeating_assume_front_layer_created),
                            self.destructive,
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
        latest_keys_down: &HashSet<egui::Key>,
        current_pointer_pos: &egui::Pos2,
        client: &mut EditorClient,
    ) {
        let layer = map.active_layer().unwrap();
        let (offset, parallax) = layer.get_offset_and_parallax();

        let brush = self.brush.as_ref().unwrap();

        let is_primary_allowed_down = !latest_modifiers.ctrl && latest_pointer.primary_down();
        let is_primary_allowed_pressed = !latest_modifiers.ctrl && latest_pointer.primary_pressed();

        // reset brush
        if latest_pointer.secondary_pressed() {
            self.brush = None;
            self.shift_pointer_down_world_pos = None;
        } else if (latest_modifiers.shift
            && (!is_primary_allowed_down || is_primary_allowed_pressed))
            || self.shift_pointer_down_world_pos.is_some()
        {
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
                    map.groups.user.parallax_aware_zoom,
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

                if !is_primary_allowed_down {
                    self.apply_brush_repeating_internal(
                        brush,
                        map,
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
            } else if is_primary_allowed_pressed {
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
                self.shift_pointer_down_world_pos = Some(TileBrushDownPos {
                    world: pos,
                    ui: *current_pointer_pos,
                });
            }
        }
        // fill tool
        else if latest_keys_down.contains(&egui::Key::B)
            && Self::brush_tiles_match_layer(&brush.tiles, &layer)
            && is_primary_allowed_pressed
        {
            let (w, h) = layer.get_width_and_height();
            let pos = current_pointer_pos;

            let pos = vec2::new(pos.x, pos.y);

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
                map.groups.user.parallax_aware_zoom,
            );
            let pos = usvec2::new(
                pos_on_map
                    .x
                    .clamp(0.0, (w.get() - brush.w.get()) as f32 * TILE_VISUAL_SIZE)
                    as u16,
                pos_on_map
                    .y
                    .clamp(0.0, (h.get() - brush.h.get()) as f32 * TILE_VISUAL_SIZE)
                    as u16,
            );
            let (fill_tiles, is_background, group_index, layer_index) = match layer {
                EditorLayerUnionRef::Physics {
                    layer, layer_index, ..
                } => (
                    match layer {
                        EditorPhysicsLayer::Arbitrary(_) => {
                            panic!("not supported.")
                        }
                        EditorPhysicsLayer::Game(layer) => {
                            let MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Game(
                                brush_tiles,
                            )) = &brush.tiles
                            else {
                                panic!("Was checked beforehand, else code bug.")
                            };
                            Self::fill_tiles(
                                &layer.layer.tiles,
                                brush_tiles,
                                w,
                                h,
                                &pos,
                                false,
                                |tiles| {
                                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Game(
                                        tiles,
                                    ))
                                },
                            )
                        }
                        EditorPhysicsLayer::Front(layer) => {
                            let MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Front(
                                brush_tiles,
                            )) = &brush.tiles
                            else {
                                panic!("Was checked beforehand, else code bug.")
                            };
                            Self::fill_tiles(
                                &layer.layer.tiles,
                                brush_tiles,
                                w,
                                h,
                                &pos,
                                false,
                                |tiles| {
                                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Front(
                                        tiles,
                                    ))
                                },
                            )
                        }
                        EditorPhysicsLayer::Tele(layer) => {
                            let MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tele(
                                brush_tiles,
                            )) = &brush.tiles
                            else {
                                panic!("Was checked beforehand, else code bug.")
                            };
                            Self::fill_tiles(
                                &layer.layer.base.tiles,
                                brush_tiles,
                                w,
                                h,
                                &pos,
                                false,
                                |tiles| {
                                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tele(
                                        tiles,
                                    ))
                                },
                            )
                        }
                        EditorPhysicsLayer::Speedup(layer) => {
                            let MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Speedup(
                                brush_tiles,
                            )) = &brush.tiles
                            else {
                                panic!("Was checked beforehand, else code bug.")
                            };
                            Self::fill_tiles(
                                &layer.layer.tiles,
                                brush_tiles,
                                w,
                                h,
                                &pos,
                                false,
                                |tiles| {
                                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Speedup(
                                        tiles,
                                    ))
                                },
                            )
                        }
                        EditorPhysicsLayer::Switch(layer) => {
                            let MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Switch(
                                brush_tiles,
                            )) = &brush.tiles
                            else {
                                panic!("Was checked beforehand, else code bug.")
                            };
                            Self::fill_tiles(
                                &layer.layer.base.tiles,
                                brush_tiles,
                                w,
                                h,
                                &pos,
                                false,
                                |tiles| {
                                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Switch(
                                        tiles,
                                    ))
                                },
                            )
                        }
                        EditorPhysicsLayer::Tune(layer) => {
                            let MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tune(
                                brush_tiles,
                            )) = &brush.tiles
                            else {
                                panic!("Was checked beforehand, else code bug.")
                            };
                            Self::fill_tiles(
                                &layer.layer.base.tiles,
                                brush_tiles,
                                w,
                                h,
                                &pos,
                                false,
                                |tiles| {
                                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tune(
                                        tiles,
                                    ))
                                },
                            )
                        }
                    },
                    false,
                    0,
                    layer_index,
                ),
                EditorLayerUnionRef::Design {
                    layer: EditorLayer::Tile(layer),
                    is_background,
                    group_index,
                    layer_index,
                    ..
                } => {
                    let MapTileLayerTiles::Design(brush_tiles) = &brush.tiles else {
                        panic!("Was checked beforehand, else code bug.")
                    };
                    (
                        Self::fill_tiles(
                            &layer.layer.tiles,
                            brush_tiles,
                            w,
                            h,
                            &pos,
                            false,
                            MapTileLayerTiles::Design,
                        ),
                        is_background,
                        group_index,
                        layer_index,
                    )
                }
                _ => {
                    panic!("Not a tile layer. code bug.");
                }
            };

            let (old_tiles, tiles, pos, w, h) = fill_tiles;
            let actions = match tiles {
                MapTileLayerTiles::Design(tiles) => {
                    let MapTileLayerTiles::Design(old_tiles) = old_tiles else {
                        panic!("Expected phy tiles. Code bug.")
                    };
                    Self::design_brush_actions(
                        old_tiles,
                        tiles,
                        is_background,
                        group_index,
                        layer_index,
                        pos.x,
                        pos.y,
                        w.get(),
                        h.get(),
                    )
                }
                MapTileLayerTiles::Physics(tiles) => {
                    let MapTileLayerTiles::Physics(old_tiles) = old_tiles else {
                        panic!("Expected phy tiles. Code bug.")
                    };
                    Self::phy_brush_actions(
                        map,
                        &map.groups.physics.attr,
                        old_tiles,
                        tiles,
                        pos.x,
                        w.get(),
                        pos.y,
                        h.get(),
                        layer_index,
                        None,
                    )
                }
            };

            client.execute_group(EditorActionGroup {
                actions,
                identifier: None,
            });
        }
        // apply brush
        else if !latest_keys_down.contains(&egui::Key::B) && is_primary_allowed_down {
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
                map.groups.user.parallax_aware_zoom,
            );

            let x = (x / TILE_VISUAL_SIZE).floor() as i32;
            let y = (y / TILE_VISUAL_SIZE).floor() as i32;

            let x = x - brush.negative_offset.x as i32;
            let y = y - brush.negative_offset.y as i32;

            Self::apply_brush_internal(
                self.brush_id_counter,
                map,
                &layer,
                brush,
                client,
                x,
                y,
                0,
                0,
                brush.w.get(),
                brush.h.get(),
                None,
                self.destructive,
            );
        }
    }

    fn render_selection(
        &mut self,
        ui_canvas: &UiCanvasSize,
        tp: &Arc<rayon::ThreadPool>,
        graphics_mt: &GraphicsMultiThreaded,
        backend_handle: &GraphicsBackendHandle,
        shader_storage_handle: &GraphicsShaderStorageHandle,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        canvas_handle: &GraphicsCanvasHandle,
        stream_handle: &GraphicsStreamHandle,
        map: &EditorMap,
        latest_keys_down: &HashSet<egui::Key>,
        latest_pointer: &egui::PointerState,
        latest_modifiers: &egui::Modifiers,
        current_pointer_pos: &egui::Pos2,
        entities_container: &mut EntitiesContainer,
        fake_texture_2d_array: &TextureContainer2dArray,
    ) {
        let is_primary_allowed_down = !latest_modifiers.ctrl && latest_pointer.primary_down();
        // if pointer was already down
        if self.pointer_down_world_pos.is_some() && is_primary_allowed_down && self.brush.is_some()
        {
            self.render_brush(
                ui_canvas,
                tp,
                graphics_mt,
                backend_handle,
                shader_storage_handle,
                buffer_object_handle,
                canvas_handle,
                stream_handle,
                map,
                latest_keys_down,
                current_pointer_pos,
                entities_container,
                fake_texture_2d_array,
                true,
            );
        }
    }

    fn render_brush_repeating_internal(
        &self,
        brush: &TileBrushTiles,
        map: &EditorMap,
        design_attr: Option<&MapTileLayerAttr>,
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
                    obj:
                        TileLayerBufferedVisualObjects {
                            shader_storage: Some(shader_storage),
                            ..
                        },
                },
            ..
        })
        | BrushVisual::Physics(PhysicsTileLayerVisuals {
            base:
                TileLayerVisuals {
                    base:
                        TileLayerBufferedVisuals {
                            base,
                            obj:
                                TileLayerBufferedVisualObjects {
                                    shader_storage: Some(shader_storage),
                                    ..
                                },
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
                        map.game_camera()
                            .project(canvas_handle, &mut state, group_attr.as_ref());
                        state.canvas_br.x += center.x - pos_x;
                        state.canvas_br.y += center.y - pos_y;
                        state.canvas_tl.x += center.x - pos_x;
                        state.canvas_tl.y += center.y - pos_y;
                        brush.map_render.render_tile_layer(
                            &state,
                            (&brush.texture).into(),
                            shader_storage,
                            &get_animated_color(map, design_attr),
                            PoolVec::from_without_pool(vec![TileLayerDrawInfo {
                                quad_offset,
                                quad_count: draw_count,
                                pos_y: brush_y as f32,
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
        design_attr: Option<&MapTileLayerAttr>,
        canvas_handle: &GraphicsCanvasHandle,
        center: vec2,
        group_attr: Option<MapGroupAttr>,
    ) {
        if let BrushVisual::Design(TileLayerVisuals {
            base:
                TileLayerBufferedVisuals {
                    obj:
                        TileLayerBufferedVisualObjects {
                            shader_storage: Some(shader_storage),
                            ..
                        },
                    ..
                },
            ..
        })
        | BrushVisual::Physics(PhysicsTileLayerVisuals {
            base:
                TileLayerVisuals {
                    base:
                        TileLayerBufferedVisuals {
                            obj:
                                TileLayerBufferedVisualObjects {
                                    shader_storage: Some(shader_storage),
                                    ..
                                },
                            ..
                        },
                    ..
                },
            ..
        }) = &brush.render
        {
            let mut state = State::new();
            map.game_camera()
                .project(canvas_handle, &mut state, group_attr.as_ref());
            state.canvas_br.x += center.x;
            state.canvas_br.y += center.y;
            state.canvas_tl.x += center.x;
            state.canvas_tl.y += center.y;
            brush.map_render.render_tile_layer(
                &state,
                (&brush.texture).into(),
                shader_storage,
                &get_animated_color(map, design_attr),
                PoolVec::from_without_pool(
                    (0..brush.h.get() as usize)
                        .map(|y| TileLayerDrawInfo {
                            quad_offset: y * brush.w.get() as usize,
                            quad_count: brush.w.get() as usize,
                            pos_y: y as f32,
                        })
                        .collect(),
                ),
            );
        }
    }

    pub fn selection_size(old: &vec2, cur: &vec2) -> (vec2, vec2, vec2, u16, u16) {
        let pos_old = vec2::new(
            (old.x / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE,
            (old.y / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE,
        );
        let pos_cur = vec2::new(
            (cur.x / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE,
            (cur.y / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE,
        );
        let width = (pos_cur.x - pos_old.x).abs() as u16 + 1;
        let height = (pos_cur.y - pos_old.y).abs() as u16 + 1;
        let pos_min = vec2::new(pos_cur.x.min(pos_old.x), pos_cur.y.min(pos_old.y));

        (pos_min, pos_old, pos_cur, width, height)
    }

    pub fn pos_on_map(
        map: &EditorMap,
        ui_canvas: &UiCanvasSize,
        canvas_handle: &GraphicsCanvasHandle,
        pos: &egui::Pos2,
        offset: &vec2,
        parallax: &vec2,
    ) -> vec2 {
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
            map.groups.user.parallax_aware_zoom,
        );
        vec2::new(
            (pos_on_map.x / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE,
            (pos_on_map.y / TILE_VISUAL_SIZE).floor() * TILE_VISUAL_SIZE,
        )
    }

    fn render_brush(
        &mut self,
        ui_canvas: &UiCanvasSize,
        tp: &Arc<rayon::ThreadPool>,
        graphics_mt: &GraphicsMultiThreaded,
        backend_handle: &GraphicsBackendHandle,
        shader_storage_handle: &GraphicsShaderStorageHandle,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        canvas_handle: &GraphicsCanvasHandle,
        stream_handle: &GraphicsStreamHandle,
        map: &EditorMap,
        latest_keys_down: &HashSet<egui::Key>,
        current_pointer_pos: &egui::Pos2,
        entities_container: &mut EntitiesContainer,
        fake_texture_2d_array: &TextureContainer2dArray,
        clamp_pos: bool,
    ) {
        let layer = map.active_layer().unwrap();
        let (offset, parallax) = layer.get_offset_and_parallax();
        let design_attr = if let EditorLayerUnionRef::Design {
            layer: EditorLayer::Tile(layer),
            ..
        } = layer
        {
            Some(&layer.layer.attr)
        } else {
            None
        };

        let brush = self.brush.as_ref().unwrap();

        let pos = current_pointer_pos;

        let pos_on_map = Self::pos_on_map(map, ui_canvas, canvas_handle, pos, &offset, &parallax);
        let pos = pos_on_map;
        let mut pos = vec2::new(
            pos.x - brush.negative_offset.x as f32 * TILE_VISUAL_SIZE,
            pos.y - brush.negative_offset.y as f32 * TILE_VISUAL_SIZE,
        );
        if clamp_pos {
            let (w, h) = layer.get_width_and_height();
            pos = vec2::new(
                pos.x
                    .clamp(0.0, (w.get() - brush.w.get()) as f32 * TILE_VISUAL_SIZE),
                pos.y
                    .clamp(0.0, (h.get() - brush.h.get()) as f32 * TILE_VISUAL_SIZE),
            );
        }
        let pos = egui::pos2(pos.x, pos.y);

        let brush_size = vec2::new(brush.w.get() as f32, brush.h.get() as f32) * TILE_VISUAL_SIZE;
        let rect =
            egui::Rect::from_min_max(pos, egui::pos2(pos.x + brush_size.x, pos.y + brush_size.y));

        if let Some(TileBrushDownPos { world, .. }) = &self.shift_pointer_down_world_pos {
            let (pos_min, pos_old, pos_cur, width, height) =
                Self::selection_size(world, &pos_on_map);

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
                design_attr,
                canvas_handle,
                -vec2::new(pos_min.x, pos_min.y),
                Some(layer.get_or_fake_group_attr()),
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
        } else if latest_keys_down.contains(&egui::Key::B)
            && brush.w.get() == 1
            && brush.h.get() == 1
            && Self::brush_tiles_match_layer(&brush.tiles, &layer)
        {
            // render fill of given tile
            let (w, h) = layer.get_width_and_height();
            let pos = usvec2::new(
                pos_on_map
                    .x
                    .clamp(0.0, (w.get() - brush.w.get()) as f32 * TILE_VISUAL_SIZE)
                    as u16,
                pos_on_map
                    .y
                    .clamp(0.0, (h.get() - brush.h.get()) as f32 * TILE_VISUAL_SIZE)
                    as u16,
            );

            fn last_fill_tiles<T: AsRef<TileBase> + Copy + Default + Eq>(
                tiles: &[T],
                brush_tiles: &[T],
                w: NonZeroU16MinusOne,
                h: NonZeroU16MinusOne,
                pos: &usvec2,
                tp: &Arc<rayon::ThreadPool>,
                graphics_mt: &GraphicsMultiThreaded,
                shader_storage_handle: &GraphicsShaderStorageHandle,
                buffer_object_handle: &GraphicsBufferObjectHandle,
                backend_handle: &GraphicsBackendHandle,
                last_fill: &mut Option<TileBrushLastFill>,
                texture: &TextureContainer2dArray,
                to_tiles: impl Fn(Vec<T>) -> MapTileLayerTiles,
            ) {
                let brush_tiles_layer = to_tiles(brush_tiles.to_vec());
                if last_fill.as_ref().is_some_and(|last_fill| {
                    last_fill.pointer_pos == *pos && last_fill.brush_tiles == brush_tiles_layer
                }) {
                    return;
                }
                let (_, tiles, min, width, height) =
                    TileBrush::fill_tiles(tiles, brush_tiles, w, h, pos, true, to_tiles);
                *last_fill = Some(TileBrushLastFill {
                    x: min.x,
                    y: min.y,
                    brush_tiles: brush_tiles_layer,
                    pointer_pos: *pos,
                    render: TileBrushTiles {
                        tiles: tiles.clone(),
                        w: width,
                        h: height,
                        negative_offset: Default::default(),
                        negative_offsetf: Default::default(),
                        render: TileBrush::create_brush_visual(
                            tp,
                            graphics_mt,
                            shader_storage_handle,
                            buffer_object_handle,
                            backend_handle,
                            width,
                            height,
                            &tiles,
                        ),
                        map_render: MapGraphics::new(backend_handle),
                        texture: texture.clone(),
                        last_apply: Default::default(),
                    },
                });
            }
            match layer {
                EditorLayerUnionRef::Physics { layer, .. } => {
                    let ent = entities_container.get_or_default(&ContainerKey::default());
                    let ent = ent.get_or_default("ddnet");
                    match layer {
                        EditorPhysicsLayer::Arbitrary(_) => {
                            panic!("not supported.")
                        }
                        EditorPhysicsLayer::Game(layer) => {
                            let MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Game(
                                brush_tiles,
                            )) = &brush.tiles
                            else {
                                panic!("Was checked beforehand, else code bug.")
                            };
                            last_fill_tiles(
                                &layer.layer.tiles,
                                brush_tiles,
                                w,
                                h,
                                &pos,
                                tp,
                                graphics_mt,
                                shader_storage_handle,
                                buffer_object_handle,
                                backend_handle,
                                &mut self.fill,
                                ent,
                                |tiles| {
                                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Game(
                                        tiles,
                                    ))
                                },
                            );
                        }
                        EditorPhysicsLayer::Front(layer) => {
                            let MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Front(
                                brush_tiles,
                            )) = &brush.tiles
                            else {
                                panic!("Was checked beforehand, else code bug.")
                            };
                            last_fill_tiles(
                                &layer.layer.tiles,
                                brush_tiles,
                                w,
                                h,
                                &pos,
                                tp,
                                graphics_mt,
                                shader_storage_handle,
                                buffer_object_handle,
                                backend_handle,
                                &mut self.fill,
                                ent,
                                |tiles| {
                                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Front(
                                        tiles,
                                    ))
                                },
                            );
                        }
                        EditorPhysicsLayer::Tele(layer) => {
                            let MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tele(
                                brush_tiles,
                            )) = &brush.tiles
                            else {
                                panic!("Was checked beforehand, else code bug.")
                            };
                            last_fill_tiles(
                                &layer.layer.base.tiles,
                                brush_tiles,
                                w,
                                h,
                                &pos,
                                tp,
                                graphics_mt,
                                shader_storage_handle,
                                buffer_object_handle,
                                backend_handle,
                                &mut self.fill,
                                ent,
                                |tiles| {
                                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tele(
                                        tiles,
                                    ))
                                },
                            );
                        }
                        EditorPhysicsLayer::Speedup(layer) => {
                            let MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Speedup(
                                brush_tiles,
                            )) = &brush.tiles
                            else {
                                panic!("Was checked beforehand, else code bug.")
                            };
                            last_fill_tiles(
                                &layer.layer.tiles,
                                brush_tiles,
                                w,
                                h,
                                &pos,
                                tp,
                                graphics_mt,
                                shader_storage_handle,
                                buffer_object_handle,
                                backend_handle,
                                &mut self.fill,
                                ent,
                                |tiles| {
                                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Speedup(
                                        tiles,
                                    ))
                                },
                            );
                        }
                        EditorPhysicsLayer::Switch(layer) => {
                            let MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Switch(
                                brush_tiles,
                            )) = &brush.tiles
                            else {
                                panic!("Was checked beforehand, else code bug.")
                            };
                            last_fill_tiles(
                                &layer.layer.base.tiles,
                                brush_tiles,
                                w,
                                h,
                                &pos,
                                tp,
                                graphics_mt,
                                shader_storage_handle,
                                buffer_object_handle,
                                backend_handle,
                                &mut self.fill,
                                ent,
                                |tiles| {
                                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Switch(
                                        tiles,
                                    ))
                                },
                            );
                        }
                        EditorPhysicsLayer::Tune(layer) => {
                            let MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tune(
                                brush_tiles,
                            )) = &brush.tiles
                            else {
                                panic!("Was checked beforehand, else code bug.")
                            };
                            last_fill_tiles(
                                &layer.layer.base.tiles,
                                brush_tiles,
                                w,
                                h,
                                &pos,
                                tp,
                                graphics_mt,
                                shader_storage_handle,
                                buffer_object_handle,
                                backend_handle,
                                &mut self.fill,
                                ent,
                                |tiles| {
                                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tune(
                                        tiles,
                                    ))
                                },
                            );
                        }
                    }
                }
                EditorLayerUnionRef::Design {
                    layer: EditorLayer::Tile(layer),
                    ..
                } => {
                    let MapTileLayerTiles::Design(brush_tiles) = &brush.tiles else {
                        panic!("Was checked beforehand, else code bug.")
                    };
                    let tex = layer
                        .layer
                        .attr
                        .image_array
                        .and_then(|i| map.resources.image_arrays.get(i).map(|i| &i.user.user))
                        .unwrap_or(fake_texture_2d_array);
                    last_fill_tiles(
                        &layer.layer.tiles,
                        brush_tiles,
                        w,
                        h,
                        &pos,
                        tp,
                        graphics_mt,
                        shader_storage_handle,
                        buffer_object_handle,
                        backend_handle,
                        &mut self.fill,
                        tex,
                        MapTileLayerTiles::Design,
                    );
                }
                _ => {
                    panic!("Not a tile layer. code bug.");
                }
            }

            if let Some(fill) = self.fill.as_ref() {
                self.render_brush_internal(
                    &fill.render,
                    map,
                    design_attr,
                    canvas_handle,
                    -vec2::new(fill.x as f32, fill.y as f32),
                    Some(layer.get_or_fake_group_attr()),
                );
            }
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
            // render blur during selection phase, to make the selection clear to the user.
            if let Some(TileBrushDown { shift, .. }) = self.pointer_down_world_pos {
                render_blur(
                    backend_handle,
                    stream_handle,
                    canvas_handle,
                    true,
                    DEFAULT_BLUR_RADIUS,
                    DEFAULT_BLUR_MIX_LENGTH,
                    &if shift {
                        vec4::new(1.0, 0.0, 0.0, 25.0 / 255.0)
                    } else {
                        vec4::new(1.0, 1.0, 1.0, 1.0 / 255.0)
                    },
                );
            }
            render_swapped_frame(canvas_handle, stream_handle);

            self.render_brush_internal(
                brush,
                map,
                design_attr,
                canvas_handle,
                -vec2::new(pos.x, pos.y),
                Some(layer.get_or_fake_group_attr()),
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
        shader_storage_handle: &GraphicsShaderStorageHandle,
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
                shader_storage_handle,
                buffer_object_handle,
                backend_handle,
                canvas_handle,
                entities_container,
                fake_texture_2d_array,
                map,
                latest_pointer,
                latest_modifiers,
                latest_keys_down,
                current_pointer_pos,
                available_rect,
                client,
            );
        } else {
            self.handle_brush_draw(
                ui_canvas,
                canvas_handle,
                map,
                latest_pointer,
                latest_modifiers,
                latest_keys_down,
                current_pointer_pos,
                client,
            );
        }
    }

    pub fn render(
        &mut self,
        ui_canvas: &UiCanvasSize,
        tp: &Arc<rayon::ThreadPool>,
        graphics_mt: &GraphicsMultiThreaded,
        backend_handle: &GraphicsBackendHandle,
        shader_storage_handle: &GraphicsShaderStorageHandle,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        stream_handle: &GraphicsStreamHandle,
        canvas_handle: &GraphicsCanvasHandle,
        entities_container: &mut EntitiesContainer,
        fake_texture_2d_array: &TextureContainer2dArray,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        latest_modifiers: &egui::Modifiers,
        latest_keys_down: &HashSet<egui::Key>,
        current_pointer_pos: &egui::Pos2,
        available_rect: &egui::Rect,
    ) {
        let layer = map.active_layer();
        let design_attr = if let Some(EditorLayerUnionRef::Design {
            layer: EditorLayer::Tile(layer),
            ..
        }) = layer
        {
            Some(&layer.layer.attr)
        } else if let Some(EditorLayerUnionRef::Physics { .. }) = layer {
            None
        } else {
            return;
        };

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
            state.map_canvas(0.0, 0.0, ui_canvas.width(), ui_canvas.height());
            render_checkerboard_background(stream_handle, render_rect, &state);

            state.map_canvas(
                tl_x,
                tl_y,
                tl_x + ui_canvas.width() * size_ratio_x,
                tl_y + ui_canvas.height() * size_ratio_y,
            );
            let texture = match layer.as_ref().unwrap() {
                EditorLayerUnionRef::Physics { layer, .. } => match layer {
                    EditorPhysicsLayer::Arbitrary(_) | EditorPhysicsLayer::Game(_) => {
                        &self.tile_picker.physics_overlay.game.texture
                    }
                    EditorPhysicsLayer::Front(_) => &self.tile_picker.physics_overlay.front.texture,
                    EditorPhysicsLayer::Tele(_) => &self.tile_picker.physics_overlay.tele.texture,
                    EditorPhysicsLayer::Speedup(_) => {
                        &self.tile_picker.physics_overlay.speedup.texture
                    }
                    EditorPhysicsLayer::Switch(_) => {
                        &self.tile_picker.physics_overlay.switch.texture
                    }
                    EditorPhysicsLayer::Tune(_) => &self.tile_picker.physics_overlay.tune.texture,
                },
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
            let color = get_animated_color(map, design_attr);
            let shader_storage = self
                .tile_picker
                .render
                .base
                .obj
                .shader_storage
                .as_ref()
                .unwrap();
            self.tile_picker.map_render.render_tile_layer(
                &state,
                texture.into(),
                shader_storage,
                &color,
                PoolVec::from_without_pool(
                    (0..16)
                        .map(|y| TileLayerDrawInfo {
                            quad_offset: y * 16,
                            quad_count: 16,
                            pos_y: y as f32,
                        })
                        .collect(),
                ),
            );

            if let Some(shader_storage) = map
                .user
                .options
                .show_tile_numbers
                .then_some(
                    self.tile_picker
                        .render
                        .tile_index_obj
                        .shader_storage
                        .as_ref(),
                )
                .flatten()
            {
                self.tile_picker.map_render.render_tile_layer(
                    &state,
                    (&entities_container
                        .get_or_default::<ContainerKey>(&"default".try_into().unwrap())
                        .text_overlay_bottom)
                        .into(),
                    shader_storage,
                    &color,
                    PoolVec::from_without_pool(
                        (0..16)
                            .map(|y| TileLayerDrawInfo {
                                quad_offset: y * 16,
                                quad_count: 16,
                                pos_y: y as f32,
                            })
                            .collect(),
                    ),
                );
            }
            // render rect border
            state.map_canvas(0.0, 0.0, ui_canvas.width(), ui_canvas.height());

            render_rect_from_state(
                stream_handle,
                state,
                render_rect,
                ubvec4::new(0, 0, 255, 255),
            );

            if let Some(TileBrushDown {
                pos: TileBrushDownPos { ui, .. },
                ..
            }) = &self.pointer_down_world_pos
            {
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
                tp,
                graphics_mt,
                backend_handle,
                shader_storage_handle,
                buffer_object_handle,
                canvas_handle,
                stream_handle,
                map,
                latest_keys_down,
                latest_pointer,
                latest_modifiers,
                current_pointer_pos,
                entities_container,
                fake_texture_2d_array,
            );
        } else {
            self.render_brush(
                ui_canvas,
                tp,
                graphics_mt,
                backend_handle,
                shader_storage_handle,
                buffer_object_handle,
                canvas_handle,
                stream_handle,
                map,
                latest_keys_down,
                current_pointer_pos,
                entities_container,
                fake_texture_2d_array,
                false,
            );
        }
    }
}
