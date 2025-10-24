pub mod list;
pub mod main_frame;

use std::ops::Deref;
use std::{collections::HashMap, net::SocketAddr};

use base::linked_hash_map_view::FxLinkedHashSet;
use serde::{Deserialize, Serialize};
use serde_with::DefaultOnError;
use serde_with::serde_as;
use url::Url;

#[serde_as]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ServerIpList(#[serde_as(as = "serde_with::VecSkipError<_>")] pub Vec<SocketAddr>);

impl Deref for ServerIpList {
    type Target = Vec<SocketAddr>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub name: String,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub servers: HashMap<String, ServerIpList>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IconUrlHash {
    Sha256 {
        sha256: String,
    },
    Blake3 {
        blake3: String,
    },
    #[default]
    None,
}

#[serde_as]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Icon {
    #[serde(flatten)]
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub hash: IconUrlHash,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub url: Option<Url>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    servers: Option<Vec<Server>>,
}

impl Icon {
    pub fn servers_for_ddnet_bug_workaround(&mut self) -> Option<Vec<Server>> {
        self.servers.take()
    }
}

#[derive(Debug, Default, Clone)]
pub struct CommunityServers {
    servers: Vec<Server>,

    all_ips: FxLinkedHashSet<SocketAddr>,
}

impl CommunityServers {
    pub fn new(servers: Vec<Server>) -> Self {
        Self {
            all_ips: servers
                .iter()
                .flat_map(|s| s.servers.iter().flat_map(|s| s.1.0.iter().copied()))
                .collect(),
            servers,
        }
    }
}

impl Serialize for CommunityServers {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.servers.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CommunityServers {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let servers = <Vec<Server>>::deserialize(deserializer)?;
        Ok(Self::new(servers))
    }
}

impl Deref for CommunityServers {
    type Target = FxLinkedHashSet<SocketAddr>;

    fn deref(&self) -> &Self::Target {
        &self.all_ips
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Community {
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub id: String,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub name: String,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub has_finishes: bool,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub icon: Icon,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub servers: CommunityServers,
    #[serde(default)]
    #[serde_as(as = "serde_with::VecSkipError<_>")]
    pub contact_urls: Vec<Url>,
}
