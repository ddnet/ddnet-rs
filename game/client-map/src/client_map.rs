use std::{collections::BTreeMap, path::Path, rc::Rc, sync::Arc};

use anyhow::anyhow;
use base_io::{io::Io, path_to_url::relative_path_to_url, runtime::IoRuntimeTask};
use client_render_base::map::render_map_base::{ClientMapRender, RenderMapLoading};
use client_render_game::render_game::{RenderGameCreateOptions, RenderGameInterface, RenderModTy};
use config::config::ConfigDebug;

use game_database::dummy::DummyDb;
use game_interface::{
    interface::{GameStateCreateOptions, MAX_MAP_NAME_LEN},
    types::game::GameTickType,
};
use graphics::graphics::graphics::Graphics;

use base::{
    hash::{fmt_hash, generate_hash_for, Hash},
    network_string::NetworkReducedAsciiString,
    system::System,
};
use game_state_wasm::game::state_wasm_manager::{
    GameStateMod, GameStateWasmManager, STATE_MODS_PATH,
};
use graphics_backend::backend::GraphicsBackend;
use map::map::Map;
use rayon::ThreadPool;
pub use render_game_wasm::render::render_wasm_manager::RenderGameWasmManager;
use render_game_wasm::render::render_wasm_manager::{RenderGameMod, RENDER_MODS_PATH};

use game_base::{connecting_log::ConnectingLog, network::messages::GameModification};
use sound::sound::SoundManager;

#[derive(Debug)]
pub enum ClientGameStateModTask {
    Native,
    Ddnet,
    Wasm { file: IoRuntimeTask<Vec<u8>> },
}

impl ClientGameStateModTask {
    pub fn is_finished(&self) -> bool {
        match self {
            ClientGameStateModTask::Native => true,
            ClientGameStateModTask::Ddnet => true,
            ClientGameStateModTask::Wasm { file } => file.is_finished(),
        }
    }

    pub fn to_game_state_mod(self) -> GameStateMod {
        match self {
            ClientGameStateModTask::Native => GameStateMod::Native,
            ClientGameStateModTask::Ddnet => GameStateMod::Ddnet,
            ClientGameStateModTask::Wasm { file } => GameStateMod::Wasm {
                file: file.get_storage().unwrap(),
            },
        }
    }
}

#[derive(Debug)]
pub struct ClientMapLoadingFile {
    pub task: IoRuntimeTask<Vec<u8>>,
    io: Io,
    thread_pool: Arc<rayon::ThreadPool>,
    as_menu_map: bool,
    map_name: NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
    pub game_mod_task: ClientGameStateModTask,
    pub game_options: GameStateCreateOptions,
    props: RenderGameCreateOptions,

    config_debug: ConfigDebug,
    sound: SoundManager,
    graphics: Graphics,
    backend: Rc<GraphicsBackend>,
    sys: System,
    log: ConnectingLog,
}

