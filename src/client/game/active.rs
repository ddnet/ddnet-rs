use std::{collections::BTreeMap, sync::Arc, time::Duration};

use anyhow::anyhow;
use base::{linked_hash_map_view::FxLinkedHashMap, system::SystemTimeInterface};
use client_console::console::remote_console::RemoteConsole;
use client_ghost::GhostViewer;
use client_map::client_map::GameMap;
use client_notifications::overlay::ClientNotifications;
use client_render_game::render_game::{ObservedPlayer, RenderGameForPlayer};
use client_replay::replay::Replay;
use command_parser::parser::ParserCache;
use demo::{
    recorder::{DemoRecorder, DemoRecorderCreateProps},
    DemoEvent,
};
use game_base::{
    game_types::time_until_tick,
    local_server_info::LocalServerInfo,
    network::messages::{
        MsgClInputPlayerChain, MsgClReadyResponse, MsgClSnapshotAck, MsgSvAddLocalPlayerResponse,
        PlayerInputChainable,
    },
};
use game_config::config::ConfigGame;
use game_interface::{
    events::GameEvents,
    types::{
        game::{GameTickType, NonZeroGameTickType},
        id_types::PlayerId,
        input::CharacterInputInfo,
        snapshot::SnapshotClientInfo,
    },
};
use game_network::messages::{
    ClientToServerMessage, MsgSvLoadVotes, MsgSvResetVotes, MsgSvStartVoteResult,
    ServerToClientMessage,
};
use game_server::server::Server;
use game_state_wasm::game::state_wasm_manager::GameStateWasmManager;
use ghost::recorder::GhostRecorder;
use pool::{
    datatypes::{PoolBTreeMap, PoolVec},
    mt_pool::Pool as MtPool,
    pool::Pool,
    rc::PoolRc,
};
use url::Url;

use crate::{
    game::data::{ClientConnectedPlayer, SnapshotStorageItem},
    localplayer::{ClientPlayer, ServerInputForDiff},
    spatial_chat::spatial_chat::SpatialChatGameWorldTy,
};

use super::{
    data::GameData,
    types::{GameBase, GameConnect, GameMsgPipeline, GameNetwork},
    DisconnectAutoCleanup,
};

pub struct ActiveGame {
    pub network: GameNetwork,

    pub map: GameMap,

    pub auto_demo_recorder: Option<DemoRecorder>,
    pub manual_demo_recorder: Option<DemoRecorder>,
    pub race_demo_recorder: Option<DemoRecorder>,
    pub demo_recorder_props: DemoRecorderCreateProps,

    pub ghost_recorder: Option<GhostRecorder>,
    pub ghost_viewer: Option<GhostViewer>,

    pub replay: Replay,

    pub game_data: GameData,

    pub events: PoolBTreeMap<(GameTickType, bool), GameEvents>,

    pub map_votes_loaded: bool,
    pub misc_votes_loaded: bool,

    pub render_players_pool: Pool<FxLinkedHashMap<PlayerId, RenderGameForPlayer>>,
    pub render_observers_pool: Pool<Vec<ObservedPlayer>>,

    pub player_inputs_pool: Pool<FxLinkedHashMap<PlayerId, PoolVec<PlayerInputChainable>>>,
    pub player_inputs_chainable_pool: Pool<Vec<PlayerInputChainable>>,
    pub player_inputs_chain_pool: MtPool<FxLinkedHashMap<PlayerId, MsgClInputPlayerChain>>,
    pub player_inputs_chain_data_pool: MtPool<Vec<u8>>,
    pub player_inputs_ser_helper_pool: Pool<Vec<u8>>,
    pub events_pool: Pool<BTreeMap<(GameTickType, bool), GameEvents>>,

    pub remote_console: RemoteConsole,
    pub remote_console_logs: String,

    pub parser_cache: ParserCache,

    pub resource_download_server: Option<Url>,

    pub requested_account_details: bool,

    pub next_player_info_change: Option<Duration>,

    pub spatial_world: SpatialChatGameWorldTy,
    pub auto_cleanup: DisconnectAutoCleanup,
    pub connect: GameConnect,

    pub base: GameBase,

    pub send_input_every_tick: bool,
}

