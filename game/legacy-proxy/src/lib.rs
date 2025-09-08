mod client;
pub mod projectile;
mod socket;

use anyhow::anyhow;
use base::{
    hash::{Hash, fmt_hash, generate_hash_for},
    join_thread::JoinThread,
    linked_hash_map_view::FxLinkedHashMap,
    network_string::{
        MtPoolNetworkString, NetworkReducedAsciiString, NetworkString, PoolNetworkString,
    },
    reduced_ascii_str::ReducedAsciiString,
};
use base_http::{http::HttpClient, http_server::HttpDownloadServer};
use base_io::io::Io;
use client::{ClientData, ClientState, ProxyClient, SocketClient, WarnPkt};
use game_base::{
    connecting_log::ConnectingLog,
    network::{
        messages::{
            AddLocalPlayerResponseError, GameModification, MsgClChatMsg, MsgClLoadVotes,
            MsgClReadyResponse, MsgClSnapshotAck, MsgSvAddLocalPlayerResponse, MsgSvChatMsg,
            MsgSvServerInfo, PlayerInputChainable, RenderModification,
        },
        types::chat::{ChatPlayerInfo, NetChatMsg, NetChatMsgPlayerChannel},
    },
};
use game_interface::{
    client_commands::{ClientCameraMode, JoinStage, MAX_TEAM_NAME_LEN},
    events::{
        self, EventIdGenerator, GameWorldNotificationEvent, GameWorldSystemMessage,
        GameWorldsEvents,
    },
    interface::GameStateServerOptions,
    types::{
        character_info::{MAX_ASSET_NAME_LEN, NetworkCharacterInfo, NetworkSkinInfo},
        emoticons::EmoticonType,
        fixed_zoom_level::FixedZoomLevel,
        flag::FlagType,
        game::{GameTickCooldownAndLastActionCounter, GameTickType},
        id_gen::IdGenerator,
        id_types::{CharacterId, CtfFlagId, LaserId, PickupId, PlayerId, ProjectileId, StageId},
        input::{
            CharacterInput, CharacterInputConsumableDiff, CharacterInputFlags,
            CharacterInputMethodFlags, CharacterInputState, InputVarState,
            cursor::CharacterInputCursor,
        },
        laser::LaserType,
        network_stats::PlayerNetworkStats,
        pickup::PickupType,
        player_info::PlayerUniqueId,
        render::{
            character::{CharacterBuff, CharacterDebuff, TeeEye},
            game::game_match::MatchSide,
            projectiles::WeaponWithProjectile,
        },
        resource_key::{NetworkResourceKey, ResourceKeyBase},
        snapshot::SnapshotLocalPlayer,
        weapons::WeaponType,
    },
    votes::{
        MAX_CATEGORY_NAME_LEN, MapVote, MapVoteDetails, MapVoteKey, MiscVote, MiscVoteCategoryKey,
        MiscVoteKey, VoteIdentifierType, VoteState, VoteType, Voted,
    },
};
use game_network::{
    game_event_generator::{GameEventGenerator, GameEvents},
    messages::{
        ClientToServerMessage, ClientToServerPlayerMessage, MsgSvInputAck, MsgSvLoadVotes,
        MsgSvResetVotes, ServerToClientMessage,
    },
};
use game_server::{
    client::{ClientSnapshotForDiff, ClientSnapshotStorage, ServerClientPlayer},
    server_game::{ServerGame, ServerMap},
};
use hexdump::hexdump_iter;
use legacy_map::datafile::ints_to_str;
use libtw2_gamenet_ddnet::{
    SnapObj,
    enums::{self, Emote, Team, VERSION},
    msg::{
        self, Connless, Game, System, SystemOrGame,
        connless::INFO_FLAG_PASSWORD,
        game::{self, SvTeamsState, SvTeamsStateLegacy},
        system,
    },
    snap_obj::{
        self, CHARACTERFLAG_WEAPON_GRENADE, CHARACTERFLAG_WEAPON_GUN, CHARACTERFLAG_WEAPON_HAMMER,
        CHARACTERFLAG_WEAPON_LASER, CHARACTERFLAG_WEAPON_SHOTGUN, Character, DdnetCharacter,
        DdnetPlayer, obj_size,
    },
};
use libtw2_net::net::PeerId;
use libtw2_packer::{IntUnpacker, Unpacker};
use log::{Level, debug, log_enabled, warn};
use map::{file::MapFileReader, map::Map};
use math::{
    colors::{legacy_color_to_rgba, rgba_to_legacy_color},
    math::{
        PI, Rng, normalize,
        vector::{dvec2, ubvec4, vec2},
    },
};
use network::network::{
    connection::NetworkConnectionId,
    errors::KickType,
    event::NetworkEvent,
    networks::Networks,
    notifier::NetworkEventNotifier,
    packet_compressor::DefaultNetworkPacketCompressor,
    plugins::{NetworkPluginConnection, NetworkPluginPacket, NetworkPlugins},
    quinn_network::QuinnNetworks,
    types::{
        NetworkInOrderChannel, NetworkServerCertAndKey, NetworkServerCertMode,
        NetworkServerCertModeResult, NetworkServerInitOptions,
    },
    utils::create_certifified_keys,
};
use pool::{
    datatypes::{PoolFxHashSet, PoolFxLinkedHashMap, PoolVec},
    mt_datatypes,
    pool::Pool,
    rc::PoolRc,
    traits::Recyclable,
};
use projectile::{get_pos, get_vel};
use rand::RngCore;
use sha2::Digest;
use socket::Socket;
use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    ffi::CStr,
    future::Future,
    net::SocketAddr,
    num::{NonZeroI64, NonZeroU16, NonZeroU64},
    pin::Pin,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
use tokio::sync::Notify;
use vanilla::{
    collision::collision::{Collision, Tunings},
    entities::{
        character::{
            character::{
                BuffProps, CharacterCore, CharacterReusableCore, PoolCharacterReusableCore,
            },
            core::character_core::{Core, CoreEvents, CoreJumps, CorePipe, CoreReusable},
            hook::character_hook::{Hook, HookState, HookedCharacters},
            player::player::{PlayerInfo as VanillaPlayerInfo, SpectatorPlayer},
            pos::character_pos::CharacterPositionPlayfield,
        },
        flag::flag::{FlagCore, PoolFlagReusableCore},
        laser::laser::{LaserCore, PoolLaserReusableCore},
        pickup::pickup::{PickupCore, PoolPickupReusableCore},
        projectile::projectile::{
            PoolProjectileReusableCore, ProjectileCore, ProjectileReusableCore,
        },
    },
    match_state::match_state::{Match, MatchState, MatchType, MatchWinner},
    simulation_pipe::simulation_pipe::SimulationPipeCharactersGetter,
    snapshot::snapshot::{
        Snapshot, SnapshotCharacter, SnapshotCharacterPhasedState, SnapshotCharacterPlayerTy,
        SnapshotCharacterSpectateMode, SnapshotCharacters, SnapshotFlag, SnapshotFlags,
        SnapshotInactiveObject, SnapshotLaser, SnapshotLasers, SnapshotMatchManager,
        SnapshotPickup, SnapshotPickups, SnapshotPool, SnapshotProjectile, SnapshotProjectiles,
        SnapshotSpectatorPlayer, SnapshotStage, SnapshotWorld,
    },
    weapons::definitions::weapon_def::Weapon,
};

enum LegacyInputFlags {
    // Playing = 1 << 0,
    InMenu = 1 << 1,
    Chatting = 1 << 2,
    Scoreboard = 1 << 3,
    Aim = 1 << 4,
    // SpecCam = 1 << 5,
}

#[derive(Debug)]
pub struct LegacyProxy {
    pub is_finished: Arc<AtomicBool>,
    pub addresses: Vec<SocketAddr>,
    pub cert: NetworkServerCertModeResult,

    pub notifier: Arc<Notify>,

    // purposely last element, for drop order
    pub thread: JoinThread<()>,
}

