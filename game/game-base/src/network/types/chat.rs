use base::network_string::NetworkString;
use game_interface::types::{
    character_info::{MAX_ASSET_NAME_LEN, MAX_CHARACTER_NAME_LEN, NetworkSkinInfo},
    id_types::PlayerId,
    resource_key::NetworkResourceKey,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatPlayerInfo {
    pub id: PlayerId,
    pub name: NetworkString<MAX_CHARACTER_NAME_LEN>,
    pub skin: NetworkResourceKey<MAX_ASSET_NAME_LEN>,
    pub skin_info: NetworkSkinInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetChatMsgPlayerChannel {
    Global,
    GameTeam,
    // receiver
    Whisper(ChatPlayerInfo),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetChatMsg {
    pub sender: ChatPlayerInfo,
    pub msg: String,
    pub channel: NetChatMsgPlayerChannel,
}
