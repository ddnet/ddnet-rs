use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use ddnet_account_client_http_fs::cert_downloader::CertsDownloader;
use ddnet_accounts_shared::game_server::user_id::{UserId, VerifyingKey};
use game_interface::types::player_info::AccountId;
use network::network::{
    connection::NetworkConnectionId,
    errors::{BanType, Banned},
    plugins::{ConnectionEvent, NetworkPluginConnection},
};
use x509_cert::der::Encode;

#[derive(Debug, Clone)]
pub struct Ban {
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub ty: BanType,
}

#[derive(Debug, Default)]
pub struct BanState {
    account_bans: HashMap<AccountId, Ban>,
    cert_bans: HashMap<[u8; 32], Ban>,

    active_connections: HashMap<[u8; 32], HashSet<NetworkConnectionId>>,
}

impl BanState {
    fn get_ban(&mut self, user_id: UserId) -> Option<Ban> {
        let now = chrono::Utc::now();
        while let Some((account_id, ban)) = match user_id.account_id {
            Some(account_id) => self
                .account_bans
                .get(&account_id)
                .map(|v| (Some(account_id), v.clone())),
            None => self
                .cert_bans
                .get(&user_id.public_key)
                .map(|v| (None, v.clone())),
        } {
            if ban.until.is_none_or(|until| now < until) {
                return Some(ban);
            } else {
                match account_id {
                    Some(account_id) => {
                        self.account_bans.remove(&account_id);
                    }
                    None => {
                        self.cert_bans.remove(&user_id.public_key);
                    }
                }
            }
        }

        None
    }
}

/// plugin to disallow/ban certain connections by it's cert/account
#[derive(Debug)]
pub struct CertBans {
    state: Mutex<BanState>,
    account_server_certs_downloader: Arc<CertsDownloader>,
}

impl CertBans {
    pub fn new(account_server_certs_downloader: Arc<CertsDownloader>) -> Self {
        Self {
            state: Default::default(),
            account_server_certs_downloader,
        }
    }

    fn user_id(
        account_server_public_key: &[VerifyingKey],
        cert: &x509_cert::Certificate,
    ) -> UserId {
        ddnet_accounts_shared::game_server::user_id::user_id_from_cert(
            account_server_public_key,
            cert.to_der().unwrap(),
        )
    }

    /// Returns all network ids for that ip.
    #[must_use]
    pub fn ban(
        &self,
        cert: &x509_cert::Certificate,
        reason: BanType,
        until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> HashSet<NetworkConnectionId> {
        let user_id = Self::user_id(&self.account_server_certs_downloader.public_keys(), cert);
        let mut state = self.state.lock().unwrap();
        let ids = state
            .active_connections
            .get(&user_id.public_key)
            .cloned()
            .unwrap_or_default();

        match user_id.account_id {
            Some(account_id) => {
                state
                    .account_bans
                    .insert(account_id, Ban { until, ty: reason });
            }
            None => {
                state
                    .cert_bans
                    .insert(user_id.public_key, Ban { until, ty: reason });
            }
        }

        ids
    }
}

#[async_trait]
impl NetworkPluginConnection for CertBans {
    #[must_use]
    async fn on_incoming(&self, _remote_addr: &SocketAddr) -> bool {
        // This plugin prefers proper error messages instead
        // of ignoring connections
        true
    }
    #[must_use]
    async fn on_connect(
        &self,
        id: &NetworkConnectionId,
        _remote_addr: &SocketAddr,
        cert: &x509_cert::Certificate,
    ) -> ConnectionEvent {
        let user_id = Self::user_id(&self.account_server_certs_downloader.public_keys(), cert);
        let mut state = self.state.lock().unwrap();
        let ban = state.get_ban(user_id.clone());

        if let Some(ban) = ban {
            ConnectionEvent::Banned(Banned {
                msg: ban.ty,
                until: ban.until,
            })
        } else {
            state
                .active_connections
                .entry(user_id.public_key)
                .or_default()
                .insert(*id);

            ConnectionEvent::Allow
        }
    }
    async fn on_disconnect(
        &self,
        id: &NetworkConnectionId,
        _remote_addr: &SocketAddr,
        cert: &x509_cert::Certificate,
    ) {
        let user_id = Self::user_id(&self.account_server_certs_downloader.public_keys(), cert);
        let mut state = self.state.lock().unwrap();
        if let Some(connections) = state.active_connections.get_mut(&user_id.public_key) {
            connections.remove(id);
            if connections.is_empty() {
                state.active_connections.remove(&user_id.public_key);
            }
        }
    }
}
