use std::{
    collections::HashMap, future::Future, net::IpAddr, path::Path, pin::Pin, sync::Arc,
    time::Duration,
};

use anyhow::anyhow;
use base::{
    hash::{fmt_hash, name_and_hash, Hash},
    linked_hash_map_view::FxLinkedHashMap,
    network_string::NetworkReducedAsciiString,
};
use base_http::http_server::HttpDownloadServer;
use base_io::io::Io;
use base_io_traits::fs_traits::FileSystemWatcherItemInterface;
use cache::Cache;
use game_database::traits::{DbInterface, DbKind};

use game_state_wasm::game::state_wasm_manager::{
    GameStateMod, GameStateWasmManager, STATE_MODS_PATH,
};
use map::map::{resources::MapResourceMetaData, Map};
use network::network::connection::NetworkConnectionId;
use pool::{datatypes::PoolFxLinkedHashMap, pool::Pool};

use game_interface::{
    interface::{GameStateCreateOptions, GameStateInterface, MAX_MAP_NAME_LEN},
    rcon_commands::AuthLevel,
    types::{
        emoticons::EmoticonType,
        game::GameTickType,
        id_types::{CharacterId, PlayerId},
        input::CharacterInputInfo,
        player_info::{PlayerClientInfo, PlayerDropReason},
        render::character::{CharacterInfo, TeeEye},
    },
    votes::{VoteState, Voted},
};
use game_base::{
    network::messages::{GameModification, RenderModification, RequiredResources},
    player_input::PlayerInput,
};

use crate::spatial_chat::SpatialWorld;

#[derive(Debug)]
pub struct ServerPlayer {
    pub network_id: NetworkConnectionId,
    pub id: PlayerId,

    pub inp: PlayerInput,
}

impl ServerPlayer {
    pub fn new(network_id: &NetworkConnectionId, id: &PlayerId) -> Self {
        Self {
            network_id: *network_id,
            id: *id,

            inp: Default::default(),
        }
    }
}

pub struct ServerMap {
    pub name: NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
    pub map_file: Vec<u8>,
    pub resource_files: HashMap<String, Vec<u8>>,
}