impl Drop for LegacyProxy {
    fn drop(&mut self) {
        self.notifier.notify_one();

        self.is_finished
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

const TICKS_PER_SECOND: u32 = 50;

fn hexdump(level: Level, data: &[u8]) {
    if log_enabled!(level) {
        hexdump_iter(data).for_each(|s| log::log!(level, "{s}"));
    }
}

#[derive(Debug, Default)]
struct Capabilities {
    pub is_ddnet: bool,
    pub allows_dummy: bool,
    pub chat_timeout_codes: bool,
}

#[derive(Debug, Default, Clone)]
struct ServerInfo {
    pub game_type: String,
    pub passworded: bool,
}

#[derive(Debug, Clone)]
pub struct ServerMapVotes {
    pub categories: BTreeMap<NetworkString<MAX_CATEGORY_NAME_LEN>, BTreeSet<String>>,
    pub has_unfinished_map_votes: bool,
}

struct LocalPlayer {
    pub client_id: u64,
    pub player_id: PlayerId,
    pub player_info: NetworkCharacterInfo,

    pub character_snap: Option<Character>,
    pub ddnet_character_snap: Option<DdnetCharacter>,
    pub ddnet_player_snap: Option<DdnetPlayer>,
}

#[derive(Debug, Clone)]
enum ServerInfoTy {
    Partial { requires_password: bool },
    Full(ServerInfo),
}

impl ServerInfoTy {
    pub fn requires_password(&self) -> bool {
        match self {
            Self::Partial { requires_password } => *requires_password,
            Self::Full(info) => info.passworded,
        }
    }
}

struct ClientBase {
    vanilla_snap_pool: SnapshotPool,
    stage_0_id: StageId,
    id_generator: IdGenerator,
    client_snap_storage: BTreeMap<u64, ClientSnapshotStorage>,
    snap_id: u64,
    cur_monotonic_tick: GameTickType,
    latest_client_snap: Option<ClientSnapshotForDiff>,
    ack_input_tick: i32,
    last_snap_tick: i32,

    own_teams: HashMap<PlayerId, (NetworkString<MAX_TEAM_NAME_LEN>, ubvec4, i32)>,

    emoticons: HashMap<i32, (Duration, enums::Emoticon)>,
    teams: HashMap<i32, (i32, StageId)>,

    char_legacy_to_new_id: HashMap<i32, CharacterId>,
    char_new_id_to_legacy: HashMap<CharacterId, i32>,
    /// Only the main connection is allowed to fill this
    /// based on player info snap objects
    confirmed_player_ids: HashSet<i32>,
    proj_legacy_to_new_id: HashMap<i32, ProjectileId>,
    laser_legacy_to_new_id: HashMap<i32, LaserId>,
    pickup_legacy_to_new_id: HashMap<i32, PickupId>,
    flag_legacy_to_new_id: HashMap<i32, CtfFlagId>,

    legacy_id_in_stage_id: HashMap<i32, StageId>,

    inputs_to_ack: BTreeMap<i32, (MsgSvInputAck, bool)>,

    events: events::GameEvents,
    event_id_generator: EventIdGenerator,

    capabilities: Capabilities,

    tunes: Tunings,

    server_info: ServerInfoTy,

    votes: ServerMapVotes,
    vote_state: Option<(VoteState, Duration)>,
    vote_list_updated: bool,
    loaded_map_votes: bool,
    loaded_misc_votes: bool,

    local_players: BTreeMap<i32, LocalPlayer>,

    join_password: String,

    is_first_map_pkt: bool,

    // ping calculations
    last_ping: Option<Duration>,
    last_ping_uuid: u128,
    last_pings: BTreeMap<u128, Duration>,
    last_pong: Option<Duration>,

    // helpers
    input_deser: Pool<Vec<u8>>,
    player_snap_pool: Pool<Vec<u8>>,
}

struct Client {
    base: ClientBase,

    connect_addr: SocketAddr,

    con_id: Option<NetworkConnectionId>,

    time: base::steady_clock::SteadyClock,
    tp: Arc<rayon::ThreadPool>,
    io: Io,
    http_server: Option<HttpDownloadServer>,
    collisions: Option<Box<Collision>>,

    server_has_new_events: Arc<AtomicBool>,
    server_event_handler: Arc<GameEventGenerator<ClientToServerMessage<'static>>>,
    players: PoolFxLinkedHashMap<PlayerId, ProxyClient>,

    server_network: QuinnNetworks,
    notifier_server: Arc<NetworkEventNotifier>,

    finish_notifier: Arc<Notify>,
    is_finished: Arc<AtomicBool>,

    log: ConnectingLog,

    last_snapshot: Snapshot,
}

impl Client {
    fn run(
        io: &Io,
        time: &base::steady_clock::SteadyClock,
        addr: SocketAddr,
        log: ConnectingLog,
    ) -> anyhow::Result<LegacyProxy> {
        let fs = io.fs.clone();

        log.log("Preparing proxy socket");
        let zstd_dicts = io.rt.spawn(async move {
            let client_send = fs.read_file("dict/client_send".as_ref()).await;
            let server_send = fs.read_file("dict/server_send".as_ref()).await;

            Ok(client_send.and_then(|c| server_send.map(|s| (c, s)))?)
        });
        let fs = io.fs.clone();

        let has_new_events_server = Arc::new(AtomicBool::new(false));
        let game_event_generator_server =
            Arc::new(GameEventGenerator::new(has_new_events_server.clone()));

        let connection_plugins: Vec<Arc<dyn NetworkPluginConnection>> = vec![];

        let mut packet_plugins: Vec<Arc<dyn NetworkPluginPacket>> = vec![];

        if let Ok((client_send, server_send)) = zstd_dicts.get() {
            packet_plugins.push(Arc::new(DefaultNetworkPacketCompressor::new_with_dict(
                server_send,
                client_send,
            )));
        } else {
            packet_plugins.push(Arc::new(DefaultNetworkPacketCompressor::new()));
        }

        let (cert, private_key) = create_certifified_keys();
        let (network_server, cert, sock_addrs, notifier_server) = Networks::init_server(
            "127.0.0.1".parse()?,
            "::1".parse()?,
            0,
            0,
            game_event_generator_server.clone(),
            NetworkServerCertMode::FromCertAndPrivateKey(Box::new(NetworkServerCertAndKey {
                cert,
                private_key,
            })),
            time,
            NetworkServerInitOptions::new()
                .with_max_thread_count(2)
                .with_disable_retry_on_connect(true)
                .with_packet_capacity_and_size(8, 256)
                //.with_ack_config(5, Duration::from_millis(50), 5 - 1)
                // since there are many packets, increase loss detection thresholds
                //.with_loss_detection_cfg(25, 2.0)
                .with_timeout(Duration::from_secs(20)),
            NetworkPlugins {
                packet_plugins: Arc::new(packet_plugins),
                connection_plugins: Arc::new(connection_plugins),
            },
        )?;

        let time = time.clone();

        let is_finished: Arc<AtomicBool> = Default::default();
        let is_finished_thread = is_finished.clone();

        let finish_notifier = Arc::new(Notify::default());
        let finish_notifier_thread = finish_notifier.clone();

        let thread = std::thread::Builder::new()
            .name("legacy-proxy".into())
            .spawn(move || {
                let io = Io::new(|_| fs, Arc::new(HttpClient::new()));
                let mut server_info = None;
                {
                    log.log(format!("Getting server info from: {addr}"));
                    // first get the server info
                    let mut conless = SocketClient::new(&io, addr).unwrap();
                    let mut is_connect = false;
                    let mut is_ready = false;
                    let mut is_map_ready = false;

                    let mut tokens = vec![rand::rng().next_u32() as u8];
                    conless.sendc(
                        addr,
                        Connless::RequestInfo(msg::connless::RequestInfo { token: tokens[0] }),
                    );
                    let start_time = time.now();
                    let mut last_req = start_time;
                    let mut last_reconnect = start_time;
                    while server_info.is_none()
                        && !is_finished_thread.load(std::sync::atomic::Ordering::SeqCst)
                    {
                        conless.run_once(|conless, event| match event {
                            libtw2_net::net::ChunkOrEvent::Chunk(libtw2_net::net::Chunk {
                                data,
                                pid,
                                ..
                            }) => {
                                let msg = match msg::decode(
                                    &mut WarnPkt(pid, data),
                                    &mut Unpacker::new(data),
                                ) {
                                    Ok(m) => m,
                                    Err(err) => {
                                        debug!("decode err during startup: {err:?}");
                                        return;
                                    }
                                };

                                if matches!(msg, SystemOrGame::System(System::MapChange(_))) {
                                    is_map_ready = true;
                                } else if matches!(msg, SystemOrGame::System(System::Reconnect(_)))
                                {
                                    log.log("Reconnecting");
                                    conless
                                        .net
                                        .disconnect(
                                            &mut conless.socket,
                                            conless.server_pid,
                                            b"reconnect",
                                        )
                                        .unwrap();
                                    let (pid, res) = conless.net.connect(&mut conless.socket, addr);
                                    res.unwrap();
                                    conless.server_pid = pid;
                                }
                            }
                            libtw2_net::net::ChunkOrEvent::Connless(msg) => {
                                log.log("Processing connless packet");
                                server_info = server_info
                                    .clone()
                                    .or(Self::on_connless_packet(&tokens, msg.addr, msg.data)
                                        .map(ServerInfoTy::Full));
                            }
                            libtw2_net::net::ChunkOrEvent::Connect(_) => {
                                log.log("Initial connecting established");
                                is_connect = true
                            }
                            libtw2_net::net::ChunkOrEvent::Ready(_) => {
                                log.log("Initial connecting ready");
                                is_ready = true;
                            }
                            libtw2_net::net::ChunkOrEvent::Disconnect(_, items) => {
                                let reason = String::from_utf8_lossy(items);
                                log.log(format!("Connection lost: {reason}"));
                                if reason.contains("password") {
                                    server_info =
                                        server_info.clone().or(Some(ServerInfoTy::Partial {
                                            requires_password: true,
                                        }));
                                }
                            }
                        });
                        if server_info.is_none()
                            && !is_finished_thread.load(std::sync::atomic::Ordering::SeqCst)
                        {
                            std::thread::sleep(Duration::from_millis(10));
                        }

                        let cur_time = time.now();
                        // send new request
                        if cur_time.saturating_sub(last_req) > Duration::from_secs(1) {
                            log.log("Sending new info request after 1s timeout");
                            let token = rand::rng().next_u32() as u8;
                            conless.sendc(
                                addr,
                                Connless::RequestInfo(msg::connless::RequestInfo { token }),
                            );

                            tokens.push(token);

                            last_req = cur_time;
                        }

                        // try to reconnect
                        if cur_time.saturating_sub(last_reconnect) > Duration::from_secs(3) {
                            log.log("Trying to reconnect after 3s timeout");
                            conless
                                .net
                                .disconnect(&mut conless.socket, conless.server_pid, b"reconnect")
                                .unwrap();
                            let (pid, res) = conless.net.connect(&mut conless.socket, addr);
                            res.unwrap();
                            conless.server_pid = pid;

                            last_reconnect = cur_time;
                        }

                        // send info, even if password is wrong and this results in a kick
                        if is_connect
                            && is_ready
                            && cur_time.saturating_sub(start_time) > Duration::from_secs(2)
                        {
                            log.log("Sending client info for initial connection after 2s timeout");
                            conless.sends(System::Info(system::Info {
                                version: VERSION.as_bytes(),
                                password: Some(b""),
                            }));
                            conless.flush();
                            is_ready = false;
                        }

                        // timeout
                        if cur_time.saturating_sub(start_time) > Duration::from_secs(20) {
                            log.log("Server was not responding after a total of 20 seconds");
                            debug!("giving up to connect after 20 seconds.");
                            return;
                        }
                    }
                }

                // then start proxy
                let id_generator: IdGenerator = Default::default();
                let vanilla_snap_pool = SnapshotPool::new(64, 64);

                let event_id_generator: EventIdGenerator = Default::default();

                let server_info = match server_info {
                    Some(i) => {
                        log.log("Got initial server info");
                        i
                    }
                    None => {
                        log::warn!("Got no server info at all, falling back to partial.");
                        ServerInfoTy::Partial {
                            requires_password: false,
                        }
                    }
                };

                let mut app = Client {
                    last_snapshot: Snapshot::new(
                        &vanilla_snap_pool,
                        id_generator.peek_next_id(),
                        None,
                        Default::default(),
                    ),

                    base: ClientBase {
                        vanilla_snap_pool,
                        stage_0_id: id_generator.next_id(),
                        id_generator,
                        client_snap_storage: Default::default(),
                        snap_id: 0,
                        cur_monotonic_tick: 0,
                        latest_client_snap: None,
                        player_snap_pool: Pool::with_capacity(2),
                        ack_input_tick: -1,
                        last_snap_tick: i32::MAX,
                        input_deser: Pool::with_capacity(8),
                        inputs_to_ack: Default::default(),

                        emoticons: Default::default(),
                        teams: Default::default(),
                        own_teams: Default::default(),

                        char_legacy_to_new_id: Default::default(),
                        char_new_id_to_legacy: Default::default(),
                        proj_legacy_to_new_id: Default::default(),
                        laser_legacy_to_new_id: Default::default(),
                        pickup_legacy_to_new_id: Default::default(),
                        flag_legacy_to_new_id: Default::default(),
                        confirmed_player_ids: Default::default(),

                        legacy_id_in_stage_id: Default::default(),

                        events: events::GameEvents {
                            event_id: event_id_generator.peek_next_id(),
                            worlds: GameWorldsEvents::new_without_pool(),
                        },
                        event_id_generator,

                        capabilities: Capabilities::default(),
                        tunes: Default::default(),

                        votes: ServerMapVotes {
                            categories: Default::default(),
                            has_unfinished_map_votes: false,
                        },
                        vote_state: None,
                        vote_list_updated: false,
                        loaded_map_votes: false,
                        loaded_misc_votes: false,

                        local_players: Default::default(),

                        is_first_map_pkt: true,

                        server_info,

                        join_password: Default::default(),

                        last_ping: None,
                        last_ping_uuid: Default::default(),
                        last_pings: Default::default(),
                        last_pong: None,
                    },

                    con_id: None,

                    connect_addr: addr,

                    time,
                    tp: Arc::new(
                        rayon::ThreadPoolBuilder::new()
                            .thread_name(|index| format!("legacy-proxy-{index}"))
                            .num_threads(2)
                            .build()
                            .unwrap(),
                    ),
                    io,
                    http_server: None,

                    collisions: None,

                    server_has_new_events: has_new_events_server,
                    server_event_handler: game_event_generator_server,
                    server_network: network_server,
                    notifier_server: Arc::new(notifier_server),
                    players: PoolFxLinkedHashMap::new_without_pool(),

                    finish_notifier: finish_notifier_thread,
                    is_finished: is_finished_thread,

                    log,
                };

                app.run_loop().unwrap();
            })
            .unwrap();
        Ok(LegacyProxy {
            thread: JoinThread::new(thread),
            addresses: sock_addrs,
            cert,
            notifier: finish_notifier,
            is_finished,
        })
    }
}

impl Client {
    fn player_info_mut<'a>(
        id: i32,
        base: &ClientBase,
        snapshot: &'a mut Snapshot,
    ) -> Option<(CharacterId, &'a mut VanillaPlayerInfo)> {
        base.legacy_id_in_stage_id
            .get(&id)
            .and_then(|stage_id| snapshot.stages.get_mut(stage_id))
            .and_then(|s| {
                base.char_legacy_to_new_id
                    .get(&id)
                    .and_then(|char_id| s.world.characters.get_mut(char_id).map(|c| (char_id, c)))
            })
            .map(|(id, p)| (*id, &mut p.player_info))
            .or_else(|| {
                base.char_legacy_to_new_id.get(&id).and_then(|char_id| {
                    snapshot
                        .spectator_players
                        .get_mut(char_id)
                        .map(|p| (*char_id, &mut p.player.player_info))
                })
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_snapshot(
        snapshot: &mut Snapshot,
        items: Vec<(SnapObj, i32)>,
        ddnet_characters: HashMap<i32, DdnetCharacter>,
        ddnet_players: HashMap<i32, DdnetPlayer>,
        tick: i32,
        base: &mut ClientBase,
        player_id: CharacterId,
        player: &mut ClientData,
        player_stage: StageId,
        collision: Option<&Collision>,
        cur_time: Duration,
    ) {
        let add_proj = |snapshot: &mut Snapshot,
                        id,
                        owner_id: i32,
                        pos: vec2,
                        type_: enums::Weapon,
                        start_tick: snap_obj::Tick,
                        vel: vec2| {
            let stage_id = base
                .teams
                .get(&owner_id)
                .map(|(_, id)| *id)
                .unwrap_or(player_stage);
            let stage = snapshot.stages.get_mut(&stage_id).unwrap();
            let proj_id = base.proj_legacy_to_new_id.get(&id).copied().unwrap();
            let start_tick = start_tick.0;
            let now = tick;
            let (curvature, speed) = match type_ {
                enums::Weapon::Hammer | enums::Weapon::Rifle | enums::Weapon::Ninja => (0.0, 0.0),
                enums::Weapon::Pistol => (base.tunes.gun_curvature, base.tunes.gun_speed),
                enums::Weapon::Shotgun => (base.tunes.shotgun_curvature, base.tunes.shotgun_speed),
                enums::Weapon::Grenade => (base.tunes.grenade_curvature, base.tunes.grenade_speed),
            };
            stage.world.projectiles.insert(
                proj_id,
                SnapshotProjectile {
                    core: ProjectileCore {
                        pos: get_pos(pos, vel, speed, curvature, now, start_tick),
                        vel: get_vel(now, start_tick, vel, speed, curvature),
                        life_span: 100,
                        damage: 0,
                        force: 0.0,
                        is_explosive: match type_ {
                            enums::Weapon::Hammer
                            | enums::Weapon::Pistol
                            | enums::Weapon::Shotgun
                            | enums::Weapon::Rifle
                            | enums::Weapon::Ninja => false,
                            enums::Weapon::Grenade => true,
                        },
                        ty: match type_ {
                            enums::Weapon::Hammer
                            | enums::Weapon::Ninja
                            | enums::Weapon::Rifle
                            | enums::Weapon::Pistol => WeaponWithProjectile::Gun,
                            enums::Weapon::Shotgun => WeaponWithProjectile::Shotgun,
                            enums::Weapon::Grenade => WeaponWithProjectile::Grenade,
                        },
                        side: None,
                    },
                    reusable_core: PoolProjectileReusableCore::from_without_pool(
                        ProjectileReusableCore {},
                    ),
                    game_el_id: proj_id,
                    owner_game_el_id: player_id,
                },
            );
        };
        let add_laser = |snapshot: &mut Snapshot, id, owner_id: i32, laser: snap_obj::Laser| {
            let stage_id = base
                .teams
                .get(&owner_id)
                .map(|(_, id)| *id)
                .unwrap_or(player_stage);
            let stage = snapshot.stages.get_mut(&stage_id).unwrap();
            let laser_id = base.laser_legacy_to_new_id.get(&id).copied().unwrap();
            let pos = vec2::new(laser.x as f32, laser.y as f32);
            let from = vec2::new(laser.from_x as f32, laser.from_y as f32);
            let next_eval_in = (TICKS_PER_SECOND as f32 * base.tunes.laser_bounce_delay / 1000.0)
                .ceil()
                - (tick - laser.start_tick.0) as f32;
            let next_eval_in = next_eval_in.clamp(0.0, f32::MAX) as u64;
            stage.world.lasers.insert(
                laser_id,
                SnapshotLaser {
                    core: LaserCore {
                        pos,
                        from,
                        dir: normalize(&(pos - from)),
                        bounces: 0,
                        can_hit_others: true,
                        can_hit_own: true,
                        energy: -1.0,
                        ty: LaserType::Rifle,
                        side: None,
                        next_eval_in: next_eval_in.into(),
                    },
                    reusable_core: PoolLaserReusableCore::new_without_pool(),
                    game_el_id: laser_id,
                    owner_game_el_id: player_id,
                },
            );
        };
        let add_pickup = |snapshot: &mut Snapshot, id, pickup: snap_obj::Pickup| {
            let stage = snapshot.stages.get_mut(&player_stage).unwrap();
            let pickup_id = base.pickup_legacy_to_new_id.get(&id).copied().unwrap();
            let pos = vec2::new(pickup.x as f32, pickup.y as f32);
            stage.world.pickups.insert(
                pickup_id,
                SnapshotPickup {
                    core: PickupCore {
                        pos,
                        ty: match pickup.type_ {
                            0 => PickupType::PowerupHealth,
                            1 => PickupType::PowerupArmor,
                            2 => PickupType::PowerupWeapon(match pickup.subtype {
                                0 => WeaponType::Hammer,
                                1 => WeaponType::Gun,
                                2 => WeaponType::Shotgun,
                                3 => WeaponType::Grenade,
                                _ => WeaponType::Laser,
                            }),
                            3 => PickupType::PowerupNinja,
                            // TODO: armor shields
                            _ => PickupType::PowerupArmor,
                        },
                    },
                    reusable_core: PoolPickupReusableCore::new_without_pool(),
                    game_el_id: pickup_id,
                },
            );
        };
        for (item, id) in items {
            match item {
                SnapObj::PlayerInput(inp) => {
                    debug!("[NOT IMPLEMENTED] player input: {inp:?}");
                }
                SnapObj::Projectile(projectile) => {
                    add_proj(
                        snapshot,
                        id,
                        -1,
                        vec2::new(projectile.x as f32, projectile.y as f32),
                        projectile.type_,
                        projectile.start_tick,
                        vec2::new(
                            projectile.vel_x as f32 / 100.0,
                            projectile.vel_y as f32 / 100.0,
                        ),
                    );
                }
                SnapObj::Laser(laser) => {
                    add_laser(snapshot, id, -1, laser);
                }
                SnapObj::Pickup(pickup) => {
                    add_pickup(snapshot, id, pickup);
                }
                SnapObj::Flag(flag) => {
                    let stage = snapshot.stages.get_mut(&player_stage).unwrap();
                    let flag_id = base.flag_legacy_to_new_id.get(&id).copied().unwrap();
                    let pos = vec2::new(flag.x as f32, flag.y as f32);
                    let ty = if flag.team == Team::Red as i32 {
                        FlagType::Red
                    } else {
                        FlagType::Blue
                    };
                    let flag = SnapshotFlag {
                        core: FlagCore {
                            pos,
                            spawn_pos: pos,
                            vel: Default::default(),
                            ty,
                            carrier: None,
                            drop_ticks: None,
                            non_linear_event: 0,
                        },
                        reusable_core: PoolFlagReusableCore::new_without_pool(),
                        game_el_id: flag_id,
                    };
                    match ty {
                        FlagType::Red => {
                            stage.world.red_flags.insert(flag_id, flag);
                        }
                        FlagType::Blue => {
                            stage.world.blue_flags.insert(flag_id, flag);
                        }
                    }
                }
                SnapObj::GameInfo(game_info) => {
                    let stage = snapshot.stages.get_mut(&player_stage).unwrap();
                    const GAMESTATEFLAG_GAMEOVER: i32 = 1 << 0;
                    const GAMESTATEFLAG_SUDDENDEATH: i32 = 1 << 1;
                    const GAMESTATEFLAG_PAUSED: i32 = 1 << 2;
                    const GAMESTATEFLAG_RACETIME: i32 = 1 << 3;
                    let round_start_tick =
                        if (game_info.game_state_flags & GAMESTATEFLAG_RACETIME) != 0 {
                            -game_info.warmup_timer
                        } else {
                            game_info.round_start_tick.0
                        };
                    let round_ticks_passed = tick.saturating_sub(round_start_tick);
                    let round_ticks_left = game_info
                        .time_limit
                        .unsigned_abs()
                        .checked_sub(round_ticks_passed.unsigned_abs())
                        .map(|t| t as u64)
                        .unwrap_or_default()
                        .into();
                    let is_game_over = (game_info.game_state_flags & GAMESTATEFLAG_GAMEOVER) != 0;
                    let is_sudden_death =
                        (game_info.game_state_flags & GAMESTATEFLAG_SUDDENDEATH) != 0;
                    let is_paused = (game_info.game_state_flags & GAMESTATEFLAG_PAUSED) != 0;
                    stage.match_manager.game_match.state = if is_game_over {
                        MatchState::GameOver {
                            // TODO: does ddnet expect to manually calc it?
                            winner: MatchWinner::Side(MatchSide::Red),
                            new_game_in: 1000.into(),
                            round_ticks_passed: round_ticks_passed as u64,
                            by_cooldown: false,
                        }
                    } else if is_sudden_death {
                        MatchState::SuddenDeath {
                            round_ticks_passed: round_ticks_passed as u64,
                            by_cooldown: false,
                        }
                    } else if is_paused {
                        MatchState::Paused {
                            round_ticks_passed: round_ticks_passed as u64,
                            round_ticks_left,
                        }
                    } else {
                        MatchState::Running {
                            round_ticks_passed: round_ticks_passed as u64,
                            round_ticks_left,
                        }
                    }
                }
                SnapObj::GameData(game_data) => {
                    let stage = snapshot.stages.get_mut(&player_stage).unwrap();
                    stage.match_manager.game_match.ty = MatchType::Sided {
                        scores: [
                            game_data.teamscore_red as i64,
                            game_data.teamscore_blue as i64,
                        ],
                    };
                    stage.world.red_flags.iter_mut().for_each(|f| {
                        f.1.core.carrier = base
                            .char_legacy_to_new_id
                            .get(&game_data.flag_carrier_red)
                            .copied();
                    });
                    stage.world.blue_flags.iter_mut().for_each(|f| {
                        f.1.core.carrier = base
                            .char_legacy_to_new_id
                            .get(&game_data.flag_carrier_blue)
                            .copied();
                    });
                }
                SnapObj::CharacterCore(character_core) => {
                    debug!("[NOT IMPLEMENTED] character core: {character_core:?}");
                }
                SnapObj::Character(character) => {
                    let stage_id = base
                        .teams
                        .get(&id)
                        .map(|(_, id)| *id)
                        .unwrap_or(player_stage);
                    let character_core = character.character_core;
                    let stage = snapshot.stages.get_mut(&stage_id).unwrap();
                    base.legacy_id_in_stage_id.insert(id, stage_id);

                    let char_id = *base.char_legacy_to_new_id.get(&id).unwrap();
                    let is_local = char_id == player_id;
                    let player_info = VanillaPlayerInfo {
                        player_info: <PoolRc<NetworkCharacterInfo>>::from_item_without_pool(
                            NetworkCharacterInfo::explicit_default(),
                        ),
                        version: 1,
                        unique_identifier: PlayerUniqueId::CertFingerprint(Default::default()),
                        account_name: None,
                        id: base
                            .local_players
                            .get(&id)
                            .map(|d| d.client_id)
                            .unwrap_or(player.server_client.id),
                    };

                    let ddnet_char = ddnet_characters.get(&id);

                    let mut buffs: FxLinkedHashMap<_, _> = Default::default();
                    let active_weapon = match character.weapon {
                        enums::WEAPON_HAMMER => WeaponType::Hammer,
                        enums::WEAPON_PISTOL => WeaponType::Gun,
                        enums::WEAPON_SHOTGUN => WeaponType::Shotgun,
                        enums::WEAPON_GRENADE => WeaponType::Grenade,
                        enums::WEAPON_RIFLE => WeaponType::Laser,
                        // Weapon ninja
                        _ => {
                            buffs.insert(
                                CharacterBuff::Ninja,
                                BuffProps {
                                    interact_cursor_dir: Default::default(),
                                    remaining_tick: 100.into(),
                                    interact_tick: 0.into(),
                                    interact_val: 0.0,
                                },
                            );
                            WeaponType::Hammer
                        }
                    };
                    let mut weapons: FxLinkedHashMap<WeaponType, Weapon> = Default::default();
                    if let Some(ddnet_char) = ddnet_char {
                        if (ddnet_char.flags & CHARACTERFLAG_WEAPON_HAMMER) != 0 {
                            weapons.insert(WeaponType::Hammer, Weapon::default());
                        }
                        if (ddnet_char.flags & CHARACTERFLAG_WEAPON_GUN) != 0 {
                            weapons.insert(WeaponType::Gun, Weapon::default());
                        }
                        if (ddnet_char.flags & CHARACTERFLAG_WEAPON_SHOTGUN) != 0 {
                            weapons.insert(WeaponType::Shotgun, Weapon::default());
                        }
                        if (ddnet_char.flags & CHARACTERFLAG_WEAPON_GRENADE) != 0 {
                            weapons.insert(WeaponType::Grenade, Weapon::default());
                        }
                        if (ddnet_char.flags & CHARACTERFLAG_WEAPON_LASER) != 0 {
                            weapons.insert(WeaponType::Laser, Weapon::default());
                        }
                    }
                    let active_weapon = if !weapons.contains_key(&active_weapon) {
                        weapons.insert(active_weapon, Default::default());
                        active_weapon
                    } else {
                        active_weapon
                    };
                    let mut hook = if character_core.hook_state == 0 {
                        Hook::None
                    } else if character_core.hook_state == -1 {
                        Hook::WaitsForRelease
                    } else {
                        Hook::Active {
                            hook_pos: math::math::vector::vec2::new(
                                character_core.hook_x as f32,
                                character_core.hook_y as f32,
                            ),
                            hook_dir: math::math::vector::vec2::new(
                                character_core.hook_dx as f32 / 256.0,
                                character_core.hook_dy as f32 / 256.0,
                            ),
                            hook_tele_base: Default::default(),
                            hook_tick: character_core.hook_tick,
                            hook_state: match character_core.hook_state {
                                1 => HookState::RetractStart,
                                2 => HookState::RetractMid,
                                3 => HookState::RetractEnd,
                                4 => HookState::HookFlying,
                                _ => HookState::HookGrabbed,
                            },
                        }
                    };
                    let hooked_char = if matches!(
                        hook,
                        Hook::Active {
                            hook_state: HookState::HookGrabbed,
                            ..
                        }
                    ) {
                        base.char_legacy_to_new_id
                            .get(&character_core.hooked_player)
                            .copied()
                    } else {
                        None
                    };
                    let mut pos = math::math::vector::vec2::new(
                        character_core.x as f32,
                        character_core.y as f32,
                    );
                    let mut inp = CharacterInput {
                        cursor: {
                            let mut cursor: InputVarState<CharacterInputCursor> =
                                InputVarState::default();
                            cursor.set(CharacterInputCursor::from_vec2(
                                &(math::math::vector::vec2_base::new(
                                    (character_core.angle as f64 / 256.0).cos(),
                                    (character_core.angle as f64 / 256.0).sin(),
                                ) * 10.0),
                            ));
                            cursor
                        },
                        dyn_cam_offset: Default::default(),
                        state: {
                            let mut state = CharacterInputState::default();
                            state.dir.set(character_core.direction);

                            // assume player is hooking
                            if !matches!(hook, Hook::None) {
                                state.hook.set(true);
                            }

                            let mut flags = CharacterInputFlags::empty();
                            if (character.player_flags & LegacyInputFlags::Aim as i32) != 0 {
                                flags.insert(CharacterInputFlags::HOOK_COLLISION_LINE);
                            }
                            if (character.player_flags & LegacyInputFlags::Chatting as i32) != 0 {
                                flags.insert(CharacterInputFlags::CHATTING);
                            }
                            if (character.player_flags & LegacyInputFlags::Scoreboard as i32) != 0 {
                                flags.insert(CharacterInputFlags::SCOREBOARD);
                            }
                            if (character.player_flags & LegacyInputFlags::InMenu as i32) != 0 {
                                flags.insert(CharacterInputFlags::MENU_UI);
                            }
                            state.flags.set(flags);

                            state
                        },
                        consumable: Default::default(),

                        viewport: Default::default(),
                    };
                    let mut core = Core {
                        vel: math::math::vector::vec2::new(
                            character_core.vel_x as f32 / 256.0,
                            character_core.vel_y as f32 / 256.0,
                        ),
                        jumps: CoreJumps {
                            flag: character_core.jumped,
                            ..Default::default()
                        },
                        direction: character_core.direction,
                        ..Default::default()
                    };
                    if let Some(ddnet_char) = ddnet_char {
                        core.jumps.max = ddnet_char.jumps;
                        core.jumps.count = ddnet_char.jumped_total;
                        inp.cursor.set(CharacterInputCursor::from_vec2(&dvec2::new(
                            ddnet_char.target_x as f64 / 32.0,
                            ddnet_char.target_y as f64 / 32.0,
                        )));
                    }
                    let mut reusable_core =
                        PoolCharacterReusableCore::from_without_pool(CharacterReusableCore {
                            weapons,
                            core: CoreReusable::new(),
                            buffs,
                            debuffs: Default::default(),
                            interactions: Default::default(),
                            queued_emoticon: Default::default(),
                        });
                    if let Some(ddnet_char) = ddnet_char
                        && ddnet_char.freeze_start.0 != 0
                    {
                        let remaining = ddnet_char.freeze_end.0.saturating_sub(tick);
                        reusable_core.debuffs.insert(
                            CharacterDebuff::Freeze,
                            BuffProps {
                                remaining_tick: (remaining.unsigned_abs() as u64).into(),
                                interact_tick: Default::default(),
                                interact_cursor_dir: Default::default(),
                                interact_val: 0.0,
                            },
                        );
                    }
                    if let Some(collision) = collision {
                        let mut char_tick = character_core.tick;
                        let field = CharacterPositionPlayfield::new(
                            NonZeroU16::new(collision.get_playfield_width() as u16).unwrap(),
                            NonZeroU16::new(collision.get_playfield_height() as u16).unwrap(),
                        );
                        let mut fake_pos = field.get_character_pos(pos, char_id);
                        let hooks = HookedCharacters::default();
                        let mut fake_hook = hooks.get_new_hook(char_id);
                        fake_hook.set(hook, hooked_char);
                        struct FakeCharacters;
                        impl SimulationPipeCharactersGetter for FakeCharacters {
                            fn for_other_characters_in_range_mut(
                                &mut self,
                                _char_pos: &vec2,
                                _radius: f32,
                                _for_each_func: &mut dyn FnMut(
                                    &mut vanilla::entities::character::character::Character,
                                ),
                            ) {
                            }

                            fn get_other_character_id_and_cores_iter_by_ids_mut(
                                &mut self,
                                _ids: &[CharacterId],
                                _for_each_func: &mut dyn FnMut(
                                            &CharacterId,
                                            &mut Core,
                                            &mut CoreReusable,
                                            &mut vanilla::entities::character::pos::character_pos::CharacterPos,
                                        ) -> std::ops::ControlFlow<()>,
                            ) -> std::ops::ControlFlow<()> {
                                std::ops::ControlFlow::Continue(())
                            }

                            fn get_other_character_pos_by_id(
                                &self,
                                _other_char_id: &CharacterId,
                            ) -> &vec2 {
                                unreachable!()
                            }

                            fn get_other_character_by_id_mut(
                                &mut self,
                                _other_char_id: &CharacterId,
                            ) -> &mut vanilla::entities::character::character::Character
                            {
                                unreachable!()
                            }
                        }
                        let mut inp = None;

                        if char_tick <= 0 {
                            char_tick = tick;
                        }
                        while char_tick < tick {
                            inp = (char_id == player_id)
                                .then(|| {
                                    player
                                        .latest_inputs
                                        .get(&char_tick)
                                        .map(|(inp, _)| inp)
                                        .copied()
                                })
                                .flatten()
                                .or(inp);
                            let use_inp = inp.is_some()
                                && !reusable_core.debuffs.contains_key(&CharacterDebuff::Freeze);
                            let inp = inp.unwrap_or_default();
                            core.physics_tick(
                                &mut fake_pos,
                                &mut fake_hook,
                                use_inp,
                                true,
                                &mut CorePipe {
                                    characters: &mut FakeCharacters,
                                    input: &inp,
                                },
                                collision,
                                CoreEvents {
                                    game_pending_events: &Default::default(),
                                    character_id: &char_id,
                                },
                            );
                            core.physics_move(
                                &mut fake_pos,
                                &mut CorePipe {
                                    characters: &mut FakeCharacters,
                                    input: &inp,
                                },
                                collision,
                            );
                            core.physics_quantize(&mut fake_pos, &mut fake_hook);
                            char_tick += 1;
                        }

                        pos = *fake_pos.pos();
                        hook = fake_hook.get().0;
                    }
                    let attack_recoil = if character.attack_tick > 0 {
                        let recoil_ticks_passed = tick.saturating_sub(character.attack_tick);
                        let initial_cooldown = match active_weapon {
                            WeaponType::Hammer => TICKS_PER_SECOND / 3,
                            WeaponType::Gun => TICKS_PER_SECOND / 8,
                            WeaponType::Shotgun => TICKS_PER_SECOND / 2,
                            WeaponType::Grenade => TICKS_PER_SECOND / 2,
                            WeaponType::Laser => (TICKS_PER_SECOND * 800) / 1000,
                        } as i32;
                        let cooldown_len =
                            NonZeroU64::new(initial_cooldown.unsigned_abs() as u64).unwrap();
                        if recoil_ticks_passed >= initial_cooldown {
                            GameTickCooldownAndLastActionCounter::LastActionCounter {
                                ticks_passed: recoil_ticks_passed as u64,
                                last_cooldown_len: cooldown_len,
                            }
                        } else {
                            GameTickCooldownAndLastActionCounter::Cooldown {
                                ticks_left: NonZeroU64::new(
                                    initial_cooldown
                                        .saturating_sub(recoil_ticks_passed)
                                        .unsigned_abs() as u64,
                                )
                                .unwrap(),
                                ticks_passed: recoil_ticks_passed as u64,
                                initial_cooldown_len: cooldown_len,
                            }
                        }
                    } else {
                        Default::default()
                    };
                    let (cur_emoticon, emoticon_tick) = if let Some((start_time, emoticon)) =
                        base.emoticons.get(&id)
                    {
                        let time_passed = cur_time.saturating_sub(*start_time);
                        let remaining_time = Duration::from_secs(2).saturating_sub(time_passed);
                        let millis_per_tick = 1000 / TICKS_PER_SECOND as u64;
                        let ticks_passed = time_passed.as_millis() as u64 / millis_per_tick;
                        let ticks_remaining = remaining_time.as_millis() as u64 / millis_per_tick;
                        (
                            Some(match emoticon {
                                enums::Emoticon::Oop => EmoticonType::OOP,
                                enums::Emoticon::Exclamation => EmoticonType::EXCLAMATION,
                                enums::Emoticon::Hearts => EmoticonType::HEARTS,
                                enums::Emoticon::Drop => EmoticonType::DROP,
                                enums::Emoticon::Dotdot => EmoticonType::DOTDOT,
                                enums::Emoticon::Music => EmoticonType::MUSIC,
                                enums::Emoticon::Sorry => EmoticonType::SORRY,
                                enums::Emoticon::Ghost => EmoticonType::GHOST,
                                enums::Emoticon::Sushi => EmoticonType::SUSHI,
                                enums::Emoticon::Splattee => EmoticonType::SPLATTEE,
                                enums::Emoticon::Deviltee => EmoticonType::DEVILTEE,
                                enums::Emoticon::Zomg => EmoticonType::ZOMG,
                                enums::Emoticon::Zzz => EmoticonType::ZZZ,
                                enums::Emoticon::Wtf => EmoticonType::WTF,
                                enums::Emoticon::Eyes => EmoticonType::EYES,
                                enums::Emoticon::Question => EmoticonType::QUESTION,
                            }),
                            GameTickCooldownAndLastActionCounter::Cooldown {
                                ticks_left: NonZeroU64::new(ticks_remaining.max(1)).unwrap(),
                                ticks_passed,
                                initial_cooldown_len: NonZeroU64::new(TICKS_PER_SECOND as u64 * 2)
                                    .unwrap(),
                            },
                        )
                    } else {
                        (None, Default::default())
                    };
                    let core = CharacterCore {
                        core,
                        active_weapon,
                        eye: match character.emote {
                            Emote::Normal => TeeEye::Normal,
                            Emote::Pain => TeeEye::Pain,
                            Emote::Happy => TeeEye::Happy,
                            Emote::Surprise => TeeEye::Surprised,
                            Emote::Angry => TeeEye::Angry,
                            Emote::Blink => TeeEye::Blink,
                        },
                        input: if let Some((inp, _)) = (char_id == player_id)
                            .then(|| player.latest_inputs.get(&tick))
                            .flatten()
                        {
                            *inp
                        } else {
                            inp
                        },
                        health: if is_local {
                            character.health.unsigned_abs()
                        } else {
                            10
                        },
                        armor: if is_local {
                            character.armor.unsigned_abs()
                        } else {
                            0
                        },
                        attack_recoil,
                        cur_emoticon,
                        emoticon_tick,
                        ..Default::default()
                    };
                    stage.world.characters.insert(
                        char_id,
                        SnapshotCharacter {
                            core,
                            reusable_core,
                            player_info,
                            ty: SnapshotCharacterPlayerTy::Player(PlayerNetworkStats {
                                packet_loss: 0.0,
                                ping: Duration::ZERO,
                            }),
                            pos,
                            phased: SnapshotCharacterPhasedState::Normal {
                                hook: (hook, hooked_char),
                                ingame_spectate: None,
                            },
                            score: 0,
                            game_el_id: char_id,
                        },
                    );
                }
                SnapObj::PlayerInfo(player_info) => {
                    enum PlayerFlagEx {
                        // Afk = 1 << 0,
                        Paused = 1 << 1,
                        Spec = 1 << 2,
                    }
                    let (is_pause, is_spec) = ddnet_players
                        .get(&player_info.client_id)
                        .map(|p| {
                            (
                                (p.flags & PlayerFlagEx::Paused as i32) != 0,
                                (p.flags & PlayerFlagEx::Spec as i32) != 0,
                            )
                        })
                        .unwrap_or_default();

                    let is_spec_or_pause = is_spec || is_pause;
                    let is_spectator = player_info.team == Team::Spectators;
                    if is_spectator && !is_spec_or_pause {
                        let player_id = base
                            .char_legacy_to_new_id
                            .get(&player_info.client_id)
                            .copied()
                            .unwrap();
                        let vanilla_player_info = VanillaPlayerInfo {
                            player_info: <PoolRc<NetworkCharacterInfo>>::from_item_without_pool(
                                NetworkCharacterInfo::explicit_default(),
                            ),
                            version: 1,
                            unique_identifier: PlayerUniqueId::CertFingerprint(Default::default()),
                            account_name: None,
                            id: base
                                .local_players
                                .get(&id)
                                .map(|d| d.client_id)
                                .unwrap_or(player.server_client.id),
                        };
                        snapshot.spectator_players.insert(
                            player_id,
                            SnapshotSpectatorPlayer {
                                player: SpectatorPlayer {
                                    player_info: vanilla_player_info,
                                    player_input: Default::default(),
                                    id: player_id,
                                    spectated_characters: PoolFxHashSet::new_without_pool(),
                                    default_eye: TeeEye::Normal,
                                    default_eye_reset_in: Default::default(),
                                    network_stats: PlayerNetworkStats {
                                        ping: Duration::from_millis(
                                            player_info.latency.unsigned_abs() as u64,
                                        ),
                                        packet_loss: 0.0,
                                    },
                                },
                            },
                        );
                    } else {
                        let stage_id =
                            base.legacy_id_in_stage_id
                                .get(&id)
                                .copied()
                                .unwrap_or_else(|| {
                                    base.teams
                                        .get(&id)
                                        .map(|(_, id)| *id)
                                        .unwrap_or(player_stage)
                                });
                        let stage = snapshot.stages.get_mut(&stage_id).unwrap();
                        let char_id = base
                            .char_legacy_to_new_id
                            .get(&player_info.client_id)
                            .unwrap();
                        if let Some(character) = stage.world.characters.get_mut(char_id) {
                            if let SnapshotCharacterPlayerTy::Player(ty) = &mut character.ty {
                                ty.ping = Duration::from_millis(
                                    player_info.latency.unsigned_abs() as u64
                                );
                            }
                            character.score = player_info.score as i64;
                            let mode = SnapshotCharacterSpectateMode::Free(Default::default());
                            match &mut character.phased {
                                SnapshotCharacterPhasedState::Normal {
                                    ingame_spectate, ..
                                } => {
                                    if is_pause {
                                        *ingame_spectate = Some(mode);
                                    } else if is_spec {
                                        character.phased =
                                            SnapshotCharacterPhasedState::PhasedSpectate(mode);
                                    } else {
                                        *ingame_spectate = None;
                                    }
                                }
                                SnapshotCharacterPhasedState::Dead { .. } => {}
                                SnapshotCharacterPhasedState::PhasedSpectate(spec_mode) => {
                                    if is_pause {
                                        character.phased = SnapshotCharacterPhasedState::Normal {
                                            hook: Default::default(),
                                            ingame_spectate: Some(mode),
                                        };
                                    } else {
                                        *spec_mode = mode;
                                    }
                                }
                            }
                        } else {
                            let char_player_info = VanillaPlayerInfo {
                                player_info: <PoolRc<NetworkCharacterInfo>>::from_item_without_pool(
                                    NetworkCharacterInfo::explicit_default(),
                                ),
                                version: 1,
                                unique_identifier: PlayerUniqueId::CertFingerprint(
                                    Default::default(),
                                ),
                                account_name: None,
                                id: base
                                    .local_players
                                    .get(&id)
                                    .map(|d| d.client_id)
                                    .unwrap_or(player.server_client.id),
                            };
                            let ty = SnapshotCharacterPlayerTy::Player(PlayerNetworkStats {
                                ping: Duration::from_millis(
                                    player_info.latency.unsigned_abs() as u64
                                ),
                                packet_loss: 0.0,
                            });
                            stage.world.characters.insert(
                                *char_id,
                                SnapshotCharacter {
                                    core: CharacterCore {
                                        active_weapon: WeaponType::Hammer,
                                        ..Default::default()
                                    },
                                    reusable_core: {
                                        let mut core =
                                            PoolCharacterReusableCore::new_without_pool();
                                        core.weapons.insert(WeaponType::Hammer, Weapon::default());
                                        core
                                    },
                                    player_info: char_player_info,
                                    ty,
                                    pos: Default::default(),
                                    phased: if is_spec_or_pause {
                                        SnapshotCharacterPhasedState::PhasedSpectate(
                                            SnapshotCharacterSpectateMode::Free(Default::default()),
                                        )
                                    } else {
                                        SnapshotCharacterPhasedState::Dead {
                                            respawn_in_ticks: 10000.into(),
                                        }
                                    },
                                    score: player_info.score as i64,
                                    game_el_id: *char_id,
                                },
                            );
                            base.legacy_id_in_stage_id.insert(id, stage_id);
                        }
                    }

                    if let Some((client_id, local_char_id)) = base
                        .local_players
                        .get(&id)
                        .map(|d| (d.client_id, d.player_id))
                        .or((player_info.local == 1)
                            .then_some((player.server_client.id, player_id)))
                    {
                        snapshot
                            .local_players
                            .insert(local_char_id, SnapshotLocalPlayer { id: client_id });
                    }
                }
                SnapObj::ClientInfo(client_info) => {
                    if let Some((character_id, info)) = Self::player_info_mut(id, base, snapshot) {
                        fn ints_to_net_str<const MAX_LENGTH: usize>(
                            int_arr: &[i32],
                        ) -> NetworkString<MAX_LENGTH> {
                            let mut name: [u8; 32] = Default::default();
                            ints_to_str(int_arr, &mut name);
                            let name = CStr::from_bytes_until_nul(&name);
                            name.ok()
                                .and_then(|n| n.to_str().ok())
                                .map(NetworkString::new_lossy)
                                .unwrap_or_default()
                        }
                        let mut player_info = (*info.player_info).clone();

                        // Apply as much info from known player info as possible
                        if character_id == player_id {
                            player_info = player.player_info.clone();
                        } else if let Some(dummy) = base.local_players.get(&id) {
                            player_info = dummy.player_info.clone();
                        }

                        // Then overwrite the info the server knows about
                        player_info.name = ints_to_net_str(client_info.name.as_slice());
                        player_info.clan = ints_to_net_str(client_info.clan.as_slice());
                        player_info.skin = NetworkResourceKey::from_str_lossy(
                            ints_to_net_str::<MAX_ASSET_NAME_LEN>(client_info.skin.as_slice())
                                .as_str(),
                        );
                        player_info.skin_info = if client_info.use_custom_color == 1 {
                            let body_color =
                                legacy_color_to_rgba(client_info.color_body, true, true);
                            let feet_color =
                                legacy_color_to_rgba(client_info.color_feet, true, true);
                            NetworkSkinInfo::Custom {
                                body_color,
                                feet_color,
                            }
                        } else {
                            NetworkSkinInfo::Original
                        };
                        info.player_info = PoolRc::from_item_without_pool(player_info);
                    }
                }
                SnapObj::SpectatorInfo(spectator_info) => {
                    let spectator_id = base.char_legacy_to_new_id.get(&spectator_info.spectator_id);
                    let own_char_id = base.char_legacy_to_new_id.get(&id);
                    if let Some(own_character) = own_char_id.and_then(|char_id| {
                        base.legacy_id_in_stage_id.get(&id).and_then(|stage_id| {
                            snapshot
                                .stages
                                .get_mut(stage_id)
                                .and_then(|s| s.world.characters.get_mut(char_id))
                        })
                    }) {
                        let mode = if let Some(spectator_id) = spectator_id {
                            SnapshotCharacterSpectateMode::Follows {
                                ids: PoolFxHashSet::from_without_pool(
                                    vec![*spectator_id].into_iter().collect(),
                                ),
                                locked_zoom: false,
                            }
                        } else {
                            SnapshotCharacterSpectateMode::Free(vec2::new(
                                spectator_info.x as f32,
                                spectator_info.y as f32,
                            ))
                        };
                        match &mut own_character.phased {
                            SnapshotCharacterPhasedState::Normal {
                                ingame_spectate, ..
                            } => {
                                *ingame_spectate = Some(mode);
                            }
                            SnapshotCharacterPhasedState::Dead { .. } => {}
                            SnapshotCharacterPhasedState::PhasedSpectate(spec_mode) => {
                                *spec_mode = mode;
                            }
                        }
                    } else if let Some(own_character) =
                        own_char_id.and_then(|id| snapshot.spectator_players.get_mut(id))
                    {
                        own_character.player.spectated_characters.clear();
                        if let Some(spectator_id) = spectator_id {
                            own_character
                                .player
                                .spectated_characters
                                .extend([*spectator_id]);
                        }
                    }
                }
                SnapObj::MyOwnObject(my_own_object) => {
                    debug!("[NOT IMPLEMENTED] my own object: {my_own_object:?}");
                }
                SnapObj::DdnetCharacter(_) => {
                    panic!("This snap item is purposely removed earlier");
                }
                SnapObj::DdnetPlayer(_) => {
                    panic!("This snap item is purposely removed earlier");
                }
                SnapObj::GameInfoEx(game_info_ex) => {
                    debug!("[NOT IMPLEMENTED] game info ex: {game_info_ex:?}");
                }
                SnapObj::DdraceProjectile(ddrace_projectile) => {
                    debug!("[NOT IMPLEMENTED] ddrace projectile: {ddrace_projectile:?}");
                }
                SnapObj::DdnetLaser(laser) => {
                    add_laser(
                        snapshot,
                        id,
                        laser.owner,
                        snap_obj::Laser {
                            x: laser.to_x,
                            y: laser.to_y,
                            from_x: laser.from_x,
                            from_y: laser.from_y,
                            start_tick: laser.start_tick,
                        },
                    );
                }
                SnapObj::DdnetProjectile(projectile) => {
                    let vel = if projectile.owner < 0 {
                        vec2::new(
                            projectile.vel_x as f32 / 100000.0,
                            projectile.vel_y as f32 / 100000.0,
                        )
                    } else {
                        const PROJECTILEFLAG_NORMALIZE_VEL: i32 = 1 << 4;
                        if (projectile.flags & PROJECTILEFLAG_NORMALIZE_VEL) != 0 {
                            normalize(&vec2::new(projectile.vel_x as f32, projectile.vel_y as f32))
                        } else {
                            vec2::new(
                                projectile.vel_x as f32 / 100.0,
                                projectile.vel_y as f32 / 100.0,
                            )
                        }
                    };
                    add_proj(
                        snapshot,
                        id,
                        projectile.owner,
                        vec2::new(projectile.x as f32 / 100.0, projectile.y as f32 / 100.0),
                        projectile.type_,
                        projectile.start_tick,
                        vel,
                    );
                }
                SnapObj::DdnetPickup(ddnet_pickup) => {
                    add_pickup(
                        snapshot,
                        id,
                        snap_obj::Pickup {
                            x: ddnet_pickup.x,
                            y: ddnet_pickup.y,
                            type_: ddnet_pickup.type_,
                            subtype: ddnet_pickup.subtype,
                        },
                    );
                }
                SnapObj::Common(common) => {
                    debug!("[NOT IMPLEMENTED] common: {common:?}");
                }
                SnapObj::Explosion(explosion) => {
                    let events = base
                        .events
                        .worlds
                        .entry(player_stage)
                        .or_insert_with_keep_order(|| events::GameWorldEvents {
                            events: mt_datatypes::PoolFxLinkedHashMap::new_without_pool(),
                        });
                    events.events.insert(
                        base.event_id_generator.next_id(),
                        events::GameWorldEvent::Effect(events::GameWorldEffectEvent {
                            pos: vec2::new(explosion.common.x as f32, explosion.common.y as f32)
                                / 32.0,
                            owner_id: Some(player_id),
                            ev: events::GameWorldEntityEffectEvent::Grenade(
                                events::GameGrenadeEventEffect::Explosion,
                            ),
                        }),
                    );
                }
                SnapObj::Spawn(spawn) => {
                    let events = base
                        .events
                        .worlds
                        .entry(player_stage)
                        .or_insert_with_keep_order(|| events::GameWorldEvents {
                            events: mt_datatypes::PoolFxLinkedHashMap::new_without_pool(),
                        });
                    events.events.insert(
                        base.event_id_generator.next_id(),
                        events::GameWorldEvent::Effect(events::GameWorldEffectEvent {
                            pos: vec2::new(spawn.common.x as f32, spawn.common.y as f32) / 32.0,
                            owner_id: Some(player_id),
                            ev: events::GameWorldEntityEffectEvent::Character(
                                events::GameCharacterEffectEvent::Effect(
                                    events::GameCharacterEventEffect::Spawn,
                                ),
                            ),
                        }),
                    );
                }
                SnapObj::HammerHit(hammer_hit) => {
                    let events = base
                        .events
                        .worlds
                        .entry(player_stage)
                        .or_insert_with_keep_order(|| events::GameWorldEvents {
                            events: mt_datatypes::PoolFxLinkedHashMap::new_without_pool(),
                        });
                    events.events.insert(
                        base.event_id_generator.next_id(),
                        events::GameWorldEvent::Effect(events::GameWorldEffectEvent {
                            pos: vec2::new(hammer_hit.common.x as f32, hammer_hit.common.y as f32)
                                / 32.0,
                            owner_id: Some(player_id),
                            ev: events::GameWorldEntityEffectEvent::Character(
                                events::GameCharacterEffectEvent::Effect(
                                    events::GameCharacterEventEffect::HammerHit,
                                ),
                            ),
                        }),
                    );
                }
                SnapObj::Death(death) => {
                    let events = base
                        .events
                        .worlds
                        .entry(player_stage)
                        .or_insert_with_keep_order(|| events::GameWorldEvents {
                            events: mt_datatypes::PoolFxLinkedHashMap::new_without_pool(),
                        });
                    events.events.insert(
                        base.event_id_generator.next_id(),
                        events::GameWorldEvent::Effect(events::GameWorldEffectEvent {
                            pos: vec2::new(death.common.x as f32, death.common.y as f32) / 32.0,
                            owner_id: Some(player_id),
                            ev: events::GameWorldEntityEffectEvent::Character(
                                events::GameCharacterEffectEvent::Effect(
                                    events::GameCharacterEventEffect::Death,
                                ),
                            ),
                        }),
                    );
                }
                SnapObj::SoundGlobal(snap_obj::SoundGlobal { common, sound_id })
                | SnapObj::SoundWorld(snap_obj::SoundWorld { common, sound_id }) => {
                    let events = base
                        .events
                        .worlds
                        .entry(player_stage)
                        .or_insert_with_keep_order(|| events::GameWorldEvents {
                            events: mt_datatypes::PoolFxLinkedHashMap::new_without_pool(),
                        });
                    events.events.insert(
                        base.event_id_generator.next_id(),
                        events::GameWorldEvent::Sound(events::GameWorldSoundEvent {
                            pos: Some(vec2::new(common.x as f32, common.y as f32) / 32.0),
                            owner_id: Some(player_id),
                            ev: match sound_id {
                                enums::Sound::GunFire => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::GunFire,
                                        ),
                                    )
                                }
                                enums::Sound::ShotgunFire => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::ShotgunFire,
                                        ),
                                    )
                                }
                                enums::Sound::GrenadeFire => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::GrenadeFire,
                                        ),
                                    )
                                }
                                enums::Sound::HammerFire => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::HammerFire,
                                        ),
                                    )
                                }
                                enums::Sound::HammerHit => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::HammerHit,
                                        ),
                                    )
                                }
                                enums::Sound::NinjaFire => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Buff(
                                            events::GameBuffSoundEvent::Ninja(
                                                events::GameBuffNinjaEventSound::Attack,
                                            ),
                                        ),
                                    )
                                }
                                enums::Sound::GrenadeExplode => {
                                    events::GameWorldEntitySoundEvent::Grenade(
                                        events::GameGrenadeEventSound::Explosion,
                                    )
                                }
                                enums::Sound::NinjaHit => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Buff(
                                            events::GameBuffSoundEvent::Ninja(
                                                events::GameBuffNinjaEventSound::Hit,
                                            ),
                                        ),
                                    )
                                }
                                enums::Sound::RifleFire => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::LaserFire,
                                        ),
                                    )
                                }
                                enums::Sound::RifleBounce => {
                                    events::GameWorldEntitySoundEvent::Laser(
                                        events::GameLaserEventSound::Bounce,
                                    )
                                }
                                enums::Sound::WeaponSwitch => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::WeaponSwitch {
                                                // TODO:
                                                new_weapon: WeaponType::Gun,
                                            },
                                        ),
                                    )
                                }
                                enums::Sound::PlayerPainShort => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::Pain { long: false },
                                        ),
                                    )
                                }
                                enums::Sound::PlayerPainLong => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::Pain { long: true },
                                        ),
                                    )
                                }
                                enums::Sound::BodyLand => {
                                    // TODO:
                                    continue;
                                }
                                enums::Sound::PlayerAirjump => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::AirJump,
                                        ),
                                    )
                                }
                                enums::Sound::PlayerJump => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::GroundJump,
                                        ),
                                    )
                                }
                                enums::Sound::PlayerDie => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::Death,
                                        ),
                                    )
                                }
                                enums::Sound::PlayerSpawn => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::Spawn,
                                        ),
                                    )
                                }
                                enums::Sound::PlayerSkid => {
                                    // TODO:
                                    continue;
                                }
                                enums::Sound::TeeCry => {
                                    // TODO:
                                    continue;
                                }
                                enums::Sound::HookLoop => {
                                    // TODO:
                                    continue;
                                }
                                enums::Sound::HookAttachGround => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::HookHitHookable {
                                                // TODO:
                                                hook_pos: None,
                                            },
                                        ),
                                    )
                                }
                                enums::Sound::HookAttachPlayer => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::HookHitPlayer {
                                                // TODO:!
                                                hook_pos: None,
                                            },
                                        ),
                                    )
                                }
                                enums::Sound::HookNoattach => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::HookHitUnhookable {
                                                // TODO:
                                                hook_pos: None,
                                            },
                                        ),
                                    )
                                }
                                enums::Sound::PickupHealth => {
                                    events::GameWorldEntitySoundEvent::Pickup(
                                        events::GamePickupSoundEvent::Heart(
                                            events::GamePickupHeartEventSound::Collect,
                                        ),
                                    )
                                }
                                enums::Sound::PickupArmor => {
                                    events::GameWorldEntitySoundEvent::Pickup(
                                        events::GamePickupSoundEvent::Armor(
                                            events::GamePickupArmorEventSound::Collect,
                                        ),
                                    )
                                }
                                enums::Sound::PickupGrenade => {
                                    events::GameWorldEntitySoundEvent::Grenade(
                                        events::GameGrenadeEventSound::Collect,
                                    )
                                }
                                enums::Sound::PickupShotgun => {
                                    events::GameWorldEntitySoundEvent::Shotgun(
                                        events::GameShotgunEventSound::Collect,
                                    )
                                }
                                enums::Sound::PickupNinja => {
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Buff(
                                            events::GameBuffSoundEvent::Ninja(
                                                events::GameBuffNinjaEventSound::Collect,
                                            ),
                                        ),
                                    )
                                }
                                enums::Sound::WeaponSpawn => {
                                    // TODO: per weapon
                                    events::GameWorldEntitySoundEvent::Grenade(
                                        events::GameGrenadeEventSound::Spawn,
                                    )
                                }
                                enums::Sound::WeaponNoammo => {
                                    // TODO: per weapon
                                    events::GameWorldEntitySoundEvent::Character(
                                        events::GameCharacterSoundEvent::Sound(
                                            events::GameCharacterEventSound::NoAmmo {
                                                weapon: WeaponType::Gun,
                                            },
                                        ),
                                    )
                                }
                                enums::Sound::Hit => events::GameWorldEntitySoundEvent::Character(
                                    events::GameCharacterSoundEvent::Sound(
                                        events::GameCharacterEventSound::Hit { strong: false },
                                    ),
                                ),
                                enums::Sound::ChatServer => {
                                    // TODO: not really needed
                                    continue;
                                }
                                enums::Sound::ChatClient => {
                                    // TODO: not really needed
                                    continue;
                                }
                                enums::Sound::ChatHighlight => {
                                    // TODO: not really needed
                                    continue;
                                }
                                enums::Sound::CtfDrop => events::GameWorldEntitySoundEvent::Flag(
                                    events::GameFlagEventSound::Drop,
                                ),
                                enums::Sound::CtfReturn => events::GameWorldEntitySoundEvent::Flag(
                                    events::GameFlagEventSound::Return,
                                ),
                                enums::Sound::CtfGrabPl => {
                                    // TODO: flag type is wrong
                                    events::GameWorldEntitySoundEvent::Flag(
                                        events::GameFlagEventSound::Collect(FlagType::Red),
                                    )
                                }
                                enums::Sound::CtfGrabEn => {
                                    // TODO: flag type is wrong
                                    events::GameWorldEntitySoundEvent::Flag(
                                        events::GameFlagEventSound::Collect(FlagType::Blue),
                                    )
                                }
                                enums::Sound::CtfCapture => {
                                    events::GameWorldEntitySoundEvent::Flag(
                                        events::GameFlagEventSound::Capture,
                                    )
                                }
                                enums::Sound::Menu => {
                                    // TODO: not really needed
                                    continue;
                                }
                            },
                        }),
                    );
                }
                SnapObj::DamageInd(damage_ind) => {
                    let events = base
                        .events
                        .worlds
                        .entry(player_stage)
                        .or_insert_with_keep_order(|| events::GameWorldEvents {
                            events: mt_datatypes::PoolFxLinkedHashMap::new_without_pool(),
                        });
                    let angle = PI + damage_ind.angle as f32 / 256.0;
                    events.events.insert(
                        base.event_id_generator.next_id(),
                        events::GameWorldEvent::Effect(events::GameWorldEffectEvent {
                            pos: vec2::new(damage_ind.common.x as f32, damage_ind.common.y as f32)
                                / 32.0,
                            owner_id: Some(player_id),
                            ev: events::GameWorldEntityEffectEvent::Character(
                                events::GameCharacterEffectEvent::Effect(
                                    events::GameCharacterEventEffect::DamageIndicator {
                                        vel: vec2::new(angle.cos(), angle.sin()) * 16.0,
                                    },
                                ),
                            ),
                        }),
                    );
                }
                SnapObj::MyOwnEvent(my_own_event) => {
                    debug!("[NOT IMPLEMENTED] my own event: {my_own_event:?}");
                }
                SnapObj::SpecChar(spec_char) => {
                    debug!("[NOT IMPLEMENTED] spec char: {spec_char:?}");
                }
                SnapObj::SwitchState(switch_state) => {
                    debug!("[NOT IMPLEMENTED] switch state: {switch_state:?}");
                }
                SnapObj::EntityEx(entity_ex) => {
                    debug!("[NOT IMPLEMENTED] entity ex: {entity_ex:?}");
                }
                SnapObj::DdnetSpectatorInfo(ddnet_spectator_info) => {
                    debug!("[NOT IMPLEMENTED] ddnet spectator info: {ddnet_spectator_info:?}");
                }
                SnapObj::Birthday(birthday) => {
                    debug!("[NOT IMPLEMENTED] birthday: {birthday:?}");
                }
                SnapObj::Finish(finish) => {
                    debug!("[NOT IMPLEMENTED] finish: {finish:?}");
                }
                SnapObj::MapSoundWorld(map_sound_world) => {
                    debug!("[NOT IMPLEMENTED] map sound world: {map_sound_world:?}");
                }
            }
        }

        snapshot.global_tune_zone = base.tunes;
    }

    #[allow(clippy::too_many_arguments)]
    fn on_packet(
        player_id: CharacterId,
        player: &mut ClientData,
        socket: &mut SocketClient,
        time: &base::steady_clock::SteadyClock,
        io: &Io,
        server_network: &QuinnNetworks,
        con_id: NetworkConnectionId,
        base: &mut ClientBase,
        log: &ConnectingLog,
        pid: PeerId,
        data: &[u8],
        collision: &mut Option<Box<Collision>>,
        connect_addr: &SocketAddr,
        snapshot: &mut Snapshot,
        is_active_connection: bool,
        is_main_connection: bool,
    ) {
        use ClientState::*;

        let msg = match msg::decode(&mut WarnPkt(pid, data), &mut Unpacker::new(data)) {
            Ok(m) => m,
            Err(err) => {
                let id =
                    SystemOrGame::decode_id(&mut WarnPkt(pid, data), &mut Unpacker::new(data)).ok();
                warn!("decode error {id:?} {err:?}:");
                hexdump(Level::Warn, data);
                return;
            }
        };
        let mut processed = true;
        let state = &mut player.state;
        const MAP_BASE_PATH: &str = "downloaded/legacy/maps/";

        // ignore most packages from non main/active connections
        let is_con_msg = matches!(
            &msg,
            SystemOrGame::System(System::MapChange(_) | System::ConReady(_))
                | SystemOrGame::Game(Game::SvReadyToEnter(_))
        );
        let is_snap_msg = matches!(
            &msg,
            SystemOrGame::System(System::SnapEmpty(_) | System::SnapSingle(_) | System::Snap(_))
        );
        let is_chat_msg = matches!(&msg, SystemOrGame::Game(Game::SvChat(_)));
        if !is_con_msg && !is_snap_msg && !is_chat_msg && !is_main_connection {
            return;
        }

        let mut add_vote = |vote_description: &[u8]| {
            let votes = base
                .votes
                .categories
                .entry("general".try_into().unwrap())
                .or_default();
            votes.insert(String::from_utf8_lossy(vote_description).to_string());
        };

        match (&mut *state, msg) {
            (_, SystemOrGame::System(System::Capabilities(caps))) => {
                const SERVERCAPFLAG_DDNET: i32 = 1 << 0;
                const SERVERCAPFLAG_CHATTIMEOUTCODE: i32 = 1 << 1;
                // const SERVERCAPFLAG_ANYPLAYERFLAG: i32 = 1 << 2;
                // const SERVERCAPFLAG_PINGEX: i32 = 1 << 3;
                const SERVERCAPFLAG_ALLOWDUMMY: i32 = 1 << 4;
                // const SERVERCAPFLAG_SYNCWEAPONINPUT: i32 = 1 << 5;
                if (caps.flags & SERVERCAPFLAG_DDNET) != 0 {
                    base.capabilities.is_ddnet = true;
                }
                if (caps.flags & SERVERCAPFLAG_ALLOWDUMMY) != 0 {
                    base.capabilities.allows_dummy = true;
                }
                if (caps.flags & SERVERCAPFLAG_CHATTIMEOUTCODE) != 0 {
                    base.capabilities.chat_timeout_codes = true;
                }
            }
            (_, SystemOrGame::System(System::Reconnect(_))) => {
                log.log("Proxy client will reconnect to the server. (reconnect packet)");
                socket
                    .net
                    .disconnect(&mut socket.socket, socket.server_pid, b"reconnect")
                    .unwrap();
                let (pid, res) = socket.net.connect(&mut socket.socket, *connect_addr);
                res.unwrap();
                socket.server_pid = pid;
            }
            (_, SystemOrGame::System(System::PongEx(pong_ex))) => {
                if let Some(last_time) = base.last_pings.remove(&pong_ex.id.as_u128()) {
                    base.last_pong = Some(time.now().saturating_sub(last_time));
                }
            }
            (_, SystemOrGame::System(System::MapDetails(info))) => {
                if let Some(name) = String::from_utf8(info.name.to_vec())
                    .ok()
                    .and_then(|s| ReducedAsciiString::try_from(s.as_str()).ok())
                {
                    log.log("Proxy client received map details.");
                    if !base.is_first_map_pkt {
                        base.server_info = ServerInfoTy::Partial {
                            requires_password: base.server_info.requires_password(),
                        };
                    }
                    base.is_first_map_pkt = false;
                    player.ready = Default::default();
                    // try to read file
                    let fs = io.fs.clone();
                    let map_name = name.clone();
                    let file = io
                        .rt
                        .spawn(async move {
                            Ok(fs
                                .read_file(
                                    format!(
                                        "{MAP_BASE_PATH}{}_{}.map",
                                        map_name.as_str(),
                                        fmt_hash(&info.sha256.0)
                                    )
                                    .as_ref(),
                                )
                                .await?)
                        })
                        .get();
                    match file {
                        Ok(_) => {
                            socket.sends(System::Ready(system::Ready));
                            socket.flush();
                            *state = ClientState::WaitingForMapChange {
                                name,
                                hash: info.sha256.0,
                            };
                        }
                        Err(_) => {
                            log.log("Proxy client will download the specified map.");
                            *state = ClientState::DownloadingMap {
                                expected_size: None,
                                data: Default::default(),
                                name,
                                sha256: Some(info.sha256.0),
                            };
                        }
                    }
                } else {
                    processed = false;
                }
            }
            (
                DownloadingMap { expected_size, .. },
                SystemOrGame::System(System::MapChange(info)),
            ) => {
                log.log("Proxy client map download started.");
                *expected_size = Some(info.size as usize);
                // Since crc checks are not secure, the client will always download these maps
                socket.sends(System::RequestMapData(system::RequestMapData { chunk: 0 }));
                socket.flush();
            }
            (WaitingForMapChange { name, hash }, SystemOrGame::System(System::MapChange(_))) => {
                // basically ignore, only to ensure correct order
                log.log("Proxy client map change packet and map is loaded.");
                *state = ClientState::MapReady {
                    name: std::mem::take(name),
                    hash: *hash,
                };
            }
            (_, SystemOrGame::System(System::MapChange(info))) => {
                if let Some(name) = String::from_utf8(info.name.to_vec())
                    .ok()
                    .and_then(|s| ReducedAsciiString::try_from(s.as_str()).ok())
                {
                    if is_main_connection {
                        log.log("Proxy client received map change packet (without map details).");
                        log.log("This is the legacy CRC map download, and thus cannot be skipped.");
                        if !base.is_first_map_pkt {
                            base.server_info = ServerInfoTy::Partial {
                                requires_password: base.server_info.requires_password(),
                            };
                        }
                        base.is_first_map_pkt = false;
                        player.ready = Default::default();
                        // Since crc checks are not secure, the client will always download these maps
                        socket.sends(System::RequestMapData(system::RequestMapData { chunk: 0 }));
                        socket.flush();
                        *state = ClientState::DownloadingMap {
                            expected_size: Some(info.size as usize),
                            data: Default::default(),
                            name,
                            sha256: None,
                        };
                    } else {
                        log.log("Proxy client dummy ready.");
                        socket.sends(System::Ready(system::Ready));
                        socket.flush();
                        *state = ClientState::SentServerInfo;
                    }
                } else {
                    processed = false;
                }
            }
            (
                DownloadingMap {
                    expected_size,
                    data,
                    name,
                    sha256,
                },
                SystemOrGame::System(System::MapData(map_data)),
            ) => {
                if let Some(expected_size) = *expected_size {
                    let download_chunk = map_data.chunk as usize;

                    data.insert(download_chunk, map_data.data.to_vec());
                    let next_chunk = data
                        .last_key_value()
                        .map(|(k, _)| *k)
                        .unwrap_or(download_chunk)
                        + 1;

                    let total_len = data.values().map(|d| d.len()).sum::<usize>();
                    if total_len < expected_size {
                        log.log(format!("Received map chunk: {}", map_data.chunk));
                        log.log(format!("{total_len} of {expected_size} bytes downloaded"));
                        let downloading_chunks = data.values().filter(|d| d.is_empty()).count();
                        for i in next_chunk..next_chunk + 50usize.saturating_sub(downloading_chunks)
                        {
                            data.insert(i, Default::default());
                            socket.sends(System::RequestMapData(system::RequestMapData {
                                chunk: i as i32,
                            }));
                        }
                        socket.flush();
                    } else {
                        log.log("Map successfully downloaded.");
                        let fs = io.fs.clone();
                        let file = std::mem::take(data).into_values().flatten().collect();
                        let mut hasher = sha2::Sha256::new();
                        hasher.update(&file);
                        let hash = hasher.finalize();
                        if sha256.is_none_or(|check_hash| check_hash == hash.as_slice()) {
                            let map_name = name.clone();
                            let _ = io
                                .rt
                                .spawn(async move {
                                    let _ = fs.create_dir(MAP_BASE_PATH.as_ref()).await;
                                    let _ = fs
                                        .write_file(
                                            format!(
                                                "{MAP_BASE_PATH}{}_{}.map",
                                                map_name.as_str(),
                                                fmt_hash(&hash.into())
                                            )
                                            .as_ref(),
                                            file,
                                        )
                                        .await;
                                    Ok(())
                                })
                                .get();

                            let name = std::mem::take(name);

                            log.log("Map saved to disk, client proxy is ready to join the game.");
                            socket.sends(System::Ready(system::Ready));
                            socket.flush();
                            *state = ClientState::MapReady {
                                name,
                                hash: hash.into(),
                            };
                        } else {
                            log.log("Map was invalid (sha256 check failed)");
                            socket.disconnect(b"invalid map (sha256 check failed)");
                        }
                    }
                }
            }
            (_, SystemOrGame::Game(Game::SvVoteClearOptions(_))) => {
                base.votes.categories.clear();
                base.votes.has_unfinished_map_votes = base.capabilities.is_ddnet;
                base.vote_list_updated = true;
            }
            (_, SystemOrGame::Game(Game::SvVoteOptionListAdd(votes))) => {
                for i in 0..votes.num_options.clamp(0, 15) as usize {
                    add_vote(votes.description[i]);
                }
                base.vote_list_updated = true;
            }
            (_, SystemOrGame::Game(Game::SvVoteOptionAdd(vote))) => {
                add_vote(vote.description);
                base.vote_list_updated = true;
            }
            (_, SystemOrGame::Game(Game::SvVoteOptionRemove(vote))) => {
                let votes = base
                    .votes
                    .categories
                    .entry("general".try_into().unwrap())
                    .or_default();
                votes.remove(&String::from_utf8_lossy(vote.description).to_string());
            }
            (_, SystemOrGame::Game(Game::SvVoteSet(vote))) => {
                let state = VoteState {
                    vote: VoteType::Misc {
                        key: MiscVoteCategoryKey {
                            category: "general".try_into().unwrap(),
                            vote_key: MiscVoteKey {
                                display_name: NetworkString::new_lossy(format!(
                                    "{}",
                                    String::from_utf8_lossy(vote.description),
                                )),
                                description: NetworkString::new_lossy(format!(
                                    "reason: {}",
                                    String::from_utf8_lossy(vote.reason)
                                )),
                            },
                        },
                        vote: MiscVote {
                            command: Default::default(),
                        },
                    },
                    remaining_time: Duration::from_secs(vote.timeout.unsigned_abs() as u64),
                    yes_votes: 0,
                    no_votes: 0,
                    allowed_to_vote_count: 0,
                };
                let state = (vote.timeout > 0).then_some(state);
                base.vote_state = state.clone().map(|s| (s, time.now()));
                server_network.send_in_order_to(
                    &ServerToClientMessage::Vote(state),
                    &con_id,
                    NetworkInOrderChannel::Global,
                );
            }
            (_, SystemOrGame::Game(Game::SvVoteStatus(vote))) => {
                if let Some((mut state, start_time)) = base.vote_state.clone() {
                    state.yes_votes = vote.yes.unsigned_abs() as u64;
                    state.no_votes = vote.no.unsigned_abs() as u64;
                    state.allowed_to_vote_count = vote.total.unsigned_abs() as u64;

                    let now = time.now();
                    state.remaining_time = state
                        .remaining_time
                        .saturating_sub(now.saturating_sub(start_time));

                    server_network.send_in_order_to(
                        &ServerToClientMessage::Vote(Some(state.clone())),
                        &con_id,
                        NetworkInOrderChannel::Global,
                    );

                    base.vote_state = Some((state, now));
                }
            }
            (_, SystemOrGame::System(System::ConReady(_))) => {
                player.ready.con = true;
            }
            (StartInfoSent, SystemOrGame::Game(Game::SvReadyToEnter(_))) => {
                log.log("Client proxy sends enter game packet.");
                socket.sends(System::EnterGame(system::EnterGame));
                socket.sendg(Game::ClIsDdnetLegacy(game::ClIsDdnetLegacy {
                    ddnet_version: 18090, // VERSION DDNET_RECONNECT
                }));
                socket.sendg(Game::ClSay(game::ClSay {
                    team: false,
                    message: format!(
                        "/emote {} {}",
                        match player.player_info.default_eyes {
                            TeeEye::Normal => "normal",
                            TeeEye::Pain => "pain",
                            TeeEye::Happy => "happy",
                            TeeEye::Surprised => "surprise",
                            TeeEye::Angry => "angry",
                            TeeEye::Blink => "blink",
                        },
                        99999
                    )
                    .as_bytes(),
                }));
                socket.flush();
                player.snap_manager.reset();
                player.latest_inputs.clear();
                player.server_client = ServerClientPlayer {
                    id: player.server_client.id,
                    input_storage: Default::default(),
                };
                if is_main_connection {
                    base.last_snap_tick = i32::MAX;
                    base.client_snap_storage.clear();
                    base.latest_client_snap = None;
                    base.inputs_to_ack.clear();
                    base.cur_monotonic_tick = 0;
                    base.ack_input_tick = -1;
                    base.vote_state = None;
                    base.teams.clear();
                }
                *state = ClientState::Ingame;
            }
            (_, SystemOrGame::System(snap))
                if matches!(
                    snap,
                    System::Snap(_) | System::SnapEmpty(_) | System::SnapSingle(_)
                ) =>
            {
                let (snap, tick) = match snap {
                    System::Snap(s) => (
                        player
                            .snap_manager
                            .snap(&mut WarnPkt(pid, s.data), obj_size, s),
                        s.tick,
                    ),
                    System::SnapEmpty(s) => (
                        player
                            .snap_manager
                            .snap_empty(&mut WarnPkt(pid, &[]), obj_size, s),
                        s.tick,
                    ),
                    System::SnapSingle(s) => (
                        player
                            .snap_manager
                            .snap_single(&mut WarnPkt(pid, s.data), obj_size, s),
                        s.tick,
                    ),
                    _ => unreachable!(),
                };

                // only process whil ingame, even tho the snap manager always gets the snaps
                let can_process = matches!(*state, ClientState::Ingame);
                let prev_snap_tick = base.last_snap_tick;
                if let Some(Ok(Some(snap))) = can_process.then_some(snap.as_ref()) {
                    let items: Vec<_> = snap
                        .items()
                        .map(|item| {
                            (
                                SnapObj::decode_obj(
                                    &mut WarnPkt(pid, &[]),
                                    item.type_id,
                                    &mut IntUnpacker::new(item.data),
                                ),
                                item.id as i32,
                                item.data.as_ptr() as *const (),
                            )
                        })
                        .collect();

                    // Filter unwanted items
                    let mut ddnet_characters: HashMap<i32, DdnetCharacter> = Default::default();
                    let mut ddnet_players: HashMap<i32, DdnetPlayer> = Default::default();
                    let mut items: Vec<_> = items
                        .into_iter()
                        .filter_map(|(i, id, ptr)| match i {
                            Ok(i) => {
                                if let SnapObj::DdnetCharacter(i) = i {
                                    ddnet_characters.insert(id, i);
                                    None
                                } else if let SnapObj::DdnetPlayer(i) = i {
                                    ddnet_players.insert(id, i);
                                    None
                                } else {
                                    Some((i, id, ptr))
                                }
                            }
                            Err(e) => {
                                debug!("item decode error {e:?}: {id:?}");
                                None
                            }
                        })
                        .collect();

                    // We always want character snapshot items
                    // first followed by player info.
                    // Rest does not really matter.
                    items.sort_by(|(s1, id1, ptr1), (s2, id2, ptr2)| {
                        let (char_score, player_score) = (0, 1);
                        let score1 = if matches!(s1, SnapObj::Character(_)) {
                            char_score
                        } else if matches!(s1, SnapObj::PlayerInfo(_)) {
                            player_score
                        } else {
                            2
                        };
                        let score2 = if matches!(s2, SnapObj::Character(_)) {
                            char_score
                        } else if matches!(s2, SnapObj::PlayerInfo(_)) {
                            player_score
                        } else {
                            2
                        };

                        // We first try to use strong_weak_id from ddnet snap items.
                        // But if that does not exist fallback to using the ptr of the data,
                        // even tho apparently teeworlds doesn't send in strong/weak relevant
                        // matter :/
                        // Note: id1 & id2 purposely swapped.
                        let eq_cmp = match ddnet_characters
                            .get(id2)
                            .map(|d| d.strong_weak_id)
                            .cmp(&ddnet_characters.get(id1).map(|d| d.strong_weak_id))
                        {
                            std::cmp::Ordering::Less => std::cmp::Ordering::Less,
                            std::cmp::Ordering::Equal => ptr1.cmp(ptr2),
                            std::cmp::Ordering::Greater => std::cmp::Ordering::Greater,
                        };

                        match score1.cmp(&score2) {
                            std::cmp::Ordering::Less => std::cmp::Ordering::Less,
                            std::cmp::Ordering::Equal => eq_cmp,
                            std::cmp::Ordering::Greater => std::cmp::Ordering::Greater,
                        }
                    });

                    let mut items: Vec<_> = items.into_iter().map(|(i, id, _)| (i, id)).collect();

                    // update local players
                    // only look for players which are local
                    // reverse iterator intended because of above sorting
                    for (item, id) in items.iter().rev() {
                        if let SnapObj::PlayerInfo(info) = item {
                            if info.local == 1 {
                                let mut dummy = LocalPlayer {
                                    client_id: player.server_client.id,
                                    player_id,
                                    player_info: player.player_info.clone(),

                                    character_snap: Default::default(),
                                    ddnet_character_snap: Default::default(),
                                    ddnet_player_snap: Default::default(),
                                };
                                if let Some(ddnet_player) = ddnet_players.get(id).copied() {
                                    dummy.ddnet_player_snap = Some(ddnet_player);
                                }
                                base.local_players.insert(*id, dummy);
                            }
                        } else if let SnapObj::Character(char) = item
                            && let Some(dummy) = base.local_players.get_mut(id)
                        {
                            dummy.character_snap = Some(*char);
                            if let Some(ddnet_char) = ddnet_characters.get(id).copied() {
                                dummy.ddnet_character_snap = Some(ddnet_char);
                            }
                        }
                    }

                    if is_active_connection {
                        base.last_snap_tick = tick;
                        *snapshot = Snapshot::new(
                            &base.vanilla_snap_pool,
                            base.id_generator.peek_next_id(),
                            None,
                            Default::default(),
                        );

                        let mut local_player_legacy_id = None;
                        // search legacy id of local player
                        {
                            let mut char_legacy_to_new_id =
                                std::mem::take(&mut base.char_legacy_to_new_id);
                            base.char_new_id_to_legacy.clear();
                            base.confirmed_player_ids.clear();
                            let mut proj_legacy_to_new_id =
                                std::mem::take(&mut base.proj_legacy_to_new_id);
                            let mut laser_legacy_to_new_id =
                                std::mem::take(&mut base.laser_legacy_to_new_id);
                            let mut pickup_legacy_to_new_id =
                                std::mem::take(&mut base.pickup_legacy_to_new_id);
                            let mut flag_legacy_to_new_id =
                                std::mem::take(&mut base.flag_legacy_to_new_id);

                            let mut character_snaps: HashSet<i32> = Default::default();

                            items.iter().for_each(|(i, id)| {
                                if let SnapObj::PlayerInfo(info) = i {
                                    if info.local == 1 {
                                        local_player_legacy_id = Some(*id);
                                    }
                                    base.confirmed_player_ids.insert(*id);
                                    if let Some(char_id) = base
                                        .local_players
                                        .get(id)
                                        .map(|d| d.player_id)
                                        .or_else(|| (info.local == 1).then_some(player_id))
                                    {
                                        base.char_legacy_to_new_id.insert(info.client_id, char_id);
                                        base.char_new_id_to_legacy.insert(char_id, info.client_id);
                                    } else if let Some(new_id) =
                                        char_legacy_to_new_id.remove(&info.client_id)
                                    {
                                        base.char_legacy_to_new_id.insert(info.client_id, new_id);
                                        base.char_new_id_to_legacy.insert(new_id, info.client_id);
                                    } else if !base.char_legacy_to_new_id.contains_key(id) {
                                        let char_id = base.id_generator.next_id();
                                        base.char_legacy_to_new_id.insert(*id, char_id);
                                        base.char_new_id_to_legacy.insert(char_id, *id);
                                    }
                                } else if let SnapObj::Character(_) = i {
                                    character_snaps.insert(*id);
                                    if let Some(new_id) = char_legacy_to_new_id.remove(id) {
                                        base.char_legacy_to_new_id.insert(*id, new_id);
                                        base.char_new_id_to_legacy.insert(new_id, *id);
                                    } else if !base.char_legacy_to_new_id.contains_key(id) {
                                        let char_id = base.id_generator.next_id();
                                        base.char_legacy_to_new_id.insert(*id, char_id);
                                        base.char_new_id_to_legacy.insert(char_id, *id);
                                    }
                                } else if let SnapObj::Projectile(_) = i {
                                    if let Some(legacy_id) = proj_legacy_to_new_id.remove(id) {
                                        base.proj_legacy_to_new_id.insert(*id, legacy_id);
                                    } else if !base.proj_legacy_to_new_id.contains_key(id) {
                                        base.proj_legacy_to_new_id
                                            .insert(*id, base.id_generator.next_id());
                                    }
                                } else if let SnapObj::DdnetProjectile(_) = i {
                                    if let Some(legacy_id) = proj_legacy_to_new_id.remove(id) {
                                        base.proj_legacy_to_new_id.insert(*id, legacy_id);
                                    } else if !base.proj_legacy_to_new_id.contains_key(id) {
                                        base.proj_legacy_to_new_id
                                            .insert(*id, base.id_generator.next_id());
                                    }
                                } else if let SnapObj::DdraceProjectile(_) = i {
                                    if let Some(legacy_id) = proj_legacy_to_new_id.remove(id) {
                                        base.proj_legacy_to_new_id.insert(*id, legacy_id);
                                    } else if !base.proj_legacy_to_new_id.contains_key(id) {
                                        base.proj_legacy_to_new_id
                                            .insert(*id, base.id_generator.next_id());
                                    }
                                } else if let SnapObj::Laser(_) = i {
                                    if let Some(legacy_id) = laser_legacy_to_new_id.remove(id) {
                                        base.laser_legacy_to_new_id.insert(*id, legacy_id);
                                    } else if !base.laser_legacy_to_new_id.contains_key(id) {
                                        base.laser_legacy_to_new_id
                                            .insert(*id, base.id_generator.next_id());
                                    }
                                } else if let SnapObj::DdnetLaser(_) = i {
                                    if let Some(legacy_id) = laser_legacy_to_new_id.remove(id) {
                                        base.laser_legacy_to_new_id.insert(*id, legacy_id);
                                    } else if !base.laser_legacy_to_new_id.contains_key(id) {
                                        base.laser_legacy_to_new_id
                                            .insert(*id, base.id_generator.next_id());
                                    }
                                } else if let SnapObj::Pickup(_) = i {
                                    if let Some(legacy_id) = pickup_legacy_to_new_id.remove(id) {
                                        base.pickup_legacy_to_new_id.insert(*id, legacy_id);
                                    } else if !base.pickup_legacy_to_new_id.contains_key(id) {
                                        base.pickup_legacy_to_new_id
                                            .insert(*id, base.id_generator.next_id());
                                    }
                                } else if let SnapObj::DdnetPickup(_) = i {
                                    if let Some(legacy_id) = pickup_legacy_to_new_id.remove(id) {
                                        base.pickup_legacy_to_new_id.insert(*id, legacy_id);
                                    } else if !base.pickup_legacy_to_new_id.contains_key(id) {
                                        base.pickup_legacy_to_new_id
                                            .insert(*id, base.id_generator.next_id());
                                    }
                                } else if let SnapObj::Flag(_) = i {
                                    if let Some(legacy_id) = flag_legacy_to_new_id.remove(id) {
                                        base.flag_legacy_to_new_id.insert(*id, legacy_id);
                                    } else if !base.flag_legacy_to_new_id.contains_key(id) {
                                        base.flag_legacy_to_new_id
                                            .insert(*id, base.id_generator.next_id());
                                    }
                                }
                            });

                            // add dummy snaps if they were not found
                            for (id, dummy) in &base.local_players {
                                if !character_snaps.contains(id)
                                    && let Some(char) = dummy.character_snap
                                {
                                    if let Some(ddnet_char) = dummy.ddnet_character_snap {
                                        ddnet_characters.insert(*id, ddnet_char);
                                    }
                                    if let Some(ddnet_player) = dummy.ddnet_player_snap
                                        && !ddnet_players.contains_key(id)
                                    {
                                        ddnet_players.insert(*id, ddnet_player);
                                    }
                                    base.char_legacy_to_new_id.insert(*id, dummy.player_id);
                                    base.char_new_id_to_legacy.insert(dummy.player_id, *id);
                                    items.insert(0, (SnapObj::Character(char), *id));
                                }
                            }
                        }

                        let legacy_id_in_stage_id = &mut base.legacy_id_in_stage_id;
                        legacy_id_in_stage_id.clear();

                        fn empty_stage(
                            game_el_id: StageId,
                            color: ubvec4,
                            name: &str,
                        ) -> SnapshotStage {
                            SnapshotStage {
                                game_el_id,
                                match_manager: SnapshotMatchManager::new(Match {
                                    ty: MatchType::Solo,
                                    state: MatchState::Running {
                                        round_ticks_passed: 0,
                                        round_ticks_left: 0.into(),
                                    },
                                    balance_tick: 0.into(),
                                }),
                                stage_color: color,
                                stage_name: PoolNetworkString::from_without_pool(
                                    name.try_into().unwrap(),
                                ),
                                world: SnapshotWorld {
                                    characters: SnapshotCharacters::new_without_pool(),
                                    projectiles: SnapshotProjectiles::new_without_pool(),
                                    lasers: SnapshotLasers::new_without_pool(),
                                    pickups: SnapshotPickups::new_without_pool(),
                                    red_flags: SnapshotFlags::new_without_pool(),
                                    blue_flags: SnapshotFlags::new_without_pool(),
                                    inactive_objects: SnapshotInactiveObject {
                                        blue_flags: PoolVec::new_without_pool(),
                                        hearts: PoolVec::new_without_pool(),
                                        shields: PoolVec::new_without_pool(),
                                        red_flags: PoolVec::new_without_pool(),
                                        weapons: [
                                            PoolVec::new_without_pool(),
                                            PoolVec::new_without_pool(),
                                            PoolVec::new_without_pool(),
                                            PoolVec::new_without_pool(),
                                            PoolVec::new_without_pool(),
                                        ],
                                        ninjas: PoolVec::new_without_pool(),
                                    },
                                },
                            }
                        }
                        snapshot.stages.insert(
                            base.stage_0_id,
                            empty_stage(base.stage_0_id, ubvec4::new(255, 255, 255, 0), ""),
                        );

                        for (team_index, stage_id) in base.teams.values() {
                            if !snapshot.stages.contains_key(stage_id) {
                                let mut rng = Rng::new(team_index.unsigned_abs() as u64);
                                snapshot.stages.insert(
                                    *stage_id,
                                    empty_stage(
                                        *stage_id,
                                        ubvec4::new(
                                            rng.random_int_in(128..=255) as u8,
                                            rng.random_int_in(128..=255) as u8,
                                            rng.random_int_in(128..=255) as u8,
                                            20,
                                        ),
                                        &team_index.to_string(),
                                    ),
                                );
                            }
                        }

                        Self::fill_snapshot(
                            snapshot,
                            items,
                            ddnet_characters,
                            ddnet_players,
                            tick,
                            base,
                            player_id,
                            player,
                            local_player_legacy_id
                                .and_then(|i| base.teams.get(&i).map(|(_, stage_id)| *stage_id))
                                .unwrap_or(base.stage_0_id),
                            collision.as_deref(),
                            time.now(),
                        );

                        base.cur_monotonic_tick +=
                            tick.saturating_sub(prev_snap_tick).clamp(0, i32::MAX) as u64;
                        let mut snap =
                            bincode::serde::encode_to_vec(&snapshot, bincode::config::standard())
                                .unwrap();

                        let snap_id = base.snap_id;
                        base.snap_id += 1;

                        // this should be smaller than the number of snapshots saved on the client
                        let as_diff = if base.client_snap_storage.len() < 10 {
                            base.client_snap_storage.insert(
                                snap_id,
                                ClientSnapshotStorage {
                                    snapshot: snap.to_vec(),
                                    monotonic_tick: base.cur_monotonic_tick,
                                },
                            );
                            true
                        } else {
                            false
                        };

                        let (snap_diff, diff_id, diff_monotonic_tick) =
                            if let Some(latest_client_snap) = &base.latest_client_snap {
                                let mut new_snap = base.player_snap_pool.new();
                                new_snap.resize(snap.len(), Default::default());
                                new_snap.clone_from_slice(&snap);
                                let snap_vec = &mut snap;
                                snap_vec.clear();
                                if bin_patch::diff(
                                    &latest_client_snap.snapshot,
                                    &new_snap,
                                    snap_vec,
                                )
                                .is_ok()
                                {
                                    (
                                        snap,
                                        Some(latest_client_snap.snap_id),
                                        Some(latest_client_snap.monotonic_tick),
                                    )
                                } else {
                                    snap_vec.clear();
                                    snap_vec.append(&mut new_snap);

                                    (snap, None, None)
                                }
                            } else {
                                (snap, None, None)
                            };

                        // quickly rewrite the input ack's logic overhead
                        let cur_time = time.now();
                        let mut inputs_to_ack = Vec::default();
                        while base
                            .inputs_to_ack
                            .first_key_value()
                            .is_some_and(|(&intended_tick, _)| intended_tick <= base.ack_input_tick)
                        {
                            let (_, (mut inp, _)) = base.inputs_to_ack.pop_first().unwrap();
                            inp.logic_overhead = cur_time.saturating_sub(inp.logic_overhead);
                            inputs_to_ack.push(inp);
                        }

                        server_network.send_unordered_auto_to(
                            &ServerToClientMessage::Snapshot {
                                overhead_time: Duration::ZERO,
                                snapshot: snap_diff.as_slice().into(),
                                diff_id,
                                snap_id_diffed: diff_id
                                    .map(|diff_id| snap_id - diff_id)
                                    .unwrap_or(snap_id),
                                game_monotonic_tick_diff: diff_monotonic_tick
                                    .map(|diff_monotonic_tick| {
                                        base.cur_monotonic_tick - diff_monotonic_tick
                                    })
                                    .unwrap_or(base.cur_monotonic_tick),
                                as_diff,
                                input_ack: inputs_to_ack.as_slice().into(),
                            },
                            &con_id,
                        );
                    }
                } else {
                    processed = false;
                }
            }
            (_, SystemOrGame::Game(Game::SvChat(chat))) => {
                let (name, skin, skin_info) = if let Some(character) = base
                    .legacy_id_in_stage_id
                    .get(&chat.client_id)
                    .and_then(|s| snapshot.stages.get(s))
                    .and_then(|s| {
                        base.char_legacy_to_new_id
                            .get(&chat.client_id)
                            .map(|c| (s, c))
                    })
                    .and_then(|(s, c)| s.world.characters.get(c))
                {
                    let p = &character.player_info.player_info;
                    (p.name.clone(), p.skin.clone(), p.skin_info)
                } else if let Some(p) = base
                    .char_legacy_to_new_id
                    .get(&chat.client_id)
                    .and_then(|c| snapshot.spectator_players.get(c))
                {
                    let p = &p.player.player_info.player_info;
                    (p.name.clone(), p.skin.clone(), p.skin_info)
                } else if chat.client_id == -1
                    || base.char_legacy_to_new_id.contains_key(&chat.client_id)
                {
                    (
                        "".try_into().unwrap(),
                        "".try_into().unwrap(),
                        NetworkSkinInfo::Original,
                    )
                } else {
                    // ignore the chat msg completely
                    return;
                };
                if chat.client_id == -1 && is_active_connection {
                    let events = base
                        .events
                        .worlds
                        .entry(base.stage_0_id)
                        .or_insert_with_keep_order(|| events::GameWorldEvents {
                            events: mt_datatypes::PoolFxLinkedHashMap::new_without_pool(),
                        });
                    events.events.insert(
                        base.event_id_generator.next_id(),
                        events::GameWorldEvent::Notification(GameWorldNotificationEvent::System(
                            GameWorldSystemMessage::Custom(MtPoolNetworkString::from_without_pool(
                                NetworkString::new_lossy(String::from_utf8_lossy(chat.message)),
                            )),
                        )),
                    );
                } else {
                    let (channel, process) = if chat.team == 1 {
                        (NetChatMsgPlayerChannel::GameTeam, is_active_connection)
                    } else if chat.team == 3 {
                        (
                            NetChatMsgPlayerChannel::Whisper(ChatPlayerInfo {
                                id: player_id,
                                name: player.player_info.name.clone(),
                                skin: player.player_info.skin.clone(),
                                skin_info: player.player_info.skin_info,
                            }),
                            true,
                        )
                    } else {
                        (NetChatMsgPlayerChannel::Global, is_active_connection)
                    };
                    if process {
                        server_network.send_in_order_to(
                            &ServerToClientMessage::Chat(MsgSvChatMsg {
                                msg: NetChatMsg {
                                    sender: ChatPlayerInfo {
                                        id: *base
                                            .char_legacy_to_new_id
                                            .get(&chat.client_id)
                                            .unwrap_or(&player_id),
                                        name,
                                        skin,
                                        skin_info,
                                    },
                                    msg: String::from_utf8_lossy(chat.message).to_string(),
                                    channel,
                                },
                            }),
                            &con_id,
                            NetworkInOrderChannel::Global,
                        );
                    }
                }
            }
            (_, SystemOrGame::System(System::InputTiming(timing))) => {
                // adjust timing information for input acks
                base.ack_input_tick = base.ack_input_tick.max(timing.input_pred_tick);
                for (_, (inp, was_acked)) in base
                    .inputs_to_ack
                    .iter_mut()
                    .filter(|&(&intended_tick, _)| intended_tick <= timing.input_pred_tick)
                {
                    if !*was_acked {
                        debug!(
                            "est. ping: {}",
                            time.now().saturating_sub(inp.logic_overhead).as_millis()
                        );
                        // For now use ping pong for time calculations
                        if let Some(pong_time) = base.last_pong {
                            const PREDICTION_EXTRA_MARGIN: Duration = Duration::from_millis(5);
                            inp.logic_overhead = inp
                                .logic_overhead
                                .saturating_add(pong_time)
                                .saturating_add(PREDICTION_EXTRA_MARGIN);
                        } else {
                            inp.logic_overhead = time.now();
                        }
                        *was_acked = true;
                    }
                }
            }
            (_, SystemOrGame::Game(Game::SvMotd(motd))) => {
                let events = base
                    .events
                    .worlds
                    .entry(base.stage_0_id)
                    .or_insert_with_keep_order(|| events::GameWorldEvents {
                        events: mt_datatypes::PoolFxLinkedHashMap::new_without_pool(),
                    });
                events.events.insert(
                    base.event_id_generator.next_id(),
                    events::GameWorldEvent::Notification(GameWorldNotificationEvent::Motd {
                        msg: MtPoolNetworkString::from_without_pool(NetworkString::new_lossy(
                            String::from_utf8_lossy(motd.message),
                        )),
                    }),
                );
            }
            (_, SystemOrGame::Game(Game::SvEmoticon(emoticon))) => {
                base.emoticons
                    .insert(emoticon.client_id, (time.now(), emoticon.emoticon));
            }
            (_, SystemOrGame::Game(Game::SvKillMsg(msg))) => {
                let events = base
                    .events
                    .worlds
                    .entry(base.stage_0_id)
                    .or_insert_with_keep_order(|| events::GameWorldEvents {
                        events: mt_datatypes::PoolFxLinkedHashMap::new_without_pool(),
                    });
                const WEAPON_GAME: i32 = -3; // team switching etc
                const WEAPON_SELF: i32 = -2; // console kill command
                const WEAPON_WORLD: i32 = -1; // death tiles etc
                const WEAPON_HAMMER: i32 = 0;
                const WEAPON_GUN: i32 = 1;
                const WEAPON_SHOTGUN: i32 = 2;
                const WEAPON_GRENADE: i32 = 3;
                const WEAPON_LASER: i32 = 4;
                const WEAPON_NINJA: i32 = 5;
                events.events.insert(
                    base.event_id_generator.next_id(),
                    events::GameWorldEvent::Notification(GameWorldNotificationEvent::Action(
                        events::GameWorldAction::Kill {
                            killer: base.char_legacy_to_new_id.get(&msg.killer).copied(),
                            assists: mt_datatypes::PoolVec::new_without_pool(),
                            victims: mt_datatypes::PoolVec::from_without_pool(
                                if [WEAPON_WORLD, WEAPON_SELF, WEAPON_GAME].contains(&msg.weapon) {
                                    Default::default()
                                } else {
                                    base.char_legacy_to_new_id
                                        .get(&msg.victim)
                                        .copied()
                                        .into_iter()
                                        .collect()
                                },
                            ),
                            weapon: match msg.weapon {
                                WEAPON_HAMMER => events::GameWorldActionKillWeapon::Weapon {
                                    weapon: WeaponType::Hammer,
                                },
                                WEAPON_GUN => events::GameWorldActionKillWeapon::Weapon {
                                    weapon: WeaponType::Gun,
                                },
                                WEAPON_SHOTGUN => events::GameWorldActionKillWeapon::Weapon {
                                    weapon: WeaponType::Shotgun,
                                },
                                WEAPON_GRENADE => events::GameWorldActionKillWeapon::Weapon {
                                    weapon: WeaponType::Grenade,
                                },
                                WEAPON_LASER => events::GameWorldActionKillWeapon::Weapon {
                                    weapon: WeaponType::Laser,
                                },
                                WEAPON_NINJA => events::GameWorldActionKillWeapon::Ninja,
                                // WEAPON_WORLD | WEAPON_SELF | WEAPON_GAME
                                _ => events::GameWorldActionKillWeapon::World,
                            },
                            flags: Default::default(),
                        },
                    )),
                );
            }
            (_, SystemOrGame::Game(Game::SvTuneParams(tunes))) => {
                base.tunes.ground_control_speed = tunes.ground_control_speed.to_float();
                base.tunes.ground_control_accel = tunes.ground_control_accel.to_float();
                base.tunes.ground_friction = tunes.ground_friction.to_float();
                base.tunes.ground_jump_impulse = tunes.ground_jump_impulse.to_float();
                base.tunes.air_jump_impulse = tunes.air_jump_impulse.to_float();
                base.tunes.air_control_speed = tunes.air_control_speed.to_float();
                base.tunes.air_control_accel = tunes.air_control_accel.to_float();
                base.tunes.air_friction = tunes.air_friction.to_float();
                base.tunes.hook_length = tunes.hook_length.to_float();
                base.tunes.hook_fire_speed = tunes.hook_fire_speed.to_float();
                base.tunes.hook_drag_accel = tunes.hook_drag_accel.to_float();
                base.tunes.hook_drag_speed = tunes.hook_drag_speed.to_float();
                base.tunes.gravity = tunes.gravity.to_float();
                base.tunes.velramp_start = tunes.velramp_start.to_float();
                base.tunes.velramp_range = tunes.velramp_range.to_float();
                base.tunes.velramp_curvature = tunes.velramp_curvature.to_float();
                base.tunes.gun_curvature = tunes.gun_curvature.to_float();
                base.tunes.gun_speed = tunes.gun_speed.to_float();
                base.tunes.gun_lifetime = tunes.gun_lifetime.to_float();
                base.tunes.shotgun_curvature = tunes.shotgun_curvature.to_float();
                base.tunes.shotgun_speed = tunes.shotgun_speed.to_float();
                base.tunes.shotgun_speeddiff = tunes.shotgun_speeddiff.to_float();
                base.tunes.shotgun_lifetime = tunes.shotgun_lifetime.to_float();
                base.tunes.grenade_curvature = tunes.grenade_curvature.to_float();
                base.tunes.grenade_speed = tunes.grenade_speed.to_float();
                base.tunes.grenade_lifetime = tunes.grenade_lifetime.to_float();
                base.tunes.laser_reach = tunes.laser_reach.to_float();
                base.tunes.laser_bounce_delay = tunes.laser_bounce_delay.to_float();
                base.tunes.laser_bounce_num = tunes.laser_bounce_num.to_float();
                base.tunes.laser_bounce_cost = tunes.laser_bounce_cost.to_float();
                base.tunes.laser_damage = tunes.laser_damage.to_float();
                base.tunes.player_collision = tunes.player_collision.to_float();
                base.tunes.player_hooking = tunes.player_hooking.to_float();
                base.tunes.jetpack_strength = tunes.jetpack_strength.to_float();
                base.tunes.shotgun_strength = tunes.shotgun_strength.to_float();
                base.tunes.explosion_strength = tunes.explosion_strength.to_float();
                base.tunes.hammer_strength = tunes.hammer_strength.to_float();
                base.tunes.hook_duration = tunes.hook_duration.to_float();
                base.tunes.hammer_fire_delay = tunes.hammer_fire_delay.to_float();
                base.tunes.gun_fire_delay = tunes.gun_fire_delay.to_float();
                base.tunes.shotgun_fire_delay = tunes.shotgun_fire_delay.to_float();
                base.tunes.grenade_fire_delay = tunes.grenade_fire_delay.to_float();
                base.tunes.laser_fire_delay = tunes.laser_fire_delay.to_float();
                base.tunes.ninja_fire_delay = tunes.ninja_fire_delay.to_float();
                base.tunes.hammer_hit_fire_delay = tunes.hammer_hit_fire_delay.to_float();
                // base.tunes.ground_elasticity_x = tunes.ground_elasticity_x.to_float();
                // base.tunes.ground_elasticity_y = tunes.ground_elasticity_y.to_float();
            }
            (
                _,
                SystemOrGame::Game(
                    Game::SvTeamsState(SvTeamsState { teams })
                    | Game::SvTeamsStateLegacy(SvTeamsStateLegacy { teams }),
                ),
            ) => {
                let mut cur_teams: HashMap<_, _> = std::mem::take(&mut base.teams)
                    .into_iter()
                    .map(|(_, (team_index, stage_id))| (team_index, stage_id))
                    .collect();
                for (client_id, team) in teams.iter().copied().enumerate() {
                    let stage_id = if team == 0 {
                        base.stage_0_id
                    } else {
                        cur_teams
                            .get(&team)
                            .copied()
                            .unwrap_or_else(|| base.id_generator.next_id())
                    };
                    base.teams.insert(client_id as i32, (team, stage_id));
                    // add the current teams here too, so on team duplication
                    // it reuses the existing stage id, instead of generating a new.
                    cur_teams.insert(team, stage_id);
                }
            }
            _ => {
                processed = false;
            }
        }

        if !processed {
            debug!("unprocessed message {:?} {msg:?}", &player.state);
        }
    }

    fn on_connless_packet(tokens: &[u8], addr: SocketAddr, data: &[u8]) -> Option<ServerInfo> {
        let msg = match Connless::decode(&mut WarnPkt(addr, data), &mut Unpacker::new(data)) {
            Ok(m) => m,
            Err(err) => {
                warn!("decode error {err:?}:");
                hexdump(Level::Warn, data);
                return None;
            }
        };
        let mut processed = true;
        match msg {
            Connless::Info(info) => {
                if tokens.contains(&(info.token as u8)) {
                    return Some(ServerInfo {
                        game_type: String::from_utf8_lossy(info.game_type).to_string(),
                        passworded: (info.flags & INFO_FLAG_PASSWORD) != 0,
                    });
                }
            }
            Connless::InfoExtended(info) => {
                if tokens.contains(&(info.token as u8)) {
                    return Some(ServerInfo {
                        game_type: String::from_utf8_lossy(info.game_type).to_string(),
                        passworded: (info.flags & INFO_FLAG_PASSWORD) != 0,
                    });
                }
            }
            _ => processed = false,
        }
        if !processed {
            debug!("unprocessed message {msg:?}");
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn input_to_legacy_input(
        self_char_id: CharacterId,
        latest_snapshot: &Snapshot,
        base: &ClientBase,
        latest_inputs: &BTreeMap<i32, (CharacterInput, snap_obj::PlayerInput)>,
        intended_tick: i32,
        prev_inp: &snap_obj::PlayerInput,
        inp: &CharacterInput,
        diff: CharacterInputConsumableDiff,
    ) -> snap_obj::PlayerInput {
        let cursor = if let Some((_, hook)) = diff.hook {
            hook
        } else if let Some((_, fire)) = diff.fire {
            fire
        } else {
            *inp.cursor
        }
        .to_vec2();
        let (target_x, target_y) = ((cursor.x * 32.0) as i32, (cursor.y * 32.0) as i32);
        let state = &inp.state;

        fn to_diff(weapon_diff: Option<NonZeroI64>) -> (i32, i32) {
            (
                weapon_diff
                    .map(|diff| diff.get().clamp(0, i64::MAX))
                    .unwrap_or_default() as i32
                    * 2,
                weapon_diff
                    .map(|diff| diff.get().clamp(i64::MIN, 0).abs())
                    .unwrap_or_default() as i32
                    * 2,
            )
        }
        let (mut next_weapon_diff, mut prev_weapon_diff) = to_diff(diff.weapon_diff);

        // simulate wanted weapon with weapon diff instead
        if let Some(wanted_weapon) = diff.weapon_req {
            // get latest char
            let char = base
                .char_new_id_to_legacy
                .get(&self_char_id)
                .and_then(|legacy_id| base.legacy_id_in_stage_id.get(legacy_id))
                .and_then(|stage_id| latest_snapshot.stages.get(stage_id))
                .and_then(|stage| stage.world.characters.get(&self_char_id));
            if let Some(char) = char {
                // advance active weapon to whatever is wanted
                let weapons = &char.reusable_core.weapons;
                let wanted_weapon = weapons
                    .keys()
                    .enumerate()
                    .find(|&(_, k)| *k == wanted_weapon);

                let mut extra_weapon_diff = 0;
                let rstart = base.last_snap_tick + 1;
                let rend = intended_tick;
                for (old_tick, (_, inp)) in latest_inputs.range(rstart.min(rend)..rstart.max(rend))
                {
                    if let Some(prev_inp) = latest_inputs
                        .range(0..*old_tick)
                        .next_back()
                        .map(|(_, (_, prev_inp))| prev_inp)
                    {
                        extra_weapon_diff +=
                            inp.next_weapon.saturating_sub(prev_inp.next_weapon) as i64 / 2
                                - inp.prev_weapon.saturating_sub(prev_inp.prev_weapon) as i64 / 2;
                    }
                }
                let cur_weapon = weapons
                    .keys()
                    .enumerate()
                    .find(|&(_, k)| *k == char.core.active_weapon)
                    .map(|(index, _)| index);

                let cur_weapon = cur_weapon.and_then(|index| {
                    let index = (index as i64
                        + (extra_weapon_diff % weapons.len() as i64)
                        + weapons.len() as i64) as usize
                        % weapons.len();
                    weapons
                        .keys()
                        .enumerate()
                        .nth(index)
                        .map(|(index, _)| index)
                });

                if let Some((cur_weapon_index, (wanted_weapon_index, _))) =
                    cur_weapon.zip(wanted_weapon)
                {
                    if wanted_weapon_index > cur_weapon_index {
                        next_weapon_diff += (wanted_weapon_index - cur_weapon_index) as i32 * 2;
                    } else {
                        prev_weapon_diff += (cur_weapon_index - wanted_weapon_index) as i32 * 2;
                    }
                }
            }
        }
        let wanted_weapon = 0;

        let mut player_flags = 0;
        if inp.state.flags.contains(CharacterInputFlags::CHATTING) {
            player_flags |= LegacyInputFlags::Chatting as i32;
        }
        if inp
            .state
            .flags
            .contains(CharacterInputFlags::HOOK_COLLISION_LINE)
        {
            player_flags |= LegacyInputFlags::Aim as i32;
        }
        if inp.state.flags.contains(CharacterInputFlags::SCOREBOARD) {
            player_flags |= LegacyInputFlags::Scoreboard as i32;
        }
        if inp.state.flags.contains(CharacterInputFlags::MENU_UI) {
            player_flags |= LegacyInputFlags::InMenu as i32;
        }
        let mut input = snap_obj::PlayerInput {
            direction: *state.dir,
            target_x,
            target_y,
            jump: *state.jump as i32,
            fire: prev_inp.fire
                + diff
                    .fire
                    .map(|(v, _)| (v.get() * 2).saturating_sub(*state.fire as u64))
                    .unwrap_or_default() as i32,
            hook: *state.hook as i32,
            player_flags,
            wanted_weapon,
            next_weapon: prev_inp.next_weapon + next_weapon_diff,
            prev_weapon: prev_inp.prev_weapon + prev_weapon_diff,
        };
        // if not firing make sure the value is even (a.k.a. unpressed)
        if !*state.fire {
            input.fire += input.fire % 2;
        }
        input
    }

    fn run_loop(&mut self) -> anyhow::Result<()> {
        while !self.is_finished.load(std::sync::atomic::Ordering::SeqCst) {
            self.run_once()?;
        }
        Ok(())
    }

    fn player_info_to_legacy(player_info: &NetworkCharacterInfo) -> game::ClStartInfo<'_> {
        let skin: &ResourceKeyBase = player_info.skin.borrow();
        let (use_custom_color, color_body, color_feet) = match player_info.skin_info {
            NetworkSkinInfo::Original => (false, 0, 0),
            NetworkSkinInfo::Custom {
                body_color,
                feet_color,
            } => (
                true,
                rgba_to_legacy_color(body_color, true, true),
                rgba_to_legacy_color(feet_color, true, true),
            ),
        };
        game::ClStartInfo {
            name: player_info.name.as_bytes(),
            clan: player_info.clan.as_bytes(),
            country: -1,
            skin: skin.name.as_bytes(),
            use_custom_color,
            color_body,
            color_feet,
        }
    }

    fn handle_client_events(&mut self) -> anyhow::Result<()> {
        if self
            .server_has_new_events
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            let mut events = self.server_event_handler.events.blocking_lock();
            for (con_id, timestamp, event) in events.drain(..) {
                match event {
                    GameEvents::NetworkEvent(ev) => match ev {
                        NetworkEvent::Connected { .. } => {
                            self.log.log("Local client connected to proxy.");
                            self.con_id = Some(con_id);
                            if self.base.server_info.requires_password() {
                                self.server_network.send_unordered_to(
                                    &ServerToClientMessage::RequiresPassword,
                                    &con_id,
                                );
                            } else {
                                let sock_loop = SocketClient::new(&self.io, self.connect_addr)?;

                                self.players.insert(
                                    self.base.id_generator.next_id(),
                                    ProxyClient::new(
                                        Default::default(),
                                        sock_loop,
                                        self.time.now(),
                                        0,
                                        false,
                                    ),
                                );
                            }
                        }
                        NetworkEvent::Disconnected(_) => {
                            self.log.log("Local client disconnected from proxy.");
                            self.is_finished
                                .store(true, std::sync::atomic::Ordering::SeqCst);
                            return Ok(());
                        }
                        NetworkEvent::ConnectingFailed(reason) => {
                            self.log
                                .log(format!("Local client failed to connect to proxy: {reason}"));
                            self.is_finished
                                .store(true, std::sync::atomic::Ordering::SeqCst);
                            return Ok(());
                        }
                        NetworkEvent::NetworkStats(_) => {}
                    },
                    GameEvents::NetworkMsg(ev) => match ev {
                        ClientToServerMessage::Custom(_) => {}
                        ClientToServerMessage::PasswordResponse(password) => {
                            self.log
                                .log("Received password, proxy is connecting to game.");
                            self.base.join_password = password.to_string();

                            let sock_loop = SocketClient::new(&self.io, self.connect_addr)?;

                            self.players.insert(
                                self.base.id_generator.next_id(),
                                ProxyClient::new(
                                    Default::default(),
                                    sock_loop,
                                    self.time.now(),
                                    0,
                                    false,
                                ),
                            );
                        }
                        ClientToServerMessage::Ready(msg) => {
                            self.log.log("Client ready, proxy forwards that now.");
                            if let Some(con_id) = self.con_id {
                                self.server_network.send_unordered_to(
                                    &ServerToClientMessage::ReadyResponse(
                                        MsgClReadyResponse::Success {
                                            joined_ids: msg
                                                .players
                                                .iter()
                                                .filter_map(|p| {
                                                    self.players
                                                        .iter()
                                                        .find_map(|(id, c)| {
                                                            (c.data.server_client.id == p.id)
                                                                .then_some(*id)
                                                        })
                                                        .map(|char_id| (p.id, char_id))
                                                })
                                                .collect(),
                                        },
                                    ),
                                    &con_id,
                                );
                            }
                            for player in msg.players {
                                if let Some(client_player) = self
                                    .players
                                    .values_mut()
                                    .find(|c| c.data.server_client.id == player.id)
                                {
                                    client_player.data.player_info = player.player_info;
                                    client_player.data.ready.client_con = true;
                                }
                            }

                            debug!("[NOT IMPLEMENTED] rcon auto login: {:?}", msg.rcon_secret);
                        }
                        ClientToServerMessage::AddLocalPlayer(ev) => {
                            if self.players.len() < 2
                                || (self.connect_addr.ip().is_loopback()
                                    && self.players.len() < 128)
                            {
                                let sock_loop = SocketClient::new(&self.io, self.connect_addr)?;
                                let player_id = self.base.id_generator.next_id();
                                self.players.insert(
                                    player_id,
                                    ProxyClient::new(
                                        ev.player_info,
                                        sock_loop,
                                        self.time.now(),
                                        ev.id,
                                        true,
                                    ),
                                );
                                if let Some(con_id) = self.con_id {
                                    self.server_network.send_unordered_to(
                                        &ServerToClientMessage::AddLocalPlayerResponse(
                                            MsgSvAddLocalPlayerResponse::Success {
                                                id: ev.id,
                                                player_id,
                                            },
                                        ),
                                        &con_id,
                                    );
                                }
                            } else {
                                self.server_network.send_unordered_to(
                                    &ServerToClientMessage::AddLocalPlayerResponse(
                                        MsgSvAddLocalPlayerResponse::Err {
                                            id: ev.id,
                                            err: AddLocalPlayerResponseError::MaxPlayersPerClient,
                                        },
                                    ),
                                    &con_id,
                                );
                            }
                        }
                        ClientToServerMessage::PlayerMsg((player_id, ev)) => {
                            if let Some(player) = self.players.get_mut(&player_id) {
                                match ev {
                                    ClientToServerPlayerMessage::Custom(_) => {}
                                    ClientToServerPlayerMessage::RemLocalPlayer => {
                                        self.players.remove(&player_id);
                                    }
                                    ClientToServerPlayerMessage::Chat(msg) => {
                                        match msg {
                                            MsgClChatMsg::Global { msg } => {
                                                player.socket.sendg(Game::ClSay(game::ClSay {
                                                    team: false,
                                                    message: msg.as_bytes(),
                                                }));
                                            }
                                            MsgClChatMsg::GameTeam { msg } => {
                                                player.socket.sendg(Game::ClSay(game::ClSay {
                                                    team: true,
                                                    message: msg.as_bytes(),
                                                }));
                                            }
                                            MsgClChatMsg::Whisper { receiver_id, msg } => {
                                                if let Some((_, player_info)) = self
                                                    .base
                                                    .char_new_id_to_legacy
                                                    .get(&receiver_id)
                                                    .and_then(|legacy_id| {
                                                        Self::player_info_mut(
                                                            *legacy_id,
                                                            &self.base,
                                                            &mut self.last_snapshot,
                                                        )
                                                    })
                                                {
                                                    player.socket.sendg(Game::ClSay(game::ClSay {
                                                        team: false,
                                                        message: format!(
                                                            "/w \"{}\" {}",
                                                            player_info.player_info.name.as_str(),
                                                            msg.as_str()
                                                        )
                                                        .as_bytes(),
                                                    }));
                                                }
                                            }
                                        }
                                        player.socket.flush();
                                    }
                                    ClientToServerPlayerMessage::Kill => {
                                        player.sendg(Game::ClKill(game::ClKill));
                                        player.flush();
                                    }
                                    ClientToServerPlayerMessage::JoinSpectator => {
                                        player.sendg(Game::ClSetTeam(game::ClSetTeam {
                                            team: Team::Spectators,
                                        }));
                                        player.sendg(Game::ClSetSpectatorMode(
                                            game::ClSetSpectatorMode { spectator_id: -1 },
                                        ));
                                        player.flush();
                                    }
                                    ClientToServerPlayerMessage::SwitchToCamera(mode) => {
                                        let is_spectator = self
                                            .last_snapshot
                                            .spectator_players
                                            .contains_key(&player_id);
                                        if is_spectator {
                                            let spec_id = match mode {
                                                ClientCameraMode::None => -1,
                                                ClientCameraMode::FreeCam(ids)
                                                | ClientCameraMode::PhasedFreeCam(ids) => {
                                                    if ids.is_empty() {
                                                        -1
                                                    } else {
                                                        let char_id = ids.iter().next().unwrap();
                                                        self.base
                                                            .char_new_id_to_legacy
                                                            .get(char_id)
                                                            .copied()
                                                            .unwrap_or(-1)
                                                    }
                                                }
                                            };
                                            player.sendg(Game::ClSetSpectatorMode(
                                                game::ClSetSpectatorMode {
                                                    spectator_id: spec_id,
                                                },
                                            ));
                                            player.flush();
                                        } else {
                                            let is_spec_or_pause = self
                                                .base
                                                .char_new_id_to_legacy
                                                .get(&player_id)
                                                .and_then(|id| {
                                                    self.base.legacy_id_in_stage_id.get(id)
                                                })
                                                .and_then(|stage_id| {
                                                    self.last_snapshot.stages.get(stage_id)
                                                })
                                                .and_then(|stage| {
                                                    stage.world.characters.get(&player_id)
                                                })
                                                .map(|c| {
                                                    type PhState = SnapshotCharacterPhasedState;
                                                    match &c.phased {
                                                        PhState::Normal {
                                                            ingame_spectate, ..
                                                        } => ingame_spectate.is_some(),
                                                        PhState::Dead { .. } => false,
                                                        PhState::PhasedSpectate(_) => true,
                                                    }
                                                })
                                                .unwrap_or_default();

                                            let prefix = if let ClientCameraMode::FreeCam(_) = mode
                                            {
                                                "pause"
                                            } else {
                                                "spec"
                                            };
                                            let (pause, spec_id, switch) = match mode {
                                                ClientCameraMode::None => {
                                                    ("pause".to_string(), -1, is_spec_or_pause)
                                                }
                                                ClientCameraMode::FreeCam(ids)
                                                | ClientCameraMode::PhasedFreeCam(ids) => {
                                                    if ids.is_empty() {
                                                        (prefix.to_string(), -1, !is_spec_or_pause)
                                                    } else {
                                                        let info = ids
                                                            .iter()
                                                            .next()
                                                            .and_then(|id| {
                                                                self.base
                                                                    .char_new_id_to_legacy
                                                                    .get(id)
                                                                    .copied()
                                                            })
                                                            .and_then(|id| {
                                                                Self::player_info_mut(
                                                                    id,
                                                                    &self.base,
                                                                    &mut self.last_snapshot,
                                                                )
                                                            });
                                                        if let Some((char_id, info)) = info {
                                                            (
                                                                format!(
                                                                    "{} \"{}\"",
                                                                    prefix,
                                                                    info.player_info.name.as_str()
                                                                ),
                                                                self.base
                                                                    .char_new_id_to_legacy
                                                                    .get(&char_id)
                                                                    .copied()
                                                                    .unwrap_or(-1),
                                                                !is_spec_or_pause,
                                                            )
                                                        } else {
                                                            (
                                                                prefix.to_string(),
                                                                -1,
                                                                !is_spec_or_pause,
                                                            )
                                                        }
                                                    }
                                                }
                                            };
                                            if switch {
                                                player.sendg(Game::ClSay(game::ClSay {
                                                    team: false,
                                                    message: format!("/{pause}").as_bytes(),
                                                }));
                                            }
                                            player.sendg(Game::ClSetSpectatorMode(
                                                game::ClSetSpectatorMode {
                                                    spectator_id: spec_id,
                                                },
                                            ));
                                            player.flush();
                                        }
                                    }
                                    ClientToServerPlayerMessage::StartVote(msg) => {
                                        let get_player_legacy_id = |char_id: &CharacterId| {
                                            self.base
                                                .char_new_id_to_legacy
                                                .get(char_id)
                                                .copied()
                                                .unwrap_or(-1)
                                        };
                                        let (type_, value, reason) = match msg {
                                            VoteIdentifierType::Map(vote) => (
                                                "option".as_bytes(),
                                                self.base
                                                    .votes
                                                    .categories
                                                    .get(&vote.category)
                                                    .and_then(|c| {
                                                        c.iter().find_map(|name| {
                                                            let n = NetworkReducedAsciiString::from_str_lossy(name);
                                                            if n == vote.map.name
                                                            {
                                                                Some(name.clone())
                                                            } else {
                                                                None
                                                            }
                                                        })
                                                    })
                                                    .unwrap_or_default()
                                                    .to_string(),
                                                "".to_string(),
                                            ),
                                            VoteIdentifierType::RandomUnfinishedMap(vote) => (
                                                "option".as_bytes(),
                                                self.base
                                                    .votes
                                                    .categories
                                                    .get(&vote.category)
                                                    .and_then(|c| {
                                                        c.iter().find_map(|name| {
                                                            let n = name.to_ascii_lowercase();
                                                            if n.as_str().contains("random")
                                                                && n.as_str().contains("unfinished")
                                                                && n.as_str().contains("map")
                                                            {
                                                                Some(name.clone())
                                                            } else {
                                                                None
                                                            }
                                                        })
                                                    })
                                                    .unwrap_or_default()
                                                    .to_string(),
                                                vote.difficulty
                                                    .map(|d| d.get().to_string())
                                                    .unwrap_or_default(),
                                            ),
                                            VoteIdentifierType::VoteKickPlayer(vote) => (
                                                "kick".as_bytes(),
                                                get_player_legacy_id(&vote.voted_player_id).to_string(),
                                                vote.reason.to_string(),
                                            ),
                                            VoteIdentifierType::VoteSpecPlayer(vote) => (
                                                "spectate".as_bytes(),
                                                get_player_legacy_id(&vote.voted_player_id).to_string(),
                                                vote.reason.to_string(),
                                            ),
                                            VoteIdentifierType::Misc(vote) => (
                                                "option".as_bytes(),
                                                vote.vote_key.display_name.to_string(),
                                                "".to_string(),
                                            ),
                                        };
                                        player.sendg(Game::ClCallVote(game::ClCallVote {
                                            value: value.as_bytes(),
                                            reason: reason.as_bytes(),
                                            type_,
                                        }));
                                        player.flush();
                                    }
                                    ClientToServerPlayerMessage::Voted(voted) => {
                                        player.sendg(Game::ClVote(game::ClVote {
                                            vote: match voted {
                                                Voted::No => -1,
                                                Voted::Yes => 1,
                                            },
                                        }));
                                        player.flush();
                                    }
                                    ClientToServerPlayerMessage::Emoticon(msg) => {
                                        player.sendg(Game::ClEmoticon(game::ClEmoticon {
                                            emoticon: match msg {
                                                EmoticonType::OOP => enums::Emoticon::Oop,
                                                EmoticonType::EXCLAMATION => {
                                                    enums::Emoticon::Exclamation
                                                }
                                                EmoticonType::HEARTS => enums::Emoticon::Hearts,
                                                EmoticonType::DROP => enums::Emoticon::Drop,
                                                EmoticonType::DOTDOT => enums::Emoticon::Dotdot,
                                                EmoticonType::MUSIC => enums::Emoticon::Music,
                                                EmoticonType::SORRY => enums::Emoticon::Sorry,
                                                EmoticonType::GHOST => enums::Emoticon::Ghost,
                                                EmoticonType::SUSHI => enums::Emoticon::Sushi,
                                                EmoticonType::SPLATTEE => enums::Emoticon::Splattee,
                                                EmoticonType::DEVILTEE => enums::Emoticon::Deviltee,
                                                EmoticonType::ZOMG => enums::Emoticon::Zomg,
                                                EmoticonType::ZZZ => enums::Emoticon::Zzz,
                                                EmoticonType::WTF => enums::Emoticon::Wtf,
                                                EmoticonType::EYES => enums::Emoticon::Eyes,
                                                EmoticonType::QUESTION => enums::Emoticon::Question,
                                            },
                                        }));
                                        player.flush();
                                    }
                                    ClientToServerPlayerMessage::ChangeEyes { eye, duration } => {
                                        player.sendg(Game::ClSay(game::ClSay {
                                            team: false,
                                            message: format!(
                                                "/emote {} {}",
                                                match eye {
                                                    TeeEye::Normal => "normal",
                                                    TeeEye::Pain => "pain",
                                                    TeeEye::Happy => "happy",
                                                    TeeEye::Surprised => "surprise",
                                                    TeeEye::Angry => "angry",
                                                    TeeEye::Blink => "blink",
                                                },
                                                duration.as_secs().clamp(0, 99999)
                                            )
                                            .as_bytes(),
                                        }));
                                        player.flush();
                                    }
                                    ClientToServerPlayerMessage::JoinStage(msg) => {
                                        let is_spectator = self
                                            .last_snapshot
                                            .spectator_players
                                            .contains_key(&player_id);
                                        if matches!(msg, JoinStage::Default) && is_spectator {
                                            player.sendg(Game::ClSetTeam(game::ClSetTeam {
                                                team: Team::Red,
                                            }));
                                            player.flush();
                                        } else {
                                            let team = match msg {
                                                JoinStage::Default => "0".to_string(),
                                                JoinStage::Others(name) => {
                                                    let team_index: Option<i32> = name.parse().ok();
                                                    if team_index.is_some() {
                                                        name.to_string()
                                                    } else if let Some((_, _, index)) = self
                                                        .base
                                                        .own_teams
                                                        .values()
                                                        .find(|(n, _, _)| *n == name)
                                                    {
                                                        index.to_string()
                                                    } else {
                                                        "".to_string()
                                                    }
                                                }
                                                JoinStage::Own { name, color } => {
                                                    let mut likely_teams: BTreeSet<i32> =
                                                        Default::default();

                                                    self.base.teams.values().for_each(|(id, _)| {
                                                        if *id != 0 {
                                                            likely_teams.insert(*id);
                                                        }
                                                    });
                                                    let mut likely_team_index = 1;
                                                    for i in 1..256 {
                                                        if !likely_teams.contains(&i) {
                                                            likely_team_index = i;
                                                            break;
                                                        }
                                                    }
                                                    self.base.own_teams.insert(
                                                        player_id,
                                                        (
                                                            name,
                                                            ubvec4::new(
                                                                color[0], color[1], color[2], 20,
                                                            ),
                                                            likely_team_index,
                                                        ),
                                                    );
                                                    "-1".to_string()
                                                }
                                            };

                                            player.sendg(Game::ClSay(game::ClSay {
                                                team: false,
                                                message: format!("/team {team}").as_bytes(),
                                            }));
                                            player.flush();
                                        }
                                    }
                                    ClientToServerPlayerMessage::JoinVanillaSide(msg) => {
                                        player.sendg(Game::ClSetTeam(game::ClSetTeam {
                                            team: match msg {
                                                MatchSide::Red => Team::Red,
                                                MatchSide::Blue => Team::Blue,
                                            },
                                        }));
                                        player.flush();
                                    }
                                    ClientToServerPlayerMessage::UpdateCharacterInfo {
                                        info,
                                        ..
                                    } => {
                                        player.data.player_info = *info;
                                        let info =
                                            Self::player_info_to_legacy(&player.data.player_info);
                                        player.socket.sendg(Game::ClChangeInfo(
                                            game::ClChangeInfo {
                                                name: info.name,
                                                clan: info.clan,
                                                country: info.country,
                                                skin: info.skin,
                                                use_custom_color: info.use_custom_color,
                                                color_body: info.color_body,
                                                color_feet: info.color_feet,
                                            },
                                        ));
                                        player.flush();
                                    }
                                    ClientToServerPlayerMessage::RconExec { ident_text, args } => {
                                        debug!(
                                            "[NOT IMPLEMENTED] rcon exec: {ident_text:?} {args:?}"
                                        );
                                    }
                                }
                            }
                        }
                        ClientToServerMessage::Inputs {
                            id,
                            inputs,
                            snap_ack,
                        } => {
                            let mut highest_intended_tick = 0;

                            for (player_id, inp_chain) in inputs.iter() {
                                let mut highest_player_intended_tick = 0;
                                if let Some(player) = self.players.get_mut(player_id) {
                                    let Some(def_inp) = (if let Some(diff_id) = inp_chain.diff_id {
                                        player
                                            .data
                                            .server_client
                                            .input_storage
                                            .get(&diff_id)
                                            .copied()
                                    } else {
                                        Some(PlayerInputChainable::default())
                                    }) else {
                                        log::debug!(target: "server", "had to drop an input from the client for diff id: {:?}", inp_chain.diff_id);
                                        continue;
                                    };

                                    let mut def = self.base.input_deser.new();
                                    let def_len = bincode::serde::encode_into_std_write(
                                        def_inp,
                                        &mut *def,
                                        bincode::config::standard().with_fixed_int_encoding(),
                                    )
                                    .unwrap();
                                    let mut old = def;
                                    let mut offset = 0;

                                    let mut latest_inp = None;
                                    while let Some(patch) =
                                        inp_chain.data.get(offset..offset + def_len)
                                    {
                                        let mut new = self.base.input_deser.new();
                                        bin_patch::patch_exact_size(&old, patch, &mut new).unwrap();

                                        if let Ok((inp, _)) = bincode::serde::decode_from_slice::<
                                            PlayerInputChainable,
                                            _,
                                        >(
                                            &new,
                                            bincode::config::standard()
                                                .with_fixed_int_encoding()
                                                .with_limit::<{ 1024 * 1024 * 4 }>(),
                                        ) {
                                            let as_diff = inp_chain.as_diff;
                                            if as_diff {
                                                // this should be higher than the number of inputs saved on the client
                                                // (since reordering of packets etc.)
                                                while player.data.server_client.input_storage.len()
                                                    >= 50
                                                {
                                                    player
                                                        .data
                                                        .server_client
                                                        .input_storage
                                                        .pop_first();
                                                }
                                                player
                                                    .data
                                                    .server_client
                                                    .input_storage
                                                    .insert(id, inp);
                                            }

                                            let pred_tick_diff = (inp
                                                .for_monotonic_tick
                                                .saturating_sub(self.base.cur_monotonic_tick))
                                                as i32;
                                            let intended_tick =
                                                self.base.last_snap_tick + pred_tick_diff;
                                            latest_inp = Some(inp.inp.inp);
                                            highest_intended_tick =
                                                highest_intended_tick.max(intended_tick);
                                            highest_player_intended_tick =
                                                highest_player_intended_tick.max(intended_tick);
                                        }

                                        offset += def_len;
                                        old = new;
                                    }
                                    if let Some(latest_inp) = latest_inp {
                                        player.data.latest_input = Self::input_to_legacy_input(
                                            *player_id,
                                            &self.last_snapshot,
                                            &self.base,
                                            &player.data.latest_inputs,
                                            highest_player_intended_tick,
                                            &player.data.latest_input,
                                            &latest_inp,
                                            latest_inp
                                                .consumable
                                                .diff(&player.data.latest_char_input.consumable),
                                        );
                                        player.data.latest_char_input = latest_inp;
                                        while player.data.latest_inputs.len()
                                            > TICKS_PER_SECOND as usize * 5
                                        {
                                            player.data.latest_inputs.pop_first();
                                        }
                                        player.data.latest_inputs.insert(
                                            highest_player_intended_tick,
                                            (
                                                player.data.latest_char_input,
                                                player.data.latest_input,
                                            ),
                                        );
                                        player.sends(System::Input(system::Input {
                                            ack_snapshot: self.base.last_snap_tick,
                                            intended_tick: highest_player_intended_tick,
                                            input_size: std::mem::size_of::<snap_obj::PlayerInput>()
                                                as i32,
                                            input: player.data.latest_input,
                                        }));
                                        player.flush();
                                    }
                                }
                            }

                            // at least ack the snapshot
                            if inputs.is_empty() {
                                warn!("Did not get player inputs this tick.");
                            } else {
                                // add ack early to make the timing more accurate
                                self.base
                                    .inputs_to_ack
                                    .entry(highest_intended_tick)
                                    .or_insert((
                                        MsgSvInputAck {
                                            id,
                                            // reuse this field this one time
                                            logic_overhead: timestamp,
                                        },
                                        false,
                                    ));
                            }

                            for &MsgClSnapshotAck { snap_id } in snap_ack.iter() {
                                if let Some(snap) = self.base.client_snap_storage.remove(&snap_id) {
                                    self.base.latest_client_snap = Some(ClientSnapshotForDiff {
                                        snap_id,
                                        snapshot: snap.snapshot,
                                        monotonic_tick: snap.monotonic_tick,
                                    });
                                }
                                while self
                                    .base
                                    .client_snap_storage
                                    .first_entry()
                                    .is_some_and(|entry| *entry.key() < snap_id)
                                {
                                    self.base.client_snap_storage.pop_first();
                                }
                            }
                        }
                        ClientToServerMessage::LoadVotes(msg) => {
                            let votes = match msg {
                                MsgClLoadVotes::Map { .. } => {
                                    self.base.loaded_map_votes = true;
                                    MsgSvLoadVotes::Map {
                                        categories: self
                                            .base
                                            .votes
                                            .categories
                                            .iter()
                                            .map(|(c_name, c)| {
                                                let votes = c
                                                    .iter()
                                                    .map(|v_name| {
                                                        type N<const SIZE: usize> =
                                                            NetworkReducedAsciiString<SIZE>;
                                                        let name = N::from_str_lossy(v_name);
                                                        let key = MapVoteKey { name, hash: None };
                                                        let vote = MapVote {
                                                            thumbnail_resource: None,
                                                            details: MapVoteDetails::None,
                                                            is_default_map: false,
                                                        };
                                                        (key, vote)
                                                    })
                                                    .collect();
                                                (c_name.clone(), votes)
                                            })
                                            .collect(),
                                        has_unfinished_map_votes: self
                                            .base
                                            .votes
                                            .has_unfinished_map_votes,
                                    }
                                }
                                MsgClLoadVotes::Misc { .. } => {
                                    self.base.loaded_misc_votes = true;
                                    MsgSvLoadVotes::Misc {
                                        votes: self
                                            .base
                                            .votes
                                            .categories
                                            .iter()
                                            .map(|(c_name, c)| {
                                                (
                                                    c_name.clone(),
                                                    c.iter()
                                                        .map(|v_name| {
                                                            (
                                                                MiscVoteKey {
                                                                    display_name:
                                                                        NetworkString::new_lossy(
                                                                            v_name.as_str(),
                                                                        ),
                                                                    description: Default::default(),
                                                                },
                                                                MiscVote {
                                                                    command: Default::default(),
                                                                },
                                                            )
                                                        })
                                                        .collect(),
                                                )
                                            })
                                            .collect(),
                                    }
                                }
                            };

                            self.server_network.send_in_order_to(
                                &ServerToClientMessage::LoadVotes(votes),
                                &con_id,
                                NetworkInOrderChannel::Global,
                            );
                        }
                        ClientToServerMessage::AccountChangeName { .. } => {}
                        ClientToServerMessage::AccountRequestInfo => {}
                        ClientToServerMessage::SpatialChat { .. } => {}
                        ClientToServerMessage::SpatialChatDeactivated => {}
                    },
                }
            }
            self.server_has_new_events
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }

    fn handle_server_events_and_sleep(&mut self) -> anyhow::Result<()> {
        if let Some(con_id) = self.con_id {
            let mut event_handler = |socket: &mut SocketClient,
                                     ev: libtw2_net::net::ChunkOrEvent<'_, SocketAddr>,
                                     base: &mut ClientBase,
                                     collisions: &mut Option<Box<Collision>>,
                                     player_id: PlayerId,
                                     player_data: &mut ClientData,
                                     is_active_connection: bool,
                                     is_main_connection: bool| {
                use libtw2_net::net::ChunkOrEvent::*;
                match ev {
                    Chunk(c) => Self::on_packet(
                        player_id,
                        player_data,
                        socket,
                        &self.time,
                        &self.io,
                        &self.server_network,
                        con_id,
                        base,
                        &self.log,
                        c.pid,
                        c.data,
                        collisions,
                        &self.connect_addr,
                        &mut self.last_snapshot,
                        is_active_connection,
                        is_main_connection,
                    ),
                    Connless(data) => {
                        if let ClientState::RequestedLegacyServerInfo { token, .. } =
                            &player_data.state
                        {
                            self.log.log("Recevied connectionless packet.");
                            if self.connect_addr == data.addr {
                                player_data.ready.received_server_info =
                                    player_data.ready.received_server_info.clone().or(
                                        Self::on_connless_packet(
                                            &[*token],
                                            self.connect_addr,
                                            data.data,
                                        ),
                                    );
                            }
                        }
                    }
                    Ready(_) => {
                        self.log.log("Proxy client ready, sending info.");
                        socket.sends(System::Info(system::Info {
                            version: VERSION.as_bytes(),
                            password: Some(base.join_password.as_bytes()),
                        }));
                        socket.flush();
                    }
                    Disconnect(_, reason) => {
                        let reason = String::from_utf8_lossy(reason).to_string();
                        self.log
                            .log(format!("Proxy client got disconnected: {reason}"));
                        socket.skip_disconnect_on_drop = true;
                        if is_main_connection {
                            self.server_network.kick(&con_id, KickType::Kick(reason));
                        }
                    }
                    Connect(_) => {
                        // ignore
                    }
                }
            };
            fn calc_is_active_connection(player_data: &ClientData) -> bool {
                !player_data
                    .latest_char_input
                    .state
                    .input_method_flags
                    .contains(CharacterInputMethodFlags::DUMMY)
            }

            let mut is_active_connection = None;
            // rev here, so the first dummy without dummy input is active connection
            for (index, (&player_id, player)) in self.players.iter_mut().enumerate().rev() {
                let is_main_connection = index == 0;
                is_active_connection = match is_active_connection {
                    Some((_, player_id)) => Some((false, player_id)),
                    None => calc_is_active_connection(&player.data).then_some((true, player_id)),
                };
                let is_active_connection = is_active_connection.map(|(b, _)| b).unwrap_or_default();
                player.socket.run_once(|socket, ev| {
                    event_handler(
                        socket,
                        ev,
                        &mut self.base,
                        &mut self.collisions,
                        player_id,
                        &mut player.data,
                        is_active_connection,
                        is_main_connection,
                    )
                });

                if let ClientState::MapReady { name, hash } = &mut player.data.state {
                    if matches!(self.base.server_info, ServerInfoTy::Partial { .. }) {
                        self.log
                            .log("Proxy client only has partial server info, requesting full.");
                        let token = rand::rng().next_u32() as u8;
                        player.socket.sendc(
                            self.connect_addr,
                            Connless::RequestInfo(msg::connless::RequestInfo { token }),
                        );
                        player.data.state = ClientState::RequestedLegacyServerInfo {
                            name: std::mem::take(name),
                            hash: *hash,
                            token,
                        };
                    } else {
                        player.data.state = ClientState::ReceivedLegacyServerInfo {
                            name: std::mem::take(name),
                            hash: *hash,
                        };
                    }
                }
                if let ClientState::RequestedLegacyServerInfo { name, hash, .. } =
                    &mut player.data.state
                    && let Some(server_info) = player.data.ready.received_server_info.take()
                {
                    self.base.server_info = ServerInfoTy::Full(server_info);
                    player.data.state = ClientState::ReceivedLegacyServerInfo {
                        name: std::mem::take(name),
                        hash: *hash,
                    };
                }
                if let ClientState::ReceivedLegacyServerInfo { name, hash } = &mut player.data.state
                {
                    let mut send_server_info_and_prepare_map_download =
                        |map_name: ReducedAsciiString, hash: &Hash| {
                            if !is_main_connection {
                                return;
                            }

                            self.log.log(format!(
                                "Client proxy is converting map: {}. \
                                This might take a moment.",
                                map_name.as_str()
                            ));
                            let (map, resources) = ServerMap::legacy_to_new(
                                Some("downloaded".as_ref()),
                                &self.tp,
                                &self.io,
                                &NetworkReducedAsciiString::try_from(map_name.clone()).unwrap(),
                                Some(hash),
                                anyhow!("The legacy proxy always loads legacy maps."),
                            )
                            .unwrap();

                            let (phy_group, _) = Map::read_physics_group_and_config(
                                &MapFileReader::new(map.clone()).unwrap(),
                            )
                            .unwrap();

                            let new_collision = Collision::new(phy_group, true).unwrap();

                            self.log.log("Client proxy prepares map collision");
                            self.collisions = Some(new_collision);

                            let map_hash = generate_hash_for(&map);

                            self.log.log("Client proxy prepares http download server");
                            let http_server = ServerGame::prepare_download_server(
                                map_name.as_str(),
                                map_hash,
                                &map,
                                &resources,
                                [].into_iter(),
                                Default::default(),
                                0,
                                0,
                            )
                            .unwrap();

                            let first_connect = self.http_server.is_none();

                            let ServerInfoTy::Full(server_info) = &self.base.server_info else {
                                panic!("server info not received, bug in code.");
                            };
                            let game_type = server_info.game_type.to_lowercase();
                            let is_race = game_type == "race"
                                || game_type.contains("ddrace")
                                || game_type.contains("block")
                                || game_type == "gores";
                            let is_race = is_race && self.base.capabilities.is_ddnet;

                            let server_info = MsgSvServerInfo {
                                map: map_name.try_into().unwrap(),
                                map_blake3_hash: map_hash,
                                required_resources: Default::default(),
                                game_mod: GameModification::Ddnet,
                                render_mod: RenderModification::Native,
                                mod_config: None,
                                resource_server_fallback: Some(http_server.port_v4),
                                hint_start_camera_pos: Default::default(),
                                server_options: GameStateServerOptions {
                                    forced_ingame_camera_zoom: (!is_race)
                                        .then_some(FixedZoomLevel::new_lossy(1.0)),
                                    allow_stages: is_race,
                                    has_ingame_freecam: is_race,
                                    ..Default::default()
                                },
                                spatial_chat: false,
                                send_input_every_tick: true,
                            };

                            self.http_server = Some(http_server);

                            if first_connect {
                                self.server_network.send_unordered_to(
                                    &ServerToClientMessage::ServerInfo {
                                        info: server_info,
                                        overhead: self
                                            .time
                                            .now()
                                            .saturating_sub(player.data.connect_time),
                                    },
                                    &con_id,
                                );
                            } else {
                                self.server_network.send_unordered_to(
                                    &ServerToClientMessage::Load(server_info),
                                    &con_id,
                                );
                            }
                        };
                    send_server_info_and_prepare_map_download(std::mem::take(name), hash);

                    player.data.state = ClientState::SentServerInfo;
                    self.log.log(
                        "Proxy client received all information required \
                        and sent the server info to the real client.",
                    );
                }
                if player.data.ready.con
                    && player.data.ready.client_con
                    && matches!(player.data.state, ClientState::SentServerInfo)
                {
                    player
                        .socket
                        .sendg(Game::ClStartInfo(Self::player_info_to_legacy(
                            &player.data.player_info,
                        )));
                    player.flush();
                    player.data.state = ClientState::StartInfoSent;
                    player.data.ready.con = false;
                    player.data.ready.client_con = false;
                    self.log.log("Proxy client sent start info (player info).");
                }
            }

            // check if local players are not connected anymore
            self.base
                .local_players
                .retain(|id, _| self.base.confirmed_player_ids.contains(id));

            if !self.base.events.worlds.is_empty() {
                let mut events = self.base.events.clone();
                events.event_id = self.base.event_id_generator.peek_next_id();
                self.base.events.worlds.clear();
                self.server_network.send_in_order_to(
                    &ServerToClientMessage::Events {
                        // sub by one since most servers send snapshots only
                        // every second tick
                        game_monotonic_tick: self.base.cur_monotonic_tick.saturating_sub(1),
                        events,
                    },
                    &con_id,
                    NetworkInOrderChannel::Global,
                );
            }

            if self.base.vote_list_updated
                && (self.base.loaded_map_votes || self.base.loaded_misc_votes)
            {
                if self.base.loaded_map_votes {
                    self.server_network.send_in_order_to(
                        &ServerToClientMessage::ResetVotes(MsgSvResetVotes::Map),
                        &con_id,
                        NetworkInOrderChannel::Global,
                    );
                    self.base.loaded_map_votes = false;
                }
                if self.base.loaded_misc_votes {
                    self.server_network.send_in_order_to(
                        &ServerToClientMessage::ResetVotes(MsgSvResetVotes::Misc),
                        &con_id,
                        NetworkInOrderChannel::Global,
                    );
                    self.base.loaded_misc_votes = false;
                }
                self.base.vote_list_updated = false;
            }

            // Update emoticons (a.k.a. drop them)
            let cur_time = self.time.now();
            self.base.emoticons.retain(|_, (start_time, _)| {
                cur_time.saturating_sub(*start_time) <= Duration::from_secs(2)
            });

            let net = self.notifier_server.clone();
            let finish_notify = self.finish_notifier.clone();
            let receivers: Vec<_> = self
                .players
                .values()
                .map(|p| p.socket.socket.receivers())
                .collect();
            let (pkt, index) = self
                .io
                .rt
                .spawn(async move {
                    type SockFuture =
                        Pin<Box<dyn Future<Output = Option<(Vec<u8>, SocketAddr)>> + Send>>;
                    let mut futures: Vec<SockFuture> = vec![
                        Box::pin(async move {
                            net.wait_for_event_async(None).await;
                            None
                        }),
                        Box::pin(async move {
                            finish_notify.notified().await;
                            None
                        }),
                    ];
                    futures.extend(receivers.into_iter().map(|(v4, v6)| {
                        let future: SockFuture =
                            Box::pin(async move { Socket::recv_from(v4, v6).await });
                        future
                    }));
                    let (res, index, _) = futures::future::select_all(futures).await;
                    Ok((res, index))
                })
                .get()
                .unwrap();
            if let Some((data, addr)) = pkt
                && index > 1
            {
                let index = index - 2;

                let other_active = if index > 0 {
                    self.players
                        .values()
                        .take(index - 1)
                        .any(|p| calc_is_active_connection(&p.data))
                } else {
                    false
                };

                let (player_id, player) = self.players.iter_mut().nth(index).unwrap();
                player.socket.run_recv((addr, data), &mut |socket, ev| {
                    let is_active_connection =
                        !other_active && calc_is_active_connection(&player.data);
                    event_handler(
                        socket,
                        ev,
                        &mut self.base,
                        &mut self.collisions,
                        *player_id,
                        &mut player.data,
                        is_active_connection,
                        index == 0,
                    )
                });
            }
        } else {
            let notify = self.notifier_server.clone();
            let finish_notify = self.finish_notifier.clone();
            let _ = self
                .io
                .rt
                .spawn(async move {
                    tokio::select! {
                        _ = notify.wait_for_event_async(None) => {}
                        _ = finish_notify.notified() => {}
                    }
                    Ok(())
                })
                .get();
        }

        Ok(())
    }

    fn run_once(&mut self) -> anyhow::Result<()> {
        self.handle_client_events()?;

        self.handle_server_events_and_sleep()?;

        // do 10 pings per second to determine accurate ping
        let time_now = self.time.now();
        if self.base.last_ping.is_none_or(|last_ping| {
            time_now.saturating_sub(last_ping) > Duration::from_millis(1000 / 10)
        }) {
            self.base.last_ping = Some(time_now);

            if let Some(player) = self.players.values_mut().next()
                && matches!(player.data.state, ClientState::Ingame)
            {
                let pkt = system::PingEx {
                    id: hex::encode(self.base.last_ping_uuid.to_ne_bytes())
                        .parse()
                        .unwrap(),
                };
                player.socket.sends(System::PingEx(pkt));
                player.socket.flush();

                self.base
                    .last_pings
                    .insert(self.base.last_ping_uuid, time_now);
                while self.base.last_pings.len() > 50 {
                    self.base.last_pings.pop_first();
                }
                self.base.last_ping_uuid += 1;
            }
        }

        Ok(())
    }
}

pub fn proxy_run(
    io: &Io,
    time: &base::steady_clock::SteadyClock,
    addr: SocketAddr,
    log: ConnectingLog,
) -> anyhow::Result<LegacyProxy> {
    Client::run(io, time, addr, log)
}
