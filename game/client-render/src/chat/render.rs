use std::{
    collections::{HashSet, VecDeque},
    time::Duration,
};

use base::linked_hash_map_view::FxLinkedHashMap;
use client_containers::skins::SkinContainer;
use client_render_base::render::tee::RenderTee;
use client_ui::chat::{
    page::ChatUi,
    user_data::{ChatEvent, ChatMode, MsgInChat, UserData},
};
use egui::Color32;
use game_interface::types::{
    id_types::{CharacterId, PlayerId},
    render::character::CharacterInfo,
};
use graphics::{
    graphics::graphics::Graphics,
    handles::{
        backend::backend::GraphicsBackendHandle, canvas::canvas::GraphicsCanvasHandle,
        stream::stream::GraphicsStreamHandle, texture::texture::GraphicsTextureHandle,
    },
};

use ui_base::{
    remember_mut::RememberMut,
    types::UiRenderPipe,
    ui::{UiContainer, UiCreator},
    ui_render::render_ui,
};
use ui_generic::traits::UiPageInterface;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatRenderOptions {
    pub is_chat_input_active: bool,
    pub show_chat_history: bool,
}

pub struct ChatRenderPipe<'a> {
    pub cur_time: &'a Duration,
    pub options: ChatRenderOptions,
    pub msg: &'a mut String,
    pub input: &'a mut Option<egui::RawInput>,
    pub mode: ChatMode,
    pub skin_container: &'a mut SkinContainer,
    pub tee_render: &'a RenderTee,
    pub character_infos: &'a FxLinkedHashMap<CharacterId, CharacterInfo>,
    pub local_character_ids: &'a HashSet<CharacterId>,
}

pub struct ChatRender {
    pub ui: UiContainer,
    chat_ui: ChatUi,

    pub msgs: RememberMut<VecDeque<MsgInChat>>,
    pub last_render_options: Option<ChatRenderOptions>,

    find_player_prompt: String,
    find_player_id: Option<PlayerId>,
    cur_whisper_player_id: Option<PlayerId>,

    backend_handle: GraphicsBackendHandle,
    canvas_handle: GraphicsCanvasHandle,
    stream_handle: GraphicsStreamHandle,
    texture_handle: GraphicsTextureHandle,
}

impl ChatRender {
    pub fn new(graphics: &Graphics, creator: &UiCreator) -> Self {
        let mut ui = UiContainer::new(creator);
        ui.set_main_panel_color(&Color32::TRANSPARENT);
        Self {
            ui,
            chat_ui: ChatUi::new(),

            msgs: Default::default(),
            last_render_options: None,

            find_player_prompt: Default::default(),
            find_player_id: Default::default(),
            cur_whisper_player_id: Default::default(),

            backend_handle: graphics.backend_handle.clone(),
            canvas_handle: graphics.canvas_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),
            texture_handle: graphics.texture_handle.clone(),
        }
    }

    pub fn render(&mut self, pipe: &mut ChatRenderPipe) -> Vec<ChatEvent> {
        if self.msgs.len() > 120 {
            self.msgs.truncate(100);
        }

        let mut res: Vec<ChatEvent> = Default::default();
        let canvas_width = self.canvas_handle.canvas_width();
        let canvas_height = self.canvas_handle.canvas_height();
        let pixels_per_point = self.canvas_handle.pixels_per_point();

        let force_rerender = self.msgs.was_accessed_mut()
            || !self
                .last_render_options
                .is_some_and(|last_options| last_options == pipe.options)
            || pipe.options.is_chat_input_active;

        self.last_render_options = Some(pipe.options);

        let mut user_data = UserData {
            entries: &self.msgs,
            msg: pipe.msg,
            is_input_active: pipe.options.is_chat_input_active,
            show_chat_history: pipe.options.show_chat_history || pipe.options.is_chat_input_active,
            chat_events: &mut res,
            canvas_handle: &self.canvas_handle,
            stream_handle: &self.stream_handle,
            skin_container: pipe.skin_container,
            render_tee: pipe.tee_render,
            mode: pipe.mode,
            character_infos: pipe.character_infos,
            local_character_ids: pipe.local_character_ids,
            find_player_prompt: &mut self.find_player_prompt,
            find_player_id: &mut self.find_player_id,
            cur_whisper_player_id: &mut self.cur_whisper_player_id,
        };
        let mut dummy_pipe = UiRenderPipe::new(*pipe.cur_time, &mut user_data);
        let (screen_rect, full_output, zoom_level) = self.ui.render_cached(
            canvas_width,
            canvas_height,
            pixels_per_point,
            |ui, inner_pipe, ui_state| self.chat_ui.render(ui, inner_pipe, ui_state),
            &mut dummy_pipe,
            pipe.input.clone().unwrap_or_default(),
            false,
            force_rerender,
        );
        let platform_output = render_ui(
            &mut self.ui,
            full_output,
            &screen_rect,
            zoom_level,
            &self.backend_handle,
            &self.texture_handle,
            &self.stream_handle,
            false,
        );
        if pipe.options.is_chat_input_active {
            res.push(ChatEvent::PlatformOutput(platform_output));
        }
        res
    }
}
