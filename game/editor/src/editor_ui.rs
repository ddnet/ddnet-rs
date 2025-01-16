use std::{sync::Arc, time::Duration};

use base_io::io::Io;
use config::config::ConfigEngine;
use egui::{Color32, InputState};
use graphics::{
    graphics::graphics::Graphics,
    graphics_mt::GraphicsMultiThreaded,
    handles::{
        backend::backend::GraphicsBackendHandle,
        buffer_object::buffer_object::GraphicsBufferObjectHandle,
        canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
        texture::texture::GraphicsTextureHandle,
    },
};
use ui_base::{
    types::UiRenderPipe,
    ui::{UiContainer, UiCreator},
};
use ui_generic::generic_ui_renderer;

use crate::{
    tools::{tile_layer::auto_mapper::TileLayerAutoMapper, tool::Tools},
    ui::{
        page::EditorUi,
        user_data::{EditorMenuDialogMode, EditorTabsRefMut, EditorUiEvent, UserData},
    },
    utils::UiCanvasSize,
};

pub struct EditorUiRenderPipe<'a> {
    pub cur_time: Duration,
    pub config: &'a ConfigEngine,
    pub inp: egui::RawInput,
    pub editor_tabs: EditorTabsRefMut<'a>,
    pub ui_events: &'a mut Vec<EditorUiEvent>,
    pub unused_rect: &'a mut Option<egui::Rect>,
    pub input_state: &'a mut Option<InputState>,
    pub canvas_size: &'a mut Option<UiCanvasSize>,
    pub tools: &'a mut Tools,
    pub auto_mapper: &'a mut TileLayerAutoMapper,
    pub io: &'a Io,
}

pub struct EditorUiRender {
    pub ui: UiContainer,
    editor_ui: EditorUi,

    menu_dialog_mode: EditorMenuDialogMode,

    backend_handle: GraphicsBackendHandle,
    canvas_handle: GraphicsCanvasHandle,
    stream_handle: GraphicsStreamHandle,
    texture_handle: GraphicsTextureHandle,
    buffer_object_handle: GraphicsBufferObjectHandle,
    graphics_mt: GraphicsMultiThreaded,

    tp: Arc<rayon::ThreadPool>,
}

impl EditorUiRender {
    pub fn new(graphics: &Graphics, tp: Arc<rayon::ThreadPool>, creator: &UiCreator) -> Self {
        let mut ui = UiContainer::new(creator);
        ui.set_main_panel_color(&Color32::TRANSPARENT);

        Self {
            ui,
            editor_ui: EditorUi::new(),

            menu_dialog_mode: EditorMenuDialogMode::None,

            backend_handle: graphics.backend_handle.clone(),
            canvas_handle: graphics.canvas_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),
            texture_handle: graphics.texture_handle.clone(),
            buffer_object_handle: graphics.buffer_object_handle.clone(),
            graphics_mt: graphics.get_graphics_mt(),

            tp,
        }
    }

    pub fn render(&mut self, pipe: EditorUiRenderPipe) -> egui::PlatformOutput {
        let mut needs_pointer = false;
        generic_ui_renderer::render(
            &self.backend_handle,
            &self.texture_handle,
            &self.stream_handle,
            &self.canvas_handle,
            &mut self.ui,
            &mut self.editor_ui,
            &mut UiRenderPipe::new(
                pipe.cur_time,
                &mut UserData {
                    config: pipe.config,
                    editor_tabs: pipe.editor_tabs,
                    ui_events: pipe.ui_events,

                    canvas_handle: &self.canvas_handle,
                    stream_handle: &self.stream_handle,

                    unused_rect: pipe.unused_rect,
                    input_state: pipe.input_state,
                    canvas_size: pipe.canvas_size,

                    menu_dialog_mode: &mut self.menu_dialog_mode,
                    tools: pipe.tools,

                    auto_mapper: pipe.auto_mapper,

                    pointer_is_used: &mut needs_pointer,
                    io: pipe.io,

                    tp: &self.tp,
                    graphics_mt: &self.graphics_mt,
                    buffer_object_handle: &self.buffer_object_handle,
                    backend_handle: &self.backend_handle,
                },
            ),
            pipe.inp,
        )
    }
}
