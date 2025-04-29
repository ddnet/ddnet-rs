use std::sync::Arc;

use game_base::mapdef_06::tile_can_rotate;
use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::{
        backend::backend::GraphicsBackendHandle,
        buffer_object::buffer_object::GraphicsBufferObjectHandle,
        shader_storage::shader_storage::GraphicsShaderStorageHandle,
    },
};
use map::map::groups::layers::tiles::{
    rotate_by_plus_90, MapTileLayerPhysicsTiles, MapTileLayerTiles, TileBase, TileFlags,
};
use math::math::vector::{dvec2, usvec2};
use rayon::{
    iter::{
        IndexedParallelIterator, IntoParallelRefIterator, IntoParallelRefMutIterator,
        ParallelIterator,
    },
    slice::{ParallelSlice, ParallelSliceMut},
};

use crate::{
    actions::actions::{
        ActTileLayerReplTilesBase, ActTileLayerReplaceTiles, ActTilePhysicsLayerReplTilesBase,
        ActTilePhysicsLayerReplaceTiles, EditorAction,
    },
    client::EditorClient,
    map::{EditorLayer, EditorLayerUnionRef, EditorPhysicsLayer},
    tools::tile_layer::{
        brush::{TileBrush, TileBrushTiles},
        selection::TileSelectionRange,
    },
};

