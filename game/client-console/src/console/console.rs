use std::collections::VecDeque;

use base::steady_clock::SteadyClock;
use client_containers::skins::SkinContainer;
use client_render_base::render::tee::RenderTee;
use client_types::console::ConsoleEntry;
use client_ui::console::{page::ConsoleUi, user_data::UserData};
use command_parser::parser::ParserCache;
use egui::Color32;
use game_config::config::Config;
use graphics::graphics::graphics::Graphics;
use tracing::instrument;
use ui_base::{
    types::{UiRenderPipe, UiState},
    ui::{UiContainer, UiCreator},
};
use ui_generic::generic_ui_renderer;

pub struct ConsoleRenderPipe<'a> {
    pub graphics: &'a Graphics,
    pub time: &'a SteadyClock,
    pub config: &'a mut Config,
    pub msgs: &'a mut String,
    pub custom_matches: &'a dyn Fn(&str) -> Option<Vec<String>>,
    #[allow(clippy::type_complexity)]
    pub render_custom_matches:
        &'a dyn Fn(&str, &str, &mut egui::Ui, &mut UiState, &mut SkinContainer, &RenderTee),
    pub skin_container: &'a mut SkinContainer,
    pub render_tee: &'a RenderTee,
}

pub trait ConsoleEvents<E> {
    fn take(&self) -> Vec<E>;
    fn push(&self, ev: E);
}

pub struct ConsoleRender<E, T> {
    pub ui: UiContainer,
    pub entries: Vec<ConsoleEntry>,
    pub text: String,
    pub text_history: VecDeque<String>,
    pub text_history_index: Option<usize>,
    pub cursor: usize,
    pub selected_index: Option<usize>,
    pub console_ui: ConsoleUi,

    console_events: Box<dyn ConsoleEvents<E>>,
    pub user: T,

    cache: ParserCache,
}

impl<E, T> ConsoleRender<E, T> {
    pub fn new(
        creator: &UiCreator,
        entries: Vec<ConsoleEntry>,
        console_events: Box<dyn ConsoleEvents<E>>,
        bg_color: Color32,
        user: T,
    ) -> Self {
        let mut ui = UiContainer::new(creator);
        ui.set_main_panel_color(&Color32::TRANSPARENT);

        Self {
            ui,
            entries,
            text: Default::default(),
            text_history: Default::default(),
            text_history_index: Default::default(),
            selected_index: None,
            cursor: 0,
            console_ui: ConsoleUi::new(bg_color),
            console_events,
            user,

            cache: Default::default(),
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip_all)]
    pub fn render(
        &mut self,
        inp: egui::RawInput,
        pipe: &mut ConsoleRenderPipe,
        can_change_client_config: bool,
    ) -> egui::PlatformOutput {
        self.ui.load_monospace_fonts();

        let mut user_data = UserData {
            entries: &self.entries,
            msgs: pipe.msgs,
            msg: &mut self.text,
            msg_history: &mut self.text_history,
            msg_history_index: &mut self.text_history_index,
            cursor: &mut self.cursor,
            select_index: &mut self.selected_index,
            config: pipe.config,
            cache: &mut self.cache,
            can_change_client_config,
            custom_matches: pipe.custom_matches,
            render_custom_matches: pipe.render_custom_matches,
            skin_container: pipe.skin_container,
            render_tee: pipe.render_tee,
        };
        let mut ui_pipe = UiRenderPipe::new(pipe.time.now(), &mut user_data);

        generic_ui_renderer::render(
            &pipe.graphics.backend_handle,
            &pipe.graphics.texture_handle,
            &pipe.graphics.stream_handle,
            &pipe.graphics.canvas_handle,
            &mut self.ui,
            &mut self.console_ui,
            &mut ui_pipe,
            inp,
        )
    }

    #[must_use]
    pub fn get_events(&self) -> Vec<E> {
        self.console_events.take()
    }

    pub fn add_event(&self, ev: E) {
        self.console_events.push(ev);
    }
}