impl ClientMapLoadingFile {
    pub fn new(
        sound: &SoundManager,
        graphics: &Graphics,
        backend: &Rc<GraphicsBackend>,
        sys: &System,
        base_path: &Path,
        map_name: &NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
        map_hash: Option<Hash>,
        io: &Io,
        thread_pool: &Arc<rayon::ThreadPool>,
        game_mod: GameModification,
        as_menu_map: bool,
        config_debug: &ConfigDebug,
        game_options: GameStateCreateOptions,
        props: RenderGameCreateOptions,
        log: ConnectingLog,
    ) -> Self {
        let downloaded_path: Option<&Path> = (!as_menu_map).then_some("downloaded".as_ref());
        let download_map_file_name = if let Some(map_hash) = map_hash {
            base_path.join(format!(
                "{}_{}.twmap",
                map_name.as_str(),
                fmt_hash(&map_hash)
            ))
        } else {
            base_path.join(format!("{}.twmap", map_name.as_str()))
        };
        let map_file_name = if let Some(downloaded_path) = downloaded_path {
            downloaded_path.join(&download_map_file_name)
        } else {
            download_map_file_name.clone()
        };

        let file_system = io.fs.clone();
        let http = io.http.clone();
        let log_load = log.clone();
        let resource_download_server_thread = props.resource_download_server.clone();
        Self {
            task: io.rt.spawn(async move {
                log_load.log(format!(
                    "Ready map file from file system: {:?}",
                    map_file_name
                ));
                let file = file_system.read_file(map_file_name.as_ref()).await;

                let file = match file {
                    Ok(file) => Ok(file),
                    Err(err) => {
                        log_load.log("Loading map failed, downloading from server now.");
                        // try to download file
                        if let Some(resource_download_server) = resource_download_server_thread
                            .and_then(|url| {
                                relative_path_to_url(&download_map_file_name)
                                    .ok()
                                    .and_then(|name| url.join(&name).ok())
                            })
                        {
                            let file = http
                                .download_binary(
                                    resource_download_server,
                                    &map_hash.unwrap_or_default(),
                                )
                                .await
                                .map_err(|err| anyhow!("failed to download map: {err}"))?
                                .to_vec();
                            // maps are allowed to be arbitrary, but all maps should still start
                            // with the twmap header.
                            anyhow::ensure!(
                                Map::validate_twmap_header(&file),
                                "not a twmap file or variant of it."
                            );
                            let file_path: &Path = map_file_name.as_ref();
                            if let Some(dir) = file_path.parent() {
                                file_system.create_dir(dir).await?;
                            }
                            file_system
                                .write_file(map_file_name.as_ref(), file.clone())
                                .await?;
                            log_load.log("Map downloaded successfully and saved to disk.");
                            Ok(file)
                        } else {
                            Err(err)
                        }
                    }
                }?;

                Ok(file)
            }),
            io: io.clone(),
            thread_pool: thread_pool.clone(),
            as_menu_map,
            map_name: map_name.clone(),
            game_mod_task: match game_mod {
                GameModification::Native => ClientGameStateModTask::Native,
                GameModification::Ddnet => ClientGameStateModTask::Ddnet,
                GameModification::Wasm { name, hash } => ClientGameStateModTask::Wasm {
                    file: {
                        let fs = io.fs.clone();
                        let http = io.http.clone();
                        let download_game_mod_file_name = format!(
                            "{}/{}_{}.wasm",
                            STATE_MODS_PATH,
                            name.as_str(),
                            fmt_hash(&hash)
                        );
                        let game_mod_file_name = if let Some(downloaded_path) = downloaded_path {
                            downloaded_path.join(&download_game_mod_file_name)
                        } else {
                            download_game_mod_file_name.as_str().into()
                        };
                        let resource_download_server_thread =
                            props.resource_download_server.clone();

                        let log = log.clone();

                        io.rt.spawn(async move {
                            log.log(format!(
                                "Loading physics wasm module: {:?}",
                                game_mod_file_name
                            ));
                            let file = fs.read_file(game_mod_file_name.as_ref()).await;

                            let file = match file {
                                Ok(file) => Ok(file),
                                Err(err) => {
                                    log.log(
                                        "Physics wasm module not found, downloading from server.",
                                    );
                                    // try to download file
                                    if let Some(resource_download_server) =
                                        resource_download_server_thread.and_then(|url| {
                                            relative_path_to_url(
                                                download_game_mod_file_name.as_ref(),
                                            )
                                            .ok()
                                            .and_then(|name| url.join(&name).ok())
                                        })
                                    {
                                        let file = http
                                            .download_binary(resource_download_server, &hash)
                                            .await
                                            .map_err(|err| {
                                                anyhow!("failed to download mod: {err}")
                                            })?
                                            .to_vec();

                                        // ensure that downloaded file is valid wasm file
                                        wasmparser::validate(&file)?;

                                        let file_path: &Path = game_mod_file_name.as_ref();
                                        if let Some(dir) = file_path.parent() {
                                            fs.create_dir(dir).await?;
                                        }
                                        fs.write_file(game_mod_file_name.as_ref(), file.clone())
                                            .await?;
                                        log.log(
                                            "Physics wasm module downloaded sucessfully \
                                            and written to disk.",
                                        );

                                        Ok(file)
                                    } else {
                                        Err(err)
                                    }
                                }
                            }?;

                            let wasm_module = GameStateWasmManager::load_module(&fs, file).await?;

                            Ok(wasm_module)
                        })
                    },
                },
            },
            config_debug: *config_debug,
            backend: backend.clone(),
            graphics: graphics.clone(),
            sound: sound.clone(),
            sys: sys.clone(),
            props,
            game_options,
            log,
        }
    }
}

pub struct GameCreateProps {
    sound: SoundManager,
    graphics: Graphics,
    backend: Rc<GraphicsBackend>,
    io: Io,
    thread_pool: Arc<ThreadPool>,
    sys: System,
    map_file: Vec<u8>,
    props: RenderGameCreateOptions,
    config: ConfigDebug,
}

pub enum GameLoading {
    Task {
        task: IoRuntimeTask<RenderGameMod>,
        props: Box<GameCreateProps>,
    },
    Game(RenderGameWasmManager),
    Err(anyhow::Error),
}

