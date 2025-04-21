use std::collections::VecDeque;

use base::linked_hash_map_view::FxLinkedHashMap;
use client_containers::skins::SkinContainer;
use client_render_base::render::tee::RenderTee;
use game_interface::types::{id_types::CharacterId, render::character::CharacterInfo};
use graphics::handles::{
    canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpectatorSelectionEvent {
    FreeView,
    Selected(Vec<CharacterId>),
    /// End the current spectate
    Unspec,
    /// Toggle between phasing the character and
    /// normal spectating
    SwitchPhaseState,
}

pub struct UserData<'a> {
    pub canvas_handle: &'a GraphicsCanvasHandle,
    pub stream_handle: &'a GraphicsStreamHandle,
    pub skin_container: &'a mut SkinContainer,
    pub skin_renderer: &'a RenderTee,
    pub character_infos: &'a FxLinkedHashMap<CharacterId, CharacterInfo>,
    pub events: &'a mut VecDeque<SpectatorSelectionEvent>,

    /// Is the list result of a ingame spec (e.g. in ddrace /pause & /spec)
    pub ingame: bool,
    /// Whether spectating should lead to phased
    /// state of the character (/spec in ddnet)
    pub into_phased: bool,
}
