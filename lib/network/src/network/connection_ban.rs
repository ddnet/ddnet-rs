use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, SocketAddr},
    sync::Mutex,
};

use async_trait::async_trait;

use super::{
    connection::NetworkConnectionId,
    errors::{BanType, Banned},
    plugins::{ConnectionEvent, NetworkPluginConnection},
};

#[derive(Debug, Clone)]
pub struct Ban {
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub ty: BanType,
}

#[derive(Debug, Default)]
pub struct BanState {
    ipv4_bans: prefix_trie::PrefixMap<ipnet::Ipv4Net, Ban>,
    ipv6_bans: prefix_trie::PrefixMap<ipnet::Ipv6Net, Ban>,

    active_connections: HashMap<IpAddr, HashSet<NetworkConnectionId>>,
}

impl BanState {
    fn get_ban(&mut self, ip: IpAddr) -> Option<Ban> {
        let now = chrono::Utc::now();
        while let Some((ip, ban)) = match ip {
            IpAddr::V4(ip) => self
                .ipv4_bans
                .get_spm(&ipnet::Ipv4Net::from(ip))
                .map(|(&ip, v)| (ipnet::IpNet::V4(ip), v.clone())),
            IpAddr::V6(ip) => self
                .ipv6_bans
                .get_spm(&ipnet::Ipv6Net::from(ip))
                .map(|(&ip, v)| (ipnet::IpNet::V6(ip), v.clone())),
        } {
            if ban.until.is_none_or(|until| now < until) {
                return Some(ban);
            } else {
                match ip {
                    ipnet::IpNet::V4(ip) => {
                        self.ipv4_bans.remove(&ip);
                    }
                    ipnet::IpNet::V6(ip) => {
                        self.ipv6_bans.remove(&ip);
                    }
                }
            }
        }

        None
    }
}

/// plugin to disallow/ban certain connections
#[derive(Debug, Default)]
pub struct ConnectionBans {
    state: Mutex<BanState>,
}

#[async_trait]
impl NetworkPluginConnection for ConnectionBans {
    async fn on_incoming(&self, _remote_addr: &SocketAddr) -> bool {
        // This plugin prefers proper error messages instead
        // of ignoring connections
        true
    }
    async fn on_connect(
        &self,
        id: &NetworkConnectionId,
        remote_addr: &SocketAddr,
        _cert: &x509_cert::Certificate,
    ) -> ConnectionEvent {
        let mut state = self.state.lock().unwrap();
        let ban = state.get_ban(remote_addr.ip());

        if let Some(ban) = ban {
            ConnectionEvent::Banned(Banned {
                msg: ban.ty,
                until: ban.until,
            })
        } else {
            state
                .active_connections
                .entry(remote_addr.ip())
                .or_default()
                .insert(*id);

            ConnectionEvent::Allow
        }
    }
    async fn on_disconnect(
        &self,
        id: &NetworkConnectionId,
        remote_addr: &SocketAddr,
        _cert: &x509_cert::Certificate,
    ) {
        let mut state = self.state.lock().unwrap();
        if let Some(connections) = state.active_connections.get_mut(&remote_addr.ip()) {
            connections.remove(id);
            if connections.is_empty() {
                state.active_connections.remove(&remote_addr.ip());
            }
        }
    }
}

impl ConnectionBans {
    /// Returns all network ids for that ip.
    #[must_use]
    pub fn ban_ip(
        &self,
        ip: IpAddr,
        reason: BanType,
        until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> HashSet<NetworkConnectionId> {
        let mut state = self.state.lock().unwrap();
        let ids = state
            .active_connections
            .get(&ip)
            .cloned()
            .unwrap_or_default();

        match ip {
            IpAddr::V4(ip) => {
                state.ipv4_bans.insert(ip.into(), Ban { until, ty: reason });
            }
            IpAddr::V6(ip) => {
                state.ipv6_bans.insert(ip.into(), Ban { until, ty: reason });
            }
        }

        ids
    }
}
