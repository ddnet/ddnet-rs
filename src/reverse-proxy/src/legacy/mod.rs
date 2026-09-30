//! Legacy protocol translation belongs to the reverse proxy, never to the backend.
mod browser;
mod events;
mod input;
mod maps;
mod snapshot;
mod tuning;
mod votes;

use crate::{config::Config, relay::backend_options};
use anyhow::{Context, ensure};
use arrayvec::ArrayVec;
use base::{network_string::NetworkString, steady_clock::SteadyClock};
use base_io_traits::fs_traits::FileSystemInterface;
use game_base::network::messages::*;
use game_interface::types::{character_info::NetworkCharacterInfo, id_types::PlayerId};
use game_network::{
    game_event_generator::{GameEventGenerator, GameEvents},
    messages::{
        ClientToServerMessage as ClientMsg, ClientToServerPlayerMessage as PlayerMsg,
        ServerToClientMessage as ServerMsg,
    },
};
use libtw2_gamenet_ddnet::msg::{Game, System, game, system};
use libtw2_net::{
    Net,
    net::{Callback, Chunk, ChunkOrEvent, PeerId},
};
use libtw2_packer::{Unpacker, with_packer};
use network::network::{
    event::NetworkEvent, packet_compressor::DefaultNetworkPacketCompressor,
    plugins::NetworkPlugins, quinn_network::QuinnNetwork, types::NetworkInOrderChannel,
};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

struct Ignore;
impl<T> warn::Warn<T> for Ignore {
    fn warn(&mut self, _: T) {}
}

use legacy_proxy::socket::Socket;

struct Client {
    backend: QuinnNetwork,
    notifier: network::network::notifier::NetworkEventNotifier,
    events: Arc<GameEventGenerator<ServerMsg<'static>>>,
    player: Option<PlayerId>,
    character: Option<NetworkCharacterInfo>,
    ready_sent: bool,
    legacy_ready: bool,
    transport_ready: bool,
    entered: bool,
    password: String,
    map: Option<Arc<maps::LegacyMap>>,
    resource_url: Option<url::Url>,
    map_task: Option<tokio::task::JoinHandle<anyhow::Result<Arc<maps::LegacyMap>>>>,
    map_result: Option<anyhow::Result<Arc<maps::LegacyMap>>>,
    snapshots: snapshot::Snapshots,
    inputs: input::Inputs,
    input_id: u64,
    pending_input_ticks: std::collections::BTreeMap<u64, (i32, u64)>,
    old_snaps: std::collections::BTreeMap<i32, libtw2_snapshot::snap::Snap>,
    ack: i32,
    tick_origin: Option<u64>,
    last_input: Instant,
    last_packet: Instant,
    tuning: Vec<u8>,
    effects: Vec<libtw2_gamenet_ddnet::snap_obj::SnapObj>,
    votes: HashMap<String, game_interface::votes::VoteIdentifierType>,
    vote_queue: std::collections::VecDeque<String>,
}

impl Client {
    fn send(&self, msg: ClientMsg<'_>) {
        self.backend
            .send_in_order_to_server(&msg, NetworkInOrderChannel::Global);
    }

    async fn wait_for_work(&mut self) {
        tokio::select! {
            _ = self.notifier.wait_for_event_async(None) => {},
            result = async {
                match &mut self.map_task {
                    Some(task) => task.await,
                    None => std::future::pending().await,
                }
            } => {
                self.map_task = None;
                self.map_result = Some(result.map_err(anyhow::Error::from).and_then(|r| r));
            }
        }
    }

