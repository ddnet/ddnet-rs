use std::{
    collections::{BTreeMap, VecDeque},
    time::Duration,
};

use base::{
    linked_hash_map_view::{FxLinkedHashMap, FxLinkedHashSet},
    network_string::NetworkString,
};
use binds::binds::{
    bind_to_str, gen_local_player_action_hash_map, gen_local_player_action_hash_map_rev,
    syn_to_bind, BindAction, BindActionsCharacter, BindActionsLocalPlayer,
};
use client_types::console::{entries_to_parser, ConsoleEntry};
use command_parser::parser::{self, Command, CommandType, ParserCache, Syn};
use game_base::{
    network::{
        messages::{MsgClSnapshotAck, PlayerInputChainable},
        types::chat::NetChatMsg,
    },
    player_input::PlayerInput,
};
use game_config::config::ConfigGame;
use game_interface::{
    interface::GameStateServerOptions,
    types::{
        game::GameTickType,
        id_types::{CharacterId, PlayerId},
        input::{cursor::CharacterInputCursor, CharacterInputInfo, CharacterInputMethodFlags},
        snapshot::SnapshotLocalPlayers,
        weapons::WeaponType,
    },
    votes::{MapVote, MapVoteKey, MiscVote, MiscVoteKey, VoteState, Voted, MAX_CATEGORY_NAME_LEN},
};
use input_binds::binds::{BindKey, Binds, MouseExtra};
use math::math::vector::luffixed;
use native::native::{KeyCode, MouseButton, PhysicalKey};
use pool::{
    datatypes::{PoolVec, PoolVecDeque},
    mt_datatypes::PoolCow,
    pool::Pool,
    rc::PoolRc,
};
use prediction_timer::prediction_timing::PredictionTimer;

use crate::{
    client::input::input_handling::DeviceToLocalPlayerIndex,
    localplayer::{
        dummy_control::{DummyControlState, DummyHammerState},
        ClientPlayer, ClientPlayerInputPerTick, LocalPlayers,
    },
};

#[derive(Debug)]
pub struct SnapshotStorageItem {
    pub snapshot: Vec<u8>,
    pub monotonic_tick: GameTickType,
}

#[derive(Debug, Default)]
pub struct NetworkByteStats {
    pub last_timestamp: Duration,
    pub last_bytes_sent: u64,
    pub last_bytes_recv: u64,
    pub bytes_per_sec_sent: luffixed,
    pub bytes_per_sec_recv: luffixed,
}

#[derive(Debug, Clone, Copy)]
pub enum ClientConnectedPlayer {
    Connecting { is_dummy: bool },
    Connected { is_dummy: bool, player_id: PlayerId },
}

pub struct LocalPlayerGameData {
    pub local_players: LocalPlayers,
    pub expected_local_players: FxLinkedHashMap<u64, ClientConnectedPlayer>,
    pub local_player_id_counter: u64,
    /// This effictively is the current player (either main player or dummy)
    pub active_local_player_id: u64,
}

impl LocalPlayerGameData {
    pub fn active_local_player(&self) -> Option<(&PlayerId, &ClientPlayer)> {
        self.expected_local_players
            .get(&self.active_local_player_id)
            .and_then(|p| match p {
                ClientConnectedPlayer::Connecting { .. } => None,
                ClientConnectedPlayer::Connected { player_id, .. } => {
                    self.local_players.get(player_id).map(|p| (player_id, p))
                }
            })
    }

    pub fn active_local_player_mut(&mut self) -> Option<(&PlayerId, &mut ClientPlayer)> {
        self.expected_local_players
            .get(&self.active_local_player_id)
            .and_then(|p| match p {
                ClientConnectedPlayer::Connecting { .. } => None,
                ClientConnectedPlayer::Connected { player_id, .. } => self
                    .local_players
                    .get_mut(player_id)
                    .map(|p| (player_id, p)),
            })
    }

    pub fn inactive_local_players(&self) -> impl Iterator<Item = (&PlayerId, &ClientPlayer)> {
        self.expected_local_players
            .iter()
            .filter(|(&id, _)| id != self.active_local_player_id)
            .filter_map(|(_, p)| match p {
                ClientConnectedPlayer::Connecting { .. } => None,
                ClientConnectedPlayer::Connected { player_id, .. } => {
                    self.local_players.get(player_id).map(|p| (player_id, p))
                }
            })
    }

