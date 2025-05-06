use std::collections::HashMap;

use base_io::io::Io;
use binds::binds::{
    gen_local_player_action_hash_map, BindAction, BindActionsCharacter, BindActionsHotkey,
    BindActionsLocalPlayer,
};
use client_render_base::map::render_tools::{CanvasType, RenderTools};
use client_types::console::ConsoleEntry;
use client_ui::chat::user_data::ChatMode;
use client_ui::console::utils::run_command;
use client_ui::emote_wheel::user_data::EmoteWheelEvent;
use command_parser::parser::CommandTypeRef;
use config::config::ConfigEngine;
use egui::{Context, CursorIcon};
use game_config::config::ConfigGame;
use game_interface::types::emoticons::EmoticonType;
use game_interface::types::id_types::PlayerId;
use game_interface::types::input::cursor::CharacterInputCursor;
use game_interface::types::input::dyn_cam::CharacterInputDynCamOffset;
use game_interface::types::input::viewport::CharacterInputViewport;
use game_interface::types::input::{
    CharacterInput, CharacterInputFlags, CharacterInputMethodFlags,
};
use game_interface::types::render::character::{PlayerCameraMode, TeeEye};
use game_interface::types::weapons::WeaponType;
use graphics::graphics::graphics::{Graphics, ScreenshotCb};
use math::math::{length, normalize_pre_length, vector::dvec2};

use input_binds::binds::{BindKey, Binds, MouseExtra};
use native::native::NativeImpl;
use native::native::{DeviceId, MouseButton, MouseScrollDelta, PhysicalKey, Window};
use ui_base::{types::UiState, ui::UiContainer};

use crate::game::data::{GameData, LocalPlayerGameData};
use crate::localplayer::dummy_control::{DummyControlState, DummyHammerState};
use crate::localplayer::{ClientPlayer, ClientPlayerZoomMode, ClientPlayerZoomState};

pub type DeviceToLocalPlayerIndex = HashMap<DeviceId, usize>;

#[derive(Debug, Clone)]
pub struct InputKeyEv {
    pub key: BindKey,
    pub is_down: bool,
    pub device: DeviceId,
}

#[derive(Debug, Clone)]
pub struct InputAxisMoveEv {
    pub device: DeviceId,
    pub xrel: f64,
    pub yrel: f64,
}

#[derive(Debug, Clone)]
pub enum InputEv {
    Key(InputKeyEv),
    Move(InputAxisMoveEv),
}

impl InputEv {
    pub fn device(&self) -> &DeviceId {
        match self {
            InputEv::Key(ev) => &ev.device,
            InputEv::Move(ev) => &ev.device,
        }
    }
}

pub struct InputCloneRes {
    pub egui: Option<egui::RawInput>,
    pub evs: Vec<InputEv>,
}

pub struct InputRes {
    pub egui: Option<egui::RawInput>,
}

struct Input {
    egui: Option<egui::RawInput>,
    evs: Vec<InputEv>,
}

impl Input {
    pub fn new() -> Self {
        Self {
            egui: Default::default(),
            evs: Default::default(),
        }
    }

    pub fn take(&mut self) -> InputRes {
        self.evs.clear();
        InputRes {
            egui: self.egui.take(),
        }
    }

    pub fn cloned(&mut self) -> InputCloneRes {
        InputCloneRes {
            egui: self.egui.clone(),
            evs: self.evs.clone(),
        }
    }
}

pub enum InputHandlingEvent {
    Kill {
        local_player_id: PlayerId,
    },
    Emoticon {
        local_player_id: PlayerId,
        emoticon: EmoticonType,
    },
    ChangeEyes {
        local_player_id: PlayerId,
        eye: TeeEye,
    },
    VoteYes,
    VoteNo,
}

pub struct InputHandling {
    pub state: egui_winit::State,

    last_known_cursor: Option<CursorIcon>,

    inp: Input,

    bind_cmds: HashMap<&'static str, BindActionsLocalPlayer>,
}

impl InputHandling {
    pub fn new(window: &Window) -> Self {
        let ctx = Context::default();
        ctx.options_mut(|options| {
            options.zoom_with_keyboard = false;
        });

        let bind_cmds = gen_local_player_action_hash_map();
        Self {
            state: egui_winit::State::new(
                ctx,
                Default::default(),
                window,
                Some(window.scale_factor().clamp(0.00001, f64::MAX) as f32),
                None,
                None,
            ),
            last_known_cursor: None,
            inp: Input::new(),
            bind_cmds,
        }
    }

    pub fn new_frame(&mut self) {
        self.inp.take();
    }

    /// use this if you want to consume the input, all further calls will get `None` (for the current frame)
    pub fn take_inp(&mut self) -> InputRes {
        self.inp.take()
    }

