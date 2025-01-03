use std::{
    collections::{HashSet, VecDeque},
    time::Duration,
};

use base::linked_hash_map_view::FxLinkedHashMap;
use client_containers::skins::SkinContainer;
use client_render_base::render::tee::RenderTee;
use client_types::chat::ServerMsg;
use game_interface::types::{
    id_types::{CharacterId, PlayerId},
    render::character::CharacterInfo,
};
use graphics::handles::{
    canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ChatMode {
    Global,
    Team,
    Whisper(Option<PlayerId>),
}

#[derive(Serialize, Deserialize)]
pub enum ChatEvent {
    CurMsg { msg: String, mode: ChatMode },
    MsgSend { msg: String, mode: ChatMode },
    ChatClosed,
    PlatformOutput(egui::PlatformOutput),
}

#[derive(Debug, Clone)]
pub struct MsgInChat {
    pub msg: ServerMsg,
    pub add_time: Duration,
}

pub struct UserData<'a> {
    pub entries: &'a VecDeque<MsgInChat>,
    pub msg: &'a mut String,
    pub is_input_active: bool,
    pub show_chat_history: bool,
    pub chat_events: &'a mut Vec<ChatEvent>,
    pub stream_handle: &'a GraphicsStreamHandle,
    pub canvas_handle: &'a GraphicsCanvasHandle,
    pub mode: ChatMode,

    pub character_infos: &'a FxLinkedHashMap<CharacterId, CharacterInfo>,
    pub local_character_ids: &'a HashSet<CharacterId>,

    pub skin_container: &'a mut SkinContainer,
    pub render_tee: &'a RenderTee,

    pub find_player_prompt: &'a mut String,
    pub find_player_id: &'a mut Option<PlayerId>,
    pub cur_whisper_player_id: &'a mut Option<PlayerId>,
}
