use game_base::network::types::chat::NetChatMsgPlayerChannel;
use game_interface::types::{character_info::NetworkSkinInfo, resource_key::ResourceKey};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMsg {
    pub player: String,
    pub clan: String,
    pub skin_name: ResourceKey,
    pub skin_info: NetworkSkinInfo,
    pub msg: String,
    pub channel: NetChatMsgPlayerChannel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMsgPlayerSkin {
    pub skin_name: ResourceKey,
    pub skin_info: NetworkSkinInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsgSystem {
    pub msg: String,
    pub front_skin: Option<SystemMsgPlayerSkin>,
    pub end_skin: Option<SystemMsgPlayerSkin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    Chat(ChatMsg),
    System(MsgSystem),
}
