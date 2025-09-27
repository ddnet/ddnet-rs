use base::linked_hash_map_view::FxLinkedHashMap;
use client_containers::{ctf::CtfContainer, skins::SkinContainer};
use client_render_base::render::tee::RenderTee;
use game_interface::types::{
    game::{GameTickType, NonZeroGameTickType},
    id_types::CharacterId,
    render::{character::CharacterInfo, game::GameRenderInfo},
};
use graphics::handles::{
    canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
};
use pool::datatypes::PoolString;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct RenderDateTime {
    pub time: PoolString,
    pub date: PoolString,
}

pub struct UserData<'a> {
    pub canvas_handle: &'a GraphicsCanvasHandle,
    pub stream_handle: &'a GraphicsStreamHandle,
    pub race_round_timer_counter: &'a GameTickType,
    pub ticks_per_second: &'a NonZeroGameTickType,
    pub game: Option<&'a GameRenderInfo>,

    pub skin_container: &'a mut SkinContainer,
    pub skin_renderer: &'a RenderTee,

    pub ctf_container: &'a mut CtfContainer,

    pub character_infos: &'a FxLinkedHashMap<CharacterId, CharacterInfo>,

    pub date_time: &'a Option<RenderDateTime>,
}