    pub fn first_inactive_local_players_mut(&mut self) -> Option<(&PlayerId, &mut ClientPlayer)> {
        let local_players = &mut self.local_players;
        self.expected_local_players
            .iter()
            .filter(|(&id, _)| id != self.active_local_player_id)
            .filter_map(|(_, p)| match p {
                ClientConnectedPlayer::Connecting { .. } => None,
                ClientConnectedPlayer::Connected { player_id, .. } => Some(player_id),
            })
            .next()
            .and_then(|player_id| local_players.get_mut(player_id).map(|p| (player_id, p)))
    }
}

pub struct GameData {
    pub local: LocalPlayerGameData,
    pub dummy_control: DummyControlState,

    /// Snapshot that still has to be acknowledged.
    pub snap_acks: Vec<MsgClSnapshotAck>,

    pub device_to_local_player_index: DeviceToLocalPlayerIndex, // TODO: keyboard and mouse are different devices
    pub input_per_tick: ClientPlayerInputPerTick,

    /// This is only used to make sure old snapshots are not handled.
    pub handled_snap_id: Option<u64>,
    pub last_snap: Option<(PoolCow<'static, [u8]>, GameTickType)>,

    /// Only interesting for future tick prediction
    pub cur_state_snap: Option<PoolCow<'static, [u8]>>,

    /// Ever increasing id for sending input packages.
    pub input_id: u64,

    /// last (few) snapshot diffs & id client used
    pub snap_storage: BTreeMap<u64, SnapshotStorageItem>,

    /// Last snapshots (only for unpredicted gameplay)
    pub last_snaps: BTreeMap<GameTickType, Vec<u8>>,

    /// A tracker of sent inputs and their time
    /// used to evaluate the estimated RTT/ping.
    pub sent_input_ids: BTreeMap<u64, Duration>,

    pub prediction_timer: PredictionTimer,
    pub net_byte_stats: NetworkByteStats,

    pub last_game_tick: Duration,
    pub last_frame_time: Duration,
    pub intra_tick_time: Duration,

    pub chat_msgs_pool: Pool<VecDeque<NetChatMsg>>,
    pub chat_msgs: PoolVecDeque<NetChatMsg>,
    pub player_inp_pool: Pool<FxLinkedHashMap<PlayerId, PlayerInput>>,
    pub player_snap_pool: Pool<Vec<u8>>,
    pub player_inputs_state_pool: Pool<FxLinkedHashMap<PlayerId, CharacterInputInfo>>,
    pub player_ids_pool: Pool<FxLinkedHashSet<PlayerId>>,

    /// current vote in the game and the network timestamp when it arrived
    pub vote: Option<(PoolRc<VoteState>, Option<Voted>, Duration)>,

    pub map_votes: BTreeMap<NetworkString<MAX_CATEGORY_NAME_LEN>, BTreeMap<MapVoteKey, MapVote>>,
    pub has_unfinished_map_votes: bool,
    pub misc_votes: BTreeMap<NetworkString<MAX_CATEGORY_NAME_LEN>, BTreeMap<MiscVoteKey, MiscVote>>,
}

impl GameData {
    pub fn new(
        cur_time: Duration,
        prediction_timer: PredictionTimer,
        local_player_id_counter: u64,
        active_local_player_id: u64,
        expected_local_players: FxLinkedHashMap<u64, ClientConnectedPlayer>,
    ) -> Self {
        let chat_and_system_msgs_pool = Pool::with_capacity(2);

        Self {
            local: LocalPlayerGameData {
                local_players: LocalPlayers::default(),
                expected_local_players,
                local_player_id_counter,
                active_local_player_id,
            },
            dummy_control: Default::default(),

            snap_acks: Vec::with_capacity(16),

            input_id: 0,
            last_snap: None,

            cur_state_snap: None,

            snap_storage: Default::default(),
            last_snaps: Default::default(),

            device_to_local_player_index: Default::default(),
            input_per_tick: Default::default(),

            sent_input_ids: Default::default(),

            handled_snap_id: None,
            prediction_timer,
            net_byte_stats: Default::default(),

            last_game_tick: cur_time,
            intra_tick_time: Duration::ZERO,
            last_frame_time: cur_time,

            chat_msgs: chat_and_system_msgs_pool.new(),
            chat_msgs_pool: chat_and_system_msgs_pool,
            player_inp_pool: Pool::with_capacity(64),
            player_snap_pool: Pool::with_capacity(2),
            player_inputs_state_pool: Pool::with_capacity(2),
            player_ids_pool: Pool::with_capacity(4),

            vote: None,
            map_votes: Default::default(),
            has_unfinished_map_votes: false,
            misc_votes: Default::default(),
        }
    }
}

impl GameData {
    fn default_binds(mut on_bind: impl FnMut(&[BindKey], Vec<BindAction>)) {
        for (keys, actions) in [
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::KeyA))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                    BindActionsCharacter::MoveLeft,
                ))],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::KeyD))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                    BindActionsCharacter::MoveRight,
                ))],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::Space))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                    BindActionsCharacter::Jump,
                ))],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::Escape))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::OpenMenu)],
            ),
            (
                &[BindKey::Mouse(MouseButton::Left)],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                    BindActionsCharacter::Fire,
                ))],
            ),
            (
                &[BindKey::Mouse(MouseButton::Right)],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                    BindActionsCharacter::Hook,
                ))],
            ),
            (
                &[BindKey::Extra(MouseExtra::WheelDown)],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                    BindActionsCharacter::PrevWeapon,
                ))],
            ),
            (
                &[BindKey::Extra(MouseExtra::WheelUp)],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                    BindActionsCharacter::NextWeapon,
                ))],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::Digit1))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                    BindActionsCharacter::Weapon(WeaponType::Hammer),
                ))],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::Digit2))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                    BindActionsCharacter::Weapon(WeaponType::Gun),
                ))],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::Digit3))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                    BindActionsCharacter::Weapon(WeaponType::Shotgun),
                ))],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::Digit4))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                    BindActionsCharacter::Weapon(WeaponType::Grenade),
                ))],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::Digit5))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                    BindActionsCharacter::Weapon(WeaponType::Laser),
                ))],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::KeyS))],
                vec![BindAction::LocalPlayer(
                    BindActionsLocalPlayer::ShowHookCollision,
                )],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::KeyG))],
                vec![BindAction::LocalPlayer(
                    BindActionsLocalPlayer::ToggleDummyCopyMoves,
                )],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::KeyH))],
                vec![BindAction::LocalPlayer(
                    BindActionsLocalPlayer::ToggleDummyHammerFly,
                )],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::Enter))],
                vec![BindAction::LocalPlayer(
                    BindActionsLocalPlayer::ActivateChatInput,
                )],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::KeyT))],
                vec![BindAction::LocalPlayer(
                    BindActionsLocalPlayer::ActivateChatInput,
                )],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::KeyU))],
                vec![BindAction::LocalPlayer(
                    BindActionsLocalPlayer::ShowChatHistory,
                )],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::KeyY))],
                vec![BindAction::LocalPlayer(
                    BindActionsLocalPlayer::ActivateSideOrStageChatInput,
                )],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::KeyI))],
                vec![BindAction::LocalPlayer(
                    BindActionsLocalPlayer::ActivateWhisperChatInput,
                )],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::Tab))],
                vec![BindAction::LocalPlayer(
                    BindActionsLocalPlayer::ShowScoreboard,
                )],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::ShiftLeft))],
                vec![BindAction::LocalPlayer(
                    BindActionsLocalPlayer::ShowEmoteWheel,
                )],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::ShiftRight))],
                vec![BindAction::LocalPlayer(
                    BindActionsLocalPlayer::ShowSpectatorSelection,
                )],
            ),
            (
                &[BindKey::Mouse(MouseButton::Middle)],
                vec![BindAction::LocalPlayer(
                    BindActionsLocalPlayer::ShowSpectatorSelection,
                )],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::KeyK))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::Kill)],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::KeyQ))],
                vec![BindAction::LocalPlayer(
                    BindActionsLocalPlayer::PhasedFreeCam,
                )],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::KeyP))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::FreeCam)],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::Pause))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::FreeCam)],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::F3))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::VoteYes)],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::F4))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::VoteNo)],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::NumpadSubtract))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::ZoomOut)],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::NumpadAdd))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::ZoomIn)],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::NumpadMultiply))],
                vec![BindAction::LocalPlayer(BindActionsLocalPlayer::ZoomReset)],
            ),
            (
                &[BindKey::Key(PhysicalKey::Code(KeyCode::PageUp))],
                vec![BindAction::Command(Command {
                    ident: "toggle".into(),
                    cmd_text: "toggle".into(),
                    cmd_range: 0..0,
                    args: vec![(
                        Syn::Command(Box::new(Command {
                            ident: "map.physics_layer_opacity".into(),
                            cmd_text: "map.physics_layer_opacity".into(),
                            cmd_range: 0..0,
                            args: vec![
                                (Syn::Number(0.to_string()), 0..0),
                                (Syn::Number(100.to_string()), 0..0),
                            ],
                        })),
                        0..0,
                    )],
                })],
            ),
        ] {
            on_bind(keys, actions);
        }
    }

    pub fn init_local_player_binds(
        config: &mut ConfigGame,
        binds: &mut Binds<Vec<BindAction>>,
        is_dummy: bool,
        console_entries: &[ConsoleEntry],
        cache: &ParserCache,
    ) {
        let map = gen_local_player_action_hash_map();

        if let Some(player) = if is_dummy {
            if config.profiles.dummy.copy_binds_from_main {
                config.players.get_mut(config.profiles.main as usize)
            } else {
                config.players.get_mut(config.profiles.dummy.index as usize)
            }
        } else {
            config.players.get_mut(config.profiles.main as usize)
        } {
            for bind in &player.binds {
                let cmds = parser::parse(bind, &entries_to_parser(console_entries), cache);
                for cmd in &cmds {
                    match cmd {
                        CommandType::Full(cmd) => match syn_to_bind(&cmd.args, &map) {
                            Ok((keys, actions)) => {
                                binds.register_bind(&keys, actions);
                            }
                            Err(err) => {
                                log::info!(
                                    "ignored invalid bind (syntax error): {bind}, err: {err}"
                                );
                            }
                        },
                        CommandType::Partial(err) => {
                            log::info!("ignored invalid bind: {bind}, err: {err}");
                        }
                    }
                }
            }

            if player.binds.is_empty() {
                let map = gen_local_player_action_hash_map_rev();
                Self::default_binds(|keys, actions| {
                    binds.register_bind(keys, actions);
                });
                Self::default_binds(|keys, actions| {
                    player.binds.push(bind_to_str(keys, actions, &map));
                });
            }
        }
    }

    pub fn handle_local_players_from_snapshot(
        local_players: &mut LocalPlayers,
        expected_local_players: &FxLinkedHashMap<u64, ClientConnectedPlayer>,
        config: &mut ConfigGame,
        console_entries: &[ConsoleEntry],
        mut snap_local_players: SnapshotLocalPlayers,
        cache: &ParserCache,
        options: &GameStateServerOptions,
    ) {
        local_players.retain_with_order(|player_id, _| {
            if let Some(ClientConnectedPlayer::Connected {
                player_id: client_player_id,
                ..
            }) = snap_local_players
                .get(player_id)
                .and_then(|p| expected_local_players.get(&p.id))
            {
                client_player_id == player_id
            } else {
                false
            }
        });
        snap_local_players.drain().for_each(|(id, snap_player)| {
            let Some(ClientConnectedPlayer::Connected {
                player_id: client_player_id,
                is_dummy,
            }) = expected_local_players.get(&snap_player.id)
            else {
                return;
            };
            if *client_player_id != id {
                return;
            }
            if !local_players.contains_key(&id) {
                let mut local_player: ClientPlayer = ClientPlayer {
                    is_dummy: *is_dummy,
                    zoom: options
                        .forced_ingame_camera_zoom
                        .map(|z| z.as_f64() as f32)
                        .unwrap_or(1.0),
                    ..Default::default()
                };
                let binds = &mut local_player.binds;
                Self::init_local_player_binds(config, binds, *is_dummy, console_entries, cache);

                local_players.insert(id, local_player);
            }
            // sort
            if let Some(local_player) = local_players.to_back(&id) {
                local_player.input_cam_mode = snap_player.input_cam_mode;
            }
        });
    }

    pub fn get_and_update_latest_input(
        &mut self,
        cur_time: Duration,
        time_per_tick: Duration,
        ticks_to_send: GameTickType,
        tick_of_inp: GameTickType,
        player_inputs: &mut FxLinkedHashMap<PlayerId, PoolVec<PlayerInputChainable>>,
        player_inputs_chainable_pool: &Pool<Vec<PlayerInputChainable>>,
    ) {
        let mut handle_character =
            |local_player_id: &CharacterId, local_player: &mut ClientPlayer| {
                let should_send_rates = local_player
                    .sent_input_time
                    .is_none_or(|time| cur_time - time >= time_per_tick);
                let consumable_input_changed =
                    local_player.sent_input.inp.consumable != local_player.input.inp.consumable;
                let send_by_input_change = (consumable_input_changed
                    && (!local_player
                        .input
                        .inp
                        .consumable
                        .only_weapon_diff_changed(&local_player.sent_input.inp.consumable)
                        || should_send_rates))
                    || local_player.sent_input.inp.state != local_player.input.inp.state
                    || (local_player.sent_input.inp.cursor != local_player.input.inp.cursor
                        && should_send_rates);
                let should_send_old_input =
                    tick_of_inp.saturating_sub(local_player.sent_inp_tick) < ticks_to_send;
                if send_by_input_change || (should_send_old_input && should_send_rates) {
                    local_player.sent_input_time = Some(cur_time);

                    if send_by_input_change {
                        local_player.sent_inp_tick = tick_of_inp;
                    }

                    let net_inp = &mut local_player.input;
                    net_inp.inc_version();
                    local_player.sent_input = *net_inp;

                    let player_input_chains = player_inputs
                        .entry(*local_player_id)
                        .or_insert_with(|| player_inputs_chainable_pool.new());

                    for tick in
                        tick_of_inp.saturating_sub(ticks_to_send.saturating_sub(1))..=tick_of_inp
                    {
                        if tick != tick_of_inp {
                            // look for old inputs from previous ticks
                            if let Some(old_inp) = self
                                .input_per_tick
                                .get(&tick)
                                .and_then(|inps| inps.get(local_player_id))
                            {
                                player_input_chains.push(PlayerInputChainable {
                                    for_monotonic_tick: tick,
                                    inp: *old_inp,
                                });
                            }
                        } else {
                            player_input_chains.push(PlayerInputChainable {
                                for_monotonic_tick: tick_of_inp,
                                inp: *net_inp,
                            });
                        }
                    }
                }
            };

        let mut copied_input = None;

        // handle the active player first
        let active_player = self.local.active_local_player_mut();
        let active_player_id = active_player.as_ref().map(|(&id, _)| id);
        if let Some((id, local_player)) = active_player {
            if self.dummy_control.dummy_copy_moves {
                copied_input = Some((
                    local_player
                        .input
                        .inp
                        .consumable
                        .diff(&local_player.sent_input.inp.consumable),
                    local_player.input.inp.state,
                    local_player.input.inp.cursor,
                    local_player.input.inp.viewport,
                ));
            }
            handle_character(id, local_player);
        }

        let local_players = &mut self.local.local_players;
        for (local_player_id, local_player) in local_players
            .iter_mut()
            .filter(|(&id, _)| Some(id) != active_player_id)
        {
            if let DummyHammerState::Active { last_hammer } = &self.dummy_control.dummy_hammer {
                if last_hammer
                    .is_none_or(|time| cur_time.saturating_sub(time) > Duration::from_millis(500))
                {
                    let cursor = CharacterInputCursor::from_vec2(&local_player.cursor_pos_dummy);
                    local_player.input.inp.cursor.set(cursor);
                    local_player.input.inp.consumable.fire.add(1, cursor);
                    local_player
                        .input
                        .inp
                        .consumable
                        .set_weapon_req(Some(WeaponType::Hammer));
                    local_player
                        .input
                        .inp
                        .state
                        .input_method_flags
                        .set(CharacterInputMethodFlags::DUMMY);
                }
            }
            if let Some((consumable, state, cursor, viewport)) = &copied_input {
                let mut inp = local_player.input.inp;
                if let Some((v, cursor)) = consumable.fire {
                    inp.consumable.fire.add(v.get(), cursor);
                }
                if let Some((v, cursor)) = consumable.hook {
                    inp.consumable.hook.add(v.get(), cursor);
                }
                if let Some(v) = consumable.weapon_req {
                    inp.consumable.set_weapon_req(Some(v));
                }
                if let Some(v) = consumable.weapon_diff {
                    inp.consumable.weapon_diff.add(v.get());
                }
                if let Some(v) = consumable.jump {
                    inp.consumable.jump.add(v.get());
                }
                inp.state = *state;
                inp.cursor = *cursor;
                inp.viewport = *viewport;
                inp.state
                    .input_method_flags
                    .set(CharacterInputMethodFlags::DUMMY);
                local_player
                    .input
                    .try_overwrite(&inp, local_player.input.version() + 1, true);
            }

            handle_character(local_player_id, local_player);
        }
        if let DummyHammerState::Active { last_hammer } = &mut self.dummy_control.dummy_hammer {
            if last_hammer
                .is_none_or(|time| cur_time.saturating_sub(time) > Duration::from_millis(500))
            {
                *last_hammer = Some(cur_time);
            }
        }
    }
}
