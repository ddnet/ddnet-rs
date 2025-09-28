use std::time::Duration;

use base::linked_hash_map_view::FxLinkedHashMap;
use client_containers::{ctf::CtfContainer, skins::SkinContainer};
use client_render_base::render::tee::RenderTee;
use client_ui::hud::{
    page::HudUi,
    user_data::{RenderDateTime, UserData},
};
use egui::Color32;
use game_interface::types::{
    game::{GameTickType, NonZeroGameTickType},
    id_types::CharacterId,
    render::{character::CharacterInfo, game::GameRenderInfo},
};
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

pub struct HudRenderPipe<'a> {
    pub race_timer_counter: &'a GameTickType,
    pub ticks_per_second: &'a NonZeroGameTickType,
    pub cur_time: &'a Duration,
    pub game: Option<&'a GameRenderInfo>,
    pub skin_container: &'a mut SkinContainer,
    pub skin_renderer: &'a RenderTee,
    pub ctf_container: &'a mut CtfContainer,
    pub character_infos: &'a FxLinkedHashMap<CharacterId, CharacterInfo>,
    pub date_time: &'a Option<RenderDateTime>,
}

pub struct HudRender {
    pub ui: UiContainer,
    hud_ui: HudUi,

    backend_handle: GraphicsBackendHandle,
    canvas_handle: GraphicsCanvasHandle,
    stream_handle: GraphicsStreamHandle,
    texture_handle: GraphicsTextureHandle,
}

impl HudRender {
    pub fn new(graphics: &Graphics, creator: &UiCreator) -> Self {
        let mut ui = UiContainer::new(creator);
        ui.set_main_panel_color(&Color32::TRANSPARENT);
        Self {
            ui,
            hud_ui: HudUi::new(),

            backend_handle: graphics.backend_handle.clone(),
            canvas_handle: graphics.canvas_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),
            texture_handle: graphics.texture_handle.clone(),
        }
    }

    pub fn render(&mut self, pipe: &mut HudRenderPipe) {
        let mut user_data = UserData {
            race_round_timer_counter: pipe.race_timer_counter,
            ticks_per_second: pipe.ticks_per_second,
            game: pipe.game,
            skin_container: pipe.skin_container,
            skin_renderer: pipe.skin_renderer,
            ctf_container: pipe.ctf_container,
            character_infos: pipe.character_infos,
            canvas_handle: &self.canvas_handle,
            stream_handle: &self.stream_handle,
            date_time: pipe.date_time,
        };
        let mut dummy_pipe = UiRenderPipe::new(*pipe.cur_time, &mut user_data);

        generic_ui_renderer::render(
            &self.backend_handle,
            &self.texture_handle,
            &self.stream_handle,
            &self.canvas_handle,
            &mut self.ui,
            &mut self.hud_ui,
            &mut dummy_pipe,
            Default::default(),
        );
    }
}
