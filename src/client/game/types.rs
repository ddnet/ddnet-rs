use std::{
    net::SocketAddr,
    ops::Deref,
    rc::Rc,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use base::system::System;
use base_io::io::Io;
use client_notifications::overlay::ClientNotifications;
use client_types::{cert::ServerCertMode, console::ConsoleEntry};
use client_ui::ingame_menu::account_info::AccountInfo;
use client_ui::{
    connect::user_data::ConnectMode,
    ingame_menu::{client_info::ClientInfo, votes::Votes},
    main_menu::{player_settings_ntfy::PlayerSettingsSync, spatial_chat},
};
use config::config::ConfigEngine;
use egui::FontDefinitions;
use game_base::{local_server_info::LocalServerInfo, server_browser::ServerBrowserData};
use game_config::config::ConfigGame;
use game_network::{game_event_generator::GameEventGenerator, messages::ServerToClientMessage};
use graphics::graphics::graphics::Graphics;
use graphics_backend::backend::GraphicsBackend;
use network::network::quinn_network::QuinnNetwork;
use pool::datatypes::StringPool;
use sound::sound::SoundManager;
use ui_base::types::UiState;

use crate::spatial_chat::spatial_chat::SpatialChat;

pub struct GameBase {
    pub graphics: Graphics,
    pub graphics_backend: Rc<GraphicsBackend>,
    pub sound: SoundManager,
    pub sys: System,
    pub tp: Arc<rayon::ThreadPool>,
    pub fonts: FontDefinitions,
}

/// Automatically reset some state if the client dropped.
///
/// Mostly some Ui stuff
#[derive(Debug)]
pub struct DisconnectAutoCleanup {
    pub spatial_chat: spatial_chat::SpatialChat,
    pub client_info: ClientInfo,
    pub account_info: AccountInfo,
    pub player_settings_sync: PlayerSettingsSync,
    pub votes: Votes,
}

impl Drop for DisconnectAutoCleanup {
    fn drop(&mut self) {
        self.spatial_chat.support(false);
        self.client_info.set_local_player_count(0);
        self.account_info.fill_account_info(None);
        self.player_settings_sync.did_player_info_change();
        self.player_settings_sync.did_controls_change();
        self.player_settings_sync.did_team_settings_change();
        self.votes.needs_map_votes();
        self.votes.fill_map_votes(Default::default(), false);
    }
}

pub struct GameConnect {
    pub rcon_secret: Option<[u8; 32]>,
    pub addr: SocketAddr,
    pub mode: ConnectMode,
    pub server_cert: ServerCertMode,
    pub browser_data: ServerBrowserData,
}

pub struct GameNetwork {
    pub network: QuinnNetwork,
    pub game_event_generator_client: Arc<GameEventGenerator<ServerToClientMessage<'static>>>,
    pub has_new_events_client: Arc<AtomicBool>,
    pub server_connect_time: Duration,
}

impl Deref for GameNetwork {
    type Target = QuinnNetwork;

    fn deref(&self) -> &Self::Target {
        &self.network
    }
}

pub struct GameMsgPipeline<'a> {
    pub runtime_thread_pool: &'a Arc<rayon::ThreadPool>,
    pub io: &'a Io,
    pub console_entries: &'a Vec<ConsoleEntry>,
    pub config: &'a mut ConfigEngine,
    pub config_game: &'a mut ConfigGame,
    pub shared_info: &'a Arc<LocalServerInfo>,
    pub account_info: &'a AccountInfo,
    pub ui: &'a mut UiState,
    pub sys: &'a System,
    pub string_pool: &'a StringPool,
    pub spatial_chat: &'a SpatialChat,
    pub notifications: &'a mut ClientNotifications,
}