    fn player_msg(&self, msg: PlayerMsg<'_>) {
        if let Some(player) = self.player {
            self.send(ClientMsg::PlayerMsg((player, msg)));
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(task) = self.map_task.take() {
            task.abort();
        }
    }
}

fn sends(
    net: &mut Net<SocketAddr>,
    socket: &mut Socket,
    pid: PeerId,
    msg: impl Into<System<'static>>,
) -> anyhow::Result<()> {
    send_system(net, socket, pid, msg.into(), true)
}

fn send_system(
    net: &mut Net<SocketAddr>,
    socket: &mut Socket,
    pid: PeerId,
    msg: System<'_>,
    vital: bool,
) -> anyhow::Result<()> {
    let mut buf: ArrayVec<[u8; 2048]> = ArrayVec::new();
    with_packer(&mut buf, |p| msg.encode(p))
        .map_err(|e| anyhow::anyhow!("legacy message: {e:?}"))?;
    net.send(
        socket,
        Chunk {
            pid,
            vital,
            data: &buf,
        },
    )
    .map_err(|e| anyhow::anyhow!("send: {e:?}"))?;
    Ok(())
}

fn send_game(
    net: &mut Net<SocketAddr>,
    socket: &mut Socket,
    pid: PeerId,
    msg: impl Into<Game<'static>>,
) -> anyhow::Result<()> {
    send_game_ref(net, socket, pid, msg.into())
}

fn send_game_ref(
    net: &mut Net<SocketAddr>,
    socket: &mut Socket,
    pid: PeerId,
    msg: Game<'_>,
) -> anyhow::Result<()> {
    let mut buf: ArrayVec<[u8; 2048]> = ArrayVec::new();
    with_packer(&mut buf, |p| msg.encode(p))
        .map_err(|e| anyhow::anyhow!("legacy message: {e:?}"))?;
    net.send(
        socket,
        Chunk {
            pid,
            vital: true,
            data: &buf,
        },
    )
    .map_err(|e| anyhow::anyhow!("send: {e:?}"))?;
    Ok(())
}

fn ready(client: &mut Client) {
    if !client.ready_sent
        && client.legacy_ready
        && client.map.is_some()
        && let Some(character) = &client.character
    {
        client.send(ClientMsg::Ready(MsgClReady {
            players: vec![MsgClAddLocalPlayer {
                player_info: character.clone(),
                id: 0,
            }],
            rcon_secret: None,
        }));
        client.ready_sent = true;
    }
}

fn text<const N: usize>(bytes: &[u8]) -> NetworkString<N> {
    NetworkString::new_lossy(String::from_utf8_lossy(bytes).into_owned())
}

fn chunk(
    client: &mut Client,
    net: &mut Net<SocketAddr>,
    socket: &mut Socket,
    pid: PeerId,
    data: &[u8],
    vital: bool,
) -> anyhow::Result<()> {
    client.last_packet = Instant::now();
    client.transport_ready = true;
    if let Ok(msg) = System::decode(&mut Ignore, &mut Unpacker::new(data)) {
        if !vital && !matches!(msg, System::Input(_) | System::Ping(_) | System::PingEx(_)) {
            return Ok(());
        }
        match msg {
            System::Info(info) => {
                ensure!(
                    info.version == libtw2_gamenet_ddnet::enums::VERSION.as_bytes(),
                    "unsupported legacy protocol version"
                );
                client.password =
                    String::from_utf8_lossy(info.password.unwrap_or_default()).into_owned();
                client.send(ClientMsg::PasswordResponse(text(
                    info.password.unwrap_or_default(),
                )));
            }
            System::RequestMapData(req) => {
                if let Some(map) = &client.map
                    && let Ok(chunk) = usize::try_from(req.chunk)
                    && let Some(start) = chunk.checked_mul(896)
                    && start < map.bytes.len()
                {
                    let end = (start + 896).min(map.bytes.len());
                    send_system(
                        net,
                        socket,
                        pid,
                        system::MapData {
                            last: i32::from(end == map.bytes.len()),
                            crc: map.crc,
                            chunk: req.chunk,
                            data: &map.bytes[start..end],
                        }
                        .into(),
                        true,
                    )?;
                }
            }
            System::Ready(_) => {
                if client.map.is_some() {
                    sends(net, socket, pid, system::ConReady)?;
                    client.legacy_ready = true;
                    ready(client);
                }
            }
            System::EnterGame(_) => {
                client.entered = client.player.is_some();
            }
            System::Ping(_) => sends(net, socket, pid, system::PingReply)?,
            System::PingEx(p) => sends(net, socket, pid, system::PongEx { id: p.id })?,
            System::RconAuth(_) => {
                sends(
                    net,
                    socket,
                    pid,
                    system::RconAuthStatus {
                        auth_level: Some(0),
                        receive_commands: Some(0),
                    },
                )?;
                sends(net,socket,pid,system::RconLine{line:b"Remote console authentication is unavailable through the legacy proxy."})?;
            }
            System::WhatIs(p) => sends(net, socket, pid, system::IDontKnow { uuid: p.uuid })?,
            System::Input(inp) => {
                if !client.entered {
                    return Ok(());
                }
                if client.old_snaps.contains_key(&inp.ack_snapshot) {
                    client.ack = inp.ack_snapshot;
                }
                let Some(origin) = client.tick_origin else {
                    return Ok(());
                };
                let Some(tick) = u64::try_from(inp.intended_tick)
                    .ok()
                    .and_then(|t| origin.checked_add(t))
                else {
                    return Ok(());
                };
                if tick.abs_diff(client.snapshots.latest_tick) > 150 {
                    return Ok(());
                }
                if let Some(input) = client.inputs.translate(inp.intended_tick, inp.input) {
                    let chain = PlayerInputChainable {
                        inp: input,
                        for_monotonic_tick: tick,
                    };
                    let config = bincode::config::standard().with_fixed_int_encoding();
                    let baseline =
                        bincode::serde::encode_to_vec(PlayerInputChainable::default(), config)?;
                    let bytes = bincode::serde::encode_to_vec(chain, config)?;
                    let mut patch = Vec::new();
                    bin_patch::diff_exact_size(&baseline, &bytes, &mut patch)?;
                    let mut inputs = pool::mt_datatypes::PoolFxLinkedHashMap::new_without_pool();
                    inputs.insert(
                        client.player.unwrap(),
                        MsgClInputPlayerChain {
                            data: pool::mt_datatypes::PoolVec::from_without_pool(patch),
                            diff_id: None,
                            as_diff: false,
                        },
                    );
                    let ack = client
                        .snapshots
                        .latest_id
                        .map(|snap_id| MsgClSnapshotAck { snap_id })
                        .into_iter()
                        .collect::<Vec<_>>();
                    client.input_id += 1;
                    client
                        .pending_input_ticks
                        .insert(client.input_id, (inp.intended_tick, tick));
                    // Bound outstanding inputs if acknowledgements are lost.
                    while client.pending_input_ticks.len() > 256 {
                        client.pending_input_ticks.pop_first();
                    }
                    client
                        .backend
                        .send_unordered_auto_to_server(&ClientMsg::Inputs {
                            id: client.input_id,
                            inputs,
                            snap_ack: ack.as_slice().into(),
                        });
                    client.last_input = Instant::now();
                }
            }
            _ => {}
        }
    } else if vital && let Ok(msg) = Game::decode(&mut Ignore, &mut Unpacker::new(data)) {
        match msg {
            Game::ClStartInfo(info) => {
                if client.character.is_none() {
                    let mut character = NetworkCharacterInfo::explicit_default();
                    character.name = text(info.name);
                    character.clan = text(info.clan);
                    character.skin_info =
                        skin_info(info.use_custom_color, info.color_body, info.color_feet);
                    character.skin = String::from_utf8_lossy(info.skin)
                        .as_ref()
                        .try_into()
                        .unwrap_or_default();
                    client.character = Some(character);
                    ready(client);
                }
            }
            Game::ClChangeInfo(info) => {
                if let Some(character) = &mut client.character {
                    character.name = text(info.name);
                    character.clan = text(info.clan);
                    character.skin_info =
                        skin_info(info.use_custom_color, info.color_body, info.color_feet);
                    character.skin =
                        game_interface::types::resource_key::NetworkResourceKey::from_str_lossy(
                            &String::from_utf8_lossy(info.skin),
                        );
                    let character = character.clone();
                    client.input_id += 1;
                    client.player_msg(PlayerMsg::UpdateCharacterInfo {
                        version: std::num::NonZeroU64::new(client.input_id).unwrap(),
                        info: Box::new(character),
                    });
                }
            }
            Game::ClVote(v) => {
                if v.vote != 0 {
                    client.player_msg(PlayerMsg::Voted(if v.vote > 0 {
                        game_interface::votes::Voted::Yes
                    } else {
                        game_interface::votes::Voted::No
                    }));
                }
            }
            Game::ClCallVote(v) => {
                use game_interface::votes::{PlayerVoteKey, VoteIdentifierType};
                let vote = match v.type_ {
                    b"option" => client
                        .votes
                        .get(String::from_utf8_lossy(v.value).as_ref())
                        .cloned(),
                    b"kick" | b"spectate" => String::from_utf8_lossy(v.value)
                        .parse::<u16>()
                        .ok()
                        .and_then(|id| {
                            client
                                .snapshots
                                .players
                                .iter()
                                .find(|(_, v)| **v == id)
                                .map(|(id, _)| *id)
                        })
                        .map(|id| {
                            let key = PlayerVoteKey {
                                voted_player_id: id,
                                reason: text(v.reason),
                            };
                            if v.type_ == b"kick" {
                                VoteIdentifierType::VoteKickPlayer(key)
                            } else {
                                VoteIdentifierType::VoteSpecPlayer(key)
                            }
                        }),
                    _ => None,
                };
                if let Some(vote) = vote {
                    client.player_msg(PlayerMsg::StartVote(vote));
                }
            }
            Game::ClEmoticon(e) => {
                use game_interface::types::emoticons::{EmoticonType as E, IntoEnumIterator};
                if let Some(emoticon) = E::iter().nth(e.emoticon as usize) {
                    client.player_msg(PlayerMsg::Emoticon(emoticon));
                }
            }
            Game::ClSetSpectatorMode(mode) => {
                let ids = client
                    .snapshots
                    .players
                    .iter()
                    .filter(|(_, id)| **id as i32 == mode.spectator_id)
                    .map(|(id, _)| *id)
                    .collect();
                client.player_msg(PlayerMsg::SwitchToCamera(
                    game_interface::client_commands::ClientCameraMode::FreeCam(ids),
                ));
            }
            Game::ClKill(_) => client.player_msg(PlayerMsg::Kill),
            Game::ClSay(msg) => client.player_msg(PlayerMsg::Chat(if msg.team {
                MsgClChatMsg::GameTeam {
                    msg: text(msg.message),
                }
            } else {
                MsgClChatMsg::Global {
                    msg: text(msg.message),
                }
            })),
            Game::ClSetTeam(msg) => {
                if msg.team == libtw2_gamenet_ddnet::enums::Team::Spectators {
                    client.player_msg(PlayerMsg::JoinSpectator);
                } else {
                    client.player_msg(PlayerMsg::JoinStage(
                        game_interface::client_commands::JoinStage::Default,
                    ));
                    client.player_msg(PlayerMsg::JoinVanillaSide(
                        if msg.team == libtw2_gamenet_ddnet::enums::Team::Blue {
                            game_interface::types::render::game::game_match::MatchSide::Blue
                        } else {
                            game_interface::types::render::game::game_match::MatchSide::Red
                        },
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn server_message(
    client: &mut Client,
    net: &mut Net<SocketAddr>,
    socket: &mut Socket,
    pid: PeerId,
    msg: ServerMsg<'static>,
    fs: &Arc<dyn FileSystemInterface>,
    config: &Config,
    rt: &tokio::runtime::Handle,
    tp: &Arc<rayon::ThreadPool>,
    map_cache: &maps::Cache,
) -> anyhow::Result<()> {
    match msg {
        ServerMsg::RequiresPassword => client.send(ClientMsg::PasswordResponse(text(
            client.password.as_bytes(),
        ))),
        ServerMsg::ServerInfo { info, .. } | ServerMsg::Load(info) => {
            ensure!(
                matches!(
                    info.game_mod,
                    GameModification::Native | GameModification::Ddnet
                ),
                "this game module has no legacy snapshot translator"
            );
            if let Some(task) = client.map_task.take() {
                task.abort();
            }
            client.map = None;
            client.map_result = None;
            client.ready_sent = false;
            client.legacy_ready = false;
            client.player = None;
            client.entered = false;
            client.snapshots = Default::default();
            client.snapshots.game_config = info
                .mod_config
                .as_deref()
                .map(serde_json::from_slice)
                .transpose()?
                .unwrap_or_default();
            client.inputs = Default::default();
            client.pending_input_ticks.clear();
            client.inputs.race = info.server_options.physics_group_name.as_str() == "ddnet";
            client.snapshots.race = client.inputs.race;
            client.old_snaps.clear();
            client.ack = -1;
            client.tick_origin = None;
            client.tuning.clear();
            client.effects.clear();
            client.map_task = Some(rt.spawn(maps::cached(
                map_cache.clone(),
                fs.clone(),
                config.backend_s2s.ip(),
                info,
                tp.clone(),
                client.resource_url.clone(),
            )));
        }
        ServerMsg::ReadyResponse(response) => {
            let joined = match response {
                MsgClReadyResponse::Success { joined_ids }
                | MsgClReadyResponse::PartialSuccess { joined_ids, .. } => joined_ids,
                MsgClReadyResponse::Error { err, .. } => {
                    anyhow::bail!("backend rejected player: {err}")
                }
            };
            client.player = Some(joined.first().context("backend joined no player")?.1);
            send_game(net, socket, pid, game::SvReadyToEnter)?;
            client.send(ClientMsg::LoadVotes(MsgClLoadVotes::Map {
                cached_votes: None,
            }));
            client.send(ClientMsg::LoadVotes(MsgClLoadVotes::Misc {
                cached_votes: None,
            }));
        }
        ServerMsg::Snapshot {
            snapshot,
            diff_id,
            snap_id_diffed,
            game_monotonic_tick_diff,
            as_diff,
            overhead_time,
            input_ack,
        } => {
            if let Some((tick, snapshot)) = client.snapshots.decode(
                &snapshot,
                diff_id,
                snap_id_diffed,
                game_monotonic_tick_diff,
                as_diff,
            )? {
                for ack in input_ack.iter() {
                    if let Some((input_pred_tick, input_tick)) =
                        client.pending_input_ticks.remove(&ack.id)
                    {
                        send_system(
                            net,
                            socket,
                            pid,
                            system::InputTiming {
                                input_pred_tick,
                                time_left: input::time_left(
                                    input_tick,
                                    tick,
                                    overhead_time,
                                    ack.logic_overhead,
                                ),
                            }
                            .into(),
                            false,
                        )?;
                    }
                }
                let ack = vec![MsgClSnapshotAck {
                    snap_id: client.snapshots.latest_id.unwrap(),
                }];
                client.input_id += 1;
                client
                    .backend
                    .send_unordered_auto_to_server(&ClientMsg::Inputs {
                        id: client.input_id,
                        inputs: pool::mt_datatypes::PoolFxLinkedHashMap::new_without_pool(),
                        snap_ack: ack.as_slice().into(),
                    });
                if !client.entered {
                    return Ok(());
                }
                let origin = *client.tick_origin.get_or_insert(tick.saturating_sub(1));
                let tick = i32::try_from(
                    tick.checked_sub(origin)
                        .context("snapshot tick moved backwards")?,
                )
                .context("legacy tick range exhausted; reconnect")?;
                let local_pos = client.player.and_then(|id| {
                    snapshot
                        .stages
                        .values()
                        .find_map(|stage| stage.world.characters.get(&id).map(|c| c.pos))
                });
                let effective_tune = client
                    .map
                    .as_ref()
                    .and_then(|map| map.tuning.as_ref())
                    .zip(local_pos)
                    .map_or(&snapshot.global_tune_zone, |(map, pos)| {
                        map.at(&pos, &snapshot.global_tune_zone)
                    });
                let tune = tuning::translate(effective_tune);
                let mut tune_bytes = ArrayVec::<[u8; 2048]>::new();
                with_packer(&mut tune_bytes, |p| Game::from(tune).encode(p))
                    .map_err(|e| anyhow::anyhow!("tuning: {e:?}"))?;
                if client.tuning != tune_bytes.as_slice() {
                    send_game(net, socket, pid, tune)?;
                    client.tuning = tune_bytes.to_vec();
                }
                let snap = client.snapshots.build(
                    &snapshot,
                    client.player.context("snapshot before player joined")?,
                    tick,
                    &client.effects,
                )?;
                let mut delta = libtw2_snapshot::snap::Delta::new();
                let base = client.old_snaps.get(&client.ack);
                delta.create(base.unwrap_or(&libtw2_snapshot::snap::Snap::empty()), &snap);
                let mut data = Vec::with_capacity(65536);
                with_packer(&mut data, |p| {
                    delta.write(libtw2_gamenet_ddnet::snap_obj::obj_size, p)
                })
                .map_err(|e| anyhow::anyhow!("snapshot delta: {e:?}"))?;
                for msg in libtw2_snapshot::snap::delta_chunks(
                    tick,
                    if base.is_some() { client.ack } else { -1 },
                    &data,
                    snap.crc(),
                ) {
                    send_system(net, socket, pid, msg.into(), false)?;
                }
                client.effects.clear();
                client.old_snaps.insert(tick, snap);
                while client.old_snaps.len() > 150 {
                    client.old_snaps.pop_first();
                }
            }
        }
        ServerMsg::Events { events, .. } => events::translate(client, net, socket, pid, events)?,
        ServerMsg::LoadVotes(v) => votes::load(client, v),
        ServerMsg::ResetVotes(kind) => {
            votes::reset(client, kind);
            send_game(net, socket, pid, game::SvVoteClearOptions)?;
        }
        ServerMsg::Vote(vote) => votes::state(net, socket, pid, vote)?,
        ServerMsg::QueueInfo(message) => send_game_ref(
            net,
            socket,
            pid,
            game::SvBroadcast {
                message: message.as_str().as_bytes(),
            }
            .into(),
        )?,
        ServerMsg::Chat(chat) => {
            let id = client
                .snapshots
                .players
                .get(&chat.msg.sender.id)
                .map_or(-1, |v| *v as i32);
            use game_base::network::types::chat::NetChatMsgPlayerChannel;
            let team = match chat.msg.channel {
                NetChatMsgPlayerChannel::Global => 0,
                NetChatMsgPlayerChannel::GameTeam => 1,
                NetChatMsgPlayerChannel::Whisper(_) => {
                    if Some(chat.msg.sender.id) == client.player {
                        2
                    } else {
                        3
                    }
                }
            };
            let message = text::<512>(chat.msg.msg.replace('\0', "").as_bytes());
            send_game_ref(
                net,
                socket,
                pid,
                game::SvChat {
                    team,
                    client_id: id,
                    message: message.as_str().as_bytes(),
                }
                .into(),
            )?;
        }
        _ => {}
    }
    Ok(())
}

pub fn run(
    config: Config,
    fs: Arc<dyn FileSystemInterface>,
    rt: &tokio::runtime::Handle,
    socket_rt: &base_io::runtime::IoRuntime,
) -> anyhow::Result<()> {
    ensure!(
        config.max_connections > 0,
        "max_connections must be positive"
    );
    ensure!(
        config.connect_timeout_seconds > 0 && config.idle_timeout_seconds > 0,
        "timeouts must be positive"
    );
    let mut info = rt.block_on(crate::s2s::discover(&config))?;
    let backend = SocketAddr::new(
        config.backend_s2s.ip(),
        if config.backend_s2s.is_ipv4() {
            info.game_port_v4
        } else {
            info.game_port_v6
        },
    );
    let mut socket = Socket::bind(
        socket_rt,
        config.listen_game_v4.into(),
        config.listen_game_v6.into(),
    )?;
    let tp = Arc::new(rayon::ThreadPoolBuilder::new().num_threads(2).build()?);
    let dicts = rt
        .block_on(async {
            Ok::<_, std::io::Error>((
                fs.read_file("dict/client_send".as_ref()).await?,
                fs.read_file("dict/server_send".as_ref()).await?,
            ))
        })
        .ok();
    let config = Arc::new(config);
    let map_cache: maps::Cache = Default::default();
    let masters = rt.block_on(game_base::server_list_urls::load(&*fs));
    let registration_config = config.clone();
    let registration_info = info.clone();
    let registration = rt.spawn(async move {
        crate::s2s::run_with_info(
            &registration_config,
            &registration_info,
            &masters,
            "tw-0.6+udp",
            browser::registration,
        )
        .await
    });
    let mut net = Net::server();
    let mut clients: HashMap<PeerId, Client> = HashMap::new();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_task = stop.clone();
    let mut signal = rt.spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        stop_task.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    eprintln!(
        "legacy-server-proxy listening on {} and {}, backend {backend}",
        socket.local_addr(true)?,
        socket.local_addr(false)?
    );
    let mut pending_packet = None;
    let mut browser_window = Instant::now();
    let mut browser_count = 0;
    let mut refresh_at = Instant::now();
    let mut refresh: Option<tokio::task::JoinHandle<anyhow::Result<game_base::s2s::ServerInfo>>> =
        None;
    let mut refresh_result: Option<anyhow::Result<game_base::s2s::ServerInfo>> = None;
    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        if refresh_at.elapsed() > Duration::from_secs(10) && refresh.is_none() {
            let config = config.clone();
            refresh = Some(rt.spawn(async move { crate::s2s::discover(&config).await }));
            refresh_at = Instant::now();
        }
        if let Some(Ok(updated)) = refresh_result.take() {
            ensure!(
                (updated.game_port_v4, updated.game_port_v6)
                    == (info.game_port_v4, info.game_port_v6),
                "backend game ports changed; restart the proxy"
            );
            info = updated;
        }
        if browser_window.elapsed() > Duration::from_secs(1) {
            browser_window = Instant::now();
            browser_count = 0;
        }
        {
            for _ in 0..512 {
                let received = pending_packet
                    .take()
                    .map(Ok)
                    .unwrap_or_else(|| socket.try_recv());
                let (addr, packet) = match received {
                    Ok(value) => value,
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                    Err(e) => return Err(anyhow::anyhow!("legacy UDP receiver stopped: {e}")),
                };
                let mut scratch = Vec::with_capacity(4096);
                let (events, result) =
                    net.feed(&mut socket, &mut Ignore, addr, &packet, &mut scratch);
                if result.is_err() {
                    continue;
                }
                for mut event in events {
                    if !net.is_receive_chunk_still_valid(&mut event) {
                        continue;
                    }
                    match event {
                        ChunkOrEvent::Connect(pid) => {
                            if clients.len() >= config.max_connections {
                                let _ = net.reject(&mut socket, pid, b"proxy full");
                                continue;
                            }
                            let connected = connect_client(&config, addr, backend, &dicts);
                            match connected {
                                Ok(mut client) => {
                                    client.resource_url =
                                        info.browser_info.resource_server_url.clone();
                                    net.accept(&mut socket, pid)?;
                                    clients.insert(pid, client);
                                }
                                Err(e) => {
                                    eprintln!("legacy backend connect: {e:#}");
                                    let _ =
                                        net.reject(&mut socket, pid, b"backend connection failed");
                                }
                            }
                        }
                        ChunkOrEvent::Chunk(chunk_data) => {
                            let pid = chunk_data.pid;
                            if let Some(client) = clients.get_mut(&pid)
                                && let Err(e) = chunk(
                                    client,
                                    &mut net,
                                    &mut socket,
                                    pid,
                                    chunk_data.data,
                                    chunk_data.vital,
                                )
                            {
                                eprintln!("legacy client: {e:#}");
                                let reason = format!("{e}");
                                let _ = net.disconnect(
                                    &mut socket,
                                    pid,
                                    &reason.as_bytes()[..reason.len().min(127)],
                                );
                                clients.remove(&pid);
                            }
                        }
                        ChunkOrEvent::Connless(msg) => {
                            if browser_count < 20 {
                                browser_count += 1;
                                let _ = browser::respond(
                                    &mut net,
                                    &mut socket,
                                    msg.addr,
                                    msg.data,
                                    &info,
                                );
                            }
                        }
                        ChunkOrEvent::Disconnect(pid, _) => {
                            clients.remove(&pid);
                        }
                        _ => {}
                    }
                }
            }
        }
        let mut disconnected = Vec::new();
        for (&pid, client) in &mut clients {
            let result = (|| -> anyhow::Result<()> {
                ensure!(
                    client.last_packet.elapsed() < Duration::from_secs(config.idle_timeout_seconds),
                    "legacy client timed out"
                );
                if !client.transport_ready {
                    return Ok(());
                }
                if let Some(map) = client.map_result.take() {
                    let map = map?;
                    send_system(
                        &mut net,
                        &mut socket,
                        pid,
                        system::MapChange {
                            name: map.name.as_bytes(),
                            crc: map.crc,
                            size: map.bytes.len() as i32,
                        }
                        .into(),
                        true,
                    )?;
                    client.map = Some(map);
                    ready(client);
                }
                let events = client.events.clone();
                let mut pending = events.events.blocking_lock();
                for _ in 0..256 {
                    let Some((_, _, event)) = pending.pop_front() else {
                        break;
                    };
                    match event {
                        GameEvents::NetworkMsg(msg) => server_message(
                            client,
                            &mut net,
                            &mut socket,
                            pid,
                            msg,
                            &fs,
                            &config,
                            rt,
                            &tp,
                            &map_cache,
                        )?,
                        GameEvents::NetworkEvent(NetworkEvent::Disconnected(e)) => {
                            anyhow::bail!("{e}")
                        }
                        GameEvents::NetworkEvent(NetworkEvent::ConnectingFailed(e)) => {
                            anyhow::bail!("{e}")
                        }
                        _ => {}
                    }
                }
                if !pending.is_empty() {
                    client.notifier.notify_one();
                }
                drop(pending);
                for _ in 0..8 {
                    let Some(label) = client.vote_queue.pop_front() else {
                        break;
                    };
                    send_game_ref(
                        &mut net,
                        &mut socket,
                        pid,
                        game::SvVoteOptionAdd {
                            description: label.as_bytes(),
                        }
                        .into(),
                    )?;
                }
                if !client.vote_queue.is_empty() {
                    client.notifier.notify_one();
                }
                net.flush(&mut socket, pid)
                    .map_err(|e| anyhow::anyhow!("flush: {e:?}"))?;
                Ok(())
            })();
            if let Err(e) = result {
                eprintln!("legacy client: {e:#}");
                let reason = format!("{e}");
                let _ = net.disconnect(
                    &mut socket,
                    pid,
                    &reason.as_bytes()[..reason.len().min(127)],
                );
                disconnected.push(pid);
            }
        }
        for pid in disconnected {
            clients.remove(&pid);
        }
        for _ in net.tick(&mut socket) {}
        // Wake for actual work or a protocol deadline, never a polling interval.
        let mut deadline = refresh
            .is_none()
            .then_some(refresh_at + Duration::from_secs(10));
        if let Some(tick) = net.needs_tick().to_opt() {
            let tick = Instant::now()
                + Duration::from_micros(
                    tick.as_usecs_since_epoch()
                        .saturating_sub(socket.time().as_usecs_since_epoch()),
                );
            deadline = Some(deadline.map_or(tick, |d| d.min(tick)));
        }
        for client in clients.values() {
            let idle = client.last_packet + Duration::from_secs(config.idle_timeout_seconds);
            deadline = Some(deadline.map_or(idle, |d| d.min(idle)));
        }
        rt.block_on(async {
            let mut waits: Vec<std::pin::Pin<Box<dyn std::future::Future<Output = ()> + '_>>> = Vec::new();
            for client in clients.values_mut() {
                waits.push(Box::pin(client.wait_for_work()));
            }
            let backend_ready = std::future::poll_fn(|cx| {
                for wait in &mut waits {
                    if wait.as_mut().poll(cx).is_ready() {
                        return std::task::Poll::Ready(());
                    }
                }
                std::task::Poll::Pending
            });
            tokio::select! {
                packet = async {
                    let (v4, v6) = socket.receivers();
                    Socket::recv_from(v4, v6).await
                } => {
                    let (data, addr) = packet.ok_or_else(|| std::io::Error::other("legacy UDP receiver stopped"))?;
                    pending_packet = Some((addr, data));
                },
                _ = backend_ready => {},
                _ = async {
                    match deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
                        None => std::future::pending().await,
                    }
                } => {},
                result = async {
                    match &mut refresh {
                        Some(task) => task.await,
                        None => std::future::pending().await,
                    }
                } => {
                    refresh = None;
                    refresh_result = Some(result.map_err(anyhow::Error::from).and_then(|r| r));
                },
                _ = &mut signal => {},
            }
            Ok::<_, std::io::Error>(())
        })?;
    }
    for &pid in clients.keys() {
        let _ = net.disconnect(&mut socket, pid, b"proxy shutting down");
    }
    if let Some(refresh) = refresh {
        refresh.abort();
    }
    registration.abort();
    signal.abort();
    Ok(())
}

fn skin_info(
    custom: bool,
    body: i32,
    feet: i32,
) -> game_interface::types::character_info::NetworkSkinInfo {
    use game_interface::types::character_info::NetworkSkinInfo;
    if custom {
        NetworkSkinInfo::Custom {
            body_color: math::colors::legacy_color_to_rgba(body, true, true),
            feet_color: math::colors::legacy_color_to_rgba(feet, true, true),
        }
    } else {
        NetworkSkinInfo::Original
    }
}

fn connect_client(
    config: &Config,
    addr: SocketAddr,
    backend: SocketAddr,
    dicts: &Option<(Vec<u8>, Vec<u8>)>,
) -> anyhow::Result<Client> {
    let (guest, _) = network::network::utils::create_certifified_keys();
    let options = backend_options(config, addr, &guest)?;
    let events = Arc::new(GameEventGenerator::new(Arc::new(AtomicBool::new(false))));
    let compressor = match dicts {
        Some((send, recv)) => {
            DefaultNetworkPacketCompressor::new_with_dict(send.clone(), recv.clone())
        }
        None => DefaultNetworkPacketCompressor::new(),
    };
    let (backend, notifier) = QuinnNetwork::init_client(
        None,
        events.clone(),
        &SteadyClock::start(),
        options,
        NetworkPlugins {
            packet_plugins: Arc::new(vec![Arc::new(compressor)]),
            ..Default::default()
        },
        &backend.to_string(),
    )?;
    Ok(Client {
        backend,
        notifier,
        events,
        player: None,
        character: None,
        ready_sent: false,
        legacy_ready: false,
        transport_ready: false,
        entered: false,
        password: String::new(),
        map: None,
        resource_url: None,
        map_task: None,
        map_result: None,
        snapshots: Default::default(),
        inputs: Default::default(),
        input_id: 0,
        pending_input_ticks: Default::default(),
        old_snaps: Default::default(),
        ack: -1,
        tick_origin: None,
        last_input: Instant::now(),
        last_packet: Instant::now(),
        tuning: Vec::new(),
        effects: Vec::new(),
        votes: Default::default(),
        vote_queue: Default::default(),
    })
}

#[cfg(test)]
mod tests;
