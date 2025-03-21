use std::time::Duration;

use base::linked_hash_map_view::FxLinkedHashMap;
use client_containers::{flags::FlagsContainer, skins::SkinContainer};
use client_render_base::render::tee::RenderTee;
use client_ui::scoreboard::{page::ScoreboardUi, user_data::UserData};
use egui::Color32;
use graphics::{
    graphics::graphics::Graphics,
    handles::{
        backend::backend::GraphicsBackendHandle, canvas::canvas::GraphicsCanvasHandle,
        stream::stream::GraphicsStreamHandle, texture::texture::GraphicsTextureHandle,
    },
};

use game_interface::types::{
    id_types::CharacterId,
    render::{character::CharacterInfo, scoreboard::Scoreboard},
};
use ui_base::{
    types::UiRenderPipe,
    ui::{UiContainer, UiCreator},
};

use ui_generic::generic_ui_renderer;

pub struct ScoreboardRenderPipe<'a> {
    pub cur_time: &'a Duration,
    pub scoreboard: &'a Scoreboard,
    pub character_infos: &'a FxLinkedHashMap<CharacterId, CharacterInfo>,
    pub skin_container: &'a mut SkinContainer,
    pub tee_render: &'a RenderTee,
    pub flags_container: &'a mut FlagsContainer,

    pub own_character_id: &'a CharacterId,
}

pub struct ScoreboardRender {
    ui: UiContainer,
    scoreboard_ui: ScoreboardUi,

    backend_handle: GraphicsBackendHandle,
    canvas_handle: GraphicsCanvasHandle,
    stream_handle: GraphicsStreamHandle,
    texture_handle: GraphicsTextureHandle,
}

impl ScoreboardRender {
    pub fn new(graphics: &Graphics, creator: &UiCreator) -> Self {
        let mut ui = UiContainer::new(creator);
        ui.set_main_panel_color(&Color32::TRANSPARENT);
        Self {
            ui,
            scoreboard_ui: ScoreboardUi::new(),
            backend_handle: graphics.backend_handle.clone(),
            canvas_handle: graphics.canvas_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),
            texture_handle: graphics.texture_handle.clone(),
        }
    }

    pub fn render(&mut self, pipe: &mut ScoreboardRenderPipe) {
        generic_ui_renderer::render(
            &self.backend_handle,
            &self.texture_handle,
            &self.stream_handle,
            &self.canvas_handle,
            &mut self.ui,
            &mut self.scoreboard_ui,
            &mut UiRenderPipe::new(
                *pipe.cur_time,
                &mut UserData {
                    scoreboard: pipe.scoreboard,
                    character_infos: pipe.character_infos,
                    canvas_handle: &self.canvas_handle,
                    stream_handle: &self.stream_handle,
                    skin_container: pipe.skin_container,
                    render_tee: pipe.tee_render,
                    flags_container: pipe.flags_container,

                    own_character_id: pipe.own_character_id,
                },
            ),
            Default::default(),
        );
    }
}
