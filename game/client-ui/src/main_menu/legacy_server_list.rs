use master_server_types::addr::Addr;
use serde::Deserialize;
use serde_with::{DefaultOnError, serde_as};

#[derive(Debug, Clone, Deserialize)]
pub struct Skin {
    pub name: Option<String>,
    pub color_body: Option<i32>,
    pub color_feet: Option<i32>,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
pub struct Client {
    pub name: String,
    pub clan: String,
    pub country: i32,
    pub skin: Option<Skin>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub score: i64,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub afk: bool,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
pub struct Info {
    pub map: Map,
    pub name: String,
    pub game_type: String,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub version: String,
    pub max_clients: u32,
    pub max_players: u32,
    pub passworded: bool,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub requires_login: bool,
    #[serde(default)]
    #[serde_as(as = "serde_with::VecSkipError<_>")]
    pub clients: Vec<Client>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Map {
    pub name: String,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
pub struct Server {
    pub info: Info,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub location: String,
    #[serde_as(as = "serde_with::VecSkipError<_>")]
    pub addresses: Vec<Addr>,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
pub struct LegacyServerList {
    #[serde_as(as = "serde_with::VecSkipError<_>")]
    pub servers: Vec<Server>,
}