pub enum ClientMapComponentLoadingType {
    Game(GameLoading),
    Menu(ClientMapRender),
}

pub struct ClientMapComponentLoading {
    ty: ClientMapComponentLoadingType,
    io: Io,
    thread_pool: Arc<rayon::ThreadPool>,
    log: ConnectingLog,
}

impl ClientMapComponentLoading {
    pub fn new(
        thread_pool: Arc<rayon::ThreadPool>,
        file: Vec<u8>,
        io: Io,
        sound: &SoundManager,
        graphics: &Graphics,
        backend: &Rc<GraphicsBackend>,
        sys: &System,
        config: &ConfigDebug,
        as_menu_map: bool,
        props: RenderGameCreateOptions,
        log: ConnectingLog,
    ) -> Self {
        Self {
            ty: if as_menu_map {
                ClientMapComponentLoadingType::Menu(ClientMapRender::new(RenderMapLoading::new(
                    thread_pool.clone(),
                    file,
                    props.resource_download_server,
                    io.clone(),
                    sound,
                    props.sound_props,
                    graphics,
                    config,
                    None,
                )))
            } else {
                let fs = io.fs.clone();
                let render_mod = props.render_mod.clone();
                let log = log.clone();
                log.log("Preparing rendering module");
                ClientMapComponentLoadingType::Game(GameLoading::Task {
                    task: io.rt.spawn(async move {
                        let required = matches!(&render_mod, RenderModTy::Required { .. });
                        let local_name = if let RenderModTy::Try { local_name, .. } = &render_mod {
                            local_name.clone()
                        } else {
                            None
                        };
                        match render_mod {
                            RenderModTy::Native => Ok(RenderGameMod::Native),
                            RenderModTy::Try { name, hash, .. }
                            | RenderModTy::Required { name, hash } => {
                                // load the wasm file
                                let path_str = if let Some(hash) = hash {
                                    format!(
                                        "{}/{}_{}.wasm",
                                        RENDER_MODS_PATH,
                                        name.as_str(),
                                        fmt_hash(&hash)
                                    )
                                } else {
                                    format!("{}/{}.wasm", RENDER_MODS_PATH, name.as_str())
                                };
                                log.log(format!("Reading rendering module: {}", path_str));
                                let file = fs
                                    .read_file(path_str.as_ref())
                                    .await
                                    .map_err(|err| anyhow!(err))
                                    .and_then(|file| {
                                        if let Some(hash) = hash {
                                            if generate_hash_for(&file) == hash {
                                                Ok(file)
                                            } else {
                                                Err(anyhow!(
                                                    "render mod could not be load, \
                                                    because of a hash mismatch"
                                                ))
                                            }
                                        } else {
                                            Ok(file)
                                        }
                                    });

                                let file = if let (Err(err), Some(local_name)) = (&file, local_name)
                                {
                                    log.log(format!(
                                        "Failed to load optional render mod: {err}. \
                                        Falling back to local mod."
                                    ));
                                    log::info!(
                                        "Failed to load optional render mod: {err}. \
                                        Falling back to local mod."
                                    );
                                    fs.read_file(
                                        format!(
                                            "{}/{}.wasm",
                                            RENDER_MODS_PATH,
                                            local_name.as_str()
                                        )
                                        .as_ref(),
                                    )
                                    .await
                                    .map_err(|err| anyhow!(err))
                                } else {
                                    file
                                };

                                let module = match file {
                                    Ok(file) => RenderGameWasmManager::load_module(&fs, file).await,
                                    Err(err) => Err(err),
                                };

                                if required {
                                    module.map(|module| RenderGameMod::Wasm { file: module })
                                } else {
                                    match module {
                                        Ok(module) => Ok(RenderGameMod::Wasm { file: module }),
                                        Err(err) => {
                                            log::info!("Failed to load optional render mod: {err}");
                                            Ok(RenderGameMod::Native)
                                        }
                                    }
                                }
                            }
                        }
                    }),
                    props: Box::new(GameCreateProps {
                        sound: sound.clone(),
                        graphics: graphics.clone(),
                        backend: backend.clone(),
                        io: io.clone(),
                        thread_pool: thread_pool.clone(),
                        sys: sys.clone(),
                        map_file: file,
                        config: *config,
                        props,
                    }),
                })
            },
            io,
            thread_pool,
            log,
        }
    }
}

pub struct GameUnpredicted {
    pub prev: Option<GameTickType>,
    pub cur: Option<GameTickType>,
    pub state: GameStateWasmManager,
}

