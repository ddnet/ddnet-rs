use std::{collections::BTreeMap, time::Duration};

use base::linked_hash_map_view::FxLinkedHashMap;
use binds::binds::BindAction;
use client_ui::{chat::user_data::ChatMode, emote_wheel::user_data::EmoteWheelEvent};
use game_base::{network::messages::PlayerInputChainable, player_input::PlayerInput};
use game_interface::types::{
    game::GameTickType, id_types::PlayerId, render::character::PlayerCameraMode,
};
use input_binds::binds::Binds;
use math::math::vector::dvec2;
use pool::datatypes::PoolFxLinkedHashMap;

pub mod dummy_control;

pub type ClientPlayerInputPerTick =
    FxLinkedHashMap<GameTickType, PoolFxLinkedHashMap<PlayerId, PlayerInput>>;

#[derive(Debug)]
pub struct ServerInputForDiff {
    pub id: u64,
    pub inp: PlayerInputChainable,
}

#[derive(Debug)]
pub enum ClientPlayerZoomMode {
    ZoomingIn,
    ZoomingOut,
}

#[derive(Debug)]
pub struct ClientPlayerZoomState {
    pub mode: ClientPlayerZoomMode,
    pub last_apply_time: Option<Duration>,
}

#[derive(Debug, Default)]
pub struct ClientPlayer {
    pub input: PlayerInput,
    pub sent_input: PlayerInput,
    pub sent_input_time: Option<Duration>,
    /// The game tick the input was sent in
    pub sent_inp_tick: GameTickType,

    pub binds: Binds<Vec<BindAction>>,

    pub chat_input_active: Option<ChatMode>,
    pub chat_msg: String,

    /// show a longer chat history
    pub show_chat_all: bool,
    pub show_scoreboard: bool,

    pub emote_wheel_active: bool,
    pub last_emote_wheel_selection: Option<EmoteWheelEvent>,

    pub spectator_selection_active: bool,

    /// For updating the player info on the server.
    pub player_info_version: u64,

    /// last input the server knows about
    pub server_input: Option<ServerInputForDiff>,
    /// inputs the client still knows about,
    /// [`PlayerInputChainable`] here is always the last of a chain that is send.
    pub server_input_storage: BTreeMap<u64, PlayerInputChainable>,

    pub is_dummy: bool,
    pub cursor_pos_dummy: dvec2,

    pub zoom: f32,
    pub zoom_state: Option<ClientPlayerZoomState>,

    pub input_cam_mode: PlayerCameraMode,
    pub free_cam_pos: dvec2,
    pub cursor_pos: dvec2,
    /// Last known cursor position done by a real player
    pub player_cursor_pos: dvec2,
}

pub type LocalPlayers = FxLinkedHashMap<PlayerId, ClientPlayer>;
