pub mod list;
pub mod main_frame;

use std::ops::Deref;
use std::{collections::HashMap, net::SocketAddr};

use base_io::runtime::IoRuntimeTask;
use graphics::handles::texture::texture::TextureContainer;
use graphics_types::types::GraphicsBackendMemory;
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

#[serde_as]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Icon {
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub sha256: String,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub url: Option<Url>,
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
    pub servers: Vec<Server>,
    #[serde(default)]
    #[serde_as(as = "serde_with::VecSkipError<_>")]
    pub contact_urls: Vec<Url>,
}

#[derive(Debug)]
pub enum CommunityIcon {
    Icon {
        texture: TextureContainer,
        width: u32,
        height: u32,
    },
    Loading(Result<IoRuntimeTask<(GraphicsBackendMemory, u32, u32)>, String>),
}

pub type CommunityIcons = HashMap<String, CommunityIcon>;