impl GameUnpredicted {
    pub fn from_snapshots(
        &mut self,
        last_snaps: &BTreeMap<GameTickType, Vec<u8>>,
        first_tick: GameTickType,
    ) {
        use game_interface::interface::GameStateInterface;
        use pool::mt_datatypes::PoolCow;
        let mut changed_state = false;
        let first_snap = last_snaps.range(0..=first_tick).next_back();
        if let Some((id, snapshot)) = first_snap {
            if self.prev.is_none_or(|tick| tick != *id) {
                self.state
                    .build_from_snapshot_for_prev(&PoolCow::from_without_pool(
                        snapshot.clone().into(),
                    ));
                self.prev = Some(*id);
                changed_state = true;
            }
        }
        if let Some((id, snapshot)) = last_snaps.range(first_tick + 1..).next().or(first_snap) {
            if self.cur.is_none_or(|tick| tick != *id) {
                let _ = self
                    .state
                    .build_from_snapshot(&PoolCow::from_without_pool(snapshot.clone().into()));
                self.cur = Some(*id);
                changed_state = true;
            }
        }
        if changed_state {
            self.state.clear_events();
        }
    }
}

pub struct GameMap {
    pub render: RenderGameWasmManager,
    /// client local calculated game
    pub game: GameStateWasmManager,
    /// unpredicted local game (similar to how a demo works)
    /// for non-anti-ping calculations
    pub unpredicted_game: GameUnpredicted,
}

pub enum ClientMapFile {
    Menu { render: ClientMapRender },
    Game(GameMap),
}

pub enum ClientMapLoading {
    /// load the "raw" map file
    File(ClientMapLoadingFile),
    /// wait for the individual components to finish parsing the map file
    /// physics and graphics independently
    PrepareComponents {
        render: ClientMapComponentLoading,
        map: Vec<u8>,
        map_name: NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
        game_mod: GameStateMod,
        game_options: GameStateCreateOptions,
    },
    /// finished loading
    Map(ClientMapFile),
    /// Map is in an error state
    Err(anyhow::Error),
    /// map not loading
    None,
}

impl ClientMapLoading {
    pub fn new(
        sound: &SoundManager,
        graphics: &Graphics,
        backend: &Rc<GraphicsBackend>,
        sys: &System,
        base_path: &Path,
        map_name: &NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
        map_hash: Option<Hash>,
        io: &Io,
        thread_pool: &Arc<rayon::ThreadPool>,
        game_mod: GameModification,
        as_menu_map: bool,
        config_debug: &ConfigDebug,
        game_options: GameStateCreateOptions,
        props: RenderGameCreateOptions,
        log: ConnectingLog,
    ) -> Self {
        Self::File(ClientMapLoadingFile::new(
            sound,
            graphics,
            backend,
            sys,
            base_path,
            map_name,
            map_hash,
            io,
            thread_pool,
            game_mod,
            as_menu_map,
            config_debug,
            game_options,
            props,
            log,
        ))
    }

    pub fn try_get(&self) -> Option<&ClientMapFile> {
        if let Self::Map(map_file) = self {
            Some(map_file)
        } else {
            None
        }
    }

    pub fn try_get_mut(&mut self) -> Option<&mut ClientMapFile> {
        if let Self::Map(map_file) = self {
            Some(map_file)
        } else {
            None
        }
    }

    pub fn err(&self) -> anyhow::Result<(), String> {
        if let Self::Err(err) = self {
            Err(err.to_string())
        } else {
            Ok(())
        }
    }

    pub fn is_fully_loaded(&self) -> bool {
        if let Self::Map(_map_file) = self {
            return true;
        }
        false
    }