impl ActiveGame {
    pub fn send_input(
        &mut self,
        player_inputs: &FxLinkedHashMap<PlayerId, PoolVec<PlayerInputChainable>>,
        sys: &dyn SystemTimeInterface,
    ) {
        if !player_inputs.is_empty()
            || (!self.send_input_every_tick && !self.game_data.snap_acks.is_empty())
        {
            let mut player_inputs_send = self.player_inputs_chain_pool.new();
            for (player_id, player_inputs) in player_inputs.iter() {
                let player = self
                    .game_data
                    .local
                    .local_players
                    .get_mut(player_id)
                    .unwrap();
                let mut data = self.player_inputs_chain_data_pool.new();
                let (diff_id, def_inp) = player
                    .server_input
                    .as_ref()
                    .map(|inp| (Some(inp.id), inp.inp))
                    .unwrap_or_default();

                let mut def = self.player_inputs_ser_helper_pool.new();
                bincode::serde::encode_into_std_write(
                    def_inp,
                    &mut *def,
                    bincode::config::standard().with_fixed_int_encoding(),
                )
                .unwrap();

                let mut cur_diff = def;
                for player_input in player_inputs.iter() {
                    let mut inp = self.player_inputs_ser_helper_pool.new();
                    bincode::serde::encode_into_std_write(
                        player_input,
                        &mut *inp,
                        bincode::config::standard().with_fixed_int_encoding(),
                    )
                    .unwrap();

                    bin_patch::diff_exact_size(&cur_diff, &inp, &mut data).unwrap();

                    cur_diff = inp;
                }

                let player_input = player_inputs.last().unwrap();
                // this should be smaller than the number of inputs saved on the server
                let as_diff = if player.server_input_storage.len() < 10 {
                    player
                        .server_input_storage
                        .insert(self.game_data.input_id, *player_input);
                    true
                } else {
                    false
                };

                player_inputs_send.insert(
                    *player_id,
                    MsgClInputPlayerChain {
                        data,
                        diff_id,
                        as_diff,
                    },
                );
            }

            let cur_time = sys.time_get();
            // remove some old sent input timings
            while self
                .game_data
                .sent_input_ids
                .first_key_value()
                .is_some_and(|(_, sent_at)| {
                    cur_time.saturating_sub(*sent_at) > Duration::from_secs(3)
                })
            {
                self.game_data.sent_input_ids.pop_first();
            }
            self.game_data
                .sent_input_ids
                .insert(self.game_data.input_id, cur_time);
            self.network
                .send_unordered_auto_to_server(&ClientToServerMessage::Inputs {
                    id: self.game_data.input_id,
                    inputs: player_inputs_send,
                    snap_ack: self.game_data.snap_acks.as_slice().into(),
                });

            self.game_data.snap_acks.clear();
            self.game_data.input_id += 1;
        }
    }

    fn ack_input(player: &mut ClientPlayer, input_id: u64) {
        if let Some(inp) = player.server_input_storage.remove(&input_id) {
            player.server_input = Some(ServerInputForDiff { id: input_id, inp });
        }
        while player
            .server_input_storage
            .first_entry()
            .is_some_and(|entry| *entry.key() < input_id)
        {
            player.server_input_storage.pop_first();
        }
    }

    fn local_player_connected(
        &mut self,
        notifications: &mut ClientNotifications,
        id: u64,
        player_id: PlayerId,
    ) {
        if let Some(player) = self.game_data.local.expected_local_players.get_mut(&id) {
            *player = match player {
                ClientConnectedPlayer::Connecting { is_dummy } => {
                    ClientConnectedPlayer::Connected {
                        is_dummy: *is_dummy,
                        player_id,
                    }
                }
                ClientConnectedPlayer::Connected { .. } => {
                    notifications.add_err(
                        "Server send a player response to \
                        an already connected player"
                            .to_string(),
                        Duration::from_secs(10),
                    );
                    *player
                }
            };
        }
    }

