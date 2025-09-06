use base::network_string::NetworkString;
use game_interface::types::{
    character_info::{
        MAX_ASSET_NAME_LEN, MAX_CHARACTER_CLAN_LEN, MAX_CHARACTER_NAME_LEN, MAX_FLAG_NAME_LEN,
        NetworkSkinInfo,
    },
    resource_key::NetworkResourceKey,
};
use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Hiarc, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FavoritePlayer {
    pub name: NetworkString<MAX_CHARACTER_NAME_LEN>,
    pub clan: NetworkString<MAX_CHARACTER_CLAN_LEN>,
    pub skin: NetworkResourceKey<MAX_ASSET_NAME_LEN>,
    pub skin_info: NetworkSkinInfo,
    pub flag: NetworkString<MAX_FLAG_NAME_LEN>,
}

pub type FavoritePlayers = Vec<FavoritePlayer>;