    pub fn continue_loading(&mut self) -> Option<&ClientMapFile> {
        let mut self_helper = ClientMapLoading::None;
        std::mem::swap(&mut self_helper, self);
        match self_helper {
            Self::File(file) => {
                if file.task.is_finished() && file.game_mod_task.is_finished() {
                    match file.task.get_storage() {
                        Ok(map_file) => {
                            let game_mod = file.game_mod_task.to_game_state_mod();

                            let loading = ClientMapComponentLoading::new(
                                file.thread_pool.clone(),
                                map_file.clone(),
                                file.io.clone(),
                                &file.sound,
                                &file.graphics,
                                &file.backend,
                                &file.sys,
                                &file.config_debug,
                                file.as_menu_map,
                                file.props,
                                file.log,
                            );

                            *self = Self::PrepareComponents {
                                render: loading,
                                map: map_file,
                                map_name: file.map_name,
                                game_mod,
                                game_options: file.game_options,
                            }
                        }
                        Err(err) => *self = Self::Err(err),
                    }
                } else {
                    *self = Self::File(file)
                }
            }
            Self::PrepareComponents {
                render,
                map,
                map_name,
                game_mod,
                game_options,
            } => {
                match render.ty {
                    ClientMapComponentLoadingType::Game(mut load_game) => {
                        if let GameLoading::Task { task, props } = load_game {
                            if task.is_finished() {
                                match task.get_storage() {
                                    Ok(file) => {
                                        match RenderGameWasmManager::new(
                                            &props.sound,
                                            &props.graphics,
                                            &props.backend,
                                            &props.io,
                                            &props.thread_pool,
                                            &props.sys,
                                            props.map_file,
                                            &props.config,
                                            file,
                                            props.props,
                                        ) {
                                            Ok(game) => load_game = GameLoading::Game(game),
                                            Err(err) => load_game = GameLoading::Err(err),
                                        }
                                    }
                                    Err(err) => load_game = GameLoading::Err(err),
                                }
                            } else {
                                load_game = GameLoading::Task { task, props };
                            }
                        }
                        match load_game {
                            GameLoading::Task { task, props } => {
                                *self = Self::PrepareComponents {
                                    render: ClientMapComponentLoading {
                                        ty: ClientMapComponentLoadingType::Game(
                                            GameLoading::Task { task, props },
                                        ),
                                        io: render.io,
                                        thread_pool: render.thread_pool,
                                        log: render.log,
                                    },
                                    map,
                                    map_name,
                                    game_mod,
                                    game_options,
                                }
                            }
                            GameLoading::Game(mut load_game) => {
                                match load_game.continue_loading() {
                                    Ok(loaded) => {
                                        if loaded {
                                            match (
                                                GameStateWasmManager::new(
                                                    game_mod.clone(),
                                                    map.clone(),
                                                    map_name.clone(),
                                                    game_options.clone(),
                                                    &render.io,
                                                    Arc::new(DummyDb),
                                                ),
                                                GameStateWasmManager::new(
                                                    game_mod,
                                                    map,
                                                    map_name,
                                                    game_options,
                                                    &render.io,
                                                    Arc::new(DummyDb),
                                                ),
                                            ) {
                                                (Ok(game), Ok(unpredicted_game)) => {
                                                    load_game.set_chat_commands(
                                                        game.info.chat_commands.clone(),
                                                    );

                                                    render.log.log("Loaded map & modules.");
                                                    // finished loading
                                                    *self =
                                                        Self::Map(ClientMapFile::Game(GameMap {
                                                            render: load_game,
                                                            game,
                                                            unpredicted_game: GameUnpredicted {
                                                                prev: None,
                                                                cur: None,
                                                                state: unpredicted_game,
                                                            },
                                                        }));
                                                }
                                                (Err(err), Ok(_)) | (Ok(_), Err(err)) => {
                                                    *self = Self::Err(err);
                                                }
                                                (Err(err1), Err(err2)) => {
                                                    *self = Self::Err(anyhow!("{err1}. {err2}"));
                                                }
                                            }
                                        } else {
                                            *self = Self::PrepareComponents {
                                                render: ClientMapComponentLoading {
                                                    ty: ClientMapComponentLoadingType::Game(
                                                        GameLoading::Game(load_game),
                                                    ),
                                                    io: render.io,
                                                    thread_pool: render.thread_pool,
                                                    log: render.log,
                                                },
                                                map,
                                                map_name,
                                                game_mod,
                                                game_options,
                                            }
                                        }
                                    }
                                    Err(err) => *self = Self::Err(anyhow!("{}", err)),
                                }
                            }
                            GameLoading::Err(err) => *self = Self::Err(err),
                        }
                    }
                    ClientMapComponentLoadingType::Menu(mut map_prepare) => {
                        match map_prepare.continue_loading() {
                            Ok(loaded) => {
                                if loaded.is_some() {
                                    *self = Self::Map(ClientMapFile::Menu {
                                        render: map_prepare,
                                    })
                                } else {
                                    *self = Self::PrepareComponents {
                                        render: ClientMapComponentLoading {
                                            ty: ClientMapComponentLoadingType::Menu(map_prepare),
                                            io: render.io,
                                            thread_pool: render.thread_pool,
                                            log: render.log,
                                        },
                                        map,
                                        map_name,
                                        game_mod,
                                        game_options,
                                    }
                                }
                            }
                            Err(err) => {
                                *self = Self::Err(err);
                            }
                        }
                    }
                }
            }
            Self::Map(map) => *self = ClientMapLoading::Map(map),
            Self::Err(err) => *self = Self::Err(err),
            Self::None => {}
        }
        self.try_get()
    }
}
