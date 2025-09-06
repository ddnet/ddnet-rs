use std::{
    collections::{HashMap, HashSet},
    net::{SocketAddrV4, SocketAddrV6},
    ops::Deref,
};

use base::{
    linked_hash_map_view::FxLinkedHashMap,
    network_string::{NetworkReducedAsciiString, NetworkString},
};
use game_interface::{interface::MAX_MAP_NAME_LEN, types::character_info::MAX_CHARACTER_NAME_LEN};
use serde::{Deserialize, Serialize};
use serde_with::DefaultOnError;
use serde_with::serde_as;
use url::Url;

use super::communities::{Community, ServerIpList};

#[serde_as]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Server {
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub name: String,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub servers: HashMap<String, ServerIpList>,
}

#[derive(Debug, Default, Clone)]
pub struct DdnetInfoCommunities(FxLinkedHashMap<String, Community>);

impl Serialize for DdnetInfoCommunities {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let res: Vec<Community> = self.0.values().cloned().collect();
        res.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DdnetInfoCommunities {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let res = <Vec<Community>>::deserialize(deserializer)?;
        Ok(Self(res.into_iter().map(|c| (c.id.clone(), c)).collect()))
    }
}

impl Deref for DdnetInfoCommunities {
    type Target = FxLinkedHashMap<String, Community>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[serde_as]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DdnetInfo {
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub name: NetworkString<MAX_CHARACTER_NAME_LEN>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub maps: HashSet<NetworkReducedAsciiString<MAX_MAP_NAME_LEN>>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub points: i64,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub servers: Vec<Server>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    #[serde(alias = "servers-kog")]
    pub servers_kog: Vec<Server>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub communities: DdnetInfoCommunities,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    #[serde(alias = "community-icons-download-url")]
    pub community_icons_download_url: Option<Url>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub news: NetworkString<2048>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    #[serde(alias = "map-download-url")]
    pub map_download_url: Option<Url>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub location: NetworkString<16>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub version: NetworkString<64>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    #[serde(alias = "stun-servers-ipv6")]
    pub stun_servers_ipv6: Vec<SocketAddrV6>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    #[serde(alias = "stun-servers-ipv4")]
    pub stun_servers_ipv4: Vec<SocketAddrV4>,
}
