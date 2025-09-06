use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::net::SocketAddrV4;
use std::net::SocketAddrV6;
use std::sync::Arc;
use std::time::Duration;

use base::hash::Hash;
use base::network_string::NetworkReducedAsciiString;
use base::network_string::NetworkString;
use game_config::config::MAX_SERVER_NAME_LEN;
use game_interface::account_info::MAX_ACCOUNT_NAME_LEN;
use game_interface::interface::MAX_MAP_NAME_LEN;
use game_interface::interface::MAX_PHYSICS_GAME_TYPE_NAME_LEN;
use game_interface::interface::MAX_VERSION_LEN;
use game_interface::types::character_info::MAX_ASSET_NAME_LEN;
use game_interface::types::character_info::MAX_CHARACTER_CLAN_LEN;
use game_interface::types::character_info::MAX_CHARACTER_NAME_LEN;
use game_interface::types::character_info::MAX_FLAG_NAME_LEN;
use game_interface::types::character_info::NetworkSkinInfo;
use game_interface::types::render::character::MAX_SCORE_STR_LEN;
use game_interface::types::render::character::TeeEye;
use game_interface::types::resource_key::NetworkResourceKey;
use hiarc::Hiarc;
use hiarc::hiarc_safer_rc_refcell;
use serde::Deserialize;
use serde::Serialize;
use serde_with::DefaultOnError;
use serde_with::serde_as;

use crate::browser_favorite_player::FavoritePlayers;

#[serde_as]
#[derive(Debug, Hiarc, Clone, Default, Serialize, Deserialize)]
pub struct ServerBrowserSkin {
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub name: NetworkResourceKey<MAX_ASSET_NAME_LEN>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub info: NetworkSkinInfo,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub eye: TeeEye,
}

#[serde_as]
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct ServerBrowserPlayer {
    #[serde(alias = "time")]
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub score: NetworkString<MAX_SCORE_STR_LEN>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub skin: ServerBrowserSkin,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub name: NetworkString<MAX_CHARACTER_NAME_LEN>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub clan: NetworkString<MAX_CHARACTER_CLAN_LEN>,
    /// The optional account name of this player on this game server.
    ///
    /// Note that this name can be different per game server,
    /// and is not something official in any way.
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub account_name: Option<NetworkString<MAX_ACCOUNT_NAME_LEN>>,
    #[serde(alias = "country")]
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub flag: NetworkString<MAX_FLAG_NAME_LEN>,
}

#[serde_as]
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize, Default)]
pub struct ServerBrowserInfoMap {
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub name: NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub blake3: Hash,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub size: usize,
}

#[serde_as]
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct ServerBrowserInfo {
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub name: NetworkString<MAX_SERVER_NAME_LEN>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub game_type: NetworkString<MAX_PHYSICS_GAME_TYPE_NAME_LEN>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub version: NetworkString<MAX_VERSION_LEN>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub map: ServerBrowserInfoMap,
    #[serde(alias = "clients")]
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub players: Vec<ServerBrowserPlayer>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub max_ingame_players: u32,
    /// Maximum number of players of all clients combined
    /// (includes their dummies etc.)
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub max_players: u32,
    /// How many players (including the main player)
    /// are allowed per client, e.g. connecting dummies.
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub max_players_per_client: u32,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub passworded: bool,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub tournament_mode: bool,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub cert_sha256_fingerprint: Hash,
    /// Whether an account is required to join this server.
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub requires_account: bool,
}

#[derive(Debug, Hiarc, Clone)]
pub struct ServerBrowserServer {
    pub info: ServerBrowserInfo,
    pub addresses: Vec<SocketAddr>,
    pub location: NetworkString<16>,

    pub legacy_server: bool,
}

#[serde_as]
#[derive(Debug, Hiarc, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerFilter {
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub search: String,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub exclude: String,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub has_players: bool,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub filter_full_servers: bool,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub fav_players_only: bool,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub no_password: bool,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub unfinished_maps: bool,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub hide_legacy_servers: bool,
}

#[derive(Debug, Hiarc, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortDir {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Hiarc, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableSort {
    pub name: String,
    pub sort_dir: SortDir,
}

#[derive(Debug, Hiarc, Default)]
pub struct FilterCache {
    filter: ServerFilter,
    favorites: FavoritePlayers,
    sort: TableSort,
    finished_maps: HashSet<NetworkReducedAsciiString<MAX_MAP_NAME_LEN>>,
}

#[derive(Debug, Hiarc, Default)]
pub struct ServerBrowserList {
    pub servers: Vec<ServerBrowserServer>,

    pub ipv4: HashMap<SocketAddrV4, usize>,
    pub ipv6: HashMap<SocketAddrV6, usize>,

    pub player_count: usize,

    pub time: Option<Duration>,
}

impl ServerBrowserList {
    pub fn find(&self, addr: SocketAddr) -> Option<ServerBrowserServer> {
        let index = match addr {
            SocketAddr::V4(addr) => self.ipv4.get(&addr),
            SocketAddr::V6(addr) => self.ipv6.get(&addr),
        };
        index.map(|index| &self.servers[*index]).cloned()
    }

    pub fn find_str(&self, addr: &str) -> Option<ServerBrowserServer> {
        addr.parse().ok().and_then(|addr| self.find(addr))
    }
}

#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc, Default)]
pub struct ServerBrowserData {
    list: Arc<ServerBrowserList>,

    cache: FilterCache,
    filtered_sorted: Option<Arc<Vec<ServerBrowserServer>>>,
}

