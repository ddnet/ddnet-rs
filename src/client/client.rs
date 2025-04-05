use std::{
    borrow::Borrow, cell::RefCell, net::SocketAddr, num::NonZeroUsize, path::PathBuf, rc::Rc,
    sync::Arc, time::Duration,
};

use anyhow::anyhow;
use base::{
    benchmark::Benchmark,
    linked_hash_map_view::FxLinkedHashMap,
    network_string::NetworkString,
    system::{System, SystemTimeInterface},
};
use base_fs::filesys::FileSystem;

use base_http::http::HttpClient;
use base_io::io::{Io, IoFileSys};
use binds::binds::{BindActionsHotkey, BindActionsLocalPlayer};
use client_accounts::accounts::{Accounts, AccountsLoading};
use client_console::console::{
    console::{ConsoleEvents, ConsoleRenderPipe},
    local_console::{LocalConsole, LocalConsoleBuilder, LocalConsoleEvent},
    remote_console::RemoteConsoleEvent,
};
use client_containers::{
    container::ContainerLoadOptions,
    entities::{EntitiesContainer, ENTITIES_CONTAINER_PATH},
    skins::{SkinContainer, SKIN_CONTAINER_PATH},
};
use client_demo::{DemoVideoEncodeProperties, DemoViewer, DemoViewerSettings, EncoderSettings};
use client_map::client_map::{ClientMapFile, ClientMapLoading, GameMap};
use client_notifications::overlay::ClientNotifications;
use client_render_base::{
    map::{
        map::RenderMap,
        map_pipeline::MapPipeline,
        render_pipe::{Camera, GameTimeInfo, RenderPipeline, RenderPipelineBase},
    },
    render::tee::RenderTee,
};
use client_render_game::render_game::{
    EmoteWheelInput, ObservedAnchoredSize, ObservedPlayer, PlayerFeedbackEvent, RenderForPlayer,
    RenderGameCreateOptions, RenderGameForPlayer, RenderGameInput, RenderGameInterface,
    RenderGameSettings, RenderModTy, RenderPlayerCameraMode,
};
use client_types::console::{entries_to_parser, ConsoleEntry};
use client_ui::{
    chat::user_data::{ChatEvent, ChatMode},
    connect::{
        page::ConnectingUi,
        user_data::{ConnectMode, ConnectModes},
    },
    console::utils::run_commands,
    events::{UiEvent, UiEvents},
    ingame_menu::{
        account_info::AccountInfo,
        client_info::{ActiveClientInfo, ClientInfo},
        page::IngameMenuUi,
        raw_input_info::{self, RawInputInfo},
        server_info::GameServerInfo,
        server_players::ServerPlayers,
        votes::Votes,
    },
    main_menu::{
        features::EnabledFeatures,
        monitors::{UiMonitor, UiMonitorVideoMode, UiMonitors},
        page::MainMenuUi,
        player_settings_ntfy::PlayerSettingsSync,
        spatial_chat::SpatialChat,
    },
    spectator_selection::user_data::SpectatorSelectionEvent,
    utils::render_tee_for_ui,
};
use command_parser::parser::ParserCache;
use config::config::{ConfigEngine, ConfigMonitor};
use demo::recorder::DemoRecorder;
use editor::editor::{EditorInterface, EditorResult};
use egui::{CursorIcon, FontDefinitions};
use game_config::config::{Config, ConfigGame, ConfigMap};
use graphics::graphics::graphics::Graphics;
use graphics_backend::{
    backend::{
        GraphicsBackend, GraphicsBackendBase, GraphicsBackendIoLoading, GraphicsBackendLoading,
    },
    window::BackendWindow,
};

use editor_wasm::editor::editor_wasm_manager::{EditorState, EditorWasmManager};
use game_interface::{
    client_commands::{ClientCameraMode, ClientCommand, JoinStage},
    events::EventClientInfo,
    interface::GameStateInterface,
    types::{
        character_info::NetworkCharacterInfo,
        game::{GameEntityId, GameTickType},
        id_types::{CharacterId, PlayerId, StageId},
        input::{
            dyn_cam::CharacterInputDynCamOffset, CharacterInputConsumableDiff, CharacterInputInfo,
        },
        render::{
            character::{CharacterInfo, PlayerCameraMode, PlayerIngameMode, TeeEye},
            game::game_match::MatchSide,
            scoreboard::ScoreboardGameType,
            stage::StageRenderInfo,
        },
        snapshot::SnapshotClientInfo,
        ticks::TickOptions,
    },
    votes::{VoteIdentifierType, VoteType, Voted},
};
use game_server::{local_server::start_local_server, server::Server};
use graphics_types::rendering::ColorRgba;
use input_binds::binds::{BindKey, Binds};
use math::math::{
    length, normalize, normalize_pre_length,
    vector::{dvec2, vec2},
};
use native::{
    input::InputEventHandler,
    native::{
        app::NativeApp, FromNativeImpl, FromNativeLoadingImpl, KeyCode, Native,
        NativeCreateOptions, NativeDisplayBackend, NativeImpl, NativeWindowMonitorDetails,
        NativeWindowOptions, PhysicalKey, PhysicalSize, WindowEvent, WindowMode,
    },
};
use network::network::types::NetworkInOrderChannel;
use pool::{
    datatypes::{PoolFxLinkedHashMap, StringPool},
    pool::Pool,
};
use rayon::ThreadPool;
use sound::{scene_object::SceneObject, sound::SoundManager};
use sound_backend::sound_backend::SoundBackend;
use steam::{init_steam, traits::SteamRaii};
use ui_base::{
    font_data::{UiFontData, UiFontDataLoading},
    types::UiRenderPipe,
    ui::UiCreator,
};
use ui_wasm_manager::{UiManagerBase, UiPageLoadingType, UiWasmManagerErrorPageErr};

use crate::{
    game::Game,
    localplayer::ClientPlayer,
    ui::pages::{
        editor::tee::TeeEditor, loading::LoadingPage, not_found::Error404Page, test::ColorTest,
    },
};

use game_base::{
    assets_url::HTTP_RESOURCE_URL,
    game_types::{intra_tick_time, intra_tick_time_to_ratio, is_next_tick, time_until_tick},
    local_server_info::{LocalServerInfo, LocalServerState},
    network::messages::{GameModification, MsgClAddLocalPlayer, MsgClChatMsg, MsgClLoadVotes},
    player_input::PlayerInput,
    server_browser::ServerBrowserData,
};

use game_network::messages::{ClientToServerMessage, ClientToServerPlayerMessage};

use super::{
    game::{
        data::{ClientConnectedPlayer, GameData},
        types::{DisconnectAutoCleanup, GameBase, GameConnect, GameMsgPipeline, ServerCertMode},
    },
    game_events::{GameEventPipeline, GameEventsClient},
    input::input_handling::{InputEv, InputHandling, InputHandlingEvent},
    localplayer::ClientPlayerInputPerTick,
    overlays::client_stats::{ClientStats, ClientStatsRenderPipe, DebugHudRenderPipe},
    spatial_chat::spatial_chat::{self, SpatialChatGameWorldTy, SpatialChatGameWorldTyRef},
};

type UiManager = UiManagerBase<Config>;

pub fn ddnet_main(
    start_arguments: Vec<String>,
    sys: System,
    shared_info: Arc<LocalServerInfo>,
    app: NativeApp,
) -> anyhow::Result<()> {
    let io = IoFileSys::new(|rt| {
        Arc::new(
            FileSystem::new(rt, "org", "", "DDNet-Rs-Alpha", "DDNet-Accounts")
                .expect("most like you are missing a data directory"),
        )
    });

    let mut config_engine = config_fs::load(&io).unwrap_or_default();

    let benchmark = Benchmark::new(config_engine.dbg.bench);

    let mut config_game = game_config_fs::fs::load(&io).unwrap_or_default();
    benchmark.bench("loading client config");

    let mut has_startup_errors = false;
    let local_console_builder = if !start_arguments.is_empty() {
        let local_console_builder = LocalConsoleBuilder::default();
        let parser_entries = entries_to_parser(&local_console_builder.entries);
        for line in start_arguments.iter().filter(|l| !l.is_empty()) {
            let cmds = command_parser::parser::parse(
                line,
                &parser_entries,
                &local_console_builder.parser_cache,
            );
            let mut res = String::default();
            let cur_cmds_succeeded = run_commands(
                &cmds,
                &local_console_builder.entries,
                &mut config_engine,
                &mut config_game,
                &mut res,
                true,
            );
            log::debug!("{}", res);
            if !cur_cmds_succeeded {
                log::error!("{}", res);
            }
            let mut has_events = true;
            let mut count = 0;
            while has_events {
                has_events = false;
                let events = local_console_builder.console_events.take();
                for ev in events {
                    if let LocalConsoleEvent::Exec { file_path } = &ev {
                        ClientNativeImpl::handle_exec(
                            &io,
                            file_path.clone(),
                            &mut config_engine,
                            &mut config_game,
                            &local_console_builder.entries,
                            &local_console_builder.parser_cache,
                            |err| {
                                log::error!("{}", err);
                                has_startup_errors = true;
                            },
                            |msg| {
                                log::info!("{}", msg);
                            },
                        );

                        has_events = true;
                    } else {
                        local_console_builder.console_events.push(ev);
                    }
                }

                count += 1;

                if count >= 16 {
                    has_startup_errors = true;
                    log::error!("Exec recursion count reached 16, which is the upper limit.");
                    break;
                }
            }
            has_startup_errors |= !cur_cmds_succeeded;
        }
        benchmark.bench("parsing start arguments");
        Some(local_console_builder)
    } else {
        None
    };

    let graphics_backend_io_loading = GraphicsBackendIoLoading::new(&config_engine.gfx, &io);
    // first prepare all io tasks of all components
    benchmark.bench("load_io of graphics backend");

    let sys_time = sys.time.clone();
    let do_bench = config_engine.dbg.bench;
    let dbg_input = config_engine.inp.dbg_mode;

    let config_wnd = config_engine.wnd.clone();

    let client = ClientNativeLoadingImpl {
        sys,
        shared_info,
        io,
        config_engine,
        config_game,
        graphics_backend_io_loading,
        graphics_backend_loading: None,
        local_console_builder,
        has_startup_errors,
    };
    let logical_pixels = native::native::Pixels {
        width: config_wnd.window_width,
        height: config_wnd.window_height,
    };
    Native::run_loop::<ClientNativeImpl, _>(
        client,
        app,
        NativeCreateOptions {
            do_bench,
            title: "DDNet".to_string(),
            sys: &sys_time,
            dbg_input,
            start_arguments,
            window: native::native::NativeWindowOptions {
                mode: if config_wnd.fullscreen {
                    WindowMode::Fullscreen {
                        resolution: (config_wnd.fullscreen_width != 0
                            && config_wnd.fullscreen_height != 0)
                            .then_some(native::native::Pixels {
                                width: config_wnd.fullscreen_width,
                                height: config_wnd.fullscreen_height,
                            }),
                        fallback_window: logical_pixels,
                    }
                } else {
                    WindowMode::Windowed(logical_pixels)
                },
                decorated: config_wnd.decorated,
                maximized: config_wnd.maximized,
                refresh_rate_milli_hertz: config_wnd.refresh_rate_mhz,
                monitor: (!config_wnd.monitor.name.is_empty()
                    && config_wnd.monitor.width != 0
                    && config_wnd.monitor.height != 0)
                    .then_some(NativeWindowMonitorDetails {
                        name: config_wnd.monitor.name,
                        size: PhysicalSize {
                            width: config_wnd.monitor.width,
                            height: config_wnd.monitor.height,
                        },
                    }),
            },
        },
    )?;
    Ok(())
}

#[cfg(feature = "alloc_track")]
fn track_report() {
    let total_consumption = std::cell::Cell::new(0);
    let report = alloc_track::backtrace_report(|_, stats| {
        let cur_consumption = stats.allocated - stats.freed;
        total_consumption.set(total_consumption.get() + cur_consumption);
        cur_consumption > 0
    });
    std::fs::write(
        "trace.txt",
        format!("BACKTRACES\n{report}\nTotal:{}", total_consumption.get()),
    )
    .unwrap();
}

#[cfg(feature = "alloc_stats")]
fn stats_report() {
    let alloc: &stats_alloc::StatsAlloc<std::alloc::System> = &stats_alloc::INSTRUMENTED_SYSTEM;
    let stats = alloc.stats();
    let cur_alloc = stats
        .bytes_allocated
        .saturating_sub(stats.bytes_deallocated);
    std::fs::write(
        "trace.txt",
        format!("Stats:\n{:?}\nCur usage:{}", stats, cur_alloc),
    )
    .unwrap();
}

#[derive(Debug)]
enum ConnectLocalServerResult {
    Connect {
        addr: SocketAddr,
        server_cert: ServerCertMode,
        rcon_secret: Option<[u8; 32]>,
    },
    KeepConnecting {
        addresses: Vec<SocketAddr>,
    },
    ErrOrNotLocalServerAddr {
        addresses: Vec<SocketAddr>,
    },
}

struct ClientNativeLoadingImpl {
    sys: System,
    shared_info: Arc<LocalServerInfo>,
    io: IoFileSys,
    config_engine: ConfigEngine,
    config_game: ConfigGame,
    graphics_backend_io_loading: GraphicsBackendIoLoading,
    graphics_backend_loading: Option<GraphicsBackendLoading>,

    local_console_builder: Option<LocalConsoleBuilder>,
    has_startup_errors: bool,
}

struct ClientNativeImpl {
    sys: System,
    shared_info: Arc<LocalServerInfo>,

    client_info: ClientInfo,
    account_info: AccountInfo,
    spatial_chat: spatial_chat::SpatialChat,
    player_settings_sync: PlayerSettingsSync,
    raw_input_info: RawInputInfo,
    browser_data: ServerBrowserData,

    scene: SceneObject,

    sound: SoundManager,
    sound_backend: Rc<SoundBackend>,
    game: Game,
    connect_info: ConnectMode,
    demo_player: Option<DemoViewer>,
    client_stats: ClientStats,
    notifications: ClientNotifications,
    thread_pool: Arc<ThreadPool>,
    io: Io,
    config: Config,
    cur_time: Duration,
    last_refresh_rate_time: Duration,

