use std::{ops::Range, sync::Arc};

use client_render_base::map::map_buffered::{
    ClientMapBufferQuadLayer, ClientMapBuffered, MapBufferPhysicsTileLayer, MapBufferTileLayer,
    PhysicsTileLayerVisuals, QuadLayerVisuals, TileLayerVisuals,
};
use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::{
        backend::backend::GraphicsBackendHandle,
        buffer_object::buffer_object::GraphicsBufferObjectHandle,
        shader_storage::shader_storage::GraphicsShaderStorageHandle,
    },
};
use map::{
    map::groups::layers::{
        design::{MapLayerQuadsAttrs, Quad},
        tiles::{MapTileLayerPhysicsTilesRef, TileBase},
    },
    types::NonZeroU16MinusOne,
};

use crate::map::{EditorLayerQuad, EditorLayerTile, EditorPhysicsLayer};

pub fn upload_physics_layer_buffer(
    graphics_mt: &GraphicsMultiThreaded,
    width: NonZeroU16MinusOne,
    height: NonZeroU16MinusOne,
    tiles: MapTileLayerPhysicsTilesRef,
) -> MapBufferPhysicsTileLayer {
    ClientMapBuffered::upload_physics_layer(graphics_mt, width, height, tiles, 0, true, true)
}

pub fn finish_physics_layer_buffer(
    shader_storage_handle: &GraphicsShaderStorageHandle,
    buffer_object_handle: &GraphicsBufferObjectHandle,
    backend_handle: &GraphicsBackendHandle,
    buffer: MapBufferPhysicsTileLayer,
) -> PhysicsTileLayerVisuals {
    ClientMapBuffered::finish_upload_physics_tile_layer(
        shader_storage_handle,
        buffer_object_handle,
        backend_handle,
        buffer,
    )
}

pub fn update_physics_layer(
    tp: &Arc<rayon::ThreadPool>,
    group_width: NonZeroU16MinusOne,
    group_height: NonZeroU16MinusOne,
    layer: &mut EditorPhysicsLayer,
    x: u16,
    y: u16,
    width: NonZeroU16MinusOne,
    height: NonZeroU16MinusOne,
) {
    ClientMapBuffered::update_physics_layer(
        tp,
        group_width,
        group_height,
        layer,
        x,
        y,
        width,
        height,
    );
}

pub fn upload_design_tile_layer_buffer(
    graphics_mt: &GraphicsMultiThreaded,
    tiles: &[TileBase],
    width: NonZeroU16MinusOne,
    height: NonZeroU16MinusOne,
    has_texture: bool,
) -> MapBufferTileLayer {
    ClientMapBuffered::upload_design_tile_layer(
        graphics_mt,
        tiles,
        width,
        height,
        has_texture,
        0,
        0,
        true,
        true,
    )
}

pub fn finish_design_tile_layer_buffer(
    shader_storage_handle: &GraphicsShaderStorageHandle,
    buffer_object_handle: &GraphicsBufferObjectHandle,
    backend_handle: &GraphicsBackendHandle,
    buffer: MapBufferTileLayer,
) -> TileLayerVisuals {
    ClientMapBuffered::finish_upload_tile_layer(
        shader_storage_handle,
        buffer_object_handle,
        backend_handle,
        buffer,
    )
}

pub fn update_design_tile_layer(
    tp: &Arc<rayon::ThreadPool>,
    layer: &mut EditorLayerTile,
    x: u16,
    y: u16,
    width: NonZeroU16MinusOne,
    height: NonZeroU16MinusOne,
) {
    ClientMapBuffered::update_design_tile_layer(tp, layer, x, y, width, height);
}

pub fn upload_design_quad_layer_buffer(
    graphics_mt: &GraphicsMultiThreaded,
    attr: &MapLayerQuadsAttrs,
    quads: &[Quad],
) -> ClientMapBufferQuadLayer {
    ClientMapBuffered::upload_design_quad_layer(graphics_mt, attr, quads, 0, 0, true)
}

pub fn finish_design_quad_layer_buffer(
    buffer_object_handle: &GraphicsBufferObjectHandle,
    backend_handle: &GraphicsBackendHandle,
    buffer: ClientMapBufferQuadLayer,
) -> QuadLayerVisuals {
    ClientMapBuffered::finish_upload_quad_layer(buffer_object_handle, backend_handle, buffer)
}

pub fn update_design_quad_layer(layer: &mut EditorLayerQuad, update_range: Range<usize>) {
    ClientMapBuffered::update_design_quad_layer(layer, update_range);
}