    /// clone the input and leave it there for other components
    pub fn clone_inp(&mut self) -> InputCloneRes {
        self.inp.cloned()
    }

    pub fn collect_events(&mut self) {
        self.inp.egui = Some(self.state.egui_input_mut().take());
    }

    pub fn set_last_known_cursor(&mut self, config: &ConfigEngine, cursor: CursorIcon) {
        if !config.inp.dbg_mode {
            self.last_known_cursor = Some(cursor);
        }
    }

    /// `apply_latest_known_cursor` is good if the ui that calls this
    /// actually doesn't have input focus right now
    pub fn handle_platform_output(
        &mut self,
        native: &mut dyn NativeImpl,
        mut platform_output: egui::PlatformOutput,
        apply_latest_known_cursor: bool,
    ) {
        if apply_latest_known_cursor {
            if let Some(cursor) = self.last_known_cursor {
                platform_output.cursor_icon = cursor;
            }
        }
        self.last_known_cursor = Some(platform_output.cursor_icon);
        native.relative_mouse(matches!(platform_output.cursor_icon, CursorIcon::None));
        self.state
            .handle_platform_output(native.borrow_window(), platform_output);
    }

    fn handle_binds_impl(
        ui: &mut UiContainer,
        local: &mut LocalPlayerGameData,
        dummy_control: &mut DummyControlState,
        evs: &mut Vec<InputHandlingEvent>,
        config_engine: &mut ConfigEngine,
        config_game: &mut ConfigGame,
        bind_cmds: &HashMap<&'static str, BindActionsLocalPlayer>,
        entries: &[ConsoleEntry],
    ) {
        let Some((local_player_id, local_player)) = local.active_local_player_mut() else {
            return;
        };
        let input = &mut local_player.input.inp;
        let actions = local_player.binds.process();

        #[derive(Debug, Default)]
        struct CharacterActions {
            dir: i32,
            dir_change: bool,
            jump: bool,
            jump_change: bool,
            fire: bool,
            fire_change: bool,
            hook: bool,
            hook_change: bool,
            next_weapon: Option<WeaponType>,
            weapon_diff: i64,
            reset: bool,
        }
        impl CharacterActions {
            fn changes_by_reset(&mut self) {
                self.dir = 0;
                self.dir_change = true;
                self.jump = false;
                self.jump_change = true;
                self.fire = false;
                self.fire_change = true;
                self.hook = false;
                self.hook_change = true;
            }
        }
        let mut flags = CharacterInputFlags::default();

        let mut character = CharacterActions::default();
        let mut dummy = CharacterActions::default();

        let mut is_zooming = false;

        let mut dummy_fire_aim_character = false;

        let mut next_show_scoreboard = false;
        let mut next_show_chat_all = false;
        let mut next_show_emote_wheel = false;
        let mut next_show_spectator_selection = false;
        for actions in actions.cur_actions.iter() {
            for action in actions {
                fn char_action(action: &BindActionsCharacter, character: &mut CharacterActions) {
                    match action {
                        BindActionsCharacter::MoveLeft => character.dir -= 1,
                        BindActionsCharacter::MoveRight => character.dir += 1,
                        BindActionsCharacter::Jump => character.jump = true,
                        BindActionsCharacter::Fire => character.fire = true,
                        BindActionsCharacter::Hook => character.hook = true,
                        BindActionsCharacter::NextWeapon
                        | BindActionsCharacter::PrevWeapon
                        | BindActionsCharacter::ResetInput
                        | BindActionsCharacter::Weapon(_) => {
                            // only listen for press or unpress
                        }
                    }
                }
                let mut handle_action = |action: &BindActionsLocalPlayer| match action {
                    BindActionsLocalPlayer::Character(action) => {
                        char_action(action, &mut character)
                    }
                    BindActionsLocalPlayer::Dummy(action) => char_action(action, &mut dummy),
                    BindActionsLocalPlayer::DummyFireAimCharacter => {
                        // set the aim request in dummy controls
                        dummy_fire_aim_character = true;
                    }
                    BindActionsLocalPlayer::ShowHookCollision => {
                        flags |= CharacterInputFlags::HOOK_COLLISION_LINE;
                    }
                    BindActionsLocalPlayer::ShowScoreboard => {
                        next_show_scoreboard = true;
                    }
                    BindActionsLocalPlayer::ShowChatHistory => {
                        next_show_chat_all = true;
                    }
                    BindActionsLocalPlayer::ShowEmoteWheel => {
                        next_show_emote_wheel = true;
                    }
                    BindActionsLocalPlayer::ShowSpectatorSelection => {
                        next_show_spectator_selection = true;
                    }
                    BindActionsLocalPlayer::OpenMenu => {
                        // only listen for click
                    }
                    BindActionsLocalPlayer::ActivateChatInput => {
                        // only listen for click
                    }
                    BindActionsLocalPlayer::ActivateSideOrStageChatInput => {
                        // only listen for click
                    }
                    BindActionsLocalPlayer::ActivateWhisperChatInput => {
                        // only listen for click
                    }
                    BindActionsLocalPlayer::Kill => {
                        // only listen for click
                    }
                    BindActionsLocalPlayer::ToggleDummyCopyMoves => {
                        // only listen for press
                    }
                    BindActionsLocalPlayer::ToggleDummyHammerFly => {
                        // only listen for press
                    }
                    BindActionsLocalPlayer::VoteYes => {
                        // only listen for click
                    }
                    BindActionsLocalPlayer::VoteNo => {
                        // only listen for click
                    }
                    BindActionsLocalPlayer::ZoomOut => {
                        is_zooming = true;
                    }
                    BindActionsLocalPlayer::ZoomIn => {
                        is_zooming = true;
                    }
                    BindActionsLocalPlayer::ZoomReset => {
                        // only listen for press
                    }
                    BindActionsLocalPlayer::FreeCam => {
                        // only listen for press
                    }
                    BindActionsLocalPlayer::PhasedFreeCam => {
                        // only listen for press
                    }
                };
                match action {
                    BindAction::LocalPlayer(action) => {
                        handle_action(action);
                    }
                    BindAction::Command(cmd) | BindAction::TriggerCommand(cmd) => {
                        if let Some(action) = bind_cmds.get(cmd.ident.as_str()) {
                            handle_action(action);
                        }
                    }
                }
            }
        }
        for actions in actions.press_actions.iter() {
            fn char_action(action: &BindActionsCharacter, character: &mut CharacterActions) {
                match action {
                    BindActionsCharacter::MoveLeft => character.dir_change = true,
                    BindActionsCharacter::MoveRight => character.dir_change = true,
                    BindActionsCharacter::Jump => character.jump_change = true,
                    BindActionsCharacter::Fire => character.fire_change = true,
                    BindActionsCharacter::Hook => character.hook_change = true,
                    BindActionsCharacter::Weapon(weapon) => {
                        character.next_weapon = Some(*weapon);
                    }
                    BindActionsCharacter::NextWeapon => {
                        character.weapon_diff += 1;
                    }
                    BindActionsCharacter::PrevWeapon => {
                        character.weapon_diff -= 1;
                    }
                    BindActionsCharacter::ResetInput => {
                        // only on unpress
                    }
                }
            }
            for action in actions {
                let mut handle_action = |action: &BindActionsLocalPlayer| match action {
                    BindActionsLocalPlayer::Character(action) => {
                        char_action(action, &mut character);
                    }
                    BindActionsLocalPlayer::Dummy(action) => {
                        char_action(action, &mut dummy);
                    }
                    BindActionsLocalPlayer::ToggleDummyCopyMoves => {
                        dummy_control.dummy_copy_moves = !dummy_control.dummy_copy_moves;
                    }
                    BindActionsLocalPlayer::ToggleDummyHammerFly => {
                        dummy_control.dummy_hammer = match dummy_control.dummy_hammer {
                            DummyHammerState::None => DummyHammerState::Active {
                                last_hammer: Default::default(),
                            },
                            DummyHammerState::Active { .. } => DummyHammerState::None,
                        };
                    }
                    BindActionsLocalPlayer::ZoomOut => {
                        local_player.zoom_state = Some(ClientPlayerZoomState {
                            mode: ClientPlayerZoomMode::ZoomingOut,
                            last_apply_time: None,
                        });
                        is_zooming = true;
                    }
                    BindActionsLocalPlayer::ZoomIn => {
                        local_player.zoom_state = Some(ClientPlayerZoomState {
                            mode: ClientPlayerZoomMode::ZoomingIn,
                            last_apply_time: None,
                        });
                        is_zooming = true;
                    }
                    BindActionsLocalPlayer::ZoomReset => {
                        local_player.zoom_state = None;
                        local_player.zoom = 1.0;
                    }
                    _ => {
                        // ignore rest
                    }
                };
                match action {
                    BindAction::LocalPlayer(action) => {
                        handle_action(action);
                    }
                    BindAction::Command(cmd) => {
                        if let Some(action) = bind_cmds.get(cmd.ident.as_str()) {
                            handle_action(action);
                        }
                    }
                    BindAction::TriggerCommand(cmd) => {
                        if let Some(action) = bind_cmds.get(cmd.ident.as_str()) {
                            handle_action(action);
                        } else if let Err(err) = run_command(
                            CommandTypeRef::Full(cmd),
                            entries,
                            config_engine,
                            config_game,
                            true,
                        ) {
                            log::debug!("{err}");
                        }
                    }
                }
            }
        }
        for actions in actions.unpress_actions.iter() {
            fn char_action(action: &BindActionsCharacter, character: &mut CharacterActions) {
                match action {
                    BindActionsCharacter::MoveLeft => character.dir_change = true,
                    BindActionsCharacter::MoveRight => character.dir_change = true,
                    BindActionsCharacter::Jump => character.jump_change = true,
                    BindActionsCharacter::Fire => character.fire_change = true,
                    BindActionsCharacter::Hook => character.hook_change = true,
                    BindActionsCharacter::Weapon(_)
                    | BindActionsCharacter::NextWeapon
                    | BindActionsCharacter::PrevWeapon => {
                        // ignore
                    }
                    BindActionsCharacter::ResetInput => {
                        character.reset = true;
                    }
                }
            }
            for action in actions {
                let mut handle_action = |action: &BindActionsLocalPlayer| match action {
                    BindActionsLocalPlayer::Character(action) => {
                        char_action(action, &mut character);
                    }
                    BindActionsLocalPlayer::Dummy(action) => {
                        char_action(action, &mut dummy);
                    }
                    _ => {
                        // ignore rest
                    }
                };
                match action {
                    BindAction::LocalPlayer(action) => {
                        handle_action(action);
                    }
                    BindAction::Command(_) | BindAction::TriggerCommand(_) => {
                        // ignore
                    }
                }
            }
        }
        for actions in actions.click_actions.iter() {
            for action in actions {
                let mut handle_action = |action: &BindActionsLocalPlayer| match action {
                    BindActionsLocalPlayer::OpenMenu => {
                        if local_player.chat_input_active.is_some() {
                            local_player.chat_input_active = None;
                        } else {
                            ui.ui_state.is_ui_open = true;
                        }
                    }
                    BindActionsLocalPlayer::ActivateChatInput => {
                        local_player.chat_input_active = Some(ChatMode::Global);
                    }
                    BindActionsLocalPlayer::ActivateSideOrStageChatInput => {
                        local_player.chat_input_active = Some(ChatMode::Team);
                    }
                    BindActionsLocalPlayer::ActivateWhisperChatInput => {
                        if !matches!(local_player.chat_input_active, Some(ChatMode::Whisper(_))) {
                            local_player.chat_input_active = Some(ChatMode::Whisper(None));
                        }
                    }
                    BindActionsLocalPlayer::Kill => evs.push(InputHandlingEvent::Kill {
                        local_player_id: *local_player_id,
                    }),
                    BindActionsLocalPlayer::VoteYes => {
                        evs.push(InputHandlingEvent::VoteYes);
                    }
                    BindActionsLocalPlayer::VoteNo => {
                        evs.push(InputHandlingEvent::VoteNo);
                    }
                    BindActionsLocalPlayer::FreeCam => {
                        // only listen for press
                    }
                    BindActionsLocalPlayer::PhasedFreeCam => {
                        // only listen for press
                    }
                    _ => {}
                };
                match action {
                    BindAction::LocalPlayer(action) => {
                        handle_action(action);
                    }
                    BindAction::Command(cmd) | BindAction::TriggerCommand(cmd) => {
                        if let Some(action) = bind_cmds.get(cmd.ident.as_str()) {
                            handle_action(action);
                        } else if let Err(err) = run_command(
                            CommandTypeRef::Full(cmd),
                            entries,
                            config_engine,
                            config_game,
                            true,
                        ) {
                            log::debug!("{err}");
                        }
                    }
                }
            }
        }

        fn set(input: &mut CharacterInput, character: CharacterActions) {
            if character.weapon_diff != 0 {
                input.consumable.weapon_diff.add(character.weapon_diff)
            }
            if !*input.state.jump && character.jump && character.jump_change {
                input.consumable.jump.add(1)
            }
            if !*input.state.fire && character.fire && character.fire_change {
                input.consumable.fire.add(1, *input.cursor);
            }
            if !*input.state.hook && character.hook && character.hook_change {
                input.consumable.hook.add(1, *input.cursor);
            }
            if character.jump_change {
                input.state.jump.set(character.jump);
            }
            if character.fire_change {
                input.state.fire.set(character.fire);
            }
            if character.hook_change {
                input.state.hook.set(character.hook);
            }
            if character.dir_change {
                input.state.dir.set(character.dir.clamp(-1, 1));
            }
            input.consumable.set_weapon_req(character.next_weapon);
        }
        if character.reset {
            character.changes_by_reset();
            local_player.binds.reset_cur_keys();
        }
        if !next_show_spectator_selection {
            set(input, character);
        }

        local_player.show_scoreboard = next_show_scoreboard;
        local_player.show_chat_all = next_show_chat_all;

        if !is_zooming {
            local_player.zoom_state = None;
        }

        input
            .state
            .input_method_flags
            .set(CharacterInputMethodFlags::MOUSE_KEYBOARD);

        // generate emoticon/tee-eye event if needed
        if local_player.emote_wheel_active
            && !next_show_emote_wheel
            && local_player.last_emote_wheel_selection.is_some()
        {
            let ev = local_player.last_emote_wheel_selection.unwrap();
            match ev {
                EmoteWheelEvent::EmoticonSelected(emoticon) => {
                    evs.push(InputHandlingEvent::Emoticon {
                        local_player_id: *local_player_id,
                        emoticon,
                    });
                }
                EmoteWheelEvent::EyeSelected(eye) => {
                    evs.push(InputHandlingEvent::ChangeEyes {
                        local_player_id: *local_player_id,
                        eye,
                    });
                }
            }
        }
        local_player.emote_wheel_active = next_show_emote_wheel;

        local_player.spectator_selection_active = next_show_spectator_selection;

        if local_player.chat_input_active.is_some() {
            flags |= CharacterInputFlags::CHATTING;
        }
        if local_player.show_scoreboard {
            flags |= CharacterInputFlags::SCOREBOARD;
        }
        if ui.ui_state.is_ui_open {
            flags |= CharacterInputFlags::MENU_UI;
        }
        input.state.flags.set(flags);

        if let Some((_, local_dummy)) = local.first_inactive_local_players_mut() {
            if dummy.reset {
                dummy.changes_by_reset();
                local_dummy.binds.reset_cur_keys();
            }
            let will_hook = (*local_dummy.input.inp.state.hook && !dummy.hook_change)
                || (dummy.hook && dummy.hook_change);
            if dummy_fire_aim_character && !will_hook {
                local_dummy.cursor_pos = local_dummy.cursor_pos_dummy;
                let cursor = CharacterInputCursor::from_vec2(&local_dummy.cursor_pos_dummy);
                local_dummy.input.inp.cursor.set(cursor);
            } else if will_hook || !dummy_fire_aim_character {
                // For hooks reset to human input cursor
                local_dummy.cursor_pos = local_dummy.player_cursor_pos;
                let cursor = local_dummy.cursor_pos;
                let cursor = CharacterInputCursor::from_vec2(&cursor);
                local_dummy.input.inp.cursor.set(cursor);
            }
            if !next_show_spectator_selection {
                set(&mut local_dummy.input.inp, dummy);
            }

            local_dummy
                .input
                .inp
                .state
                .input_method_flags
                .set(CharacterInputMethodFlags::DUMMY);
        }
    }