    pub fn on_msg(
        &mut self,
        timestamp: &Duration,
        msg: ServerToClientMessage<'static>,
        pipe: &mut GameMsgPipeline,
    ) {
        use game_interface::interface::GameStateInterface;
        match msg {
            ServerToClientMessage::Custom(_) => {
                // ignore
            }
            ServerToClientMessage::RequiresPassword => {
                // ignore
            }
            ServerToClientMessage::ServerInfo { .. } => {
                // TODO: update some stuff or just ignore?
            }
            ServerToClientMessage::Snapshot {
                overhead_time,
                mut snapshot,
                game_monotonic_tick_diff,
                snap_id_diffed,
                diff_id,
                as_diff,
                input_ack,
            } => {
                // first handle input acks, so no early returns can prevent that.
                for player in self.game_data.local.local_players.values_mut() {
                    for input in input_ack.iter() {
                        Self::ack_input(player, input.id);
                    }
                }

                // add the estimated ping to our prediction timer
                for input in input_ack.iter() {
                    if let Some(sent_at) = self.game_data.sent_input_ids.remove(&input.id) {
                        self.game_data.prediction_timer.add_ping(
                            timestamp
                                .saturating_sub(sent_at)
                                .saturating_sub(input.logic_overhead),
                            *timestamp,
                        );
                    }
                }

                let snapshot_and_id = if let Some(diff_id) = diff_id {
                    self.game_data.snap_storage.get(&diff_id)
                        .map(|old| {
                            let mut patch = self.game_data.player_snap_pool.new();
                            patch.resize(snapshot.len(), Default::default());
                            patch.clone_from_slice(snapshot.as_ref());
                            snapshot.to_mut().clear();
                            let patch_res = bin_patch::patch(&old.snapshot, &patch,  snapshot.to_mut());
                            patch_res.map(|_| (snapshot, game_monotonic_tick_diff + old.monotonic_tick)).map_err(|err| anyhow!(err))
                        }).unwrap_or_else(|| Err(anyhow!("patching snapshot difference failed, because the previous snapshot was missing.")))
                        .map(|(snap, game_monotonic_tick)| (snap, snap_id_diffed + diff_id, game_monotonic_tick))
                } else {
                    Ok((snapshot, snap_id_diffed, game_monotonic_tick_diff))
                };
                let (snapshot, snap_id, game_monotonic_tick) = match snapshot_and_id {
                    Ok((snapshot, snap_id, game_monotonic_tick)) => {
                        (snapshot, snap_id, game_monotonic_tick)
                    }
                    Err(err) => {
                        log::debug!(target: "network_logic", "had to drop a snapshot from the server with diff_id {:?}: {err}", diff_id);
                        return;
                    }
                };

                if let Some(demo_recorder) = &mut self.auto_demo_recorder {
                    demo_recorder.add_snapshot(game_monotonic_tick, snapshot.as_ref().to_vec());
                }
                if let Some(demo_recorder) = &mut self.manual_demo_recorder {
                    demo_recorder.add_snapshot(game_monotonic_tick, snapshot.as_ref().to_vec());
                }
                if let Some(demo_recorder) = &mut self.race_demo_recorder {
                    demo_recorder.add_snapshot(game_monotonic_tick, snapshot.as_ref().to_vec());
                }
                self.replay
                    .add_snapshot(game_monotonic_tick, snapshot.as_ref().to_vec());

                let GameMap { game, .. } = &mut self.map;
                let ticks_per_second = game.game_tick_speed();
                let tick_time = time_until_tick(ticks_per_second);
                let monotonic_tick = game_monotonic_tick;

                // prepare the unpredicted world if needed
                self.game_data.last_snaps.insert(
                    game_monotonic_tick,
                    std::mem::take(&mut *snapshot.clone().to_mut()),
                );
                while self
                    .game_data
                    .last_snaps
                    .first_key_value()
                    .is_some_and(|(tick, _)| {
                        *tick
                            <= game
                                .predicted_game_monotonic_tick
                                .saturating_sub(game.game_tick_speed().get() * 3)
                    })
                {
                    self.game_data.last_snaps.pop_first();
                }

                let mut prev_tick = game.predicted_game_monotonic_tick;
                if self.game_data.handled_snap_id.is_none_or(|id| id < snap_id) {
                    // Reset cur state snap for future tick
                    self.game_data.cur_state_snap = None;

                    self.game_data.handled_snap_id = Some(snap_id);
                    if as_diff {
                        // this should be higher than the number of snapshots saved on the server
                        // (since reordering of packets etc.)
                        while self.game_data.snap_storage.len() >= 50 {
                            self.game_data.snap_storage.pop_first();
                        }
                        self.game_data.snap_storage.insert(
                            snap_id,
                            SnapshotStorageItem {
                                snapshot: std::mem::take(&mut *snapshot.clone().to_mut()),
                                monotonic_tick: game_monotonic_tick,
                            },
                        );
                    }
                    self.game_data.snap_acks.push(MsgClSnapshotAck { snap_id });

                    let predicted_game_monotonic_tick = monotonic_tick.max(prev_tick);

                    // if the incoming snapshot is older than the prediction tick, then we can use it directly
                    let snapshot =
                        if monotonic_tick < prev_tick || self.game_data.last_snap.is_none() {
                            self.game_data.last_snap = Some((snapshot, monotonic_tick));
                            None
                        } else if monotonic_tick == prev_tick {
                            Some(snapshot)
                        } else {
                            None
                        };

                    fn advance_game_state(
                        prev_tick: &mut GameTickType,
                        monotonic_tick: GameTickType,
                        game_data: &mut GameData,
                        game: &mut GameStateWasmManager,
                        ticks_per_second: NonZeroGameTickType,
                        config_game: &ConfigGame,
                        shared_info: &Arc<LocalServerInfo>,
                        timestamp: &Duration,
                        tick_time: Duration,
                        overhead_time: Option<Duration>,
                    ) {
                        match (*prev_tick).cmp(&monotonic_tick) {
                            std::cmp::Ordering::Greater => {
                                let max_tick = *prev_tick;
                                // the clamp ensures that the game at most predicts 3 seconds back, to prevent major fps drops
                                let min_tick = monotonic_tick.clamp(
                                    prev_tick.saturating_sub(game.game_tick_speed().get() * 3),
                                    *prev_tick,
                                );
                                (min_tick..max_tick).for_each(|new_tick| {
                                    // apply the player input if the tick had any
                                    let prev_tick_of_inp = new_tick;
                                    let tick_of_inp = new_tick + 1;
                                    if let (Some(inp), prev_inp) = (
                                        game_data.input_per_tick.get(&tick_of_inp).or_else(|| {
                                            game_data.input_per_tick.iter().rev().find_map(
                                                |(&tick, inp)| (tick <= tick_of_inp).then_some(inp),
                                            )
                                        }),
                                        game_data.input_per_tick.get(&prev_tick_of_inp),
                                    ) {
                                        let mut inps = game_data.player_inputs_state_pool.new();
                                        inp.iter().for_each(|(player_id, player_inp)| {
                                            let mut prev_player_inp = prev_inp
                                                .or(Some(inp))
                                                .and_then(|inps| inps.get(player_id).cloned())
                                                .unwrap_or_default();

                                            if let Some(diff) = prev_player_inp.try_overwrite(
                                                &player_inp.inp,
                                                player_inp.version(),
                                                true,
                                            ) {
                                                inps.insert(
                                                    *player_id,
                                                    CharacterInputInfo {
                                                        inp: player_inp.inp,
                                                        diff,
                                                    },
                                                );
                                            }
                                        });
                                        game.set_player_inputs(inps);
                                    }
                                    game.tick(Default::default());
                                    Server::dbg_game(
                                        &config_game.dbg,
                                        &game_data.last_game_tick,
                                        game,
                                        game_data
                                            .input_per_tick
                                            .get(&tick_of_inp)
                                            .map(|inps| inps.values().map(|inp| &inp.inp)),
                                        new_tick + 1,
                                        ticks_per_second.get(),
                                        shared_info,
                                        "client-pred",
                                    );
                                    // Game events from prediction code is not interesting
                                    game.clear_events();
                                });
                            }
                            std::cmp::Ordering::Less => {
                                if let Some(overhead_time) = overhead_time {
                                    game_data.last_game_tick = timestamp
                                        .saturating_sub(
                                            game_data
                                                .prediction_timer
                                                .pred_max_smoothing(tick_time),
                                        )
                                        .saturating_sub(overhead_time);
                                    *prev_tick = monotonic_tick;
                                }
                            }
                            std::cmp::Ordering::Equal => {
                                // ignore
                            }
                        }
                    }

                    // advance the previous state to to just before the prediction tick
                    if let Some((prev_snapshot, prev_state_tick)) = &self.game_data.last_snap {
                        let local_players = game.build_from_snapshot(prev_snapshot);
                        // set local players
                        GameData::handle_local_players_from_snapshot(
                            &mut self.game_data.local.local_players,
                            &self.game_data.local.expected_local_players,
                            pipe.config_game,
                            pipe.console_entries,
                            local_players,
                            &self.parser_cache,
                            &game.info.options,
                        );
                        let prev_state_tick = *prev_state_tick;
                        advance_game_state(
                            &mut predicted_game_monotonic_tick.saturating_sub(1),
                            prev_state_tick,
                            &mut self.game_data,
                            game,
                            ticks_per_second,
                            pipe.config_game,
                            pipe.shared_info,
                            timestamp,
                            tick_time,
                            None,
                        );

                        let prev_state_snap = game.snapshot_for(SnapshotClientInfo::Everything);
                        game.build_from_snapshot_for_prev(&prev_state_snap);
                    }

                    let advance_from_monotonic_tick = if let Some(snapshot) = snapshot {
                        let local_players = game.build_from_snapshot(&snapshot);
                        // set local players
                        GameData::handle_local_players_from_snapshot(
                            &mut self.game_data.local.local_players,
                            &self.game_data.local.expected_local_players,
                            pipe.config_game,
                            pipe.console_entries,
                            local_players,
                            &self.parser_cache,
                            &game.info.options,
                        );

                        self.game_data.last_snap = Some((snapshot, monotonic_tick));
                        monotonic_tick
                    } else {
                        predicted_game_monotonic_tick.saturating_sub(1)
                    };

                    game.predicted_game_monotonic_tick = predicted_game_monotonic_tick;

                    advance_game_state(
                        &mut prev_tick,
                        advance_from_monotonic_tick,
                        &mut self.game_data,
                        game,
                        ticks_per_second,
                        pipe.config_game,
                        pipe.shared_info,
                        timestamp,
                        tick_time,
                        Some(overhead_time),
                    );

                    // Game events from prediction code is not interesting
                    // Here again since build_from_snapshot* calls might also add events
                    // The client never cares about those
                    game.clear_events();

                    // drop queued input that was before or at the server monotonic tick
                    while self
                        .game_data
                        .input_per_tick
                        .front()
                        .is_some_and(|(&tick, _)| tick < monotonic_tick)
                    {
                        self.game_data.input_per_tick.pop_front();
                    }
                }
                let prediction_timer = &mut self.game_data.prediction_timer;
                let predict_max = prediction_timer.pred_max_smoothing(tick_time);
                let ticks_in_pred = (predict_max.as_nanos() / tick_time.as_nanos()) as u64;
                let time_in_pred =
                    Duration::from_nanos((predict_max.as_nanos() % tick_time.as_nanos()) as u64);

                // we remove the overhead of the server here,
                // the reason is simple: if the server required 10ms for 63 players snapshots
                // the 64th player's client would "think" it runs 10ms behind and speeds up
                // computation, but the inputs are handled much earlier on the server then.
                let timestamp = timestamp.saturating_sub(overhead_time);
                let time_diff =
                    timestamp.as_secs_f64() - self.game_data.last_game_tick.as_secs_f64();
                let pred_tick = prev_tick;

                let tick_diff =
                    (pred_tick as i128 - monotonic_tick as i128) as f64 - ticks_in_pred as f64;
                let time_diff = time_diff - time_in_pred.as_secs_f64();

                let time_diff = tick_diff * tick_time.as_secs_f64() + time_diff;

                prediction_timer.add_snap(time_diff, timestamp);
            }
            ServerToClientMessage::Events {
                events,
                game_monotonic_tick,
            } => {
                if let Some(demo_recorder) = &mut self.auto_demo_recorder {
                    demo_recorder.add_event(game_monotonic_tick, DemoEvent::Game(events.clone()));
                }
                if let Some(demo_recorder) = &mut self.manual_demo_recorder {
                    demo_recorder.add_event(game_monotonic_tick, DemoEvent::Game(events.clone()));
                }
                if let Some(demo_recorder) = &mut self.race_demo_recorder {
                    demo_recorder.add_event(game_monotonic_tick, DemoEvent::Game(events.clone()));
                }
                self.replay
                    .add_event(game_monotonic_tick, DemoEvent::Game(events.clone()));

                let event_id = events.event_id;
                self.events.insert((game_monotonic_tick, false), events);
                self.map.game.sync_event_id(event_id);
            }
            ServerToClientMessage::Load(_) => {
                panic!("this should be handled by earlier logic.");
            }
            ServerToClientMessage::QueueInfo(_) => {
                // ignore
            }
            ServerToClientMessage::Chat(chat_msg) => {
                if let Some(demo_recorder) = &mut self.auto_demo_recorder {
                    demo_recorder.add_event(
                        self.map.game.predicted_game_monotonic_tick,
                        DemoEvent::Chat(Box::new(chat_msg.msg.clone())),
                    );
                }
                if let Some(demo_recorder) = &mut self.manual_demo_recorder {
                    demo_recorder.add_event(
                        self.map.game.predicted_game_monotonic_tick,
                        DemoEvent::Chat(Box::new(chat_msg.msg.clone())),
                    );
                }
                if let Some(demo_recorder) = &mut self.race_demo_recorder {
                    demo_recorder.add_event(
                        self.map.game.predicted_game_monotonic_tick,
                        DemoEvent::Chat(Box::new(chat_msg.msg.clone())),
                    );
                }
                self.replay.add_event(
                    self.map.game.predicted_game_monotonic_tick,
                    DemoEvent::Chat(Box::new(chat_msg.msg.clone())),
                );

                self.game_data.chat_msgs.push_back(chat_msg.msg);
            }
            ServerToClientMessage::StartVoteRes(res) => {
                if let Some(msg) = match res {
                    MsgSvStartVoteResult::Success => {
                        // ignore, vote will be displayed
                        None
                    }
                    MsgSvStartVoteResult::AnotherVoteAlreadyActive => {
                        Some("Another vote is already active.".to_string())
                    }
                    MsgSvStartVoteResult::MapVoteDoesNotExist => {
                        Some("Map vote does not exist.".to_string())
                    }
                    MsgSvStartVoteResult::CantVoteSelf => {
                        Some("You cannot vote yourself.".to_string())
                    }
                    MsgSvStartVoteResult::CantSameClient => {
                        Some("You cannot vote your dummy.".to_string())
                    }
                    MsgSvStartVoteResult::CantSameNetwork => {
                        Some("You cannot vote a player from the same network.".to_string())
                    }
                    MsgSvStartVoteResult::CantVoteAdminOrModerator => {
                        Some("You cannot vote an admin or moderator.".to_string())
                    }
                    MsgSvStartVoteResult::CantVoteFromOtherStage => {
                        Some("You cannot vote a player from a different team.".to_string())
                    }
                    MsgSvStartVoteResult::PlayerDoesNotExist => {
                        Some("The voted player does not exist.".to_string())
                    }
                    MsgSvStartVoteResult::TooFewPlayersToVote => Some(
                        "There are too few players on the server to start a player vote."
                            .to_string(),
                    ),
                    MsgSvStartVoteResult::MiscVoteDoesNotExist => {
                        Some("Misc vote does not exist.".to_string())
                    }
                    MsgSvStartVoteResult::CantVoteAsSpectator => {
                        Some("You cannot vote as spectator.".to_string())
                    }
                    MsgSvStartVoteResult::RandomUnfinishedMapUnsupported => {
                        Some("Random unfinished map votes are not supported.".to_string())
                    }
                } {
                    pipe.notifications.add_info(msg, Duration::from_secs(3));
                }
            }
            ServerToClientMessage::Vote(vote_state) => {
                let voted = self
                    .game_data
                    .vote
                    .as_ref()
                    .and_then(|(_, voted, _)| *voted);
                self.game_data.vote =
                    vote_state.map(|v| (PoolRc::from_item_without_pool(v), voted, *timestamp));
            }
            ServerToClientMessage::LoadVotes(votes) => match votes {
                MsgSvLoadVotes::Map {
                    categories,
                    has_unfinished_map_votes,
                } => {
                    self.game_data.map_votes = categories;
                    self.game_data.has_unfinished_map_votes = has_unfinished_map_votes;
                }
                MsgSvLoadVotes::Misc { votes } => {
                    self.game_data.misc_votes = votes;
                }
            },
            ServerToClientMessage::ResetVotes(votes) => match votes {
                MsgSvResetVotes::Map => {
                    self.game_data.map_votes.clear();
                    self.game_data.has_unfinished_map_votes = false;
                    self.map_votes_loaded = false;
                }
                MsgSvResetVotes::Misc => {
                    self.game_data.misc_votes.clear();
                    self.misc_votes_loaded = false;
                }
            },
            ServerToClientMessage::RconEntries(cmds) => {
                self.remote_console.fill_entries(cmds.cmds, cmds.vars);
            }
            ServerToClientMessage::RconExecResult { results } => {
                let has_results = !results.is_empty();
                self.remote_console_logs.push_str(
                    &results
                        .into_iter()
                        .map(|r| match r {
                            Ok(r) => r.into(),
                            Err(err) => err.into(),
                        })
                        .collect::<Vec<String>>()
                        .join("\n"),
                );
                if has_results {
                    self.remote_console_logs.push('\n');
                }
            }
            ServerToClientMessage::AccountRenameRes(new_name) => match new_name {
                Ok(new_name) => {
                    pipe.account_info.fill_last_action_response(Some(None));
                    if let Some((mut account_info, creation_date)) =
                        pipe.account_info.account_info().clone()
                    {
                        account_info.name = new_name;
                        pipe.account_info
                            .fill_account_info(Some((account_info, creation_date)));
                    }
                }
                Err(err) => {
                    pipe.account_info
                        .fill_last_action_response(Some(Some(err.to_string())));
                }
            },
            ServerToClientMessage::AccountDetails(details) => match details {
                Ok(details) => {
                    pipe.account_info.fill_last_action_response(None);
                    let creation_date = details
                        .creation_date
                        .to_chrono()
                        .map(|d| chrono::DateTime::<chrono::Local>::from(d).to_string())
                        .unwrap_or_default();
                    pipe.account_info
                        .fill_account_info(Some((details, creation_date)));
                }
                Err(err) => {
                    pipe.account_info
                        .fill_last_action_response(Some(Some(err.to_string())));
                }
            },
            ServerToClientMessage::SpatialChat { entities } => {
                pipe.spatial_chat.on_input(
                    self.spatial_world
                        .as_mut()
                        .map(|world| (world, self.map.game.collect_characters_info())),
                    entities,
                    pipe.config_game,
                );
            }
            ServerToClientMessage::ReadyResponse(res) => {
                match res {
                    MsgClReadyResponse::Success { joined_ids } => {
                        for (id, player_id) in joined_ids {
                            self.local_player_connected(pipe.notifications, id, player_id);
                        }
                    }
                    MsgClReadyResponse::PartialSuccess {
                        joined_ids,
                        non_joined_ids,
                    } => {
                        for (id, player_id) in joined_ids {
                            self.local_player_connected(pipe.notifications, id, player_id);
                        }
                        for id in non_joined_ids {
                            self.game_data.local.expected_local_players.remove(&id);
                        }
                        pipe.notifications.add_warn(
                            "Some local players could not \
                            connect to the server, the server is full.",
                            Duration::from_secs(10),
                        );
                    }
                    MsgClReadyResponse::Error {
                        err,
                        non_joined_ids,
                    } => {
                        for id in non_joined_ids {
                            self.game_data.local.expected_local_players.remove(&id);
                        }
                        pipe.notifications
                            .add_err(err.to_string(), Duration::from_secs(10));
                    }
                }
                // make sure the client has an active local player
                if !self
                    .game_data
                    .local
                    .expected_local_players
                    .contains_key(&self.game_data.local.active_local_player_id)
                {
                    self.game_data.local.active_local_player_id = self
                        .game_data
                        .local
                        .expected_local_players
                        .front()
                        .map(|(id, _)| *id)
                        .unwrap_or_default();
                }
            }
            ServerToClientMessage::AddLocalPlayerResponse(res) => {
                match res {
                    MsgSvAddLocalPlayerResponse::Success { id, player_id } => {
                        self.local_player_connected(pipe.notifications, id, player_id);
                    }
                    MsgSvAddLocalPlayerResponse::Err { id, err } => {
                        self.game_data.local.expected_local_players.remove(&id);
                        if self.game_data.local.active_local_player_id == id {
                            self.game_data.local.active_local_player_id = self
                                .game_data
                                .local
                                .expected_local_players
                                .front()
                                .map(|(id, _)| *id)
                                .unwrap_or_default();
                        }
                        pipe.notifications
                            .add_err(err.to_string(), Duration::from_secs(10));
                    }
                }

                self.auto_cleanup
                    .client_info
                    .set_local_player_count(self.game_data.local.expected_local_players.len());
            }
        }
    }
}