impl ServerMap {
    pub fn new(
        map_name: &NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
        io: &Io,
        runtime_thread_pool: &Arc<rayon::ThreadPool>,
    ) -> anyhow::Result<Self> {
        let map_file_str = map_name.to_string();
        let fs = io.fs.clone();
        let map = io.rt.spawn(async move {
            let map_path = format!("map/maps/{}.twmap", map_file_str);
            let map_file = fs.read_file(map_path.as_ref()).await?;

            let (resources, _) = Map::read_resources_and_header(&map_file)?;
            let mut resource_files: HashMap<String, Vec<u8>> = Default::default();

            let (names, files): (Vec<_>, Vec<_>) = {
                type LoadFuture = Pin<Box<dyn Future<Output = anyhow::Result<Vec<u8>>> + Send>>;
                let mut resource_load: HashMap<String, LoadFuture> = Default::default();

                for image in resources.images.iter().chain(resources.image_arrays.iter()) {
                    let mut read_file = |meta: &MapResourceMetaData| {
                        let path = format!(
                            "map/resources/images/{}_{}.{}",
                            image.name.as_str(),
                            fmt_hash(&meta.blake3_hash),
                            meta.ty.as_str()
                        );
                        let fs = fs.clone();
                        let path_task = path.clone();
                        let task = async move {
                            fs.read_file(path_task.as_ref())
                                .await
                                .map_err(|err| anyhow!("loading images failed: {err}"))
                        };
                        resource_load.insert(path, Box::pin(task));
                    };
                    read_file(&image.meta);
                    if let Some(hq_meta) = &image.hq_meta {
                        read_file(hq_meta);
                    }
                }

                for sound in &resources.sounds {
                    let mut read_file = |meta: &MapResourceMetaData| {
                        let path = format!(
                            "map/resources/sounds/{}_{}.{}",
                            sound.name.as_str(),
                            fmt_hash(&meta.blake3_hash),
                            meta.ty.as_str()
                        );
                        let fs = fs.clone();
                        let path_task = path.clone();
                        let task = async move {
                            fs.read_file(path_task.as_ref())
                                .await
                                .map_err(|err| anyhow!("loading sound failed: {err}"))
                        };
                        resource_load.insert(path, Box::pin(task));
                    };
                    read_file(&sound.meta);
                    if let Some(hq_meta) = &sound.hq_meta {
                        read_file(hq_meta);
                    }
                }
                resource_load.into_iter().unzip()
            };
            let files = futures::future::join_all(files).await;

            for (path, file) in names.into_iter().zip(files.into_iter()) {
                resource_files.insert(path, file?);
            }

            Ok((map_file, resource_files))
        });

        let map_res = map.get_storage();

        // try to load legacy map with that name, convert it to new format
        let (map_file, resource_files) = match map_res {
            Ok((map_file, resource_files)) => anyhow::Ok((map_file, resource_files)),
            #[cfg(feature = "legacy")]
            Err(map_res_err) => {
                let map_path = format!("legacy/maps/{}.map", map_name.as_str());
                let fs = io.fs.clone();
                let cache = Arc::new(Cache::<2024115>::new("legacy-to-new-map-server", io));

                let map_name = map_name.to_string();
                let tp = runtime_thread_pool.clone();
                let legacy_to_new = || {
                    let map_name = map_name.to_string();
                    let tp = tp.clone();
                    let fs = fs.clone();
                    let map_res_err = map_res_err.to_string();
                    Box::new(move |map_file: Vec<u8>| {
                        let map_name = map_name.to_string();
                        let tp = tp.clone();
                        let fs = fs.clone();
                        let map_res_err = map_res_err.to_string();
                        Box::pin(async move {
                            let map = map_convert_lib::legacy_to_new::legacy_to_new_from_buf_async(
                                map_file,
                                &map_name.clone(),
                                |path| {
                                    let path = path.to_path_buf();
                                    let fs = fs.clone();
                                    Box::pin(async move { Ok(fs.read_file(&path).await?) })
                                },
                                &tp,
                                true,
                            )
                            .await
                            .map_err(|err| {
                                anyhow!(
                                    "Loading map failed: {map_res_err}, \
                                    legacy map loading failed too: {err}"
                                )
                            })?;
                            let mut map_bytes = Vec::new();
                            map.map.write(&mut map_bytes, &tp)?;
                            let mut resource_files: HashMap<String, Vec<u8>> = Default::default();
                            for (blake3_hash, resource) in map.resources.images.into_iter() {
                                let path = format!(
                                    "map/resources/images/{}_{}.{}",
                                    resource.name,
                                    fmt_hash(&blake3_hash),
                                    resource.ty
                                );
                                resource_files.insert(path, resource.buf);
                            }
                            for (blake3_hash, resource) in map.resources.sounds.into_iter() {
                                let path = format!(
                                    "map/resources/sounds/{}_{}.{}",
                                    resource.name,
                                    fmt_hash(&blake3_hash),
                                    resource.ty
                                );
                                resource_files.insert(path, resource.buf);
                            }
                            for (path, resource_file) in resource_files {
                                let fs = fs.clone();
                                let path: &Path = path.as_ref();
                                if let Some(path) = path.parent() {
                                    fs.create_dir(path).await?;
                                }
                                fs.write_file(path, resource_file).await?;
                            }
                            Ok(map_bytes)
                        })
                    })
                };

                let cache = cache.clone();
                let legacy_to_new_task = legacy_to_new();
                let map_path_task = map_path.clone();
                let map_file = io
                    .rt
                    .spawn(async move {
                        let path = map_path_task.as_ref();
                        let map = cache
                            .load(path, move |map_file| legacy_to_new_task(map_file))
                            .await?;
                        Ok(map)
                    })
                    .get_storage()?;

                let map = Map::read(&map_file, runtime_thread_pool)?;
                let load_resources = || {
                    let mut resource_files: HashMap<String, Vec<u8>> = Default::default();
                    for path in map
                        .resources
                        .image_arrays
                        .iter()
                        .chain(map.resources.images.iter())
                        .flat_map(|resource| {
                            [format!(
                                "map/resources/images/{}_{}.{}",
                                resource.name.as_str(),
                                fmt_hash(&resource.meta.blake3_hash),
                                resource.meta.ty.as_str()
                            )]
                            .into_iter()
                            .chain(
                                resource
                                    .hq_meta
                                    .as_ref()
                                    .map(|hq_meta| {
                                        format!(
                                            "map/resources/images/{}_{}.{}",
                                            resource.name.as_str(),
                                            fmt_hash(&hq_meta.blake3_hash),
                                            hq_meta.ty.as_str()
                                        )
                                    })
                                    .into_iter(),
                            )
                            .collect::<Vec<_>>()
                        })
                        .chain(map.resources.sounds.iter().flat_map(|resource| {
                            [format!(
                                "map/resources/sounds/{}_{}.{}",
                                resource.name.as_str(),
                                fmt_hash(&resource.meta.blake3_hash),
                                resource.meta.ty.as_str()
                            )]
                            .into_iter()
                            .chain(
                                resource
                                    .hq_meta
                                    .as_ref()
                                    .map(|hq_meta| {
                                        format!(
                                            "map/resources/sounds/{}_{}.{}",
                                            resource.name.as_str(),
                                            fmt_hash(&hq_meta.blake3_hash),
                                            hq_meta.ty.as_str()
                                        )
                                    })
                                    .into_iter(),
                            )
                            .collect::<Vec<_>>()
                        }))
                    {
                        let file_path = path.clone();
                        let fs = io.fs.clone();
                        let file = io
                            .rt
                            .spawn(async move { Ok(fs.read_file(file_path.as_ref()).await?) })
                            .get_storage()?;
                        resource_files.insert(path, file);
                    }

                    anyhow::Ok(resource_files)
                };
                let resource_files = match load_resources() {
                    Ok(resources) => resources,
                    Err(_) => {
                        // try to load the whole map again
                        let legacy_to_new_task = legacy_to_new();
                        let map_path_task = map_path.clone();
                        io.rt
                            .spawn(async move {
                                let map_file = fs.read_file(map_path_task.as_ref()).await?;

                                legacy_to_new_task(map_file).await?;

                                Ok(())
                            })
                            .get_storage()?;
                        load_resources()?
                    }
                };

                Ok((map_file, resource_files))
            }
            #[cfg(not(feature = "legacy"))]
            Err(err) => Err(err),
        }?;

        Ok(Self {
            name: map_name.clone(),
            map_file,
            resource_files,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ClientAuth {
    pub cert: Arc<x509_cert::Certificate>,
    pub level: AuthLevel,
}

#[derive(Debug, Default)]
pub enum ServerExtraVoteInfo {
    Player {
        to_kick_player: NetworkConnectionId,
        ip: IpAddr,
    },
    #[default]
    None,
}

#[derive(Debug)]
pub struct ServerVote {
    pub state: VoteState,
    pub started_at: Duration,

    pub extra_vote_info: ServerExtraVoteInfo,

    pub participating_ip: HashMap<IpAddr, Voted>,
}

pub const RESERVED_VANILLA_NAMES: [&str; 4] = ["", "vanilla", "native", "default"];
pub const RESERVED_DDNET_NAMES: [&str; 1] = ["ddnet"];

pub struct ServerGame {
    pub players: FxLinkedHashMap<PlayerId, ServerPlayer>,
    pub game: GameStateWasmManager,
    pub cur_monotonic_tick: GameTickType,
    pub map: ServerMap,
    pub map_blake3_hash: Hash,
    pub required_resources: RequiredResources,
    pub game_mod: GameModification,
    pub render_mod: RenderModification,

    game_mod_fs_change_watcher: Option<Box<dyn FileSystemWatcherItemInterface>>,

    pub http_server: Option<HttpDownloadServer>,

    // votes
    pub cur_vote: Option<ServerVote>,

    pub queued_inputs: FxLinkedHashMap<GameTickType, FxLinkedHashMap<PlayerId, PlayerInput>>,

    pub spatial_world: Option<SpatialWorld>,

    pub cached_character_infos: PoolFxLinkedHashMap<CharacterId, CharacterInfo>,

    // pools
    pub(crate) inps_pool: Pool<FxLinkedHashMap<PlayerId, CharacterInputInfo>>,
}

impl ServerGame {
    pub fn new(
        map_name: &NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
        game_mod: &str,
        render_mod: &str,
        render_mod_hash: &[u8; 32],
        render_mod_required: bool,
        config: Option<Vec<u8>>,
        runtime_thread_pool: &Arc<rayon::ThreadPool>,
        io: &Io,
        db: &Arc<dyn DbInterface>,
        account_db: Option<DbKind>,
        spatial_chat: bool,
        download_server_port_v4: u16,
        download_server_port_v6: u16,
    ) -> anyhow::Result<Self> {
        let fs = io.fs.clone();
        let required_resources = io.rt.spawn(async move {
            let file = fs.read_file("required_resources.json".as_ref()).await?;
            Ok(serde_json::from_slice(&file)?)
        });

        let map = ServerMap::new(map_name, io, runtime_thread_pool)?;
        let (game_state_mod, game_mod, game_mod_file, game_mod_name, game_mod_blake3_hash) =
            match game_mod {
                x if RESERVED_VANILLA_NAMES.contains(&x) => (
                    GameStateMod::Native,
                    GameModification::Native,
                    Vec::new(),
                    "vanilla".to_string(),
                    None,
                ),
                x if RESERVED_DDNET_NAMES.contains(&x) => (
                    GameStateMod::Ddnet,
                    GameModification::Ddnet,
                    Vec::new(),
                    "ddnet".to_string(),
                    None,
                ),
                game_mod => {
                    let path = format!("{}/{}.wasm", STATE_MODS_PATH, game_mod);
                    let file_path = path.clone();
                    let (file, wasm_module) = {
                        let fs = io.fs.clone();

                        io.rt
                            .spawn(async move {
                                let file = fs.read_file(file_path.as_ref()).await?;
                                let wasm_module =
                                    GameStateWasmManager::load_module(&fs, file.clone()).await?;

                                Ok((file, wasm_module))
                            })
                            .get_storage()?
                    };
                    let (name, hash) = name_and_hash(game_mod, &file);
                    (
                        GameStateMod::Wasm { file: wasm_module },
                        GameModification::Wasm {
                            name: name.as_str().try_into()?,
                            hash,
                        },
                        file,
                        name,
                        Some(hash),
                    )
                }
            };
        let game = GameStateWasmManager::new(
            game_state_mod,
            map.map_file.clone(),
            map.name.clone(),
            GameStateCreateOptions {
                hint_max_characters: None, // TODO:
                config,
                account_db,
            },
            io,
            db.clone(),
        )?;
        let (map_name, map_hash) = name_and_hash(map.name.as_str(), &map.map_file);

        let fs_change_watcher = game_mod_blake3_hash.is_some().then(|| {
            io.fs.watch_for_change(
                STATE_MODS_PATH.as_ref(),
                Some(format!("{}.wasm", game_mod_name).as_ref()),
            )
        });

        if let Some(config) = game.info.config.clone() {
            let game_mod_name = game_mod_name.clone();
            let fs = io.fs.clone();
            io.rt.spawn_without_lifetime(async move {
                fs.create_dir("config".as_ref()).await?;
                fs.write_file(format!("config/{game_mod_name}.json").as_ref(), config)
                    .await?;
                Ok(())
            });
        }

        let render_mod_lower = render_mod.to_lowercase();
        let render_mod = if RESERVED_VANILLA_NAMES.contains(&render_mod_lower.as_str())
            || RESERVED_DDNET_NAMES.contains(&render_mod_lower.as_str())
        {
            RenderModification::Native
        } else if render_mod_required {
            RenderModification::RequiresWasm {
                name: render_mod.try_into()?,
                hash: *render_mod_hash,
            }
        } else {
            RenderModification::TryWasm {
                name: render_mod.try_into()?,
                hash: *render_mod_hash,
            }
        };

        Ok(Self {
            http_server: {
                Some(HttpDownloadServer::new(
                    vec![(
                        format!("map/maps/{}_{}.twmap", map_name, fmt_hash(&map_hash)),
                        map.map_file.clone(),
                    )]
                    .into_iter()
                    .chain(map.resource_files.clone().into_iter())
                    .chain(
                        game_mod_blake3_hash
                            .map(|game_mod_blake3_hash| {
                                (
                                    format!(
                                        "{}/{}_{}.wasm",
                                        STATE_MODS_PATH,
                                        game_mod_name,
                                        fmt_hash(&game_mod_blake3_hash)
                                    ),
                                    game_mod_file,
                                )
                            })
                            .into_iter(),
                    )
                    .collect(),
                    [(
                        "thumbnails".to_string(),
                        io.fs.get_save_path().join("thumbnails"),
                    )]
                    .into_iter()
                    .collect(),
                    download_server_port_v4,
                    download_server_port_v6,
                )?)
            },

            players: Default::default(),
            game,
            cur_monotonic_tick: 0,
            map,
            map_blake3_hash: map_hash,
            required_resources: required_resources.get_storage().ok().unwrap_or_default(),
            game_mod,
            render_mod,

            game_mod_fs_change_watcher: fs_change_watcher,

            // votes
            cur_vote: None,

            queued_inputs: Default::default(),

            spatial_world: spatial_chat.then(SpatialWorld::default),

            cached_character_infos: PoolFxLinkedHashMap::new_without_pool(),

            inps_pool: Pool::with_capacity(2),
        })
    }

    pub fn should_reload(&self) -> bool {
        self.game_mod_fs_change_watcher
            .as_ref()
            .map(|watcher| watcher.has_file_change())
            .unwrap_or_default()
    }

    pub fn player_join(
        &mut self,
        network_id: &NetworkConnectionId,
        player_info: &PlayerClientInfo,
    ) -> PlayerId {
        let player_id = self.game.player_join(player_info);
        self.players
            .insert(player_id, ServerPlayer::new(network_id, &player_id));
        player_id
    }

    pub fn player_drop(&mut self, player_id: &PlayerId, reason: PlayerDropReason) {
        self.players.remove(player_id);
        self.game.player_drop(player_id, reason);
    }

    pub fn player_inp(
        &mut self,
        player_id: &PlayerId,
        player_input: PlayerInput,
        for_monotonic_tick: GameTickType,
    ) {
        if let Some(player) = self.players.get_mut(player_id) {
            let cur_monotonic_tick = self.cur_monotonic_tick;

            // `<=` is intentional here. If the input is really for a previous tick,
            // then at least check whether the input is still newer than what already exists.
            if for_monotonic_tick <= cur_monotonic_tick + 1 {
                if let Some(diff) =
                    player
                        .inp
                        .try_overwrite(&player_input.inp, player_input.version(), false)
                {
                    let mut inps = self.inps_pool.new();
                    inps.insert(
                        *player_id,
                        CharacterInputInfo {
                            inp: player.inp.inp,
                            diff,
                        },
                    );
                    self.game.set_player_inputs(inps);
                }
            } else if for_monotonic_tick > cur_monotonic_tick + 1
                && (for_monotonic_tick - cur_monotonic_tick) < self.game.game_tick_speed().get() * 3
            {
                let inp = self
                    .queued_inputs
                    .entry(for_monotonic_tick)
                    .or_insert_with(Default::default);
                let entry = inp.entry(*player_id).or_insert_with(Default::default);
                entry.try_overwrite(&player_input.inp, player_input.version(), false);
            }
        }
    }

    pub fn set_player_emoticon(&mut self, player_id: &PlayerId, emoticon: EmoticonType) {
        self.game.set_player_emoticon(player_id, emoticon);
    }

    pub fn set_player_eye(&mut self, player_id: &PlayerId, eye: TeeEye, duration: Duration) {
        self.game.set_player_eye(player_id, eye, duration)
    }
}