    fn handle_global_binds_impl(
        global_binds: &mut Binds<BindActionsHotkey>,
        graphics: &Graphics,

        local_console_state: &mut UiState,
        mut remote_console_state: Option<&mut UiState>,
        debug_hud_state: &mut UiState,

        open_editor: &mut bool,

        io: &Io,
    ) {
        let actions = global_binds.process();
        for action in actions.click_actions.iter() {
            match action {
                BindActionsHotkey::Screenshot => {
                    let io = io.clone();
                    #[derive(Debug)]
                    struct Screenshot {
                        io: Io,
                    }
                    impl ScreenshotCb for Screenshot {
                        fn on_screenshot(&self, png: anyhow::Result<Vec<u8>>) {
                            match png {
                                Ok(png) => {
                                    let fs = self.io.fs.clone();

                                    self.io.rt.spawn_without_lifetime(async move {
                                        fs.create_dir("screenshots".as_ref()).await?;
                                        fs.write_file(
                                            format!(
                                                "screenshots/{}.png",
                                                chrono::Local::now().format("%Y_%m_%d_%H_%M_%S")
                                            )
                                            .as_ref(),
                                            png,
                                        )
                                        .await?;
                                        Ok(())
                                    });
                                }
                                Err(err) => {
                                    log::error!(target: "screenshot", "{err}");
                                }
                            }
                        }
                    }
                    graphics.do_screenshot(Screenshot { io }).unwrap();
                }
                BindActionsHotkey::LocalConsole => {
                    local_console_state.is_ui_open = !local_console_state.is_ui_open;
                    if let Some(remote_console_state) = remote_console_state.as_deref_mut() {
                        remote_console_state.is_ui_open = false;
                    }
                }
                BindActionsHotkey::RemoteConsole => {
                    if let Some(remote_console_state) = remote_console_state.as_deref_mut() {
                        remote_console_state.is_ui_open = !remote_console_state.is_ui_open;
                    }
                    local_console_state.is_ui_open = false;
                }
                BindActionsHotkey::ConsoleClose => {
                    local_console_state.is_ui_open = false;
                    if let Some(remote_console_state) = remote_console_state.as_deref_mut() {
                        remote_console_state.is_ui_open = false;
                    }
                }
                BindActionsHotkey::DebugHud => {
                    debug_hud_state.is_ui_open = !debug_hud_state.is_ui_open;
                }
                BindActionsHotkey::OpenEditor => {
                    *open_editor = true;
                }
            }
        }
    }