    editor: EditorState,

    entities_container: EntitiesContainer,
    skin_container: SkinContainer,
    render_tee: RenderTee,

    local_console: LocalConsole,
    console_logs: String,

    ui_manager: UiManager,
    ui_events: UiEvents,
    font_data: FontDefinitions,
    ui_creator: UiCreator,

    /// RAII object that must live as long as the app
    _steam_rt: Box<dyn SteamRaii>,

    // ui-shared objects
    accounts: Arc<Accounts>,
    server_players: ServerPlayers,
    game_server_info: GameServerInfo,
    votes: Votes,

    menu_map: ClientMapLoading,

    global_binds: Binds<BindActionsHotkey>,

    // pools & helpers
    string_pool: StringPool,

    // input & helper
    inp_manager: InputHandling,

    // put graphics at the end, so it's dropped last
    graphics: Graphics,
    graphics_backend: Rc<GraphicsBackend>,
}

impl ClientNativeImpl {
    fn check_local_server_error(
        state: &mut LocalServerState,
        notifications: &mut ClientNotifications,
    ) -> anyhow::Result<()> {
        match state {
            LocalServerState::None => {
                // ignore
            }
            LocalServerState::Starting { thread, .. } | LocalServerState::Ready { thread, .. } => {
                if thread.thread.is_finished() {
                    match thread.thread.try_join() {
                        Err(err) | Ok(Some(Err(err))) => {
                            notifications.add_err(
                                format!("Failed to start local server: {err}"),
                                Duration::from_secs(10),
                            );
                            return Err(err);
                        }
                        Ok(Some(Ok(_))) | Ok(None) => {
                            // ignore
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn connect_local_server(
        &mut self,
        addresses: Vec<SocketAddr>,
        can_start_local_server: bool,
    ) -> ConnectLocalServerResult {
        if addresses.iter().any(|addr| addr.ip().is_loopback()) {
            let mut state = self.shared_info.state.lock().unwrap();
            if let LocalServerState::Ready { connect_info, .. } = &mut *state {
                let rcon_secret = Some(connect_info.rcon_secret);
                let server_cert = ServerCertMode::Hash(connect_info.server_cert_hash);
                let addr = match connect_info.sock_addr {
                    SocketAddr::V4(addr) => {
                        SocketAddr::new("127.0.0.1".parse().unwrap(), addr.port())
                    }
                    SocketAddr::V6(addr) => SocketAddr::new("::1".parse().unwrap(), addr.port()),
                };
                drop(state);
                ConnectLocalServerResult::Connect {
                    addr,
                    server_cert,
                    rcon_secret,
                }
            } else {
                let keep_connecting =
                    can_start_local_server || matches!(*state, LocalServerState::Starting { .. });
                drop(state);
                if can_start_local_server {
                    // try to start the local server
                    start_local_server(
                        &self.sys,
                        self.shared_info.clone(),
                        self.config.engine.clone(),
                        self.config.game.clone(),
                    );
                }

                if Self::check_local_server_error(
                    &mut self.shared_info.state.lock().unwrap(),
                    &mut self.notifications,
                )
                .is_err()
                    || !keep_connecting
                {
                    ConnectLocalServerResult::ErrOrNotLocalServerAddr { addresses }
                } else {
                    ConnectLocalServerResult::KeepConnecting { addresses }
                }
            }
        } else {
            ConnectLocalServerResult::ErrOrNotLocalServerAddr { addresses }
        }
    }

    fn on_window_change(&mut self, native: &mut dyn NativeImpl) {
        let config_wnd = &self.config.engine.wnd;

        let logical_pixels = native::native::Pixels {
            width: config_wnd.window_width,
            height: config_wnd.window_height,
        };
        if let Err(err) = native.set_window_config(native::native::NativeWindowOptions {
            mode: if config_wnd.fullscreen {
                WindowMode::Fullscreen {
                    resolution: (config_wnd.fullscreen_width != 0
                        && config_wnd.fullscreen_height != 0)
                        .then_some(native::native::Pixels {
                            width: config_wnd.fullscreen_width,
                            height: config_wnd.fullscreen_height,
                        }),
                    fallback_window: logical_pixels,
                }
            } else {
                WindowMode::Windowed(logical_pixels)
            },
            decorated: config_wnd.decorated,
            maximized: config_wnd.maximized,
            refresh_rate_milli_hertz: config_wnd.refresh_rate_mhz,
            monitor: (!config_wnd.monitor.name.is_empty()
                && config_wnd.monitor.width != 0
                && config_wnd.monitor.height != 0)
                .then_some(NativeWindowMonitorDetails {
                    name: config_wnd.monitor.name.clone(),
                    size: PhysicalSize {
                        width: config_wnd.monitor.width,
                        height: config_wnd.monitor.height,
                    },
                }),
        }) {
            log::warn!("Failed to apply window settings: {err}");
            self.notifications
                .add_err(err.to_string(), Duration::from_secs(10));
        }
    }

    fn on_vsync_change(&mut self) {
        self.graphics.vsync(self.config.engine.gl.vsync);
    }

    fn on_msaa_change(&mut self) {
        self.graphics
            .multi_sampling(self.config.engine.gl.msaa_samples);
    }

    fn render_menu_background_map(&mut self) {
        if let Some(map) = self.menu_map.continue_loading() {
            let intra_tick_time = self.sys.time_get();
            let ClientMapFile::Menu { render } = &map else {
                panic!("this was not a menu map")
            };
            let render = render.try_get().unwrap();
            render.render.render_full_design(
                &render.data.buffered_map.map_visual,
                &mut RenderPipeline {
                    base: RenderPipelineBase {
                        map: &render.data.buffered_map.map_visual,
                        config: &ConfigMap::default(),
                        cur_time: &self.sys.time_get(),
                        cur_anim_time: &RenderMap::calc_anim_time(
                            50.try_into().unwrap(),
                            (self.sys.time_get().as_millis() / (1000 / 50)).max(1) as GameTickType,
                            &intra_tick_time,
                        ),
                        camera: &Camera {
                            pos: vec2::new(21.0, 15.0),
                            zoom: 1.0,
                            parallax_aware_zoom: true,
                            forced_aspect_ratio: None,
                        },
                        entities_container: &mut self.entities_container,
                        entities_key: None,
                        physics_group_name: "vanilla",
                        map_sound_volume: self.config.game.snd.render.map_sound_volume
                            * self.config.game.snd.global_volume,
                    },
                    buffered_map: &render.data.buffered_map,
                },
            )
        }
    }

    fn render_game(&mut self, native: &mut dyn NativeImpl) {
        let remote_console_open = self.game.remote_console_open();
        if let Game::Active(game) = &mut self.game {
            // prepare input
            let events = std::mem::replace(&mut game.events, game.events_pool.new());

            let GameMap {
                render,
                game: game_state,
                unpredicted_game,
            } = &mut game.map;
            let is_menu_open = self.ui_manager.ui.ui_state.is_ui_open
                || self.local_console.ui.ui_state.is_ui_open
                || remote_console_open
                || self.editor.is_open()
                || self.demo_player.is_some();

            let intra_tick_ratio = intra_tick_time_to_ratio(
                game.game_data.intra_tick_time,
                game_state.game_tick_speed(),
            );

            let main_local_char_spec = game
                .game_data
                .local
                .active_local_player()
                .map(|(_, p)| match &p.input_cam_mode {
                    PlayerCameraMode::Default => false,
                    PlayerCameraMode::Free => true,
                    PlayerCameraMode::LockedTo { locked_ingame, .. }
                    | PlayerCameraMode::LockedOn { locked_ingame, .. } => !*locked_ingame,
                })
                .unwrap_or_default();

            let (
                main_game,
                mut local_predicted_game,
                main_intra_tick_ratio,
                predicted_intra_tick_ratio,
            ) = if self.config.game.cl.anti_ping && !main_local_char_spec {
                (game_state, None, intra_tick_ratio, intra_tick_ratio)
            } else {
                let ticks_per_second = game_state.game_tick_speed();
                let tick_time = time_until_tick(ticks_per_second);
                let sub_ticks = (game
                    .game_data
                    .prediction_timer
                    .pred_tick_offset(tick_time)
                    .as_nanos()
                    / tick_time.as_nanos()) as GameTickType;
                let first_tick = game_state
                    .predicted_game_monotonic_tick
                    .saturating_sub(sub_ticks);

                let last_snaps = &game.game_data.last_snaps;
                let mut it = last_snaps.range(0..=first_tick);
                let snap_range1 = it.next_back();
                let snap_range2 = it.next_back();
                let snap_range = snap_range1
                    .zip(snap_range2.or(snap_range1))
                    .map(|((tick2, _), (tick1, _))| tick2.saturating_sub(*tick1))
                    .unwrap_or(1)
                    .max(1);
                // depending on how many snapshots arrive, lower the first tick based on that
                let first_tick = first_tick.saturating_sub(snap_range);

                let prev_snap = game.game_data.last_snaps.range(0..=first_tick).next_back();
                let prev_tick = prev_snap.map(|(tick, _)| *tick).unwrap_or(first_tick);
                let next_tick = game
                    .game_data
                    .last_snaps
                    .range(first_tick + 1..)
                    .next()
                    .map(|(tick, _)| *tick)
                    .unwrap_or(first_tick);
                let first_tick = first_tick.saturating_sub(prev_tick);
                let tick_diff = next_tick.saturating_sub(prev_tick).max(1);

                let unpredicted_intra_tick_ratio =
                    (first_tick as f64 + intra_tick_ratio) / tick_diff as f64;

                unpredicted_game.from_snapshots(&game.game_data.last_snaps, prev_tick + first_tick);
                (
                    &mut unpredicted_game.state,
                    (!main_local_char_spec).then_some(game_state),
                    unpredicted_intra_tick_ratio,
                    intra_tick_ratio,
                )
            };

            let mut character_infos = main_game.collect_characters_info();
            if let Some(local_predicted_game) = local_predicted_game.as_deref_mut() {
                // replace the local character info with the predicted one
                let mut predicted_character_infos = local_predicted_game.collect_characters_info();
                for id in game.game_data.local.local_players.keys() {
                    if let Some(char) = predicted_character_infos.remove(id) {
                        character_infos.insert(*id, char);
                    }
                }
            }

            if self.server_players.needs_player_infos() {
                self.server_players.fill_player_info(
                    character_infos
                        .iter()
                        .filter_map(|(&id, char)| {
                            char.player_info
                                .is_some()
                                .then_some((id, (**char.info).clone()))
                        })
                        .collect(),
                );
            }
            if self.client_info.wants_active_client_info() {
                if let Some(player_info) = game
                    .game_data
                    .local
                    .active_local_player()
                    .and_then(|(id, _)| character_infos.get(id))
                    .and_then(|c: &CharacterInfo| c.player_info.as_ref())
                {
                    let scoreboard_info = main_game.collect_scoreboard_info();
                    self.client_info.set_active_client_info(ActiveClientInfo {
                        ingame_mode: player_info.ingame_mode,
                        stage_names: {
                            let it: Box<dyn Iterator<Item = _>> = match &scoreboard_info.game {
                                ScoreboardGameType::SidedPlay {
                                    red_stages,
                                    blue_stages,
                                    ..
                                } => Box::new(red_stages.values().chain(blue_stages.values())),
                                ScoreboardGameType::SoloPlay { stages, .. } => {
                                    Box::new(stages.values())
                                }
                            };
                            it.map(|s| s.name.to_string()).collect()
                        },
                    });
                }
            }

            let mut stages = main_game.all_stages(main_intra_tick_ratio);
            if let Some(local_predicted_game) = local_predicted_game.as_deref_mut() {
                // replace the local stages with the predicted one
                let mut predicted_stages =
                    local_predicted_game.all_stages(predicted_intra_tick_ratio);
                for id in game.game_data.local.local_players.keys() {
                    if let Some((stage_id, mut pred_stage)) = character_infos
                        .get(id)
                        .and_then(|char| char.stage_id)
                        .and_then(|stage_id| {
                            predicted_stages
                                .remove(&stage_id)
                                .map(|stage| (stage_id, stage))
                        })
                    {
                        let stage = stages.entry(stage_id);
                        match stage {
                            hashlink::lru_cache::Entry::Occupied(mut stage) => {
                                let stage = stage.get_mut();
                                for id in game
                                    .game_data
                                    .local
                                    .local_players
                                    .keys()
                                    .filter_map(|id| {
                                        character_infos.get(id).and_then(|char| {
                                            char.stage_id.map(|stage_id| (id, stage_id))
                                        })
                                    })
                                    .filter_map(|(id, find_stage_id)| {
                                        (find_stage_id == stage_id).then_some(id)
                                    })
                                {
                                    if let Some(mut char) = pred_stage.world.characters.remove(id) {
                                        // if hook cannot be predicted because the hooked player is not, then handle this case
                                        if char.lerped_hook.is_some_and(|hook| {
                                            hook.hooked_char.is_some_and(|id| {
                                                !game
                                                    .game_data
                                                    .local
                                                    .local_players
                                                    .contains_key(&id)
                                            })
                                        }) {
                                            if let Some(unpredicted_char) =
                                                stage.world.characters.get(id)
                                            {
                                                char.lerped_hook = unpredicted_char.lerped_hook;
                                            }
                                        }
                                        stage.world.characters.insert(*id, char);
                                    }

                                    // if any of the local chars is a ctf flag carrier, add that ctf flag too
                                    while let Some(flag_id) = pred_stage
                                        .world
                                        .ctf_flags
                                        .iter()
                                        .find_map(|(flag_id, flag)| {
                                            (flag.owner_id == Some(*id)).then_some(flag_id)
                                        })
                                        .copied()
                                    {
                                        stage.world.ctf_flags.insert(
                                            flag_id,
                                            pred_stage.world.ctf_flags.remove(&flag_id).unwrap(),
                                        );
                                    }
                                }
                                stage.game = pred_stage.game;
                                stage.game_ticks_passed = pred_stage.game_ticks_passed;
                            }
                            hashlink::lru_cache::Entry::Vacant(entry) => {
                                entry.insert(pred_stage);
                            }
                        }
                    }
                }
            }

            if let SpatialChatGameWorldTy::World(spatial_world) = &mut game.spatial_world {
                spatial_chat::SpatialChat::on_entity_positions(
                    Some(spatial_world),
                    stages
                        .values()
                        .flat_map(|stage| {
                            stage
                                .world
                                .characters
                                .iter()
                                .map(|(id, c)| (*id, c.lerped_pos))
                        })
                        .collect(),
                );
            }

            let mut render_game_input = RenderGameInput {
                players: game.render_players_pool.new(),
                dummies: game.game_data.player_ids_pool.new(),
                events,
                chat_msgs: {
                    let mut chat_msgs = game.game_data.chat_msgs_pool.new();
                    chat_msgs.append(&mut game.game_data.chat_msgs);
                    chat_msgs
                },
                vote: game.game_data.vote.as_ref().map(|(v, voted, timestamp)| {
                    (
                        v.clone(),
                        *voted,
                        v.remaining_time.saturating_sub(
                            self.cur_time
                                .saturating_sub(*timestamp)
                                .saturating_sub(game.game_data.prediction_timer.ping_average()),
                        ),
                    )
                }),
                character_infos,
                stages,
                scoreboard_info: None,
                game_time_info: GameTimeInfo {
                    ticks_per_second: main_game.game_tick_speed(),
                    intra_tick_time: game.game_data.intra_tick_time,
                },
                settings: RenderGameSettings::new(
                    &self.config.game.cl.render,
                    &self.config.game.snd.render,
                    self.graphics.canvas_handle.pixels_per_point(),
                    1.0,
                    self.config.game.cl.anti_ping,
                    self.config.game.snd.global_volume,
                ),
                ext: main_game.collect_render_ext(),
            };

            type CharacterInfos = PoolFxLinkedHashMap<CharacterId, CharacterInfo>;
            type StageRenderInfos = PoolFxLinkedHashMap<StageId, StageRenderInfo>;
            let mut fill_for_player = {
                |client_player: (&PlayerId, &mut ClientPlayer),
                 character_infos: &CharacterInfos,
                 stages_render_infos: &mut StageRenderInfos|
                 -> (PlayerId, RenderGameForPlayer) {
                    let (&player_id, client_player) = client_player;
                    let (camera_player_id, is_free_cam) = match &client_player.input_cam_mode {
                        PlayerCameraMode::Default | PlayerCameraMode::LockedTo { .. } => {
                            (player_id, false)
                        }
                        PlayerCameraMode::Free => (player_id, true),
                        PlayerCameraMode::LockedOn { character_ids, .. } => (
                            {
                                if character_ids.len() == 1 {
                                    *character_ids.iter().next().unwrap()
                                } else {
                                    player_id
                                }
                            },
                            false,
                        ),
                    };
                    let local_player_render_info = if let Some(local_predicted_game) =
                        local_predicted_game.as_deref_mut()
                    {
                        // prefer local predicted version
                        local_predicted_game.collect_character_local_render_info(&camera_player_id)
                    } else {
                        main_game.collect_character_local_render_info(&camera_player_id)
                    };

                    let character_info = character_infos.get(&player_id);
                    if let Some(player) = character_info.and_then(|c| {
                        c.stage_id
                            .and_then(|stage_id| stages_render_infos.get_mut(&stage_id))
                            .and_then(|s| s.world.characters.get_mut(&player_id))
                    }) {
                        player.lerped_cursor_pos = client_player.input.inp.cursor.to_vec2();
                        player.lerped_dyn_cam_offset =
                            client_player.input.inp.dyn_cam_offset.to_vec2();

                        player.hook_collision = player.hook_collision.map(|mut hook_col| {
                            let dir = hook_col.end - hook_col.start;
                            let dir_len = length(&dir);
                            let dir = normalize(&client_player.cursor_pos);
                            let dir = vec2::new(dir.x as f32, dir.y as f32);
                            hook_col.end = hook_col.start + dir * dir_len;
                            hook_col
                        });
                    }

                    // update freecam position
                    if !is_free_cam {
                        let character_info = character_infos.get(&camera_player_id);
                        if let Some(player) = character_info.and_then(|c| {
                            c.stage_id
                                .and_then(|stage_id| stages_render_infos.get_mut(&stage_id))
                                .and_then(|s| s.world.characters.get_mut(&camera_player_id))
                        }) {
                            client_player.free_cam_pos =
                                dvec2::new(player.lerped_pos.x as f64, player.lerped_pos.y as f64);
                        }
                    }

                    let (cam_mode, force_scoreboard_visible, is_spectator) =
                        match character_info.and_then(|c| c.player_info.as_ref()) {
                            Some(info) => (
                                match &info.cam_mode {
                                    PlayerCameraMode::Default => RenderPlayerCameraMode::Default,
                                    PlayerCameraMode::Free => RenderPlayerCameraMode::AtPos {
                                        pos: vec2::new(
                                            client_player.free_cam_pos.x as f32,
                                            client_player.free_cam_pos.y as f32,
                                        ),
                                        locked_ingame: false,
                                    },
                                    PlayerCameraMode::LockedTo { pos, locked_ingame } => {
                                        RenderPlayerCameraMode::AtPos {
                                            pos: *pos,
                                            locked_ingame: *locked_ingame,
                                        }
                                    }
                                    PlayerCameraMode::LockedOn { character_ids, .. } => {
                                        RenderPlayerCameraMode::OnCharacters {
                                            character_ids: character_ids.clone(),
                                            fallback_pos: vec2::new(
                                                client_player.free_cam_pos.x as f32,
                                                client_player.free_cam_pos.y as f32,
                                            ),
                                        }
                                    }
                                },
                                info.force_scoreboard_visible,
                                matches!(info.ingame_mode, PlayerIngameMode::Spectator),
                            ),
                            None => (RenderPlayerCameraMode::Default, false, true),
                        };
                    (
                        player_id,
                        RenderGameForPlayer {
                            render_for_player: RenderForPlayer {
                                chat_info: if let Some(chat_mode) = client_player.chat_input_active
                                {
                                    Some((
                                        chat_mode,
                                        std::mem::take(&mut client_player.chat_msg),
                                        if is_menu_open {
                                            Default::default()
                                        } else {
                                            self.inp_manager.clone_inp().egui
                                        },
                                    ))
                                } else {
                                    None
                                },
                                emote_wheel_input: if client_player.emote_wheel_active
                                    && !is_menu_open
                                    && !is_spectator
                                {
                                    Some({
                                        let inp = self.inp_manager.clone_inp();

                                        let xrel = inp
                                            .evs
                                            .iter()
                                            .filter_map(|ev| {
                                                if let InputEv::Move(ev) = ev {
                                                    Some(ev.xrel)
                                                } else {
                                                    None
                                                }
                                            })
                                            .sum();
                                        let yrel = inp
                                            .evs
                                            .iter()
                                            .filter_map(|ev| {
                                                if let InputEv::Move(ev) = ev {
                                                    Some(ev.yrel)
                                                } else {
                                                    None
                                                }
                                            })
                                            .sum();

                                        EmoteWheelInput {
                                            egui: inp.egui,
                                            xrel,
                                            yrel,
                                        }
                                    })
                                } else {
                                    None
                                },
                                spectator_selection_input: if client_player
                                    .spectator_selection_active
                                    && !is_menu_open
                                    && (is_spectator || main_game.info.options.has_ingame_freecam)
                                {
                                    Some(self.inp_manager.clone_inp().egui)
                                } else {
                                    None
                                },
                                chat_show_all: client_player.show_chat_all,
                                scoreboard_active: client_player.show_scoreboard
                                    || force_scoreboard_visible,

                                local_player_info: local_player_render_info,

                                zoom: {
                                    let ingame_camera = match client_player.input_cam_mode {
                                        PlayerCameraMode::Default => true,
                                        PlayerCameraMode::Free => false,
                                        PlayerCameraMode::LockedTo { locked_ingame, .. }
                                        | PlayerCameraMode::LockedOn { locked_ingame, .. } => {
                                            locked_ingame
                                        }
                                    };
                                    if let Some(zoom) = ingame_camera
                                        .then_some(main_game.info.options.forced_ingame_camera_zoom)
                                        .flatten()
                                    {
                                        zoom.as_f64() as f32
                                    } else {
                                        client_player.zoom
                                    }
                                },
                                cam_mode,
                            },
                            observed_players: game.render_observers_pool.new(),
                            observed_anchored_size_props: ObservedAnchoredSize {
                                width: self
                                    .config
                                    .game
                                    .cl
                                    .dummy
                                    .screen_width
                                    .max(1)
                                    .try_into()
                                    .unwrap(),
                                height: self
                                    .config
                                    .game
                                    .cl
                                    .dummy
                                    .screen_height
                                    .max(1)
                                    .try_into()
                                    .unwrap(),
                            },
                        },
                    )
                }
            };

            let mut requires_scoreboard = false;
            let ids = game.game_data.local.active_local_player_mut().into_iter();
            ids.for_each(|client_player| {
                let (player_id, render_for_player) = fill_for_player(
                    client_player,
                    &render_game_input.character_infos,
                    &mut render_game_input.stages,
                );
                requires_scoreboard |= render_for_player.render_for_player.scoreboard_active;
                render_game_input
                    .players
                    .insert(player_id, render_for_player);
            });
            let inactive_players = game
                .game_data
                .local
                .inactive_local_players()
                .map(|(id, _)| id);
            render_game_input.dummies.extend(inactive_players);

            // set the dummy's potential cursor position for hammering
            if !render_game_input.dummies.is_empty() {
                let active_player_id = render_game_input.players.keys().next().copied();
                let active_player_pos = if let Some(character) = active_player_id.and_then(|id| {
                    render_game_input
                        .character_infos
                        .get(&id)
                        .and_then(|c| c.stage_id)
                        .and_then(|stage_id| render_game_input.stages.get(&stage_id))
                        .and_then(|stage| stage.world.characters.get(&id))
                }) {
                    character.lerped_pos
                } else {
                    vec2::default()
                };

                for id in render_game_input.dummies.iter() {
                    if let (Some(character), Some(local_player)) = (
                        render_game_input
                            .character_infos
                            .get(id)
                            .and_then(|c| c.stage_id)
                            .and_then(|stage_id| render_game_input.stages.get(&stage_id))
                            .and_then(|stage| stage.world.characters.get(id)),
                        game.game_data.local.local_players.get_mut(id),
                    ) {
                        let dir = active_player_pos - character.lerped_pos;
                        let dir_len = length(&dir);
                        let dir = if dir_len > 0.01 {
                            normalize_pre_length(&dir, dir_len)
                        } else {
                            vec2::new(1.0, 0.0)
                        };
                        local_player.cursor_pos_dummy = dvec2::new(dir.x as f64, dir.y as f64);
                    }
                }
            }

            // if miniscreens of the dummies should show up, add additional infor for player.
            if self.config.game.cl.dummy.mini_screen {
                if let Some((_, player)) = render_game_input.players.iter_mut().next() {
                    player
                        .observed_players
                        .extend(render_game_input.dummies.iter().map(|&player_id| {
                            ObservedPlayer::Dummy {
                                // here we don't need to use the anti ping predicted game
                                // TODO: but maybe make it a config variable? Hard to say if a miniscreen
                                // should really show anti ping predicted worlds _ever_.
                                local_player_info: main_game
                                    .collect_character_local_render_info(&player_id),
                                player_id,
                                anchor: self.config.game.cl.dummy.screen_anchor.into(),
                            }
                        }));
                }
            }
            // if a vote is ongoing and the server allows following voted players, add that to observed players
            if let (Some((_, player)), Some((vote, _, _))) = (
                render_game_input.players.iter_mut().next(),
                &render_game_input.vote,
            ) {
                if main_game.info.options.allows_voted_player_miniscreen {
                    match &vote.vote {
                        VoteType::Map { .. }
                        | VoteType::RandomUnfinishedMap { .. }
                        | VoteType::Misc { .. } => {
                            // ignore
                        }
                        VoteType::VoteKickPlayer { key, .. }
                        | VoteType::VoteSpecPlayer { key, .. } => {
                            player.observed_players.push(ObservedPlayer::Vote {
                                player_id: key.voted_player_id,
                            });
                        }
                    }
                }
            }

            if requires_scoreboard {
                let scoreboard_info = main_game.collect_scoreboard_info();
                // TODO: use predicted world info for scoreboard?
                render_game_input.scoreboard_info = Some(scoreboard_info);
            }

            let res = render.render(&self.config.game.map, &self.cur_time, render_game_input);

            // handle results
            for (player_id, player_events) in res.player_events {
                let local_player = game
                    .game_data
                    .local
                    .local_players
                    .get_mut(&player_id)
                    .unwrap();
                for player_event in player_events {
                    match player_event {
                        PlayerFeedbackEvent::Chat(ev) => match ev {
                            ChatEvent::MsgSend { msg, mode } => {
                                if let Some(msg) = match mode {
                                    ChatMode::Global => Some(MsgClChatMsg::Global {
                                        msg: NetworkString::new(&msg).unwrap(),
                                    }),
                                    ChatMode::Team => Some(MsgClChatMsg::GameTeam {
                                        msg: NetworkString::new(&msg).unwrap(),
                                    }),
                                    ChatMode::Whisper(player_id) => {
                                        player_id.map(|id| MsgClChatMsg::Whisper {
                                            receiver_id: id,
                                            msg: NetworkString::new(&msg).unwrap(),
                                        })
                                    }
                                } {
                                    game.network.send_in_order_to_server(
                                        &ClientToServerMessage::PlayerMsg((
                                            player_id,
                                            ClientToServerPlayerMessage::Chat(msg),
                                        )),
                                        NetworkInOrderChannel::Global,
                                    );
                                }
                                local_player.chat_msg.clear();
                            }
                            ChatEvent::CurMsg { msg, mode } => {
                                local_player.chat_msg = msg;
                                local_player.chat_input_active = Some(mode);
                            }
                            ChatEvent::ChatClosed => {
                                local_player.chat_input_active = None;
                            }
                            ChatEvent::PlatformOutput(output) => {
                                // no matter what egui reports, we don't want a cursor ingame
                                self.inp_manager
                                    .handle_platform_output(native, output, true);
                            }
                        },
                        PlayerFeedbackEvent::EmoteWheel(ev) => {
                            local_player.last_emote_wheel_selection = Some(ev);
                        }
                        PlayerFeedbackEvent::SpectatorSelection(ev) => match ev {
                            SpectatorSelectionEvent::FreeView => {
                                game.map.game.client_command(
                                    &player_id,
                                    ClientCommand::SetCameraMode(ClientCameraMode::None),
                                );
                                game.network.send_unordered_to_server(
                                    &ClientToServerMessage::PlayerMsg((
                                        player_id,
                                        ClientToServerPlayerMessage::SwitchToCamera(
                                            ClientCameraMode::None,
                                        ),
                                    )),
                                );
                            }
                            SpectatorSelectionEvent::Selected(spectated_characters) => {
                                game.map.game.client_command(
                                    &player_id,
                                    ClientCommand::SetCameraMode(ClientCameraMode::FreeCam(
                                        spectated_characters.iter().copied().collect(),
                                    )),
                                );
                                game.network.send_unordered_to_server(
                                    &ClientToServerMessage::PlayerMsg((
                                        player_id,
                                        ClientToServerPlayerMessage::SwitchToCamera(
                                            ClientCameraMode::FreeCam(
                                                spectated_characters.iter().copied().collect(),
                                            ),
                                        ),
                                    )),
                                );
                            }
                        },
                    }
                }
            }
        } else {
            // menu background map
            self.render_menu_background_map();
            self.graphics.backend_handle.consumble_multi_samples();
        }
    }

    fn render(&mut self, native: &mut dyn NativeImpl) {
        // first unload editor => then reload. else native library doesn't get a reload
        if self.editor.should_reload() {
            let is_open = self.editor.is_open();
            self.editor = EditorState::None;
            let editor = EditorWasmManager::new(
                &self.sound,
                &self.graphics,
                &self.graphics_backend,
                &self.io,
                &self.thread_pool,
                &self.font_data,
            );
            self.editor = if is_open {
                EditorState::Open(editor)
            } else {
                EditorState::Minimized(editor)
            };
        }
        if let EditorState::Open(editor) = &mut self.editor {
            match editor.render(
                if self.local_console.ui.ui_state.is_ui_open || self.game.remote_console_open() {
                    Default::default()
                } else {
                    self.inp_manager.take_inp().egui.unwrap_or_default()
                },
                &self.config.engine,
            ) {
                EditorResult::PlatformOutput(output) => {
                    self.inp_manager.handle_platform_output(
                        native,
                        output,
                        self.local_console.ui.ui_state.is_ui_open
                            || self.game.remote_console_open(),
                    );
                }
                EditorResult::Minimize => {
                    self.editor = match std::mem::take(&mut self.editor) {
                        EditorState::Open(editor) | EditorState::Minimized(editor) => {
                            EditorState::Minimized(editor)
                        }
                        EditorState::None => EditorState::None,
                    };
                }
                EditorResult::Close => {
                    self.editor = EditorState::None;
                }
            }
        } else {
            self.render_game(native);

            // if demo viewer is active, render it
            if let Some(demo_player) = &mut self.demo_player {
                if let Some(demo_viewer) = demo_player.try_get_mut() {
                    if demo_viewer
                        .render(
                            if self.local_console.ui.ui_state.is_ui_open
                                || self.game.remote_console_open()
                            {
                                Default::default()
                            } else {
                                self.inp_manager.take_inp().egui.unwrap_or_default()
                            },
                            &self.config.game.cl.render,
                            &self.config.game.snd.render,
                            self.config.game.snd.global_volume,
                        )
                        .is_err()
                        || demo_viewer.is_closed()
                    {
                        self.demo_player = None;
                    }
                } else if let Err(err) = demo_player.continue_loading(
                    &self.sound,
                    &self.graphics,
                    &self.graphics_backend,
                    &self.sound_backend,
                    &self.config.engine,
                    &self.config.game,
                    &self.sys,
                    &self.ui_creator,
                ) {
                    self.notifications
                        .add_err(err.to_string(), Duration::from_secs(10));
                    self.demo_player = None;
                }
            } else if self.ui_manager.ui.ui_state.is_ui_open {
                // fill raw input if ui needs raw input
                if self.raw_input_info.wants_raw_input() {
                    self.raw_input_info.set_raw_input(raw_input_info::RawInput {
                        keys: self
                            .inp_manager
                            .clone_inp()
                            .evs
                            .into_iter()
                            .filter_map(|ev| match ev {
                                InputEv::Key(ev) => ev.is_down.then_some(ev.key),
                                InputEv::Move(_) => None,
                            })
                            .collect(),
                    });
                }

                self.ui_manager.ui.zoom_level.set(Some(
                    self.graphics
                        .canvas_handle
                        .pixels_per_point()
                        .max(self.config.engine.ui.min_pixels_per_point as f32)
                        * self.config.engine.ui.scale as f32,
                ));
                // render ui last
                if let Some(output) = self.ui_manager.render(
                    &self.config.engine.ui.path.name.clone(),
                    &self.io,
                    &self.graphics,
                    &self.graphics_backend,
                    &mut self.sound,
                    &mut UiRenderPipe::new(self.sys.time_get(), &mut self.config),
                    if self.local_console.ui.ui_state.is_ui_open || self.game.remote_console_open()
                    {
                        Default::default()
                    } else {
                        self.inp_manager.take_inp().egui.unwrap_or_default()
                    },
                    true,
                ) {
                    self.inp_manager.handle_platform_output(
                        native,
                        output,
                        self.local_console.ui.ui_state.is_ui_open
                            || self.game.remote_console_open(),
                    );
                }
                let ui_events = self.ui_events.take();
                for ui_event in ui_events {
                    match ui_event {
                        UiEvent::StartLocalServer => {
                            start_local_server(
                                &self.sys,
                                self.shared_info.clone(),
                                self.config.engine.clone(),
                                self.config.game.clone(),
                            );
                        }
                        UiEvent::CheckLocalServer => {
                            let _ = Self::check_local_server_error(
                                &mut self.shared_info.state.lock().unwrap(),
                                &mut self.notifications,
                            );
                        }
                        UiEvent::PlayDemo { name } => {
                            self.demo_player = Some(DemoViewer::new(
                                &self.io,
                                &self.thread_pool,
                                name.as_ref(),
                                self.font_data.clone(),
                                None,
                            ));
                        }
                        UiEvent::EncodeDemoToVideo { name, video_name } => {
                            self.demo_player = Some(DemoViewer::new(
                                &self.io,
                                &self.thread_pool,
                                name.as_ref(),
                                self.font_data.clone(),
                                Some(DemoVideoEncodeProperties {
                                    file_name: format!("videos/{}.mp4", video_name).into(),
                                    pixels_per_point: self.config.game.cl.recorder.pixels_per_point,
                                    encoder_settings: EncoderSettings {
                                        fps: self.config.game.cl.recorder.fps,
                                        width: self.config.game.cl.recorder.width,
                                        height: self.config.game.cl.recorder.height,
                                        hw_accel: self.config.game.cl.recorder.hw_accel.clone(),
                                        max_threads: std::thread::available_parallelism()
                                            .map(|v| v.get() + 2)
                                            .unwrap_or_default()
                                            .max(2)
                                            as u64,
                                        sample_rate: self.config.game.cl.recorder.sample_rate,
                                        crf: self.config.game.cl.recorder.crf,
                                    },
                                    settings: DemoViewerSettings {
                                        global_sound_volume: self
                                            .config
                                            .game
                                            .cl
                                            .recorder
                                            .global_sound_volume,
                                        render: self.config.game.cl.recorder.render.clone(),
                                        snd: self.config.game.cl.recorder.snd.clone(),
                                    },
                                }),
                            ));
                        }
                        UiEvent::RecordDemo => {
                            if let Game::Active(game) = &mut self.game {
                                game.manual_demo_recorder = Some(DemoRecorder::new(
                                    game.demo_recorder_props.clone(),
                                    game.map.game.game_tick_speed(),
                                    None,
                                    None,
                                ));
                            }
                        }
                        UiEvent::StopRecordDemo => {
                            if let Game::Active(game) = &mut self.game {
                                game.manual_demo_recorder = None;
                            }
                        }
                        UiEvent::InstantReplay => {
                            if let Game::Active(game) = &mut self.game {
                                match game.replay.to_demo() {
                                    Ok(demo) => {
                                        self.demo_player = Some(demo);
                                    }
                                    Err(err) => {
                                        self.notifications
                                            .add_err(err.to_string(), Duration::from_secs(10));
                                    }
                                }
                            }
                        }
                        UiEvent::StartEditor => {
                            self.editor = match std::mem::take(&mut self.editor) {
                                EditorState::Open(editor) | EditorState::Minimized(editor) => {
                                    EditorState::Open(editor)
                                }
                                EditorState::None => EditorState::Open(EditorWasmManager::new(
                                    &self.sound,
                                    &self.graphics,
                                    &self.graphics_backend,
                                    &self.io,
                                    &self.thread_pool,
                                    &self.font_data,
                                )),
                            };
                        }
                        UiEvent::Connect {
                            addr,
                            rcon_secret,
                            cert_hash,
                            can_start_local_server,
                        } => {
                            // if localhost, then get the cert, rcon pw & port from the shared info
                            match self.connect_local_server(vec![addr], can_start_local_server) {
                                ConnectLocalServerResult::Connect {
                                    addr,
                                    server_cert,
                                    rcon_secret,
                                } => {
                                    self.connect_game(addr, server_cert, rcon_secret);
                                }
                                ConnectLocalServerResult::KeepConnecting { .. } => {
                                    self.ui_events.push(UiEvent::Connect {
                                        addr,
                                        rcon_secret,
                                        cert_hash,
                                        can_start_local_server: false,
                                    });
                                }
                                ConnectLocalServerResult::ErrOrNotLocalServerAddr { .. } => {
                                    // try to connect to the server with the original hash & recret
                                    self.connect_game(
                                        addr,
                                        ServerCertMode::Hash(cert_hash),
                                        rcon_secret,
                                    );
                                }
                            }
                        }
                        UiEvent::Disconnect => {
                            self.game = Game::None;
                        }
                        UiEvent::ConnectLocalPlayer { as_dummy } => {
                            if let Game::Active(game) = &mut self.game {
                                self.client_info.set_local_player_count(
                                    self.client_info.local_player_count() + 1,
                                );
                                let id = game.game_data.local.local_player_id_counter;
                                game.game_data.local.local_player_id_counter += 1;
                                game.game_data.local.expected_local_players.insert(
                                    id,
                                    ClientConnectedPlayer::Connecting { is_dummy: as_dummy },
                                );
                                game.network.send_unordered_to_server(
                                    &ClientToServerMessage::AddLocalPlayer(Box::new(
                                        MsgClAddLocalPlayer {
                                            player_info: if let Some((info, copy_info)) = as_dummy
                                                .then(|| {
                                                    self.config
                                                        .game
                                                        .players
                                                        .get(
                                                            self.config.game.profiles.dummy.index
                                                                as usize,
                                                        )
                                                        .zip(self.config.game.players.get(
                                                            self.config.game.profiles.main as usize,
                                                        ))
                                                })
                                                .flatten()
                                            {
                                                Game::network_char_info_from_config_for_dummy(
                                                    &self.config.game.cl,
                                                    info,
                                                    copy_info,
                                                    &self.config.game.profiles.dummy,
                                                )
                                            } else {
                                                // TODO
                                                NetworkCharacterInfo::explicit_default()
                                            },
                                            id,
                                        },
                                    )),
                                );
                            }
                        }
                        UiEvent::DisconnectLocalPlayer => {
                            if let Game::Active(game) = &mut self.game {
                                self.client_info.set_local_player_count(
                                    self.client_info.local_player_count().saturating_sub(1),
                                );
                                if game.game_data.local.expected_local_players.len() > 1 {
                                    let (id, player) = game
                                        .game_data
                                        .local
                                        .expected_local_players
                                        .pop_back()
                                        .unwrap();
                                    if game.game_data.local.active_local_player_id == id {
                                        game.game_data.local.active_local_player_id = *game
                                            .game_data
                                            .local
                                            .expected_local_players
                                            .front()
                                            .unwrap()
                                            .0;
                                    }
                                    if let ClientConnectedPlayer::Connected { player_id, .. } =
                                        player
                                    {
                                        game.game_data.local.local_players.remove(&player_id);
                                        game.network.send_unordered_to_server(
                                            &ClientToServerMessage::PlayerMsg((
                                                player_id,
                                                ClientToServerPlayerMessage::RemLocalPlayer,
                                            )),
                                        );
                                    }
                                }
                            }
                        }
                        UiEvent::Quit => {
                            native.quit();
                        }
                        UiEvent::Kill => {
                            if let Game::Active(game) = &mut self.game {
                                if let Some((player_id, _)) =
                                    game.game_data.local.active_local_player()
                                {
                                    game.network.send_unordered_to_server(
                                        &ClientToServerMessage::PlayerMsg((
                                            *player_id,
                                            ClientToServerPlayerMessage::Kill,
                                        )),
                                    );
                                }
                            }
                        }
                        UiEvent::JoinSpectators => {
                            if let Game::Active(game) = &mut self.game {
                                if let Some((player_id, _)) =
                                    game.game_data.local.active_local_player()
                                {
                                    game.network.send_unordered_to_server(
                                        &ClientToServerMessage::PlayerMsg((
                                            *player_id,
                                            ClientToServerPlayerMessage::JoinSpectator,
                                        )),
                                    );
                                }
                            }
                        }
                        UiEvent::JoinGame => {
                            if let Game::Active(game) = &mut self.game {
                                if let Some((player_id, _)) =
                                    game.game_data.local.active_local_player()
                                {
                                    game.network.send_unordered_to_server(
                                        &ClientToServerMessage::PlayerMsg((
                                            *player_id,
                                            ClientToServerPlayerMessage::JoinStage(
                                                JoinStage::Default,
                                            ),
                                        )),
                                    );
                                }
                            }
                        }
                        UiEvent::SwitchToFreeCam => {
                            if let Game::Active(game) = &mut self.game {
                                if let Some((player_id, _)) =
                                    game.game_data.local.active_local_player()
                                {
                                    game.network.send_unordered_to_server(
                                        &ClientToServerMessage::PlayerMsg((
                                            *player_id,
                                            ClientToServerPlayerMessage::SwitchToCamera(
                                                ClientCameraMode::FreeCam(Default::default()),
                                            ),
                                        )),
                                    );
                                }
                            }
                        }
                        UiEvent::WindowChange => {
                            self.on_window_change(native);
                        }
                        UiEvent::VsyncChanged => {
                            self.on_vsync_change();
                        }
                        UiEvent::MsaaChanged => {
                            self.on_msaa_change();
                        }
                        UiEvent::VoteKickPlayer(key) => {
                            if let Game::Active(game) = &mut self.game {
                                if let Some((player_id, _)) =
                                    game.game_data.local.active_local_player()
                                {
                                    game.network.send_unordered_to_server(
                                        &ClientToServerMessage::PlayerMsg((
                                            *player_id,
                                            ClientToServerPlayerMessage::StartVote(
                                                VoteIdentifierType::VoteKickPlayer(key),
                                            ),
                                        )),
                                    );
                                }
                            }
                        }
                        UiEvent::VoteSpecPlayer(key) => {
                            if let Game::Active(game) = &mut self.game {
                                if let Some((player_id, _)) =
                                    game.game_data.local.active_local_player()
                                {
                                    game.network.send_unordered_to_server(
                                        &ClientToServerMessage::PlayerMsg((
                                            *player_id,
                                            ClientToServerPlayerMessage::StartVote(
                                                VoteIdentifierType::VoteSpecPlayer(key),
                                            ),
                                        )),
                                    );
                                }
                            }
                        }
                        UiEvent::VoteMap(key) => {
                            if let Game::Active(game) = &mut self.game {
                                if let Some((player_id, _)) =
                                    game.game_data.local.active_local_player()
                                {
                                    game.network.send_unordered_to_server(
                                        &ClientToServerMessage::PlayerMsg((
                                            *player_id,
                                            ClientToServerPlayerMessage::StartVote(
                                                VoteIdentifierType::Map(key),
                                            ),
                                        )),
                                    );
                                }
                            }
                        }
                        UiEvent::VoteRandomUnfinishedMap(key) => {
                            if let Game::Active(game) = &mut self.game {
                                if let Some((player_id, _)) =
                                    game.game_data.local.active_local_player()
                                {
                                    game.network.send_unordered_to_server(
                                        &ClientToServerMessage::PlayerMsg((
                                            *player_id,
                                            ClientToServerPlayerMessage::StartVote(
                                                VoteIdentifierType::RandomUnfinishedMap(key),
                                            ),
                                        )),
                                    );
                                }
                            }
                        }
                        UiEvent::VoteMisc(key) => {
                            if let Game::Active(game) = &mut self.game {
                                if let Some((player_id, _)) =
                                    game.game_data.local.active_local_player()
                                {
                                    game.network.send_unordered_to_server(
                                        &ClientToServerMessage::PlayerMsg((
                                            *player_id,
                                            ClientToServerPlayerMessage::StartVote(
                                                VoteIdentifierType::Misc(key),
                                            ),
                                        )),
                                    );
                                }
                            }
                        }
                        UiEvent::JoinOwnTeam { name, color } => {
                            if let Game::Active(game) = &mut self.game {
                                for (player_id, _) in game.game_data.local.local_players.iter() {
                                    game.network.send_unordered_to_server(
                                        &ClientToServerMessage::PlayerMsg((
                                            *player_id,
                                            ClientToServerPlayerMessage::JoinStage(
                                                JoinStage::Own {
                                                    name: name
                                                        .as_str()
                                                        .try_into()
                                                        .unwrap_or_default(),
                                                    color: [color.r(), color.g(), color.b()],
                                                },
                                            ),
                                        )),
                                    );
                                }
                            }
                        }
                        UiEvent::JoinOtherTeam(name) => {
                            if let Game::Active(game) = &mut self.game {
                                for (player_id, _) in game.game_data.local.local_players.iter() {
                                    game.network.send_unordered_to_server(
                                        &ClientToServerMessage::PlayerMsg((
                                            *player_id,
                                            ClientToServerPlayerMessage::JoinStage(
                                                JoinStage::Others(
                                                    name.as_str().try_into().unwrap_or_default(),
                                                ),
                                            ),
                                        )),
                                    );
                                }
                            }
                        }
                        UiEvent::JoinDefaultTeam => {
                            if let Game::Active(game) = &mut self.game {
                                for (player_id, _) in game.game_data.local.local_players.iter() {
                                    game.network.send_unordered_to_server(
                                        &ClientToServerMessage::PlayerMsg((
                                            *player_id,
                                            ClientToServerPlayerMessage::JoinStage(
                                                JoinStage::Default,
                                            ),
                                        )),
                                    );
                                }
                            }
                        }
                        UiEvent::JoinVanillaSide { is_red_side } => {
                            if let Game::Active(game) = &mut self.game {
                                for (player_id, _) in game.game_data.local.local_players.iter() {
                                    game.network.send_unordered_to_server(
                                        &ClientToServerMessage::PlayerMsg((
                                            *player_id,
                                            ClientToServerPlayerMessage::JoinVanillaSide(
                                                if is_red_side {
                                                    MatchSide::Red
                                                } else {
                                                    MatchSide::Blue
                                                },
                                            ),
                                        )),
                                    );
                                }
                            }
                        }
                        UiEvent::ChangeAccountName { name } => {
                            if let Game::Active(game) = &mut self.game {
                                game.network.send_unordered_to_server(
                                    &ClientToServerMessage::AccountChangeName { new_name: name },
                                );
                            }
                        }
                        UiEvent::RequestAccountInfo => {
                            if let Game::Active(game) = &mut self.game {
                                if !std::mem::replace(&mut game.requested_account_details, true) {
                                    game.network.send_unordered_to_server(
                                        &ClientToServerMessage::AccountRequestInfo,
                                    );
                                }
                            }
                        }
                    }
                }

                if let Some(zoom) = self.ui_manager.ui.zoom_level.get() {
                    self.config.engine.ui.scale = zoom as f64
                        / (self.graphics.canvas_handle.pixels_per_point() as f64)
                            .max(self.config.engine.ui.min_pixels_per_point);
                }
            }
        }

        // make sure no msaa blocks ui rendering
        self.graphics.backend_handle.consumble_multi_samples();
        if self.local_console.ui.ui_state.is_ui_open {
            let mut pipe = ConsoleRenderPipe {
                graphics: &self.graphics,
                sys: &self.sys,
                config: &mut self.config,
                msgs: &mut self.console_logs,
                custom_matches: &|_| None,
                render_custom_matches: &|_, _, _, _, _, _| {},
                skin_container: &mut self.skin_container,
                render_tee: &self.render_tee,
            };
            let platform_output = self.local_console.render(
                self.inp_manager.take_inp().egui.unwrap_or_default(),
                &mut pipe,
                true,
            );
            self.inp_manager
                .handle_platform_output(native, platform_output, false);
        } else if let Some(game) = self
            .game
            .remote_console_open()
            .then(|| self.game.active_game_mut())
            .flatten()
        {
            let char_infos = RefCell::new(None);
            let mut pipe =
                ConsoleRenderPipe {
                    graphics: &self.graphics,
                    sys: &self.sys,
                    config: &mut self.config,
                    msgs: &mut game.remote_console_logs,
                    custom_matches: &|user_ty| match user_ty {
                        "PLAYER_ID" => Some(
                            game.map
                                .game
                                .collect_characters_info()
                                .keys()
                                .map(|i| i.to_string())
                                .collect(),
                        ),
                        _ => None,
                    },
                    render_custom_matches:
                        &|user_ty, arg_text, ui, ui_state, skin_container, render_tee| {
                            // v remove this once there are more matches
                            #[allow(clippy::single_match)]
                            match user_ty {
                                "PLAYER_ID" => {
                                    let mut char_infos = char_infos.borrow_mut();
                                    let char_infos = char_infos.get_or_insert_with(|| {
                                        game.map.game.collect_characters_info()
                                    });

                                    let Ok(id): Result<GameEntityId, _> = arg_text.parse() else {
                                        return;
                                    };
                                    let id: PlayerId = id.into();
                                    let Some(char) = char_infos.get(&id) else {
                                        return;
                                    };

                                    let rect = ui.available_rect_before_wrap();
                                    ui.add_space(20.0);

                                    let pos = rect.left_center() + egui::vec2(10.0, 0.0);
                                    render_tee_for_ui(
                                        &self.graphics.canvas_handle,
                                        skin_container,
                                        render_tee,
                                        ui,
                                        ui_state,
                                        ui.ctx().screen_rect(),
                                        None,
                                        char.info.skin.borrow(),
                                        Some(&char.info.skin_info),
                                        vec2::new(pos.x, pos.y),
                                        20.0,
                                        TeeEye::Normal,
                                    );

                                    ui.label(char.info.name.as_str());
                                }
                                _ => {}
                            }
                        },
                    skin_container: &mut self.skin_container,
                    render_tee: &self.render_tee,
                };
            let platform_output = game.remote_console.render(
                self.inp_manager.take_inp().egui.unwrap_or_default(),
                &mut pipe,
                false,
            );
            self.inp_manager
                .handle_platform_output(native, platform_output, false);
        }

        // handle the console events
        self.handle_console_events(native);
        if let Game::Active(game) = &mut self.game {
            let events = game.remote_console.get_events();
            for event in events {
                match event {
                    RemoteConsoleEvent::Exec { ident_text, args } => {
                        if let Some((player_id, _)) = game.game_data.local.active_local_player() {
                            if let (Ok(ident_text), Ok(args)) =
                                (ident_text.as_str().try_into(), args.as_str().try_into())
                            {
                                game.network.send_in_order_to_server(
                                    &ClientToServerMessage::PlayerMsg((
                                        *player_id,
                                        ClientToServerPlayerMessage::RconExec { ident_text, args },
                                    )),
                                    NetworkInOrderChannel::Custom(
                                        7302, // reads as "rcon"
                                    ),
                                );
                            } else {
                                self.notifications
                                    .add_err("rcon text limit reached.", Duration::from_secs(3));
                            }
                        }
                    }
                }
            }
        }

        // notifications (e.g. error popups)
        self.notifications.render();

        // fps (& debug)
        self.client_stats.render(&mut ClientStatsRenderPipe {
            debug_hud: if let Game::Active(game) = &self.game {
                Some(DebugHudRenderPipe {
                    prediction_timer: &game.game_data.prediction_timer,
                    byte_stats: &game.game_data.net_byte_stats,
                    ingame_timer: &game.game_data.last_game_tick,
                })
            } else {
                None
            },
            connection_issues: if let Game::Active(game) = &self.game {
                game.game_data.is_likely_distconnected(self.cur_time)
            } else {
                false
            },
            force_bottom: self.ui_manager.ui.ui_state.is_ui_open,
            show_fps: self.config.game.cl.show_fps,
        });

        self.sound.swap();
        self.graphics.swap();
    }

    fn connect_game(
        &mut self,
        addr: SocketAddr,
        server_cert: ServerCertMode,
        rcon_secret: Option<[u8; 32]>,
    ) {
        self.client_info.set_local_player_count(1);
        self.account_info.fill_account_info(None);
        self.config.engine.ui.path.route("connect");
        self.connect_info.set(ConnectModes::Connecting { addr });
        self.game = Game::new(
            GameBase {
                graphics: self.graphics.clone(),
                graphics_backend: self.graphics_backend.clone(),
                sound: self.sound.clone(),
                sys: self.sys.clone(),
                tp: self.thread_pool.clone(),
                fonts: self.font_data.clone(),
            },
            &self.io,
            GameConnect {
                rcon_secret,
                addr,
                mode: self.connect_info.clone(),
                server_cert,
                browser_data: self.browser_data.clone(),
            },
            &self.accounts,
            DisconnectAutoCleanup {
                spatial_chat: self.spatial_chat.spatial_chat.clone(),
                client_info: self.client_info.clone(),
                account_info: self.account_info.clone(),
                player_settings_sync: self.player_settings_sync.clone(),
                votes: self.votes.clone(),
            },
        )
        .unwrap();
    }

    fn handle_exec(
        io: &IoFileSys,
        file_path: PathBuf,
        config_engine: &mut ConfigEngine,
        config_game: &mut ConfigGame,

        entries: &[ConsoleEntry],
        parser_cache: &ParserCache,
        mut on_err: impl FnMut(String),
        mut on_log: impl FnMut(String),
    ) {
        let fs = io.fs.clone();
        let cmds_file = match io
            .rt
            .spawn(async move {
                fs.read_file(&file_path)
                    .await
                    .map_err(|err| {
                        anyhow!(
                            "failed to read config file: {file_path:?} in {:?}: {err}",
                            fs.get_save_path()
                        )
                    })
                    .and_then(|file| {
                        String::from_utf8(file).map_err(|err| {
                            anyhow!(
                                "failed to read config file: {file_path:?} in {:?}: {err}",
                                fs.get_save_path()
                            )
                        })
                    })
            })
            .get_storage()
        {
            Ok(cmds_file) => cmds_file,
            Err(err) => {
                on_err(err.to_string());
                return;
            }
        };

        let mut cmds_succeeded = true;
        let parser_entries = entries_to_parser(entries);
        for line in cmds_file.lines().filter(|l| !l.is_empty()) {
            let cmds = command_parser::parser::parse(line, &parser_entries, parser_cache);
            let mut res = String::default();
            let cur_cmds_succeeded =
                run_commands(&cmds, entries, config_engine, config_game, &mut res, true);
            log::debug!("{}", res);
            if !cur_cmds_succeeded {
                on_log(res);
            }
            cmds_succeeded &= cur_cmds_succeeded;
        }
        if !cmds_succeeded {
            on_err(
                "At least one command failed to be executed, \
                see local console for more info."
                    .to_string(),
            );
        }
    }

    fn handle_console_events_impl(
        &mut self,
        native: &mut dyn NativeImpl,
        events: Vec<LocalConsoleEvent>,
        depth: usize,
    ) {
        for event in events {
            match event {
                LocalConsoleEvent::Connect {
                    addresses,
                    can_start_local_server,
                } => {
                    // if localhost, then get the cert, rcon pw & port from the shared info
                    match self.connect_local_server(addresses, can_start_local_server) {
                        ConnectLocalServerResult::Connect {
                            addr,
                            server_cert,
                            rcon_secret,
                        } => {
                            self.connect_game(addr, server_cert, rcon_secret);
                        }
                        ConnectLocalServerResult::KeepConnecting { addresses } => {
                            self.local_console.add_event(LocalConsoleEvent::Connect {
                                addresses,
                                can_start_local_server: false,
                            });
                        }
                        ConnectLocalServerResult::ErrOrNotLocalServerAddr { addresses } => {
                            // try the first ipv4 found or the first
                            if let Some(addr) = addresses
                                .iter()
                                .find(|addr| addr.is_ipv4())
                                .or(addresses.first())
                                .and_then(|addr| (!addr.ip().is_loopback()).then_some(addr))
                            {
                                self.connect_game(*addr, ServerCertMode::Unknown, None);
                            }
                        }
                    }
                }
                LocalConsoleEvent::Bind { was_player_profile }
                | LocalConsoleEvent::Unbind { was_player_profile } => {
                    if let Game::Active(game) = &mut self.game {
                        let dummy_copies_binds =
                            self.config.game.profiles.dummy.copy_binds_from_main;
                        let mut set_binds = |local_player: &mut ClientPlayer| {
                            // delete all previous binds
                            local_player.binds = Binds::default();
                            GameData::init_local_player_binds(
                                &mut self.config.game,
                                &mut local_player.binds,
                                !was_player_profile,
                                &self.local_console.entries,
                                &game.parser_cache,
                            );
                        };

                        let local_player = if was_player_profile {
                            game.game_data.local.active_local_player_mut()
                        } else {
                            game.game_data.local.first_inactive_local_players_mut()
                        };
                        if let Some((_, local_player)) = local_player {
                            set_binds(local_player);
                        }
                        // make sure to also update the dummy settings (if wanted)
                        if let Some((_, local_player)) = (was_player_profile && dummy_copies_binds)
                            .then_some(game.game_data.local.first_inactive_local_players_mut())
                            .flatten()
                        {
                            set_binds(local_player);
                        }
                    }
                }
                LocalConsoleEvent::Exec { file_path } => Self::handle_exec(
                    &self.io.clone().into(),
                    file_path,
                    &mut self.config.engine,
                    &mut self.config.game,
                    &self.local_console.entries,
                    &self.local_console.user,
                    |err| {
                        self.notifications.add_err(err, Duration::from_secs(10));
                    },
                    |msg| {
                        self.console_logs.push_str(&msg);
                    },
                ),
                LocalConsoleEvent::Echo { text } => {
                    self.notifications.add_info(text, Duration::from_secs(2));
                }
                LocalConsoleEvent::ChangeDummy { dummy_index } => {
                    if let Game::Active(game) = &mut self.game {
                        if let Some(dummy_index) = dummy_index {
                            if let Some((index, _)) = game
                                .game_data
                                .local
                                .expected_local_players
                                .iter()
                                .filter(|(_, p)| match p {
                                    ClientConnectedPlayer::Connecting { is_dummy } => *is_dummy,
                                    ClientConnectedPlayer::Connected { is_dummy, .. } => *is_dummy,
                                })
                                .nth(dummy_index)
                            {
                                game.game_data.local.active_local_player_id = *index;
                            }
                        } else if let Some((index, _)) =
                            game.game_data.local.expected_local_players.iter().find(
                                |(_, p)| match p {
                                    ClientConnectedPlayer::Connecting { is_dummy } => !*is_dummy,
                                    ClientConnectedPlayer::Connected { is_dummy, .. } => !*is_dummy,
                                },
                            )
                        {
                            game.game_data.local.active_local_player_id = *index;
                        }
                    }
                }
                LocalConsoleEvent::ToggleDummy => {
                    if let Game::Active(game) = &mut self.game {
                        if let Some(((&dummy_index, _), (&player_index, _))) = game
                            .game_data
                            .local
                            .expected_local_players
                            .iter()
                            .find(|(_, p)| match p {
                                ClientConnectedPlayer::Connecting { .. } => false,
                                ClientConnectedPlayer::Connected { is_dummy, .. } => *is_dummy,
                            })
                            .zip(game.game_data.local.expected_local_players.iter().find(
                                |(_, p)| match p {
                                    ClientConnectedPlayer::Connecting { .. } => false,
                                    ClientConnectedPlayer::Connected { is_dummy, .. } => !*is_dummy,
                                },
                            ))
                        {
                            let active_index = &mut game.game_data.local.active_local_player_id;
                            if *active_index == player_index {
                                *active_index = dummy_index;
                            } else {
                                *active_index = player_index;
                            }
                        }
                    }
                }
                LocalConsoleEvent::Quit => native.quit(),
                LocalConsoleEvent::ConfigVariable { name } => {
                    // some special cases
                    if name.starts_with("player.") || name == "player" {
                        // player info changed, send update to server
                        self.player_settings_sync.set_player_info_changed();
                    }

                    if name.starts_with("wnd.") || name == "wnd" {
                        self.on_window_change(native);
                    }

                    if name.starts_with("inp.") || name == "inp" {
                        if let Game::Active(game) = &mut self.game {
                            // make sure all cursors are updated
                            for local_player in game.game_data.local.local_players.values_mut() {
                                InputHandling::clamp_cursor(&self.config.game, local_player);
                                local_player.cursor_pos =
                                    local_player.input.inp.cursor.to_vec2() * 32.0;
                                local_player.input.inp.dyn_cam_offset.set(
                                    CharacterInputDynCamOffset::from_vec2(
                                        InputHandling::dyn_camera_offset(
                                            &self.config.game,
                                            local_player,
                                        ),
                                    ),
                                );
                            }
                        }
                    }

                    match name.as_str() {
                        "gl.vsync" => {
                            // update vsync val in backend
                            self.on_vsync_change();
                        }
                        "gl.clear_color" => {
                            self.graphics.backend_handle.update_clear_color(ColorRgba {
                                r: self.config.engine.gl.clear_color.r as f32 / 255.0,
                                g: self.config.engine.gl.clear_color.g as f32 / 255.0,
                                b: self.config.engine.gl.clear_color.b as f32 / 255.0,
                                a: 0.0,
                            });
                        }
                        "gl.msaa" => {
                            self.on_msaa_change();
                        }
                        _ => {
                            // ignore
                        }
                    }
                }
                LocalConsoleEvent::LocalPlayerAction(action) => {
                    if let Game::Active(game) = &self.game {
                        // handle a few actions directly
                        match action {
                            BindActionsLocalPlayer::Kill => {
                                if let Some((local_player_id, _)) =
                                    game.game_data.local.active_local_player()
                                {
                                    game.network.send_unordered_to_server(
                                        &ClientToServerMessage::PlayerMsg((
                                            *local_player_id,
                                            ClientToServerPlayerMessage::Kill,
                                        )),
                                    );
                                }
                            }
                            _ => {
                                // ignore
                            }
                        }
                    }
                }
            }
        }

        let events = self.local_console.get_events();
        let max_depth_reached = depth >= 16;
        if !events.is_empty()
            && events
                .iter()
                .any(|e| matches!(e, LocalConsoleEvent::Exec { .. }))
            && !max_depth_reached
        {
            self.handle_console_events_impl(native, events, depth + 1);
        } else if max_depth_reached {
            self.notifications.add_err(
                "Max recursion limit for processing console events reached.",
                Duration::from_secs(5),
            );
        }
    }

    fn handle_console_events(&mut self, native: &mut dyn NativeImpl) {
        let events = self.local_console.get_events();
        self.handle_console_events_impl(native, events, 0);
    }
}

impl FromNativeLoadingImpl<ClientNativeLoadingImpl> for ClientNativeImpl {
    fn new(
        mut loading: ClientNativeLoadingImpl,
        native: &mut dyn NativeImpl,
    ) -> anyhow::Result<Self> {
        let first_time_setup = std::mem::take(&mut loading.config_game.cl.first_time_setup);

        let benchmark = Benchmark::new(loading.config_engine.dbg.bench);
        let io = Io::from(loading.io, Arc::new(HttpClient::new()));
        benchmark.bench("upgrading io with http client");

        let font_loading = UiFontDataLoading::new(&io);
        let accounts_loading = AccountsLoading::new(&io);
        benchmark.bench("loading client files");

        let thread_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .thread_name(|index| format!("client-rayon {index}"))
                .num_threads(
                    std::thread::available_parallelism()
                        .unwrap_or(NonZeroUsize::new(2).unwrap())
                        .get()
                        .max(4)
                        - 2,
                )
                .start_handler(|_| {
                    if let Err(err) = thread_priority::set_current_thread_priority(
                        thread_priority::ThreadPriority::Min,
                    ) {
                        log::info!("failed to apply thread priority to rayon builder: {err}");
                    }
                })
                .build()?,
        );
        benchmark.bench("creating rayon thread pool");

        let native_monitors = native.monitors();

        // read window props
        let wnd = native.window_options();
        let config_wnd = &mut loading.config_engine.wnd;
        config_wnd.fullscreen = wnd.mode.is_fullscreen();
        config_wnd.decorated = wnd.decorated;
        config_wnd.maximized = wnd.maximized;
        match wnd.mode {
            WindowMode::Fullscreen {
                resolution,
                fallback_window,
            } => {
                if let Some(resolution) = resolution {
                    config_wnd.fullscreen_width = resolution.width;
                    config_wnd.fullscreen_height = resolution.height;
                }

                config_wnd.window_width = fallback_window.width;
                config_wnd.window_height = fallback_window.height;
            }
            WindowMode::Windowed(pixels) => {
                config_wnd.window_width = pixels.width;
                config_wnd.window_height = pixels.height;
            }
        }
        config_wnd.refresh_rate_mhz = wnd.refresh_rate_milli_hertz;
        config_wnd.monitor = wnd
            .monitor
            .map(|monitor| ConfigMonitor {
                name: monitor.name,
                width: monitor.size.width,
                height: monitor.size.height,
            })
            .unwrap_or_default();

        // do first time setup
        if first_time_setup {
            loading.config_game.cl.refresh_rate = if wnd.refresh_rate_milli_hertz != 0 {
                (wnd.refresh_rate_milli_hertz as u64) * 4 / 1000
            } else {
                let fallback_refresh_rate = native_monitors
                    .iter()
                    .map(|m| m.refresh_rate_millihertz().unwrap_or_default())
                    .max();
                if let Some(fallback_refresh_rate) =
                    fallback_refresh_rate.and_then(|r| (r != 0).then_some(r))
                {
                    (fallback_refresh_rate as u64) * 4 / 1000
                } else {
                    480
                }
            };
        }

        // prepare network stuff while waiting for io
        let sound_backend = SoundBackend::new(&loading.config_engine.snd)?;
        let sound = SoundManager::new(sound_backend.clone())?;
        benchmark.bench("sound");

        let monitors: Vec<_> = native_monitors
            .into_iter()
            .map(|monitor| {
                let mut video_modes: Vec<_> = monitor
                    .video_modes()
                    .map(|mode| {
                        let size = mode.size();
                        UiMonitorVideoMode {
                            width: size.width,
                            height: size.height,
                            refresh_rate_mhz: mode.refresh_rate_millihertz(),
                        }
                    })
                    .collect();
                let video_modes = if video_modes.is_empty() {
                    let size = monitor.size();
                    vec![UiMonitorVideoMode {
                        width: size.width,
                        height: size.height,
                        refresh_rate_mhz: monitor.refresh_rate_millihertz().unwrap_or_default(),
                    }]
                } else {
                    // that the parameter names are swapped is intentional
                    // bcs what we actually want is the sort into the other direction
                    video_modes.sort_by(|v2, v1| {
                        let mut cmp = v1.width.cmp(&v2.width);
                        if matches!(cmp, std::cmp::Ordering::Equal) {
                            cmp = v1.height.cmp(&v2.height);
                            if matches!(cmp, std::cmp::Ordering::Equal) {
                                cmp = v1.refresh_rate_mhz.cmp(&v2.refresh_rate_mhz);
                            };
                        }
                        cmp
                    });
                    video_modes
                };
                UiMonitor {
                    name: monitor.name().unwrap_or_else(|| "invalid".to_string()),
                    video_modes,
                }
            })
            .collect();
        let monitors = UiMonitors::new(monitors);

        let inp_manager = InputHandling::new(native.borrow_window());
        benchmark.bench("input handling");

        let mut ui_creator = UiCreator::default();
        let font_data = UiFontData::new(font_loading)?.into_font_definitions();
        ui_creator.load_font(&font_data);
        benchmark.bench("loading font");

        let mut local_console = loading
            .local_console_builder
            .take()
            .unwrap_or_default()
            .build(&ui_creator);
        benchmark.bench("local console");

        // then prepare components allocations etc.
        let (graphics_backend, stream_data) = GraphicsBackendBase::new(
            loading.graphics_backend_io_loading,
            loading.graphics_backend_loading.take().unwrap(),
            &thread_pool,
            BackendWindow::Winit {
                window: native.borrow_window(),
            },
        )?;
        benchmark.bench("init of graphics backend");

        let window_props = graphics_backend.get_window_props();
        let graphics_backend = GraphicsBackend::new(graphics_backend);
        let mut graphics = Graphics::new(graphics_backend.clone(), stream_data, window_props);

        benchmark.bench("init of graphics");

        let scene = sound.scene_handle.create(Default::default());
        let default_entities =
            EntitiesContainer::load_default(&io, ENTITIES_CONTAINER_PATH.as_ref());
        let entities_container = EntitiesContainer::new(
            io.clone(),
            thread_pool.clone(),
            default_entities,
            None,
            None,
            "entities-container",
            &graphics,
            &sound,
            &scene,
            ENTITIES_CONTAINER_PATH.as_ref(),
            ContainerLoadOptions {
                assume_unused: true,
                ..Default::default()
            },
        );
        let default_skin = SkinContainer::load_default(&io, SKIN_CONTAINER_PATH.as_ref());
        let skin_container = SkinContainer::new(
            io.clone(),
            thread_pool.clone(),
            default_skin,
            Some(HTTP_RESOURCE_URL.try_into().unwrap()),
            None,
            "skin-container",
            &graphics,
            &sound,
            &scene,
            SKIN_CONTAINER_PATH.as_ref(),
            ContainerLoadOptions {
                assume_unused: true,
                ..Default::default()
            },
        );
        let render_tee = RenderTee::new(&graphics);

        benchmark.bench("init of components");

        let menu_map_path = format!(
            "themes/{}",
            loading.config_game.cl.menu_background_map.as_str()
        );
        let menu_map = ClientMapLoading::new(
            &sound,
            &graphics,
            &graphics_backend,
            &loading.sys,
            menu_map_path.as_ref(),
            &"day".try_into().unwrap(),
            None,
            &io,
            &thread_pool,
            GameModification::Native,
            true,
            &loading.config_engine.dbg,
            Default::default(),
            RenderGameCreateOptions {
                physics_group_name: "vanilla".try_into().unwrap(),
                resource_http_download_url: None,
                resource_download_server: None,
                fonts: font_data.clone(),
                sound_props: Default::default(),
                render_mod: RenderModTy::Native,
                required_resources: Default::default(),
                client_local_infos: Default::default(),
            },
        );
        benchmark.bench("menu map");

        let graphics_memory_usage = graphics_backend.memory_usage();
        let client_stats = ClientStats::new(
            &graphics,
            &loading.sys,
            graphics_memory_usage.texture_memory_usage,
            graphics_memory_usage.buffer_memory_usage,
            graphics_memory_usage.stream_memory_usage,
            graphics_memory_usage.staging_memory_usage,
            &ui_creator,
        );
        let mut notifications = ClientNotifications::new(&graphics, &loading.sys, &ui_creator);
        if loading.has_startup_errors {
            notifications.add_err(
                "Some startup commands failed to be parsed, \
                please read the logs for more information.",
                Duration::from_secs(5),
            );
        }

        let loading_page = Box::new(LoadingPage::new());
        let page_err = UiWasmManagerErrorPageErr::default();
        let page_404 = Box::new(Error404Page::new(page_err.clone()));
        let mut ui_manager = UiManager::new(
            &io,
            (page_404, page_err),
            UiPageLoadingType::ShowLoadingPage(loading_page),
            &ui_creator,
        );
        benchmark.bench("ui manager");

        let (steam_client, steam_rt) = init_steam(412220)?;
        benchmark.bench("steam");

        let connect_info = ConnectMode::new(ConnectModes::Connecting {
            addr: "127.0.0.1:0".parse().unwrap(),
        });
        let ui_events = UiEvents::new();
        let client_info = ClientInfo::default();

        // ui shared objects
        let accounts = Arc::new(Accounts::new(accounts_loading, steam_client));
        let server_players = ServerPlayers::default();
        let game_server_info = GameServerInfo::default();
        let votes = Votes::default();
        let account_info = AccountInfo::default();
        let spatial_chat = SpatialChat::default();
        let player_settings_sync = PlayerSettingsSync::default();
        let raw_input_info = RawInputInfo::default();
        let browser_data = ServerBrowserData::default();

        #[cfg(feature = "ffmpeg")]
        fn demo_to_video() -> bool {
            true
        }
        #[cfg(not(feature = "ffmpeg"))]
        fn demo_to_video() -> bool {
            false
        }

        #[cfg(feature = "microphone")]
        fn microphone() -> bool {
            true
        }
        #[cfg(not(feature = "microphone"))]
        fn microphone() -> bool {
            false
        }
        let enabled_features = EnabledFeatures {
            demo_to_video: demo_to_video(),
            spatial_chat: microphone(),
        };

        let main_menu = Box::new(MainMenuUi::new(
            &graphics,
            &sound,
            loading.shared_info.clone(),
            client_info.clone(),
            ui_events.clone(),
            io.clone(),
            thread_pool.clone(),
            accounts.clone(),
            monitors.clone(),
            spatial_chat.clone(),
            player_settings_sync.clone(),
            &loading.config_game,
            local_console.entries.clone(),
            raw_input_info.clone(),
            browser_data.clone(),
            enabled_features,
        ));
        let connecting_menu = Box::new(ConnectingUi::new(connect_info.clone(), ui_events.clone()));
        let ingame_menu = Box::new(IngameMenuUi::new(
            &graphics,
            &sound,
            loading.shared_info.clone(),
            client_info.clone(),
            ui_events.clone(),
            io.clone(),
            thread_pool.clone(),
            accounts.clone(),
            monitors.clone(),
            spatial_chat.clone(),
            player_settings_sync.clone(),
            &loading.config_game,
            local_console.entries.clone(),
            raw_input_info.clone(),
            browser_data.clone(),
            enabled_features,
            server_players.clone(),
            game_server_info.clone(),
            account_info.clone(),
            votes.clone(),
            &loading.sys.time_get(),
        ));
        let tee_editor = Box::new(TeeEditor::new(&mut graphics));
        let color_test = Box::new(ColorTest::default());
        ui_manager.register_path("", "", main_menu);
        ui_manager.register_path("", "connect", connecting_menu);
        ui_manager.register_path("", "ingame", ingame_menu);
        ui_manager.register_path("editor", "tee", tee_editor);
        ui_manager.register_path("", "color", color_test);
        benchmark.bench("registering ui paths");

        let cur_time = loading.sys.time_get();
        let last_refresh_rate_time = cur_time;

        native.confine_mouse(true);
        benchmark.bench("mouse grab");

        let mut global_binds = Binds::default();
        global_binds.register_bind(
            &[BindKey::Key(PhysicalKey::Code(KeyCode::F10))],
            BindActionsHotkey::Screenshot,
        );
        // TODO: remove this hack
        #[cfg(target_os = "android")]
        global_binds.register_bind(
            &[BindKey::Key(PhysicalKey::Code(KeyCode::Digit0))],
            BindActionsHotkey::LocalConsole,
        );
        #[cfg(not(target_os = "android"))]
        global_binds.register_bind(
            &[BindKey::Key(PhysicalKey::Code(KeyCode::F1))],
            BindActionsHotkey::LocalConsole,
        );
        global_binds.register_bind(
            &[BindKey::Key(PhysicalKey::Code(KeyCode::F2))],
            BindActionsHotkey::RemoteConsole,
        );
        global_binds.register_bind(
            &[BindKey::Key(PhysicalKey::Code(KeyCode::Escape))],
            BindActionsHotkey::ConsoleClose,
        );
        global_binds.register_bind(
            &[
                BindKey::Key(PhysicalKey::Code(KeyCode::ControlLeft)),
                BindKey::Key(PhysicalKey::Code(KeyCode::ShiftLeft)),
                BindKey::Key(PhysicalKey::Code(KeyCode::KeyD)),
            ],
            BindActionsHotkey::DebugHud,
        );
        benchmark.bench("global binds");

        local_console.ui.ui_state.is_ui_open = false;

        let mut client = Self {
            menu_map,

            cur_time,
            sys: loading.sys,
            shared_info: loading.shared_info,
            client_info,

            entities_container,
            skin_container,
            render_tee,

            graphics,
            graphics_backend,

            sound,
            sound_backend,
            game: Game::None,
            connect_info,
            demo_player: None,

            client_stats,
            notifications,

            thread_pool,
            io,
            config: Config::new(loading.config_game, loading.config_engine),
            last_refresh_rate_time,
            editor: Default::default(),

            local_console,
            console_logs: Default::default(),

            ui_manager,
            ui_events,
            font_data,
            ui_creator,

            _steam_rt: steam_rt,

            accounts,
            server_players,
            game_server_info,
            votes,
            account_info,
            player_settings_sync,
            raw_input_info,
            spatial_chat: spatial_chat::SpatialChat::new(spatial_chat),
            browser_data,

            scene,

            global_binds,
            inp_manager,

            // pools & helpers
            string_pool: Pool::with_sized(256, || String::with_capacity(256)), // TODO: random values rn
        };

        client.handle_console_events(native);
        benchmark.bench("finish init of client");

        Ok(client)
    }

    fn load_with_display_handle(
        loading: &mut ClientNativeLoadingImpl,
        display_handle: NativeDisplayBackend,
    ) -> anyhow::Result<()> {
        let map_pipe = MapPipeline::new_boxed();

        let graphics_backend_loading = GraphicsBackendLoading::new(
            &loading.config_engine.gfx,
            &loading.config_engine.dbg,
            &loading.config_engine.gl,
            graphics_backend::window::BackendRawDisplayHandle::Winit {
                handle: display_handle,
            },
            Some(Arc::new(parking_lot::RwLock::new(vec![map_pipe]))),
            loading.io.clone(),
        )?;
        loading.graphics_backend_loading = Some(graphics_backend_loading);
        Ok(())
    }
}

impl InputEventHandler for ClientNativeImpl {
    fn key_down(
        &mut self,
        window: &native::native::Window,
        device: &native::native::DeviceId,
        key: PhysicalKey,
    ) {
        self.inp_manager.key_down(window, device, &key)
    }

    fn key_up(
        &mut self,
        window: &native::native::Window,
        device: &native::native::DeviceId,
        key: PhysicalKey,
    ) {
        #[cfg(feature = "alloc_track")]
        if key == PhysicalKey::Code(KeyCode::Pause) {
            track_report();
        }
        #[cfg(feature = "alloc_stats")]
        if key == PhysicalKey::Code(KeyCode::Pause) {
            stats_report();
        }
        self.inp_manager.key_up(window, device, &key)
    }

    fn mouse_down(
        &mut self,
        window: &native::native::Window,
        device: &native::native::DeviceId,
        x: f64,
        y: f64,
        btn: &native::native::MouseButton,
    ) {
        self.inp_manager.mouse_down(window, device, x, y, btn)
    }

    fn mouse_up(
        &mut self,
        window: &native::native::Window,
        device: &native::native::DeviceId,
        x: f64,
        y: f64,
        btn: &native::native::MouseButton,
    ) {
        self.inp_manager.mouse_up(window, device, x, y, btn)
    }

    fn mouse_move(
        &mut self,
        window: &native::native::Window,
        device: &native::native::DeviceId,
        x: f64,
        y: f64,
        xrel: f64,
        yrel: f64,
    ) {
        self.inp_manager
            .mouse_move(window, device, x, y, xrel, yrel)
    }

    fn scroll(
        &mut self,
        window: &native::native::Window,
        device: &native::native::DeviceId,
        x: f64,
        y: f64,
        delta: &native::native::MouseScrollDelta,
    ) {
        self.inp_manager.scroll(window, device, x, y, delta)
    }

    fn raw_window_event(&mut self, window: &native::native::Window, event: &WindowEvent) -> bool {
        self.inp_manager.raw_event(window, event);
        // we never actually consume events
        false
    }
}

impl FromNativeImpl for ClientNativeImpl {
    fn run(&mut self, native: &mut dyn NativeImpl) {
        self.inp_manager.collect_events();
        self.inp_manager.handle_global_binds(
            &mut self.global_binds,
            &mut self.local_console.ui,
            self.game
                .get_remote_console_mut()
                .map(|console| &mut console.ui),
            &mut self.client_stats.ui,
            &self.graphics,
            &self.io,
        );

        let sys = &mut self.sys;
        self.cur_time = sys.time_get();

        self.game.update(
            &self.config.engine,
            &mut self.config.game,
            &self.ui_creator,
            &mut self.notifications,
            &self.local_console.entries,
        );

        GameEventsClient::update(&mut GameEventPipeline {
            game: &mut self.game,
            msgs: &mut GameMsgPipeline {
                runtime_thread_pool: &self.thread_pool,
                io: &self.io,
                config: &mut self.config.engine,
                config_game: &mut self.config.game,
                shared_info: &self.shared_info,
                ui: &mut self.ui_manager.ui.ui_state,
                sys,
                string_pool: &mut self.string_pool,
                console_entries: &self.local_console.entries,

                account_info: &self.account_info,
                spatial_chat: &mut self.spatial_chat,
                notifications: &mut self.notifications,
            },
            game_server_info: &self.game_server_info,
            spatial_chat_scene: &self.scene,
        });
        let has_input = !self.ui_manager.ui.ui_state.is_ui_open
            && !self.local_console.ui.ui_state.is_ui_open
            && !self.game.remote_console_open()
            && !self.editor.is_open()
            && self.demo_player.is_none();
        if let Game::Active(game) = &mut self.game {
            // check loading of votes
            if self.votes.needs_map_votes() {
                if !game.map_votes_loaded {
                    game.map_votes_loaded = true;
                    game.network
                        .send_unordered_to_server(&ClientToServerMessage::LoadVotes(
                            MsgClLoadVotes::Map { cached_votes: None },
                        ));
                }
                self.votes.fill_map_votes(
                    game.game_data.map_votes.clone(),
                    game.game_data.has_unfinished_map_votes,
                );
                self.votes.set_thumbnail_server_resource_download_url(
                    game.resource_download_server.clone(),
                );
            }
            if self.votes.needs_misc_votes() {
                if !game.misc_votes_loaded {
                    game.misc_votes_loaded = true;
                    game.network
                        .send_unordered_to_server(&ClientToServerMessage::LoadVotes(
                            MsgClLoadVotes::Misc { cached_votes: None },
                        ));
                }
                self.votes
                    .fill_misc_votes(game.game_data.misc_votes.clone());
            }

            if has_input {
                let evs = self.inp_manager.handle_player_binds(
                    &mut game.game_data,
                    &mut self.ui_manager.ui,
                    &mut self.config.engine,
                    &mut self.config.game,
                    &self.graphics,
                    &self.local_console.entries,
                );

                let player_id = game
                    .game_data
                    .local
                    .active_local_player()
                    .map(|(id, _)| *id);

                for ev in evs {
                    match ev {
                        InputHandlingEvent::Kill { local_player_id } => game
                            .network
                            .send_unordered_to_server(&ClientToServerMessage::PlayerMsg((
                                local_player_id,
                                ClientToServerPlayerMessage::Kill,
                            ))),
                        InputHandlingEvent::VoteYes | InputHandlingEvent::VoteNo => {
                            if let Some(player_id) = player_id {
                                let voted = if matches!(ev, InputHandlingEvent::VoteYes) {
                                    Voted::Yes
                                } else {
                                    Voted::No
                                };
                                if let Some((_, cur_voted, _)) = &mut game.game_data.vote {
                                    *cur_voted = Some(voted);
                                    game.network.send_unordered_to_server(
                                        &ClientToServerMessage::PlayerMsg((
                                            player_id,
                                            ClientToServerPlayerMessage::Voted(voted),
                                        )),
                                    );
                                }
                            }
                        }
                        InputHandlingEvent::Emoticon {
                            local_player_id,
                            emoticon,
                        } => {
                            game.map
                                .game
                                .set_player_emoticon(&local_player_id, emoticon);
                            game.network.send_unordered_to_server(
                                &ClientToServerMessage::PlayerMsg((
                                    local_player_id,
                                    ClientToServerPlayerMessage::Emoticon(emoticon),
                                )),
                            );
                        }
                        InputHandlingEvent::ChangeEyes {
                            local_player_id,
                            eye,
                        } => {
                            game.map
                                .game
                                .set_player_eye(&local_player_id, eye, Duration::MAX);
                            game.network.send_unordered_to_server(
                                &ClientToServerMessage::PlayerMsg((
                                    local_player_id,
                                    ClientToServerPlayerMessage::ChangeEyes {
                                        eye,
                                        duration: Duration::MAX,
                                    },
                                )),
                            );
                        }
                    }
                }

                let player = game.game_data.local.active_local_player();
                let needs_abs_cursor = player.is_some_and(|(_, p)| {
                    p.spectator_selection_active
                        && (game.map.game.info.options.has_ingame_freecam
                            || match p.input_cam_mode {
                                PlayerCameraMode::Default => false,
                                PlayerCameraMode::Free => true,
                                PlayerCameraMode::LockedTo { locked_ingame, .. }
                                | PlayerCameraMode::LockedOn { locked_ingame, .. } => {
                                    !locked_ingame
                                }
                            })
                });
                native.relative_mouse(!needs_abs_cursor);

                self.inp_manager.set_last_known_cursor(
                    &self.config.engine,
                    if needs_abs_cursor {
                        CursorIcon::Default
                    } else {
                        CursorIcon::None
                    },
                );
            }

            game.game_data.prediction_timer.add_frametime(
                self.cur_time.saturating_sub(game.game_data.last_frame_time),
                self.cur_time,
            );
            game.game_data.last_frame_time = self.cur_time;
            let game_state = &mut game.map.game;

            let tick_of_inp = game_state.predicted_game_monotonic_tick + 1;
            let ticks_per_second = game_state.game_tick_speed();

            let mut player_inputs = game.player_inputs_pool.new();

            let time_per_tick = Duration::from_nanos(
                (Duration::from_secs(1).as_nanos() / ticks_per_second.get() as u128) as u64,
            );
            let ticks_to_send = game
                .game_data
                .prediction_timer
                .time_units_to_respect(time_per_tick, 7.try_into().unwrap())
                as GameTickType;
            game.game_data.get_and_update_latest_input(
                self.cur_time,
                time_per_tick,
                ticks_to_send,
                tick_of_inp,
                &mut player_inputs,
                &game.player_inputs_chainable_pool,
            );

            game.send_input(&player_inputs, sys);
            let game_state = &mut game.map.game;
            // save the current input of all users for possible recalculations later
            let tick_inps = &mut game.game_data.input_per_tick;

            let add_input =
                |tick_of_inp: GameTickType, input_per_tick: &mut ClientPlayerInputPerTick| {
                    if !input_per_tick.contains_key(&tick_of_inp) {
                        input_per_tick.insert(tick_of_inp, game.game_data.player_inp_pool.new());
                    }

                    // apply input of local player to player
                    game.game_data.local.local_players.iter().for_each(
                        |(local_player_id, local_player)| {
                            let player_inp = input_per_tick.get_mut(&tick_of_inp).unwrap();
                            player_inp.insert(*local_player_id, local_player.sent_input);
                        },
                    );
                };
            add_input(tick_of_inp, tick_inps);

            let time_for_prediction = self.cur_time;

            let instant_input = self.config.game.cl.instant_input;
            // Reset the game state if needed
            if instant_input {
                if let Some(cur_state_snap) = game.game_data.cur_state_snap.take() {
                    let _ = game_state.build_from_snapshot(&cur_state_snap);
                }
            }

            fn apply_input(
                predicted_game_monotonic_tick: GameTickType,
                tick_inps: &mut FxLinkedHashMap<u64, PoolFxLinkedHashMap<PlayerId, PlayerInput>>,
                fallback_to_prev_input: bool,
                mut on_apply: impl FnMut(&PlayerId, &PlayerInput, CharacterInputConsumableDiff),
            ) {
                let tick_of_inp = predicted_game_monotonic_tick + 1;
                let (next_input, prev_input) = (
                    tick_inps.get(&tick_of_inp).or_else(|| {
                        tick_inps
                            .iter()
                            .rev()
                            .find_map(|(&tick, inp)| (tick <= tick_of_inp).then_some(inp))
                    }),
                    tick_inps.get(&predicted_game_monotonic_tick),
                );
                let check_input = if fallback_to_prev_input {
                    next_input.or(prev_input)
                } else {
                    next_input
                };
                if let Some(inputs) = check_input {
                    for (id, tick_inp) in inputs.iter() {
                        let mut inp = PlayerInput::default();
                        if let Some(prev_inp) =
                            prev_input.or(next_input).and_then(|inp| inp.get(id))
                        {
                            inp.inp = prev_inp.inp;
                        }
                        if let Some(diff) =
                            inp.try_overwrite(&tick_inp.inp, tick_inp.version(), true)
                        {
                            on_apply(id, tick_inp, diff);
                        }
                    }
                }
            }

            // do the ticks if necessary
            while is_next_tick(
                time_for_prediction,
                &mut game.game_data.last_game_tick,
                ticks_per_second,
            ) {
                // apply input of players
                let mut inps = game.game_data.player_inputs_state_pool.new();
                apply_input(
                    game_state.predicted_game_monotonic_tick,
                    tick_inps,
                    false,
                    |id, tick_inp, diff| {
                        inps.insert(
                            *id,
                            CharacterInputInfo {
                                inp: tick_inp.inp,
                                diff,
                            },
                        );
                    },
                );
                game_state.set_player_inputs(inps);

                let cur_snap = game_state.snapshot_for(SnapshotClientInfo::Everything);
                game_state.build_from_snapshot_for_prev(&cur_snap);

                game_state.predicted_game_monotonic_tick += 1;
                game_state.tick(Default::default());

                Server::dbg_game(
                    &self.config.game.dbg,
                    &game.game_data.last_game_tick,
                    game_state,
                    tick_inps
                        .get(&game_state.predicted_game_monotonic_tick)
                        .map(|inps| inps.values().map(|inp| &inp.inp)),
                    game_state.predicted_game_monotonic_tick,
                    ticks_per_second.get(),
                    &self.shared_info,
                    "client",
                );

                let mut player_ids = game.game_data.player_ids_pool.new();
                player_ids.extend(game.game_data.local.local_players.keys());
                let events = game_state.events_for(EventClientInfo {
                    client_player_ids: player_ids,
                    everything: true,
                    other_stages: true,
                });
                if !events.is_empty() {
                    game.events
                        .entry((game_state.predicted_game_monotonic_tick, true))
                        .or_insert_with(|| events);
                }
                game_state.clear_events();

                // add a "dummy" input for the next tick already, since in a bad
                // case this while-loop might run again
                add_input(game_state.predicted_game_monotonic_tick + 1, tick_inps);
            }

            // next intra tick time
            game.game_data.intra_tick_time = intra_tick_time(
                self.cur_time,
                game.game_data.last_game_tick,
                ticks_per_second,
            );

            if instant_input {
                let cur_state_snap = game_state.snapshot_for(SnapshotClientInfo::Everything);
                game_state.build_from_snapshot_for_prev(&cur_state_snap);
                game.game_data.cur_state_snap = Some(cur_state_snap);

                // there is always a prediction tick
                // apply input of players for it as if it's the next tick
                let mut pred_inps = game.game_data.player_inputs_state_pool.new();
                apply_input(
                    game_state.predicted_game_monotonic_tick,
                    tick_inps,
                    true,
                    |id, tick_inp, diff| {
                        pred_inps.insert(
                            *id,
                            CharacterInputInfo {
                                inp: tick_inp.inp,
                                diff,
                            },
                        );
                    },
                );
                game_state.set_player_inputs(pred_inps);
                game_state.tick(TickOptions {
                    is_future_tick_prediction: true,
                });
                game_state.clear_events();
            }

            game.game_data.last_game_tick = Duration::from_secs_f64(
                (game.game_data.last_game_tick.as_secs_f64()
                    + game.game_data.prediction_timer.smooth_adjustment_time())
                .clamp(0.0, f64::MAX),
            );
        }

        // rendering
        self.render(native);

        self.spatial_chat.update(
            &self.scene,
            if let Game::Active(game) = &mut self.game {
                game.spatial_world.zip_mut(
                    game.game_data
                        .local
                        .active_local_player()
                        .map(|(id, _)| (*id, &*game.network)),
                )
            } else {
                SpatialChatGameWorldTyRef::None
            },
            &self.config.game,
        );

        // sleep time related stuff
        let cur_time = self.sys.time_get();

        // force limit fps in menus
        let refresh_rate = if self.ui_manager.ui.ui_state.is_ui_open && self.demo_player.is_none() {
            (self.config.engine.wnd.refresh_rate_mhz as u64)
                .div_ceil(1000)
                .clamp(60, u64::MAX)
                .min(if self.config.game.cl.refresh_rate > 0 {
                    self.config.game.cl.refresh_rate
                } else {
                    u64::MAX
                })
        } else {
            // use full power during encoding
            if self.demo_player.as_ref().is_some_and(|p| p.is_encoder()) {
                0
            } else {
                self.config.game.cl.refresh_rate
            }
        };
        if refresh_rate > 0 {
            let time_until_tick_nanos = Duration::from_secs(1).as_nanos() as u64 / refresh_rate;

            let sleep_time_nanos = time_until_tick_nanos as i64
                - (cur_time.as_nanos() as i64 - self.last_refresh_rate_time.as_nanos() as i64);
            if sleep_time_nanos > 0 {
                std::thread::sleep(Duration::from_nanos(sleep_time_nanos as u64));
            }

            self.last_refresh_rate_time = Duration::from_nanos(
                // clamp to half of 60 FPS frame time
                (cur_time.as_nanos() as i64
                    + sleep_time_nanos.clamp(-16666666666 / 2, 16666666666 / 2))
                    as u64,
            );
        } else {
            self.last_refresh_rate_time = cur_time;
        }

        self.inp_manager.new_frame();
    }

    fn resized(&mut self, native: &mut dyn NativeImpl, new_width: u32, new_height: u32) {
        let window_props = self.graphics_backend.resized(
            &self.graphics.backend_handle.backend_cmds,
            self.graphics.stream_handle.stream_data(),
            native,
            new_width,
            new_height,
        );
        self.graphics.resized(window_props);
        // update config variables
        let wnd = &mut self.config.engine.wnd;
        let window = native.borrow_window();
        if wnd.fullscreen {
            wnd.fullscreen_width = new_width;
            wnd.fullscreen_height = new_height;
        } else {
            let scale_factor = window.scale_factor();
            wnd.window_width = new_width as f64 / scale_factor;
            wnd.window_height = new_height as f64 / scale_factor;
        }
        if let Some(monitor) = window.current_monitor() {
            wnd.refresh_rate_mhz = monitor
                .refresh_rate_millihertz()
                .unwrap_or(wnd.refresh_rate_mhz);
        }
    }

    fn window_options_changed(&mut self, wnd: NativeWindowOptions) {
        let config_wnd = &mut self.config.engine.wnd;
        config_wnd.fullscreen = wnd.mode.is_fullscreen();
        config_wnd.decorated = wnd.decorated;
        config_wnd.maximized = wnd.maximized;
        match wnd.mode {
            WindowMode::Fullscreen {
                resolution,
                fallback_window,
            } => {
                if let Some(resolution) = resolution {
                    config_wnd.fullscreen_width = resolution.width;
                    config_wnd.fullscreen_height = resolution.height;
                }

                config_wnd.window_width = fallback_window.width;
                config_wnd.window_height = fallback_window.height;
            }
            WindowMode::Windowed(pixels) => {
                config_wnd.window_width = pixels.width;
                config_wnd.window_height = pixels.height;
            }
        }
        config_wnd.refresh_rate_mhz = wnd.refresh_rate_milli_hertz;
        config_wnd.monitor = wnd
            .monitor
            .map(|monitor| ConfigMonitor {
                name: monitor.name,
                width: monitor.size.width,
                height: monitor.size.height,
            })
            .unwrap_or_default();
    }

    fn destroy(mut self) {
        #[cfg(feature = "alloc_track")]
        track_report();

        if !self.config.engine.ui.keep {
            self.config.engine.ui.path = Default::default();
        }

        // destroy everything
        config_fs::save(&self.config.engine, &self.io);
        game_config_fs::fs::save(&self.config.game, &self.io);
    }

    fn window_created_ntfy(&mut self, native: &mut dyn NativeImpl) -> anyhow::Result<()> {
        self.graphics_backend.window_created_ntfy(
            BackendWindow::Winit {
                window: native.borrow_window(),
            },
            &self.config.engine.dbg,
        )
    }

    fn window_destroyed_ntfy(&mut self, _native: &mut dyn NativeImpl) -> anyhow::Result<()> {
        self.graphics_backend.window_destroyed_ntfy()
    }
}
