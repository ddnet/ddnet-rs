use std::{collections::VecDeque, time::Duration};

use base::linked_hash_map_view::FxLinkedHashMap;
use client_containers::skins::SkinContainer;
use client_render_base::render::tee::RenderTee;
use client_ui::spectator_selection::{
    page::SpectatorSelectionUi,
    user_data::{SpectatorSelectionEvent, UserData},
};
use egui::Color32;
use game_interface::types::{id_types::CharacterId, render::character::CharacterInfo};
use graphics::{
    graphics::graphics::Graphics,
    handles::{
        backend::backend::GraphicsBackendHandle, canvas::canvas::GraphicsCanvasHandle,
        stream::stream::GraphicsStreamHandle, texture::texture::GraphicsTextureHandle,
    },
};
use ui_base::{
    types::UiRenderPipe,
    ui::{UiContainer, UiCreator},
};
use ui_generic::generic_ui_renderer;

pub struct SpectatorSelectionRenderPipe<'a> {
    pub cur_time: &'a Duration,
    pub input: &'a mut Option<egui::RawInput>,
    pub skin_container: &'a mut SkinContainer,
    pub skin_renderer: &'a RenderTee,
    pub character_infos: &'a FxLinkedHashMap<CharacterId, CharacterInfo>,
    pub ingame: bool,
    pub into_phased: bool,
}

pub struct SpectatorSelectionRender {
    pub ui: UiContainer,
    spectator_selection_ui: SpectatorSelectionUi,

    backend_handle: GraphicsBackendHandle,
    canvas_handle: GraphicsCanvasHandle,
    stream_handle: GraphicsStreamHandle,
    texture_handle: GraphicsTextureHandle,
}

impl SpectatorSelectionRender {
    pub fn new(graphics: &Graphics, creator: &UiCreator) -> Self {
        let mut ui = UiContainer::new(creator);
        ui.set_main_panel_color(&Color32::TRANSPARENT);
        Self {
            ui,
            spectator_selection_ui: SpectatorSelectionUi::new(),

            backend_handle: graphics.backend_handle.clone(),
            canvas_handle: graphics.canvas_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),
            texture_handle: graphics.texture_handle.clone(),
        }
    }

    pub fn render(
        &mut self,
        pipe: &mut SpectatorSelectionRenderPipe,
    ) -> VecDeque<SpectatorSelectionEvent> {
        let mut events: VecDeque<SpectatorSelectionEvent> = Default::default();

        let mut user_data = UserData {
            skin_container: pipe.skin_container,
            skin_renderer: pipe.skin_renderer,
            character_infos: pipe.character_infos,
            canvas_handle: &self.canvas_handle,
            stream_handle: &self.stream_handle,
            events: &mut events,
            ingame: pipe.ingame,
            into_phased: pipe.into_phased,
        };
        let mut inner_pipe = UiRenderPipe::new(*pipe.cur_time, &mut user_data);
        generic_ui_renderer::render(
            &self.backend_handle,
            &self.texture_handle,
            &self.stream_handle,
            &self.canvas_handle,
            &mut self.ui,
            &mut self.spectator_selection_ui,
            &mut inner_pipe,
            pipe.input.take().unwrap_or_default(),
        );
        events
    }
}