    fn get_max_mouse_distance(config: &ConfigGame) -> f64 {
        let camera_max_distance = 200.0;
        let follow_factor = config.inp.follow_factor_or_zero() / 100.0;
        let dead_zone = config.inp.deadzone_or_zero();
        let max_distance = config.inp.max_distance();
        (if follow_factor != 0.0 {
            camera_max_distance / follow_factor + dead_zone
        } else {
            max_distance
        })
        .min(max_distance)
    }

    pub fn dyn_camera_offset(config: &ConfigGame, local_player: &ClientPlayer) -> dvec2 {
        let mouse_pos = local_player.input.inp.cursor.to_vec2() * 32.0;
        let mouse_len = length(&mouse_pos);
        let follow_factor = config.inp.follow_factor_or_zero() / 100.0;
        let dead_zone = config.inp.deadzone_or_zero();

        let offset = ((mouse_len - dead_zone).max(0.0) * follow_factor) / 32.0;
        normalize_pre_length(&mouse_pos, mouse_len) * offset
    }

    pub fn clamp_cursor(config: &ConfigGame, local_player: &mut ClientPlayer) {
        let mouse_max = Self::get_max_mouse_distance(config);
        let min_distance = config.inp.min_distance();
        let mouse_min = min_distance;

        let cursor = local_player.input.inp.cursor.to_vec2() * 32.0;
        let mut mouse_distance = length(&cursor);
        if mouse_distance < 0.001 {
            local_player
                .input
                .inp
                .cursor
                .set(CharacterInputCursor::from_vec2(&dvec2::new(0.001, 0.0)));
            mouse_distance = 0.001;
        }
        if mouse_distance < mouse_min {
            local_player
                .input
                .inp
                .cursor
                .set(CharacterInputCursor::from_vec2(
                    &((normalize_pre_length(&cursor, mouse_distance) * mouse_min) / 32.0),
                ));
        }
        let cursor = local_player.input.inp.cursor.to_vec2() * 32.0;
        mouse_distance = length(&cursor);
        if mouse_distance > mouse_max {
            local_player
                .input
                .inp
                .cursor
                .set(CharacterInputCursor::from_vec2(
                    &((normalize_pre_length(&cursor, mouse_distance) * mouse_max) / 32.0),
                ));
        }
    }

