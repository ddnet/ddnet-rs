use std::{collections::HashSet, sync::Arc};

use client_containers::entities::EntitiesContainer;
use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::{
        backend::backend::GraphicsBackendHandle,
        buffer_object::buffer_object::GraphicsBufferObjectHandle,
        canvas::canvas::GraphicsCanvasHandle,
        stream::stream::GraphicsStreamHandle,
        texture::texture::{TextureContainer, TextureContainer2dArray},
    },
};
use hiarc::Hiarc;

use crate::{client::EditorClient, map::EditorMap, utils::UiCanvasSize};

use super::{
    quad_layer::{brush::QuadBrush, selection::QuadSelection},
    sound_layer::brush::SoundBrush,
    tile_layer::{brush::TileBrush, selection::TileSelection},
};

#[derive(Debug, Hiarc)]
pub struct ToolTileLayer {
    pub brush: TileBrush,
    pub selection: TileSelection,
}

impl ToolTileLayer {
    pub fn update(
        &mut self,
        ui_canvas: &UiCanvasSize,
        active_tool: &ActiveToolTiles,
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
        match active_tool {
            ActiveToolTiles::Brush => self.brush.update(
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
                latest_modifiers,
                current_pointer_pos,
                available_rect,
                client,
            ),
            ActiveToolTiles::Selection => self.selection.update(
                ui_canvas,
                canvas_handle,
                map,
                latest_pointer,
                current_pointer_pos,
            ),
        }
    }

    pub fn render(
        &mut self,
        ui_canvas: &UiCanvasSize,
        active_tool: &ActiveToolTiles,
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
        match active_tool {
            ActiveToolTiles::Brush => self.brush.render(
                ui_canvas,
                backend_handle,
                stream_handle,
                canvas_handle,
                entities_container,
                fake_texture_2d_array,
                map,
                latest_pointer,
                latest_keys_down,
                current_pointer_pos,
                available_rect,
            ),
            ActiveToolTiles::Selection => self.selection.render(
                ui_canvas,
                stream_handle,
                canvas_handle,
                map,
                latest_pointer,
                current_pointer_pos,
            ),
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct ToolQuadLayer {
    pub brush: QuadBrush,
    pub selection: QuadSelection,
}

impl ToolQuadLayer {
    pub fn update(
        &mut self,
        ui_canvas: &UiCanvasSize,
        active_tool: &ActiveToolQuads,
        graphics_mt: &GraphicsMultiThreaded,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        backend_handle: &GraphicsBackendHandle,
        canvas_handle: &GraphicsCanvasHandle,
        map: &mut EditorMap,
        fake_texture: &TextureContainer,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
        latest_modifiers: &egui::Modifiers,
        client: &mut EditorClient,
    ) {
        match active_tool {
            ActiveToolQuads::Brush => self.brush.update(
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
            ),
            ActiveToolQuads::Selection => self.selection.update(
                ui_canvas,
                canvas_handle,
                map,
                latest_pointer,
                current_pointer_pos,
                latest_modifiers,
                client,
            ),
        }
    }

    pub fn render(
        &mut self,
        ui_canvas: &UiCanvasSize,
        active_tool: &ActiveToolQuads,
        stream_handle: &GraphicsStreamHandle,
        canvas_handle: &GraphicsCanvasHandle,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
    ) {
        match active_tool {
            ActiveToolQuads::Brush => self.brush.render(
                ui_canvas,
                stream_handle,
                canvas_handle,
                map,
                latest_pointer,
                current_pointer_pos,
            ),
            ActiveToolQuads::Selection => self.selection.render(
                ui_canvas,
                stream_handle,
                canvas_handle,
                map,
                latest_pointer,
                current_pointer_pos,
            ),
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct ToolSoundLayer {
    pub brush: SoundBrush,
}

impl ToolSoundLayer {
    pub fn update(
        &mut self,
        ui_canvas: &UiCanvasSize,
        active_tool: &ActiveToolSounds,
        canvas_handle: &GraphicsCanvasHandle,
        map: &mut EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
        latest_modifiers: &egui::Modifiers,
        client: &mut EditorClient,
    ) {
        match active_tool {
            ActiveToolSounds::Brush => self.brush.update(
                ui_canvas,
                canvas_handle,
                map,
                latest_pointer,
                current_pointer_pos,
                latest_modifiers,
                client,
            ),
        }
    }

    pub fn render(
        &mut self,
        ui_canvas: &UiCanvasSize,
        active_tool: &ActiveToolSounds,
        stream_handle: &GraphicsStreamHandle,
        canvas_handle: &GraphicsCanvasHandle,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
    ) {
        match active_tool {
            ActiveToolSounds::Brush => self.brush.render(
                ui_canvas,
                stream_handle,
                canvas_handle,
                map,
                latest_pointer,
                current_pointer_pos,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ActiveToolTiles {
    Brush,
    Selection,
}

#[derive(Debug, Clone, Copy)]
pub enum ActiveToolQuads {
    Brush,
    Selection,
}

#[derive(Debug, Clone, Copy)]
pub enum ActiveToolSounds {
    Brush,
}

#[derive(Debug, Clone, Copy)]
pub enum ActiveTool {
    Tiles(ActiveToolTiles),
    Quads(ActiveToolQuads),
    Sounds(ActiveToolSounds),
}

#[derive(Debug)]
pub struct Tools {
    pub tiles: ToolTileLayer,
    pub quads: ToolQuadLayer,
    pub sounds: ToolSoundLayer,
    pub active_tool: ActiveTool,
}

impl Tools {
    pub fn set_tool(&mut self, tool: ActiveTool) {
        self.active_tool = tool;
    }
}
