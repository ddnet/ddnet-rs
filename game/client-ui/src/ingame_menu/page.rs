use std::{sync::Arc, time::Duration};

use base_io::io::Io;

use client_types::console::ConsoleEntry;
use game_base::{local_server_info::LocalServerInfo, server_browser::ServerBrowserData};
use game_config::config::{Config, ConfigGame};
use graphics::graphics::graphics::Graphics;
use math::math::Rng;
use sound::sound::SoundManager;
use ui_base::types::{UiRenderPipe, UiState};
use ui_generic::traits::UiPageInterface;

use crate::{
    events::UiEvents,
    main_menu::{
        features::EnabledFeatures, monitors::UiMonitors, page::MainMenuUi,
        player_settings_ntfy::PlayerSettingsSync, profiles_interface::ProfilesInterface,
        spatial_chat::SpatialChat,
    },
    thumbnail_container::{
        load_thumbnail_container, ThumbnailContainer, DEFAULT_THUMBNAIL_CONTAINER_PATH,
    },
};

use super::{
    account_info::AccountInfo, client_info::ClientInfo, main_frame, raw_input_info::RawInputInfo,
    server_info::GameServerInfo, server_players::ServerPlayers, user_data::UserData, votes::Votes,
};

pub struct IngameMenuUi {
    main_menu: MainMenuUi,
    server_players: ServerPlayers,
    game_server_info: GameServerInfo,
    account_info: AccountInfo,
    votes: Votes,
    map_vote_thumbnail_container: ThumbnailContainer,
    rng: Rng,
}

impl IngameMenuUi {
    pub fn new(
        graphics: &Graphics,
        sound: &SoundManager,
        server_info: Arc<LocalServerInfo>,
        client_info: ClientInfo,
        events: UiEvents,
        io: Io,
        tp: Arc<rayon::ThreadPool>,
        profiles: Arc<dyn ProfilesInterface>,
        monitors: UiMonitors,
        spatial_chat: SpatialChat,
        player_settings_sync: PlayerSettingsSync,
        config_game: &ConfigGame,
        console_entries: Vec<ConsoleEntry>,
        raw_input_info: RawInputInfo,
        browser_data: ServerBrowserData,
        features: EnabledFeatures,
        server_players: ServerPlayers,
        game_server_info: GameServerInfo,
        account_info: AccountInfo,
        votes: Votes,
        cur_time: &Duration,
    ) -> Self {
        let main_menu = MainMenuUi::new(
            graphics,
            sound,
            server_info,
            client_info,
            events,
            io.clone(),
            tp.clone(),
            profiles,
            monitors,
            spatial_chat,
            player_settings_sync,
            config_game,
            console_entries,
            raw_input_info,
            browser_data,
            features,
        );

        let map_vote_thumbnail_container = load_thumbnail_container(
            io,
            tp,
            DEFAULT_THUMBNAIL_CONTAINER_PATH,
            "map-vote-thumbnail-container",
            graphics,
            sound,
            main_menu.scene.clone(),
            None,
        );

        Self {
            main_menu,
            server_players,
            game_server_info,
            account_info,
            votes,
            map_vote_thumbnail_container,
            rng: Rng::new(cur_time.subsec_nanos() as u64),
        }
    }

    fn get_user_data<'a>(&'a mut self, config: &'a mut Config) -> UserData<'a> {
        UserData {
            browser_menu: self.main_menu.get_user_data(config, true),
            server_players: &self.server_players,
            game_server_info: &self.game_server_info,
            votes: &self.votes,
            account_info: &self.account_info,
            map_vote_thumbnail_container: &mut self.map_vote_thumbnail_container,
            rng: &mut self.rng,
        }
    }

    pub fn update(&mut self, cur_time: &Duration) {
        self.main_menu.update(cur_time);

        MainMenuUi::update_container(&mut self.map_vote_thumbnail_container, cur_time);
    }
}

impl UiPageInterface<Config> for IngameMenuUi {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut UiRenderPipe<Config>,
        ui_state: &mut UiState,
    ) {
        self.main_menu.check_tasks(&pipe.cur_time);

        main_frame::render(
            ui,
            &mut UiRenderPipe {
                cur_time: pipe.cur_time,
                user_data: &mut self.get_user_data(pipe.user_data),
            },
            ui_state,
        );

        self.update(&pipe.cur_time);
    }

    fn unmount(&mut self) {
        self.main_menu.unmount();
        self.map_vote_thumbnail_container.clear_except_default();
    }
}