    pub fn handle_global_binds(
        &self,
        global_binds: &mut Binds<BindActionsHotkey>,
        local_console_ui: &mut UiContainer,
        mut remote_console_ui: Option<&mut UiContainer>,
        debug_hud_ui: &mut UiContainer,
        open_editor: &mut bool,
        graphics: &Graphics,
        io: &Io,
    ) {
        for ev in &self.inp.evs {
            match ev {
                InputEv::Key(key_ev) => {
                    match &key_ev.key {
                        BindKey::Key(_) | BindKey::Mouse(_) => {
                            if key_ev.is_down {
                                global_binds.handle_key_down(&key_ev.key);
                            } else {
                                global_binds.handle_key_up(&key_ev.key);
                            }
                        }
                        BindKey::Extra(_) => {
                            global_binds.handle_key_down(&key_ev.key);
                            Self::handle_global_binds_impl(
                                global_binds,
                                graphics,
                                &mut local_console_ui.ui_state,
                                remote_console_ui.as_mut().map(|ui| &mut ui.ui_state),
                                &mut debug_hud_ui.ui_state,
                                open_editor,
                                io,
                            );
                            global_binds.handle_key_up(&key_ev.key);
                        }
                    }
                    Self::handle_global_binds_impl(
                        global_binds,
                        graphics,
                        &mut local_console_ui.ui_state,
                        remote_console_ui.as_mut().map(|ui| &mut ui.ui_state),
                        &mut debug_hud_ui.ui_state,
                        open_editor,
                        io,
                    );
                }
                InputEv::Move(_) => {}
            }
        }
    }

