pub mod active;
pub mod data;
pub mod types;

use std::{
    borrow::Cow,
    net::SocketAddr,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use active::ActiveGame;
use anyhow::anyhow;
use base::{
    hash::Hash,
    linked_hash_map_view::FxLinkedHashMap,
    network_string::{NetworkReducedAsciiString, NetworkString},
    system::SystemTimeInterface,
};
use base_io::{io::Io, runtime::IoRuntimeTask};
use client_accounts::accounts::Accounts;
use client_console::console::remote_console::{RemoteConsole, RemoteConsoleBuilder};
use client_map::client_map::{ClientMapFile, ClientMapLoading};
use client_notifications::overlay::ClientNotifications;
use client_render_game::render_game::{RenderGameCreateOptions, RenderModTy};
use client_replay::replay::Replay;
use client_types::{cert::ServerCertMode, console::ConsoleEntry};
use client_ui::{
    ingame_menu::server_info::{GameInfo, GameServerInfo},
    main_menu::page::MainMenuUi,
};
use config::config::ConfigEngine;
use data::{ClientConnectedPlayer, GameData, LocalPlayerGameData};
use demo::recorder::{DemoRecorder, DemoRecorderCreateProps, DemoRecorderCreatePropsBase};
use game_base::{
    assets_url::HTTP_RESOURCE_URL,
    connecting_log::ConnectModes,
    network::messages::{
        GameModification, MsgClAddLocalPlayer, MsgClReady, MsgSvServerInfo, RenderModification,
        RequiredResources,
    },
    server_browser::ServerBrowserServer,
};
use game_config::config::{
    ConfigClient, ConfigDummyProfile, ConfigGame, ConfigPlayer, ConfigTeeEye,
};
use game_interface::{
    interface::{GameStateCreateOptions, GameStateServerOptions, MAX_MAP_NAME_LEN},
    types::{
        character_info::NetworkCharacterInfo, render::character::TeeEye,
        resource_key::NetworkResourceKey,
    },
};
use game_network::{
    game_event_generator::GameEventGenerator,
    messages::{ClientToServerMessage, ServerToClientMessage},
};
use log::info;
use math::math::vector::vec2;
use network::network::{
    packet_compressor::DefaultNetworkPacketCompressor,
    plugins::{NetworkPluginPacket, NetworkPlugins},
    quinn_network::QuinnNetwork,
    types::{NetworkClientCertCheckMode, NetworkClientCertMode, NetworkClientInitOptions},
};
use pool::{mt_pool::Pool as MtPool, pool::Pool};
use prediction_timer::prediction_timing::PredictionTimer;
use sound::scene_object::SceneObject;
use types::{DisconnectAutoCleanup, GameBase, GameConnect, GameMsgPipeline, GameNetwork};
use ui_base::ui::UiCreator;
use url::Url;

use crate::localplayer::LocalPlayers;

use super::spatial_chat::spatial_chat::SpatialChatGameWorldTy;

type ServerCertTask = IoRuntimeTask<(ServerCertMode, Option<(Vec<ServerBrowserServer>, Duration)>)>;
pub struct PrepareConnectGame {
    pub connect: GameConnect,
    account_task: IoRuntimeTask<NetworkClientCertMode>,
    server_cert_verify_task: ServerCertTask,
    dicts_task: IoRuntimeTask<(Vec<u8>, Vec<u8>)>,
    auto_cleanup: DisconnectAutoCleanup,

    base: GameBase,
}

pub struct ConnectingGame {
    pub network: GameNetwork,

    pub connect: GameConnect,
    auto_cleanup: DisconnectAutoCleanup,

    base: GameBase,
}

pub struct LoadingGame {
    pub network: GameNetwork,
    map: ClientMapLoading,
    ping: Duration,
    prediction_timer: PredictionTimer,
    hint_start_camera_pos: vec2,
    pub connect: GameConnect,
    pub demo_recorder_props: DemoRecorderCreateProps,
    spatial_world: SpatialChatGameWorldTy,
    auto_cleanup: DisconnectAutoCleanup,

    base: GameBase,

    pub resource_download_server: Option<Url>,

    pub local: LocalPlayerGameData,

    pub send_input_every_tick: bool,
    pub server_options: GameStateServerOptions,
}

pub enum Game {
    /// the game is currently inactive, e.g. if the client
    /// is still in the main menu
    None,
    /// prepare to connect to a server
    /// e.g. load private key or whatever
    PrepareConnect(PrepareConnectGame),
    /// the game is connecting
    Connecting(ConnectingGame),
    /// the game is loading
    Loading(Box<LoadingGame>),
    WaitingForFirstSnapshot(Box<ActiveGame>),
    Active(Box<ActiveGame>),
    Err(anyhow::Error),
}

impl Game {
    pub fn new(
        base: GameBase,
        io: &Io,
        connect: GameConnect,
        accounts: &Arc<Accounts>,
        auto_cleanup: DisconnectAutoCleanup,
    ) -> anyhow::Result<Self> {
        let servers = connect.browser_data.list();
        let time_now = base.sys.time.time_get();

        let server_cert = connect.server_cert.clone();
        let http = io.http.clone();
        let addr = connect.addr;
        let server_cert_verify_task = io.rt.spawn(async move {
            // if list didn't refresh for over an hour, do it now
            let outdated = servers.time.is_none_or(|server_time| {
                time_now.saturating_sub(server_time) >= Duration::from_secs(60 * 60)
            });
            let should_check = match &server_cert {
                ServerCertMode::Cert(_) | ServerCertMode::Hash(_) => false,
                ServerCertMode::Unknown => true,
            };

            if should_check && !outdated {
                if let Some(server) = servers.find(addr) {
                    Ok((
                        ServerCertMode::Hash(server.info.cert_sha256_fingerprint),
                        None,
                    ))
                } else {
                    Err(anyhow!("Server was not found in the server list"))
                }
            } else if should_check {
                let servers = MainMenuUi::download_server_list(&http).await?;
                let server = servers
                    .iter()
                    .find(|server| server.addresses.contains(&addr))
                    .ok_or_else(|| anyhow!("Server was not found in the server list"));
                match (server_cert, server) {
                    (ServerCertMode::Unknown, Err(err)) => Err(err),
                    (server_cert, server) => {
                        let cert_mdoe = server
                            .map(|server| ServerCertMode::Hash(server.info.cert_sha256_fingerprint))
                            .unwrap_or(server_cert);
                        Ok((cert_mdoe, Some((servers, time_now))))
                    }
                }
            } else {
                Ok((server_cert, None))
            }
        });

        let accounts = accounts.clone();
        let task = io.rt.spawn(async move {
            let (game_key, cert, _) = accounts.connect_to_game_server().await;
            Ok(NetworkClientCertMode::FromCertAndPrivateKey {
                cert,
                private_key: game_key.private_key,
            })
        });

        let fs = io.fs.clone();
        let zstd_dicts = io.rt.spawn(async move {
            let client_send = fs.read_file("dict/client_send".as_ref()).await;
            let server_send = fs.read_file("dict/server_send".as_ref()).await;

            Ok(client_send.and_then(|c| server_send.map(|s| (c, s)))?)
        });

        Ok(Self::PrepareConnect(PrepareConnectGame {
            connect,
            account_task: task,
            server_cert_verify_task,
            dicts_task: zstd_dicts,
            auto_cleanup,

            base,
        }))
    }

    fn connect(
        base: GameBase,
        connect: GameConnect,
        config: &ConfigEngine,
        cert: NetworkClientCertMode,
        dicts: Option<(Vec<u8>, Vec<u8>)>,
        auto_cleanup: DisconnectAutoCleanup,
    ) -> Self {
        let has_new_events_client = Arc::new(AtomicBool::new(false));
        let game_event_generator_client =
            Arc::new(GameEventGenerator::new(has_new_events_client.clone()));

        let mut packet_plugins: Vec<Arc<dyn NetworkPluginPacket>> = vec![];

        if let Some((client_send, server_send)) = dicts {
            packet_plugins.push(Arc::new(DefaultNetworkPacketCompressor::new_with_dict(
                client_send,
                server_send,
            )));
        } else {
            packet_plugins.push(Arc::new(DefaultNetworkPacketCompressor::new()));
        }

        connect.log.log("Preparing client network socket.");
        match QuinnNetwork::init_client(
            None,
            game_event_generator_client.clone(),
            &base.sys,
            NetworkClientInitOptions::new(
                if config.dbg.untrusted_cert {
                    NetworkClientCertCheckMode::DisableCheck
                } else {
                    match &connect.server_cert {
                        ServerCertMode::Cert(cert) => {
                            NetworkClientCertCheckMode::CheckByCert { cert: cert.into() }
                        }
                        ServerCertMode::Hash(hash) => {
                            NetworkClientCertCheckMode::CheckByPubKeyHash {
                                hash: Cow::Borrowed(hash),
                            }
                        }
                        ServerCertMode::Unknown => {
                            return Self::Err(anyhow!(
                                "Server certificate could not be found \
                                in the server list or anywhere else."
                            ))
                        }
                    }
                },
                cert,
            )
            //.with_ack_config(5, Duration::from_millis(50), 5 - 1)
            // since there are many packets, increase loss detection thresholds
            //.with_loss_detection_cfg(25, 2.0)
            .with_timeout(config.net.timeout),
            NetworkPlugins {
                packet_plugins: Arc::new(packet_plugins),
                connection_plugins: Default::default(),
            },
            &connect.addr.to_string(),
        ) {
            Ok((network_client, _game_event_notifier)) => Self::Connecting(ConnectingGame {
                network: GameNetwork {
                    network: network_client,
                    game_event_generator_client,
                    has_new_events_client,
                    server_connect_time: base.sys.time_get(),
                },
                connect,
                auto_cleanup,

                base,
            }),
            Err(err) => Self::Err(err),
        }
    }

    fn load(
        base: GameBase,
        network: GameNetwork,
        tp: &Arc<rayon::ThreadPool>,
        io: &Io,
        map: &NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
        map_blake3_hash: &Hash,
        required_resources: RequiredResources,
        game_mod: GameModification,
        render_mod: RenderModification,
        timestamp: Duration,
        hint_start_camera_pos: vec2,
        config: &mut ConfigEngine,
        config_game: &mut ConfigGame,
        connect: GameConnect,
        game_options: GameStateCreateOptions,
        props: RenderGameCreateOptions,
        spatial_world: SpatialChatGameWorldTy,
        auto_cleanup: DisconnectAutoCleanup,
        expected_local_players: FxLinkedHashMap<u64, ClientConnectedPlayer>,
        local_player_id_counter: u64,
        active_local_player_id: u64,
        send_input_every_tick: bool,
        server_options: GameStateServerOptions,
    ) -> Self {
        info!("loading map: {}", map.as_str());
        let ping = timestamp.saturating_sub(network.server_connect_time);

        let demo_recorder_props = DemoRecorderCreateProps {
            base: DemoRecorderCreatePropsBase {
                map: map.clone(),
                map_hash: *map_blake3_hash,
                game_options: game_options.clone(),
                required_resources,
                client_local_infos: Self::character_net_infos(&expected_local_players, config_game),
                physics_module: game_mod.clone(),
                render_module: render_mod.clone(),
                physics_group_name: props.physics_group_name.clone(),
            },
            io: io.clone(),
            in_memory: None,
        };

        Self::Loading(Box::new(LoadingGame {
            network,
            resource_download_server: props.resource_download_server.clone(),
            map: ClientMapLoading::new(
                &base.sound,
                &base.graphics,
                &base.graphics_backend,
                &base.sys,
                "map/maps".as_ref(),
                map,
                Some(*map_blake3_hash),
                io,
                tp,
                game_mod,
                false,
                &config.dbg,
                game_options,
                props,
                connect.log.clone(),
            ),
            ping,
            prediction_timer: PredictionTimer::new(ping, timestamp),
            hint_start_camera_pos,
            connect,
            demo_recorder_props,
            spatial_world,
            auto_cleanup,

            base,

            local: LocalPlayerGameData {
                local_players: LocalPlayers::default(),
                expected_local_players,
                local_player_id_counter,
                active_local_player_id,
            },
            send_input_every_tick,
            server_options,
        }))
    }

    pub fn player_net_infos(
        expected_local_players: &FxLinkedHashMap<u64, ClientConnectedPlayer>,
        config_game: &mut ConfigGame,
    ) -> Vec<MsgClAddLocalPlayer> {
        expected_local_players
            .iter()
            .map(|(&id, player)| {
                let is_dummy = match player {
                    ClientConnectedPlayer::Connecting { is_dummy, .. } => is_dummy,
                    ClientConnectedPlayer::Connected { is_dummy, .. } => is_dummy,
                };
                let player_info = if let Some((info, copy_info)) = is_dummy
                    .then(|| {
                        config_game
                            .players
                            .get(config_game.profiles.dummy.index as usize)
                            .zip(config_game.players.get(config_game.profiles.main as usize))
                    })
                    .flatten()
                {
                    Game::network_char_info_from_config_for_dummy(
                        &config_game.cl,
                        info,
                        copy_info,
                        &config_game.profiles.dummy,
                    )
                } else if let Some(p) = config_game.players.get(config_game.profiles.main as usize)
                {
                    Self::network_char_info_from_config(&config_game.cl, p)
                } else {
                    // TODO: also support split screen some day
                    NetworkCharacterInfo::explicit_default()
                };
                MsgClAddLocalPlayer { player_info, id }
            })
            .collect()
    }

    pub fn character_net_infos(
        expected_local_players: &FxLinkedHashMap<u64, ClientConnectedPlayer>,
        config_game: &mut ConfigGame,
    ) -> Vec<NetworkCharacterInfo> {
        Self::player_net_infos(expected_local_players, config_game)
            .into_iter()
            .map(|p| p.player_info)
            .collect()
    }

    /// This function respects the copy assets from main player config.
    pub fn network_char_info_from_config_for_dummy(
        conf_client: &ConfigClient,
        player: &ConfigPlayer,
        copy_player: &ConfigPlayer,
        dummy_profile: &ConfigDummyProfile,
    ) -> NetworkCharacterInfo {
        let assets_player = if dummy_profile.copy_assets_from_main {
            copy_player
        } else {
            player
        };
        NetworkCharacterInfo {
            name: NetworkString::new(&player.name).unwrap(),
            clan: NetworkString::new(&player.clan).unwrap(),
            flag: NetworkString::new(player.flag.to_lowercase().replace("-", "_")).unwrap(),
            lang: NetworkString::new(&conf_client.language).unwrap(),

            skin: NetworkResourceKey::from_str_lossy(&player.skin.name),

            skin_info: (&player.skin).into(),
            laser_info: (&player.laser).into(),

            weapon: NetworkResourceKey::from_str_lossy(&assets_player.weapon),
            freeze: NetworkResourceKey::from_str_lossy(&assets_player.freeze),
            ninja: NetworkResourceKey::from_str_lossy(&assets_player.ninja),
            game: NetworkResourceKey::from_str_lossy(&assets_player.game),
            ctf: NetworkResourceKey::from_str_lossy(&assets_player.ctf),
            hud: NetworkResourceKey::from_str_lossy(&assets_player.hud),
            entities: NetworkResourceKey::from_str_lossy(&assets_player.entities),
            emoticons: NetworkResourceKey::from_str_lossy(&assets_player.emoticons),
            particles: NetworkResourceKey::from_str_lossy(&assets_player.particles),
            hook: NetworkResourceKey::from_str_lossy(&assets_player.hook),

            default_eyes: match player.eyes {
                ConfigTeeEye::Normal => TeeEye::Normal,
                ConfigTeeEye::Pain => TeeEye::Pain,
                ConfigTeeEye::Happy => TeeEye::Happy,
                ConfigTeeEye::Surprised => TeeEye::Surprised,
                ConfigTeeEye::Angry => TeeEye::Angry,
                ConfigTeeEye::Blink => TeeEye::Blink,
            },
        }
    }

    pub fn network_char_info_from_config(
        conf_client: &ConfigClient,
        p: &ConfigPlayer,
    ) -> NetworkCharacterInfo {
        Self::network_char_info_from_config_for_dummy(
            conf_client,
            p,
            p,
            &ConfigDummyProfile {
                index: 0,
                copy_assets_from_main: false,
                copy_binds_from_main: false,
            },
        )
    }

    pub fn update(
        &mut self,
        config: &ConfigEngine,
        config_game: &mut ConfigGame,
        ui_creator: &UiCreator,
        notifications: &mut ClientNotifications,
        entries: &[ConsoleEntry],
        cur_time: &Duration,
    ) {
        let mut selfi = Self::None;
        std::mem::swap(&mut selfi, self);
        *self = match selfi {
            Game::Active(mut game) => {
                game.update(cur_time, config_game, entries);
                Game::Active(game)
            }
            Game::None | Game::WaitingForFirstSnapshot(_) => {
                // nothing to do
                selfi
            }
            Game::Connecting(game) => Self::Connecting(game),
            Game::PrepareConnect(PrepareConnectGame {
                mut connect,
                account_task,
                dicts_task,
                server_cert_verify_task,
                auto_cleanup,
                base,
            }) => {
                if account_task.is_finished()
                    && dicts_task.is_finished()
                    && server_cert_verify_task.is_finished()
                {
                    match (server_cert_verify_task.get(), account_task.get()) {
                        (Ok((server_cert, servers)), Ok(account)) => {
                            // if servers were updated, store them in the browser data
                            if let Some((servers, time)) = servers {
                                connect.browser_data.set_servers(servers, time);
                            }
                            connect.server_cert = server_cert;

                            Self::connect(
                                base,
                                connect,
                                config,
                                account,
                                dicts_task.get().ok(),
                                auto_cleanup,
                            )
                        }
                        (Err(err1), Err(err2)) => Self::Err(anyhow!("{err1}. {err2}")),
                        (Err(err), Ok(_)) | (Ok(_), Err(err)) => Self::Err(err),
                    }
                } else {
                    Game::PrepareConnect(PrepareConnectGame {
                        connect,
                        account_task,
                        server_cert_verify_task,
                        dicts_task,
                        auto_cleanup,

                        base,
                    })
                }
            }
            Game::Loading(loading) => {
                let LoadingGame {
                    network,
                    mut map,
                    ping,
                    prediction_timer,
                    hint_start_camera_pos,
                    demo_recorder_props,
                    spatial_world,
                    auto_cleanup,
                    connect,
                    base,
                    resource_download_server,
                    local,
                    send_input_every_tick,
                    server_options,
                } = *loading;
                if map.is_fully_loaded() {
                    network.send_unordered_to_server(&ClientToServerMessage::Ready(MsgClReady {
                        players: Self::player_net_infos(&local.expected_local_players, config_game),
                        rcon_secret: connect.rcon_secret,
                    }));
                    let ClientMapLoading::Map(ClientMapFile::Game(mut map)) = map else {
                        panic!("remove this in future.")
                    };

                    let auto_demo_recorder = DemoRecorder::new(
                        demo_recorder_props.clone(),
                        map.game.game_tick_speed(),
                        Some("auto".as_ref()),
                        None,
                    );

                    let replay = Replay::new(
                        &demo_recorder_props.io,
                        &base.tp,
                        base.fonts.clone(),
                        demo_recorder_props.base.clone(),
                        map.game.game_tick_speed(),
                    );

                    // overwrite the options from the mod with the ones from the server
                    // in case they don't match
                    map.game.info.options = server_options.clone();
                    map.unpredicted_game.state.info.options = server_options;

                    let mut remote_console = RemoteConsoleBuilder::build(ui_creator);
                    remote_console.ui.ui_state.is_ui_open = false;

                    let events_pool = Pool::with_capacity(4);

                    connect
                        .log
                        .log("Map fully loaded, waiting for first snapshot from server now.");
                    Self::WaitingForFirstSnapshot(Box::new(ActiveGame {
                        network,
                        map: *map,

                        auto_demo_recorder: Some(auto_demo_recorder),
                        demo_recorder_props,

                        manual_demo_recorder: None,
                        race_demo_recorder: None,

                        ghost_recorder: None,
                        ghost_viewer: None,

                        replay,

                        game_data: GameData::new(base.sys.time_get(), prediction_timer, local),

                        events: events_pool.new(),
                        map_votes_loaded: Default::default(),
                        misc_votes_loaded: Default::default(),

                        render_players_pool: Pool::with_capacity(64),
                        render_observers_pool: Pool::with_capacity(2),

                        player_inputs_pool: Pool::with_capacity(4),
                        player_inputs_chainable_pool: Pool::with_capacity(4),
                        player_inputs_chain_pool: MtPool::with_capacity(4),
                        player_inputs_chain_data_pool: MtPool::with_capacity(4),
                        player_inputs_ser_helper_pool: Pool::with_capacity(4),
                        events_pool,

                        connect,

                        remote_console,
                        remote_console_logs: String::default(),
                        parser_cache: Default::default(),

                        requested_account_details: false,

                        next_player_info_change: None,

                        spatial_world,
                        auto_cleanup,

                        base,

                        resource_download_server,
                        send_input_every_tick,
                    }))
                } else {
                    map.continue_loading();
                    if let Err(err) = map.err() {
                        connect
                            .log
                            .set_mode(ConnectModes::ConnectingErr { msg: err });
                    }
                    Self::Loading(Box::new(LoadingGame {
                        network,
                        map,
                        ping,
                        prediction_timer,
                        hint_start_camera_pos,
                        connect,
                        demo_recorder_props,
                        spatial_world,
                        auto_cleanup,

                        base,

                        resource_download_server,

                        local,
                        send_input_every_tick,
                        server_options,
                    }))
                }
            }
            Game::Err(err) => {
                notifications.add_err(err.to_string(), Duration::from_secs(10));
                Self::None
            }
        }
    }

    fn on_load(
        &mut self,
        pipe: &mut GameMsgPipeline<'_>,
        game_server_info: &GameServerInfo,
        spatial_chat_scene: &SceneObject,
        timestamp: Duration,
        info: MsgSvServerInfo,
        local: LocalPlayerGameData,
        connect: GameConnect,
        base: GameBase,
        mut network: GameNetwork,
        auto_cleanup: DisconnectAutoCleanup,
        prediction_timer: PredictionTimer,
    ) {
        game_server_info.fill_game_info(GameInfo {
            map_name: info.map.to_string(),
        });
        game_server_info.fill_server_options(info.server_options.clone());
        pipe.spatial_chat.spatial_chat.support(info.spatial_chat);

        let mut expected_local_players = local.expected_local_players;
        expected_local_players.values_mut().for_each(|p| {
            match p {
                ClientConnectedPlayer::Connecting { .. } => {
                    // nothing to do
                }
                ClientConnectedPlayer::Connected {
                    is_dummy,
                    owns_dummies,
                    ..
                } => {
                    *p = ClientConnectedPlayer::Connecting {
                        is_dummy: *is_dummy,
                        owns_dummies: *owns_dummies,
                    };
                }
            }
        });
        let local_player_id_counter = local.local_player_id_counter;
        let active_local_player_id = local.active_local_player_id;

        let render_props = RenderGameCreateOptions {
            physics_group_name: info.server_options.physics_group_name.clone(),
            resource_http_download_url: Some(HTTP_RESOURCE_URL.try_into().unwrap()),
            resource_download_server: info.resource_server_fallback.map(|port| {
                format!("http://{}", SocketAddr::new(connect.addr.ip(), port))
                    .as_str()
                    .try_into()
                    .unwrap()
            }),
            fonts: base.fonts.clone(),
            sound_props: Default::default(),
            render_mod: RenderModTy::render_mod(&info.render_mod, pipe.config_game),
            required_resources: info.required_resources.clone(),
            client_local_infos: Self::character_net_infos(
                &expected_local_players,
                pipe.config_game,
            ),
        };
        network.server_connect_time = timestamp.saturating_sub(prediction_timer.ping_max());
        pipe.ui.is_ui_open = true;
        pipe.config.ui.path.route("connect");

        *self = Self::load(
            base,
            network,
            pipe.runtime_thread_pool,
            pipe.io,
            &info.map,
            &info.map_blake3_hash,
            info.required_resources,
            info.game_mod,
            info.render_mod,
            timestamp,
            info.hint_start_camera_pos,
            pipe.config,
            pipe.config_game,
            connect,
            GameStateCreateOptions {
                hint_max_characters: None, // TODO: get from server
                config: info.mod_config,
                account_db: None,
                initial_rcon_input: Default::default(),
            },
            render_props,
            if info.spatial_chat {
                {
                    pipe.spatial_chat
                        .create_world(spatial_chat_scene, pipe.config_game)
                }
            } else {
                SpatialChatGameWorldTy::None
            },
            auto_cleanup,
            expected_local_players,
            local_player_id_counter,
            active_local_player_id,
            info.send_input_every_tick,
            info.server_options,
        );
    }

    pub fn on_msg(
        &mut self,
        timestamp: Duration,
        msg: ServerToClientMessage<'static>,
        pipe: &mut GameMsgPipeline<'_>,
        game_server_info: &GameServerInfo,
        spatial_chat_scene: &SceneObject,
    ) {
        let mut selfi = Self::None;
        std::mem::swap(&mut selfi, self);
        let mut is_waiting = matches!(&selfi, Game::WaitingForFirstSnapshot(_));

        match selfi {
            Game::None | Game::Err(_) => {}
            Game::PrepareConnect(game) => {
                *self = Self::PrepareConnect(game);
            }
            Game::Connecting(connecting) => match msg {
                ServerToClientMessage::RequiresPassword => {
                    connecting.connect.log.log("Server requires a password.");
                    pipe.config.ui.path.route("connectpassword");
                    *self = Self::Connecting(connecting);
                }
                ServerToClientMessage::ServerInfo { info, overhead } => {
                    game_server_info.fill_game_info(GameInfo {
                        map_name: info.map.to_string(),
                    });
                    game_server_info.fill_server_options(info.server_options.clone());
                    pipe.spatial_chat.spatial_chat.support(info.spatial_chat);

                    let mut local_player_id_counter = 0;

                    let mut expected_local_players: FxLinkedHashMap<u64, ClientConnectedPlayer> =
                        Default::default();
                    expected_local_players.insert(
                        local_player_id_counter,
                        ClientConnectedPlayer::Connecting {
                            is_dummy: false,
                            owns_dummies: true,
                        },
                    );
                    let active_local_player_id = local_player_id_counter;
                    local_player_id_counter += 1;

                    let render_props = RenderGameCreateOptions {
                        physics_group_name: info.server_options.physics_group_name.clone(),
                        resource_http_download_url: Some(HTTP_RESOURCE_URL.try_into().unwrap()),
                        resource_download_server: info.resource_server_fallback.map(|port| {
                            Url::try_from(
                                format!(
                                    "http://{}",
                                    SocketAddr::new(connecting.connect.addr.ip(), port)
                                )
                                .as_str(),
                            )
                            .unwrap()
                        }),
                        fonts: connecting.base.fonts.clone(),
                        sound_props: Default::default(),
                        render_mod: RenderModTy::render_mod(&info.render_mod, pipe.config_game),
                        client_local_infos: Self::character_net_infos(
                            &expected_local_players,
                            pipe.config_game,
                        ),
                        required_resources: info.required_resources.clone(),
                    };

                    connecting
                        .connect
                        .log
                        .log("Got initial server info, loading game.");
                    *self = Self::load(
                        connecting.base,
                        connecting.network,
                        pipe.runtime_thread_pool,
                        pipe.io,
                        &info.map,
                        &info.map_blake3_hash,
                        info.required_resources,
                        info.game_mod,
                        info.render_mod,
                        timestamp.saturating_sub(overhead),
                        info.hint_start_camera_pos,
                        pipe.config,
                        pipe.config_game,
                        connecting.connect,
                        GameStateCreateOptions {
                            hint_max_characters: None, // TODO: get from server
                            config: info.mod_config,
                            account_db: None,
                            initial_rcon_input: Default::default(),
                        },
                        render_props,
                        if info.spatial_chat {
                            {
                                pipe.spatial_chat
                                    .create_world(spatial_chat_scene, pipe.config_game)
                            }
                        } else {
                            SpatialChatGameWorldTy::None
                        },
                        connecting.auto_cleanup,
                        expected_local_players,
                        local_player_id_counter,
                        active_local_player_id,
                        info.send_input_every_tick,
                        info.server_options,
                    );
                }
                ServerToClientMessage::QueueInfo(info) => {
                    connecting
                        .connect
                        .log
                        .log("Server put client into a connecting queue.");
                    connecting.connect.log.set_mode(ConnectModes::Queue {
                        msg: info.to_string(),
                    });
                    pipe.config.ui.path.route("connect");
                    *self = Self::Connecting(connecting);
                }
                _ => {
                    // collect msgs
                    *self = Self::Connecting(connecting);
                }
            },
            Game::Loading(loading) => {
                if let ServerToClientMessage::Load(info) = msg {
                    loading
                        .connect
                        .log
                        .log("Server send new server info (map change).");
                    self.on_load(
                        pipe,
                        game_server_info,
                        spatial_chat_scene,
                        timestamp,
                        info,
                        loading.local,
                        loading.connect,
                        loading.base,
                        loading.network,
                        loading.auto_cleanup,
                        loading.prediction_timer,
                    );
                } else {
                    *self = Self::Loading(loading);
                }
            }
            Game::WaitingForFirstSnapshot(mut game) | Game::Active(mut game) => {
                if let ServerToClientMessage::Load(info) = msg {
                    game.connect.log.log(
                        "Server send new server info (map change). \
                        Client was waiting for snapshot",
                    );
                    self.on_load(
                        pipe,
                        game_server_info,
                        spatial_chat_scene,
                        timestamp,
                        info,
                        game.game_data.local,
                        game.connect,
                        game.base,
                        game.network,
                        game.auto_cleanup,
                        game.game_data.prediction_timer,
                    );
                } else {
                    if let ServerToClientMessage::Snapshot {
                        overhead_time,
                        game_monotonic_tick_diff,
                        diff_id,
                        ..
                    } = &msg
                    {
                        if is_waiting {
                            game.connect
                                .log
                                .log("Got first snapshot, client fully connected.");
                            // set the first ping based on the intial packets,
                            // later prefer the network stats
                            let last_game_tick = pipe.sys.time_get()
                                - *overhead_time
                                - game.game_data.prediction_timer.pred_max_smoothing(
                                    Duration::from_nanos(
                                        (Duration::from_secs(1).as_nanos()
                                            / game.map.game.game_tick_speed().get() as u128)
                                            as u64,
                                    ),
                                );
                            game.game_data.last_game_tick = last_game_tick;

                            // set initial predicted game monotonic tick based on this first snapshot
                            game.map.game.predicted_game_monotonic_tick = diff_id
                                .and_then(|diff_id| {
                                    game.game_data
                                        .snap_storage
                                        .get(&diff_id)
                                        .map(|old| *game_monotonic_tick_diff + old.monotonic_tick)
                                })
                                .unwrap_or(*game_monotonic_tick_diff);

                            is_waiting = false;
                            pipe.ui.is_ui_open = false;
                            pipe.config.ui.path.route("ingame");
                        }
                    }
                    game.on_msg(&timestamp, msg, pipe);

                    if is_waiting {
                        *self = Self::WaitingForFirstSnapshot(game);
                    } else {
                        *self = Self::Active(game);
                    }
                }
            }
        }
    }

    pub fn get_remote_console(&self) -> Option<&RemoteConsole> {
        if let Game::Active(game) = self {
            Some(&game.remote_console)
        } else {
            None
        }
    }
    pub fn get_remote_console_mut(&mut self) -> Option<&mut RemoteConsole> {
        if let Game::Active(game) = self {
            Some(&mut game.remote_console)
        } else {
            None
        }
    }
    pub fn remote_console_open(&self) -> bool {
        self.get_remote_console()
            .is_some_and(|c| c.ui.ui_state.is_ui_open)
    }
    pub fn active_game(&self) -> Option<&ActiveGame> {
        if let Game::Active(game) = self {
            Some(game)
        } else {
            None
        }
    }
    pub fn active_game_mut(&mut self) -> Option<&mut ActiveGame> {
        if let Game::Active(game) = self {
            Some(game)
        } else {
            None
        }
    }
}