#[hiarc_safer_rc_refcell]
impl ServerBrowserData {
    fn from_servers(servers: Vec<ServerBrowserServer>, time: Duration) -> Self {
        let mut ipv4: HashMap<SocketAddrV4, usize> = Default::default();
        let mut ipv6: HashMap<SocketAddrV6, usize> = Default::default();

        let mut player_count = 0;
        for (index, server) in servers.iter().enumerate() {
            for address in server.addresses.iter().copied() {
                match address {
                    SocketAddr::V4(addr) => {
                        ipv4.insert(addr, index);
                    }
                    SocketAddr::V6(addr) => {
                        ipv6.insert(addr, index);
                    }
                }
            }

            player_count += server.info.players.len();
        }

        Self {
            list: Arc::new(ServerBrowserList {
                servers,
                ipv4,
                ipv6,

                player_count,

                time: Some(time),
            }),

            cache: Default::default(),
            filtered_sorted: Default::default(),
        }
    }

    pub fn new(servers: Vec<ServerBrowserServer>, time: Duration) -> Self {
        Self::from_servers(servers, time)
    }

    pub fn set_servers(&mut self, servers: Vec<ServerBrowserServer>, time: Duration) {
        if self.list.time.is_none_or(|list_time| list_time < time) {
            *self = Self::from_servers(servers, time);
        }
    }

    pub fn find(&self, addr: SocketAddr) -> Option<ServerBrowserServer> {
        self.list.find(addr)
    }

    pub fn find_str(&self, addr: &str) -> Option<ServerBrowserServer> {
        self.list.find_str(addr)
    }

    pub fn server_count(&self) -> usize {
        self.list.servers.len()
    }

    pub fn player_count(&self) -> usize {
        self.list.player_count
    }

    pub fn list(&self) -> Arc<ServerBrowserList> {
        self.list.clone()
    }

    pub fn locations(&self) -> BTreeSet<String> {
        self.list
            .servers
            .iter()
            .map(|s| {
                s.location
                    .to_lowercase()
                    .split_once(":")
                    .map(|(s1, s2)| if s2.is_empty() { s1 } else { s2 })
                    .unwrap_or("default")
                    .to_string()
            })
            .collect()
    }

    fn servers_filtered<'a>(
        servers: &'a [ServerBrowserServer],
        filter: &'a ServerFilter,
        favorites: &'a FavoritePlayers,
        finished_maps: &'a HashSet<NetworkReducedAsciiString<MAX_MAP_NAME_LEN>>,
    ) -> impl Iterator<Item = &'a ServerBrowserServer> {
        servers.iter().filter(move |server| {
            (server
                .info
                .map
                .name
                .as_str()
                .to_lowercase()
                .contains(&filter.search.to_lowercase())
                || server
                    .info
                    .name
                    .to_lowercase()
                    .contains(&filter.search.to_lowercase())
                || server.info.players.iter().any(|p| {
                    p.name
                        .to_lowercase()
                        .contains(&filter.search.to_lowercase())
                        || p.clan
                            .to_lowercase()
                            .contains(&filter.search.to_lowercase())
                }))
                && (!filter.has_players || !server.info.players.is_empty())
                && (!filter.filter_full_servers
                    || server.info.players.len() < server.info.max_ingame_players as usize)
                && (!filter.no_password || !server.info.passworded)
                && (!filter.fav_players_only
                    || server
                        .info
                        .players
                        .iter()
                        .any(|p| favorites.iter().any(|f| f.name == p.name)))
                && (!filter.unfinished_maps || finished_maps.contains(&server.info.map.name))
                && (!filter.hide_legacy_servers || !server.legacy_server)
        })
    }

    fn servers_sorted(servers: &mut [ServerBrowserServer], sort: &TableSort) {
        servers.sort_by(|d1, d2| {
            let order = match sort.name.as_str() {
                "Name" => d1
                    .info
                    .name
                    .to_lowercase()
                    .cmp(&d2.info.name.to_lowercase()),
                "Type" => d1
                    .info
                    .game_type
                    .to_lowercase()
                    .cmp(&d2.info.game_type.to_lowercase()),
                "Map" => d1
                    .info
                    .map
                    .name
                    .as_str()
                    .to_lowercase()
                    .cmp(&d2.info.map.name.as_str().to_lowercase()),
                "Players" => d1.info.players.len().cmp(&d2.info.players.len()),
                // TODO: "Ping"
                _ => d1
                    .info
                    .name
                    .to_lowercase()
                    .cmp(&d2.info.name.to_lowercase()),
            };

            match sort.sort_dir {
                SortDir::Asc => order,
                SortDir::Desc => order.reverse(),
            }
        });
    }

    pub fn filtered_and_sorted(
        &mut self,
        filter: &ServerFilter,
        favorites: &FavoritePlayers,
        sort: &TableSort,
        finished_maps: &HashSet<NetworkReducedAsciiString<MAX_MAP_NAME_LEN>>,
    ) -> Arc<Vec<ServerBrowserServer>> {
        if let Some(filtered_sorted) = (self.cache.filter.eq(filter)
            && self.cache.favorites.eq(favorites)
            && self.cache.sort.eq(sort)
            && self.cache.finished_maps.eq(finished_maps))
        .then_some(self.filtered_sorted.as_ref())
        .flatten()
        {
            filtered_sorted.clone()
        } else {
            let mut servers_filtered: Vec<_> =
                Self::servers_filtered(&self.list.servers, filter, favorites, finished_maps)
                    .cloned()
                    .collect();
            Self::servers_sorted(&mut servers_filtered, sort);
            let servers = Arc::new(servers_filtered);
            self.filtered_sorted = Some(servers.clone());
            servers
        }
    }
}