    /// returns a list of immediate events that are a result of an input
    pub fn handle_player_binds(
        &mut self,
        game_data: &mut GameData,
        ui: &mut UiContainer,
        config_engine: &mut ConfigEngine,
        config_game: &mut ConfigGame,
        graphics: &Graphics,
        entries: &[ConsoleEntry],
    ) -> Vec<InputHandlingEvent> {
        let mut res = Vec::new();

        self.inp.evs.retain(|ev| {
            if game_data
                .device_to_local_player_index
                .get(ev.device())
                .copied()
                .unwrap_or(0)
                < game_data.local.local_players.len()
                || game_data.local.local_players.len() == 1
            {
                let Some((local_player_id, local_player)) =
                    game_data.local.active_local_player_mut()
                else {
                    return false;
                };
                if local_player.chat_input_active.is_none() {
                    let canvas = if config_game.cl.render.use_ingame_aspect_ratio {
                        CanvasType::Custom {
                            aspect_ratio: config_game.cl.render.ingame_aspect_ratio as f32,
                        }
                    } else {
                        CanvasType::Handle(&graphics.canvas_handle)
                    };
                    let points = RenderTools::canvas_points_of_group(
                        canvas,
                        0.0,
                        0.0,
                        None,
                        local_player.zoom,
                        true,
                    );
                    let vp_width = points[2] as f64 - points[0] as f64;
                    let vp_height = points[3] as f64 - points[1] as f64;
                    match ev {
                        InputEv::Key(key_ev) => match &key_ev.key {
                            BindKey::Key(_) | BindKey::Mouse(_) => {
                                if key_ev.is_down {
                                    local_player.binds.handle_key_down(&key_ev.key);
                                } else {
                                    local_player.binds.handle_key_up(&key_ev.key);
                                }
                                Self::handle_binds_impl(
                                    ui,
                                    &mut game_data.local,
                                    &mut game_data.dummy_control,
                                    &mut res,
                                    config_engine,
                                    config_game,
                                    &self.bind_cmds,
                                    entries,
                                );
                            }
                            BindKey::Extra(_) => {
                                local_player.binds.handle_key_down(&key_ev.key);
                                Self::handle_binds_impl(
                                    ui,
                                    &mut game_data.local,
                                    &mut game_data.dummy_control,
                                    &mut res,
                                    config_engine,
                                    config_game,
                                    &self.bind_cmds,
                                    entries,
                                );
                                let Some((_, local_player)) =
                                    game_data.local.active_local_player_mut()
                                else {
                                    panic!("this should have been checked earlier");
                                };
                                local_player.binds.handle_key_up(&key_ev.key);
                            }
                        },
                        InputEv::Move(move_ev)
                            if !local_player.emote_wheel_active
                                && !local_player.spectator_selection_active =>
                        {
                            let factor = config_game.inp.sensitivity() / 100.0;

                            if let Some(cam_mode) = game_data
                                .cached_character_infos
                                .get(local_player_id)
                                .and_then(|c| c.player_info.as_ref())
                                .map(|s| &s.cam_mode)
                            {
                                match cam_mode {
                                    PlayerCameraMode::Default => {
                                        let cur = local_player.player_cursor_pos * 32.0;
                                        local_player.input.inp.cursor.set(
                                            CharacterInputCursor::from_vec2(
                                                &((cur
                                                    + dvec2::new(move_ev.xrel, move_ev.yrel)
                                                        * factor)
                                                    / 32.0),
                                            ),
                                        );
                                        Self::clamp_cursor(config_game, local_player);
                                        local_player.cursor_pos =
                                            local_player.input.inp.cursor.to_vec2();
                                        local_player.player_cursor_pos = local_player.cursor_pos;
                                    }
                                    PlayerCameraMode::Free => {
                                        let cur = local_player.free_cam_pos;

                                        let x_ratio = move_ev.xrel
                                            / graphics.canvas_handle.window_canvas_width() as f64;
                                        let y_ratio = move_ev.yrel
                                            / graphics.canvas_handle.window_canvas_height() as f64;

                                        let x = x_ratio * vp_width;
                                        let y = y_ratio * vp_height;
                                        // TODO: respect zoom;

                                        let new = cur + dvec2::new(x, y);
                                        local_player
                                            .input
                                            .inp
                                            .cursor
                                            .set(CharacterInputCursor::from_vec2(&new));
                                        local_player.free_cam_pos = new;
                                    }
                                    PlayerCameraMode::LockedTo { .. }
                                    | PlayerCameraMode::LockedOn { .. } => {
                                        // don't alter the cursor
                                    }
                                }
                                local_player.cursor_last_cam_mode = Some(cam_mode.clone());
                            }
                        }
                        InputEv::Move(_) => {
                            // else ignore mouse movement
                        }
                    }

                    let Some((_, local_player)) = game_data.local.active_local_player_mut() else {
                        panic!("this should have been checked earlier");
                    };
                    local_player
                        .input
                        .inp
                        .viewport
                        .set(CharacterInputViewport::from_vec2(&dvec2::new(
                            vp_width, vp_height,
                        )));

                    local_player.input.inp.dyn_cam_offset.set(
                        CharacterInputDynCamOffset::from_vec2(Self::dyn_camera_offset(
                            config_game,
                            local_player,
                        )),
                    );

                    local_player.emote_wheel_active || local_player.spectator_selection_active
                } else {
                    true
                }
            } else {
                true
            }
        });

        res
    }