fn mirror_y_tiles<T: Copy + Clone + Sync + Send + AsMut<TileBase>>(
    tp: &Arc<rayon::ThreadPool>,
    w: usize,
    tiles: &mut Vec<T>,
) where
    Vec<T>: rayon::iter::IntoParallelIterator,
    Vec<T>: rayon::iter::FromParallelIterator<<Vec<T> as rayon::iter::IntoParallelIterator>::Item>,
{
    let new_tiles = tp.install(|| {
        let mut new_tiles: Vec<T> = tiles
            .par_chunks_exact(w)
            .rev()
            .flat_map(|chunk| chunk.to_vec())
            .collect();
        new_tiles
            .par_iter_mut()
            .for_each(|tile| tile.as_mut().flags.toggle(TileFlags::YFLIP));
        new_tiles
    });
    *tiles = new_tiles;
}
pub fn mirror_tiles_y(
    tp: &Arc<rayon::ThreadPool>,
    graphics_mt: &GraphicsMultiThreaded,
    shader_storage_handle: &GraphicsShaderStorageHandle,
    buffer_object_handle: &GraphicsBufferObjectHandle,
    backend_handle: &GraphicsBackendHandle,
    brush: &mut TileBrushTiles,
    upload_new_layer: bool,
) {
    match &mut brush.tiles {
        MapTileLayerTiles::Design(tiles) => {
            mirror_y_tiles(tp, brush.w.get() as usize, tiles);
        }
        MapTileLayerTiles::Physics(ty) => match ty {
            MapTileLayerPhysicsTiles::Arbitrary(_) => panic!("not implemented"),
            MapTileLayerPhysicsTiles::Game(tiles) | MapTileLayerPhysicsTiles::Front(tiles) => {
                mirror_y_tiles(tp, brush.w.get() as usize, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
            MapTileLayerPhysicsTiles::Tele(tiles) => {
                mirror_y_tiles(tp, brush.w.get() as usize, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
            MapTileLayerPhysicsTiles::Speedup(tiles) => {
                mirror_y_tiles(tp, brush.w.get() as usize, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
            MapTileLayerPhysicsTiles::Switch(tiles) => {
                mirror_y_tiles(tp, brush.w.get() as usize, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
            MapTileLayerPhysicsTiles::Tune(tiles) => {
                mirror_y_tiles(tp, brush.w.get() as usize, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
        },
    }

    brush.last_apply.set(None);

    if upload_new_layer {
        upload_brush(
            tp,
            graphics_mt,
            shader_storage_handle,
            buffer_object_handle,
            backend_handle,
            brush,
        );
    }
}

fn mirror_x_tiles<T: Copy + Clone + Send + Sync + AsMut<TileBase>>(
    tp: &Arc<rayon::ThreadPool>,
    w: usize,
    tiles: &mut [T],
) {
    tp.install(|| {
        tiles
            .par_chunks_exact_mut(w)
            .for_each(|chunk| chunk.reverse());
        tiles
            .par_iter_mut()
            .for_each(|tile| tile.as_mut().flags.toggle(TileFlags::XFLIP));
    });
}

pub fn mirror_tiles_x(
    tp: &Arc<rayon::ThreadPool>,
    graphics_mt: &GraphicsMultiThreaded,
    shader_storage_handle: &GraphicsShaderStorageHandle,
    buffer_object_handle: &GraphicsBufferObjectHandle,
    backend_handle: &GraphicsBackendHandle,
    brush: &mut TileBrushTiles,
    upload_new_layer: bool,
) {
    match &mut brush.tiles {
        MapTileLayerTiles::Design(tiles) => {
            mirror_x_tiles(tp, brush.w.get() as usize, tiles);
        }
        MapTileLayerTiles::Physics(ty) => match ty {
            MapTileLayerPhysicsTiles::Arbitrary(_) => panic!("not implemented"),
            MapTileLayerPhysicsTiles::Game(tiles) | MapTileLayerPhysicsTiles::Front(tiles) => {
                mirror_x_tiles(tp, brush.w.get() as usize, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
            MapTileLayerPhysicsTiles::Tele(tiles) => {
                mirror_x_tiles(tp, brush.w.get() as usize, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
            MapTileLayerPhysicsTiles::Speedup(tiles) => {
                mirror_x_tiles(tp, brush.w.get() as usize, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
            MapTileLayerPhysicsTiles::Switch(tiles) => {
                mirror_x_tiles(tp, brush.w.get() as usize, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
            MapTileLayerPhysicsTiles::Tune(tiles) => {
                mirror_x_tiles(tp, brush.w.get() as usize, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
        },
    }

    brush.last_apply.set(None);

    if upload_new_layer {
        upload_brush(
            tp,
            graphics_mt,
            shader_storage_handle,
            buffer_object_handle,
            backend_handle,
            brush,
        );
    }
}

pub fn rotate_tiles_plus_90(
    tp: &Arc<rayon::ThreadPool>,
    graphics_mt: &GraphicsMultiThreaded,
    shader_storage_handle: &GraphicsShaderStorageHandle,
    buffer_object_handle: &GraphicsBufferObjectHandle,
    backend_handle: &GraphicsBackendHandle,
    brush: &mut TileBrushTiles,
    upload_new_layer: bool,
) {
    fn rotate_tiles<T: Copy + Clone + Send + Sync + AsMut<TileBase>>(
        tp: &Arc<rayon::ThreadPool>,
        w: usize,
        tiles: &mut Vec<T>,
    ) {
        let h = tiles.len() / w;
        let mut new_tiles = tiles.clone();

        tp.install(|| {
            // transpose
            new_tiles
                .par_iter_mut()
                .enumerate()
                .for_each(|(index, tile)| {
                    let old_index = (index % h) * w + (index / h);
                    *tile = tiles[old_index];
                });
            // reverse
            new_tiles
                .par_chunks_exact_mut(h)
                .for_each(|chunk| chunk.reverse());

            new_tiles
                .par_iter_mut()
                .for_each(|tile| rotate_by_plus_90(&mut tile.as_mut().flags));
        });
        *tiles = new_tiles;
    }

    match &mut brush.tiles {
        MapTileLayerTiles::Design(tiles) => {
            rotate_tiles(tp, brush.w.get() as usize, tiles);
        }
        MapTileLayerTiles::Physics(ty) => match ty {
            MapTileLayerPhysicsTiles::Arbitrary(_) => panic!("not implemented"),
            MapTileLayerPhysicsTiles::Game(tiles) | MapTileLayerPhysicsTiles::Front(tiles) => {
                rotate_tiles(tp, brush.w.get() as usize, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
            MapTileLayerPhysicsTiles::Tele(tiles) => {
                rotate_tiles(tp, brush.w.get() as usize, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
            MapTileLayerPhysicsTiles::Speedup(tiles) => {
                rotate_tiles(tp, brush.w.get() as usize, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
            MapTileLayerPhysicsTiles::Switch(tiles) => {
                rotate_tiles(tp, brush.w.get() as usize, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
            MapTileLayerPhysicsTiles::Tune(tiles) => {
                rotate_tiles(tp, brush.w.get() as usize, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
        },
    }

    let off_x = brush.negative_offsetf.x;
    let off_y = brush.negative_offsetf.y;

    // transpose
    let new_y = off_x;
    let new_x = off_y;
    // flip x
    let new_x = ((brush.h.get() as f64) - new_x).clamp(0.0, f64::MAX);
    brush.negative_offset = usvec2::new(
        (new_x as u16).clamp(0, brush.h.get() - 1),
        (new_y as u16).clamp(0, brush.w.get() - 1),
    );
    brush.negative_offsetf = dvec2::new(new_x, new_y);
    std::mem::swap(&mut brush.w, &mut brush.h);

    brush.last_apply.set(None);

    if upload_new_layer {
        upload_brush(
            tp,
            graphics_mt,
            shader_storage_handle,
            buffer_object_handle,
            backend_handle,
            brush,
        );
    }
}

pub fn rotate_tile_flags_plus_90(
    tp: &Arc<rayon::ThreadPool>,
    graphics_mt: &GraphicsMultiThreaded,
    shader_storage_handle: &GraphicsShaderStorageHandle,
    buffer_object_handle: &GraphicsBufferObjectHandle,
    backend_handle: &GraphicsBackendHandle,
    brush: &mut TileBrushTiles,
    upload_new_layer: bool,
) {
    fn rotate_tiles<T: Copy + Clone + Send + Sync + AsMut<TileBase>>(
        tp: &Arc<rayon::ThreadPool>,
        tiles: &mut Vec<T>,
    ) {
        tp.install(|| {
            tiles
                .par_iter_mut()
                .for_each(|tile| rotate_by_plus_90(&mut tile.as_mut().flags));
        });
    }

    match &mut brush.tiles {
        MapTileLayerTiles::Design(tiles) => {
            rotate_tiles(tp, tiles);
        }
        MapTileLayerTiles::Physics(ty) => match ty {
            MapTileLayerPhysicsTiles::Arbitrary(_) => panic!("not implemented"),
            MapTileLayerPhysicsTiles::Game(tiles) | MapTileLayerPhysicsTiles::Front(tiles) => {
                rotate_tiles(tp, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
            MapTileLayerPhysicsTiles::Tele(tiles) => {
                rotate_tiles(tp, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
            MapTileLayerPhysicsTiles::Speedup(tiles) => {
                rotate_tiles(tp, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
            MapTileLayerPhysicsTiles::Switch(tiles) => {
                rotate_tiles(tp, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
            MapTileLayerPhysicsTiles::Tune(tiles) => {
                rotate_tiles(tp, tiles);
                ensure_tile_flags_valid(tp, tiles);
            }
        },
    }

    brush.last_apply.set(None);

    if upload_new_layer {
        upload_brush(
            tp,
            graphics_mt,
            shader_storage_handle,
            buffer_object_handle,
            backend_handle,
            brush,
        );
    }
}

fn upload_brush(
    tp: &Arc<rayon::ThreadPool>,
    graphics_mt: &GraphicsMultiThreaded,
    shader_storage_handle: &GraphicsShaderStorageHandle,
    buffer_object_handle: &GraphicsBufferObjectHandle,
    backend_handle: &GraphicsBackendHandle,
    brush: &mut TileBrushTiles,
) {
    brush.render = TileBrush::create_brush_visual(
        tp,
        graphics_mt,
        shader_storage_handle,
        buffer_object_handle,
        backend_handle,
        brush.w,
        brush.h,
        &brush.tiles,
    );
}

pub fn mirror_layer_tiles_y(
    tp: &Arc<rayon::ThreadPool>,
    layer: EditorLayerUnionRef,
    range: &TileSelectionRange,
    client: &mut EditorClient,
) {
    fn get_mirror_tiles<T: Copy + Clone + Send + Sync + AsMut<TileBase>>(
        tp: &Arc<rayon::ThreadPool>,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        full_width: usize,
        tiles: &[T],
    ) -> (Vec<T>, Vec<T>)
    where
        Vec<T>: rayon::iter::FromParallelIterator<T>,
    {
        tp.install(|| {
            let old_tiles: Vec<T> = tiles
                .par_chunks_exact(full_width)
                .skip(y)
                .take(h)
                .flat_map(|tiles| tiles.par_iter().skip(x).take(w).map(|tile| *tile))
                .collect();
            let mut new_tiles = old_tiles.clone();
            mirror_y_tiles(tp, w, &mut new_tiles);
            (old_tiles, new_tiles)
        })
    }

    let (old_tiles, new_tiles) = match &layer {
        EditorLayerUnionRef::Physics {
            layer, group_attr, ..
        } => match layer {
            EditorPhysicsLayer::Arbitrary(_) => panic!("not implemented"),
            EditorPhysicsLayer::Game(layer) => {
                let (old_tiles, mut new_tiles) = get_mirror_tiles(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Game(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Game(new_tiles)),
                )
            }
            EditorPhysicsLayer::Front(layer) => {
                let (old_tiles, mut new_tiles) = get_mirror_tiles(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Front(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Front(new_tiles)),
                )
            }
            EditorPhysicsLayer::Tele(layer) => {
                let (old_tiles, mut new_tiles) = get_mirror_tiles(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.base.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tele(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tele(new_tiles)),
                )
            }
            EditorPhysicsLayer::Speedup(layer) => {
                let (old_tiles, mut new_tiles) = get_mirror_tiles(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Speedup(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Speedup(new_tiles)),
                )
            }
            EditorPhysicsLayer::Switch(layer) => {
                let (old_tiles, mut new_tiles) = get_mirror_tiles(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.base.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Switch(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Switch(new_tiles)),
                )
            }
            EditorPhysicsLayer::Tune(layer) => {
                let (old_tiles, mut new_tiles) = get_mirror_tiles(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.base.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tune(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tune(new_tiles)),
                )
            }
        },
        EditorLayerUnionRef::Design { layer, .. } => {
            if let EditorLayer::Tile(layer) = layer {
                let (old_tiles, new_tiles) = get_mirror_tiles(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    layer.layer.attr.width.get() as usize,
                    &layer.layer.tiles,
                );
                (
                    MapTileLayerTiles::Design(old_tiles),
                    MapTileLayerTiles::Design(new_tiles),
                )
            } else {
                return;
            }
        }
    };
    generate_client_action(layer, range, client, old_tiles, new_tiles);
}

pub fn mirror_layer_tiles_x(
    tp: &Arc<rayon::ThreadPool>,
    layer: EditorLayerUnionRef,
    range: &TileSelectionRange,
    client: &mut EditorClient,
) {
    fn get_mirror_tiles<T: Copy + Clone + Send + Sync + AsMut<TileBase>>(
        tp: &Arc<rayon::ThreadPool>,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        full_width: usize,
        tiles: &[T],
    ) -> (Vec<T>, Vec<T>)
    where
        Vec<T>: rayon::iter::FromParallelIterator<T>,
    {
        tp.install(|| {
            let old_tiles: Vec<T> = tiles
                .par_chunks_exact(full_width)
                .skip(y)
                .take(h)
                .flat_map(|tiles| tiles.par_iter().skip(x).take(w).map(|tile| *tile))
                .collect();
            let mut new_tiles = old_tiles.clone();
            mirror_x_tiles(tp, w, &mut new_tiles);
            (old_tiles, new_tiles)
        })
    }

    let (old_tiles, new_tiles) = match &layer {
        EditorLayerUnionRef::Physics {
            layer, group_attr, ..
        } => match layer {
            EditorPhysicsLayer::Arbitrary(_) => panic!("not implemented"),
            EditorPhysicsLayer::Game(layer) => {
                let (old_tiles, mut new_tiles) = get_mirror_tiles(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Game(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Game(new_tiles)),
                )
            }
            EditorPhysicsLayer::Front(layer) => {
                let (old_tiles, mut new_tiles) = get_mirror_tiles(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Front(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Front(new_tiles)),
                )
            }
            EditorPhysicsLayer::Tele(layer) => {
                let (old_tiles, mut new_tiles) = get_mirror_tiles(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.base.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tele(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tele(new_tiles)),
                )
            }
            EditorPhysicsLayer::Speedup(layer) => {
                let (old_tiles, mut new_tiles) = get_mirror_tiles(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Speedup(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Speedup(new_tiles)),
                )
            }
            EditorPhysicsLayer::Switch(layer) => {
                let (old_tiles, mut new_tiles) = get_mirror_tiles(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.base.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Switch(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Switch(new_tiles)),
                )
            }
            EditorPhysicsLayer::Tune(layer) => {
                let (old_tiles, mut new_tiles) = get_mirror_tiles(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.base.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tune(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tune(new_tiles)),
                )
            }
        },
        EditorLayerUnionRef::Design { layer, .. } => {
            if let EditorLayer::Tile(layer) = layer {
                let (old_tiles, new_tiles) = get_mirror_tiles(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    layer.layer.attr.width.get() as usize,
                    &layer.layer.tiles,
                );
                (
                    MapTileLayerTiles::Design(old_tiles),
                    MapTileLayerTiles::Design(new_tiles),
                )
            } else {
                return;
            }
        }
    };
    generate_client_action(layer, range, client, old_tiles, new_tiles);
}

pub fn rotate_layer_tiles_plus_90(
    tp: &Arc<rayon::ThreadPool>,
    layer: EditorLayerUnionRef,
    range: &TileSelectionRange,
    client: &mut EditorClient,
) {
    fn rotate_tiles_flags<T: Copy + Clone + Send + Sync + AsMut<TileBase>>(
        tp: &Arc<rayon::ThreadPool>,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        full_width: usize,
        tiles: &[T],
    ) -> (Vec<T>, Vec<T>)
    where
        Vec<T>: rayon::iter::FromParallelIterator<T>,
    {
        tp.install(|| {
            let old_tiles: Vec<T> = tiles
                .par_chunks_exact(full_width)
                .skip(y)
                .take(h)
                .flat_map(|tiles| tiles.par_iter().skip(x).take(w).map(|tile| *tile))
                .collect();
            let mut new_tiles = old_tiles.clone();
            new_tiles
                .iter_mut()
                .for_each(|tile| rotate_by_plus_90(&mut tile.as_mut().flags));
            (old_tiles, new_tiles)
        })
    }

    let (old_tiles, new_tiles) = match &layer {
        EditorLayerUnionRef::Physics {
            layer, group_attr, ..
        } => match layer {
            EditorPhysicsLayer::Arbitrary(_) => panic!("not implemented"),
            EditorPhysicsLayer::Game(layer) => {
                let (old_tiles, mut new_tiles) = rotate_tiles_flags(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Game(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Game(new_tiles)),
                )
            }
            EditorPhysicsLayer::Front(layer) => {
                let (old_tiles, mut new_tiles) = rotate_tiles_flags(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Front(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Front(new_tiles)),
                )
            }
            EditorPhysicsLayer::Tele(layer) => {
                let (old_tiles, mut new_tiles) = rotate_tiles_flags(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.base.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tele(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tele(new_tiles)),
                )
            }
            EditorPhysicsLayer::Speedup(layer) => {
                let (old_tiles, mut new_tiles) = rotate_tiles_flags(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Speedup(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Speedup(new_tiles)),
                )
            }
            EditorPhysicsLayer::Switch(layer) => {
                let (old_tiles, mut new_tiles) = rotate_tiles_flags(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.base.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Switch(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Switch(new_tiles)),
                )
            }
            EditorPhysicsLayer::Tune(layer) => {
                let (old_tiles, mut new_tiles) = rotate_tiles_flags(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    group_attr.width.get() as usize,
                    &layer.layer.base.tiles,
                );
                ensure_tile_flags_valid(tp, &mut new_tiles);
                (
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tune(old_tiles)),
                    MapTileLayerTiles::Physics(MapTileLayerPhysicsTiles::Tune(new_tiles)),
                )
            }
        },
        EditorLayerUnionRef::Design { layer, .. } => {
            if let EditorLayer::Tile(layer) = layer {
                let (old_tiles, new_tiles) = rotate_tiles_flags(
                    tp,
                    range.x as usize,
                    range.y as usize,
                    range.w.get() as usize,
                    range.h.get() as usize,
                    layer.layer.attr.width.get() as usize,
                    &layer.layer.tiles,
                );
                (
                    MapTileLayerTiles::Design(old_tiles),
                    MapTileLayerTiles::Design(new_tiles),
                )
            } else {
                return;
            }
        }
    };
    generate_client_action(layer, range, client, old_tiles, new_tiles);
}

fn generate_client_action(
    layer: EditorLayerUnionRef,
    range: &TileSelectionRange,
    client: &mut EditorClient,
    old_tiles: MapTileLayerTiles,
    new_tiles: MapTileLayerTiles,
) {
    match layer {
        EditorLayerUnionRef::Physics { layer_index, .. } => {
            if let (MapTileLayerTiles::Physics(old_tiles), MapTileLayerTiles::Physics(new_tiles)) =
                (old_tiles, new_tiles)
            {
                client.execute(
                    EditorAction::TilePhysicsLayerReplaceTiles(ActTilePhysicsLayerReplaceTiles {
                        base: ActTilePhysicsLayerReplTilesBase {
                            layer_index,
                            old_tiles,
                            new_tiles,
                            x: range.x,
                            y: range.y,
                            w: range.w,
                            h: range.h,
                        },
                    }),
                    Some(&format!(
                        "selection_physics_tile_layer_tools_{}",
                        layer_index
                    )),
                );
            }
        }
        EditorLayerUnionRef::Design {
            layer,
            group_index,
            layer_index,
            is_background,
            ..
        } => {
            if let (
                EditorLayer::Tile(_),
                MapTileLayerTiles::Design(old_tiles),
                MapTileLayerTiles::Design(new_tiles),
            ) = (layer, old_tiles, new_tiles)
            {
                client.execute(
                    EditorAction::TileLayerReplaceTiles(ActTileLayerReplaceTiles {
                        base: ActTileLayerReplTilesBase {
                            is_background,
                            group_index,
                            layer_index,
                            old_tiles,
                            new_tiles,
                            x: range.x,
                            y: range.y,
                            w: range.w,
                            h: range.h,
                        },
                    }),
                    Some(&format!(
                        "selection_tile_layer_tools_{}_{}_{}",
                        is_background, group_index, layer_index
                    )),
                );
            }
        }
    }
}

fn ensure_tile_flags_valid<T: Copy + Clone + Send + Sync + AsMut<TileBase>>(
    tp: &Arc<rayon::ThreadPool>,
    tiles: &mut [T],
) {
    tp.install(|| {
        tiles.par_iter_mut().for_each(|tile| {
            let tile = tile.as_mut();
            if !tile_can_rotate(tile.index) {
                if tile.flags.contains(TileFlags::ROTATE) {
                    tile.flags ^= TileFlags::ROTATE;
                }
                if tile.flags.contains(TileFlags::XFLIP) {
                    tile.flags ^= TileFlags::XFLIP;
                }
                if tile.flags.contains(TileFlags::YFLIP) {
                    tile.flags ^= TileFlags::YFLIP;
                }
            }
        });
    });
}