    pub fn key_down(
        &mut self,
        _window: &native::native::Window,
        device: &DeviceId,
        key: &PhysicalKey,
    ) {
        self.inp.evs.push(InputEv::Key(InputKeyEv {
            key: BindKey::Key(*key),
            is_down: true,
            device: *device,
        }));
    }

    pub fn key_up(
        &mut self,
        _window: &native::native::Window,
        device: &DeviceId,
        key: &PhysicalKey,
    ) {
        self.inp.evs.push(InputEv::Key(InputKeyEv {
            key: BindKey::Key(*key),
            is_down: false,
            device: *device,
        }));
    }

    pub fn mouse_down(
        &mut self,
        _window: &native::native::Window,
        device: &DeviceId,
        _x: f64,
        _y: f64,
        btn: &MouseButton,
    ) {
        self.inp.evs.push(InputEv::Key(InputKeyEv {
            key: BindKey::Mouse(*btn),
            is_down: true,
            device: *device,
        }));
    }

    pub fn mouse_up(
        &mut self,
        _window: &native::native::Window,
        device: &DeviceId,
        _x: f64,
        _y: f64,
        btn: &MouseButton,
    ) {
        self.inp.evs.push(InputEv::Key(InputKeyEv {
            key: BindKey::Mouse(*btn),
            is_down: false,
            device: *device,
        }));
    }

    pub fn mouse_move(
        &mut self,
        _window: &native::native::Window,
        device: &DeviceId,
        _x: f64,
        _y: f64,
        xrel: f64,
        yrel: f64,
    ) {
        self.inp.evs.push(InputEv::Move(InputAxisMoveEv {
            device: *device,
            xrel,
            yrel,
        }))
    }

    pub fn scroll(
        &mut self,
        _window: &native::native::Window,
        device: &DeviceId,
        _x: f64,
        _y: f64,
        delta: &MouseScrollDelta,
    ) {
        let wheel_dir = {
            match delta {
                MouseScrollDelta::LineDelta(_, delta) => {
                    if *delta < 0.0 {
                        MouseExtra::WheelDown
                    } else {
                        MouseExtra::WheelUp
                    }
                }
                MouseScrollDelta::PixelDelta(delta) => {
                    if delta.y < 0.0 {
                        MouseExtra::WheelDown
                    } else {
                        MouseExtra::WheelUp
                    }
                }
            }
        };
        self.inp.evs.push(InputEv::Key(InputKeyEv {
            key: BindKey::Extra(wheel_dir),
            is_down: false,
            device: *device,
        }));
    }

    fn consumable_event(event: &native::native::WindowEvent) -> bool {
        // we basically only want input events to be consumable
        match event {
            native::native::WindowEvent::ActivationTokenDone { .. } => false,
            native::native::WindowEvent::Resized(_) => false,
            native::native::WindowEvent::Moved(_) => false,
            native::native::WindowEvent::CloseRequested => false,
            native::native::WindowEvent::Destroyed => false,
            native::native::WindowEvent::DroppedFile(_) => false,
            native::native::WindowEvent::HoveredFile(_) => false,
            native::native::WindowEvent::HoveredFileCancelled => false,
            native::native::WindowEvent::Focused(_) => false,
            native::native::WindowEvent::KeyboardInput { .. } => true,
            native::native::WindowEvent::ModifiersChanged(_) => true,
            native::native::WindowEvent::Ime(_) => true,
            native::native::WindowEvent::CursorMoved { .. } => true,
            native::native::WindowEvent::CursorEntered { .. } => true,
            native::native::WindowEvent::CursorLeft { .. } => true,
            native::native::WindowEvent::MouseWheel { .. } => true,
            native::native::WindowEvent::MouseInput { .. } => true,
            native::native::WindowEvent::TouchpadPressure { .. } => true,
            native::native::WindowEvent::AxisMotion { .. } => true,
            native::native::WindowEvent::Touch(_) => true,
            native::native::WindowEvent::ScaleFactorChanged { .. } => false,
            native::native::WindowEvent::ThemeChanged(_) => false,
            native::native::WindowEvent::Occluded(_) => false,
            native::native::WindowEvent::RedrawRequested => false,
            native::native::WindowEvent::PinchGesture { .. } => false,
            native::native::WindowEvent::PanGesture { .. } => false,
            native::native::WindowEvent::DoubleTapGesture { .. } => false,
            native::native::WindowEvent::RotationGesture { .. } => false,
        }
    }

    pub fn raw_event(&mut self, window: &Window, event: &native::native::WindowEvent) {
        if !Self::consumable_event(event) {
            return;
        }

        let _ = self.state.on_window_event(window, event);
    }
}
