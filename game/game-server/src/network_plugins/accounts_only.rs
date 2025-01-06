use std::{net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use ddnet_account_client_http_fs::cert_downloader::CertsDownloader;
use ddnet_accounts_shared::game_server::user_id::{UserId, VerifyingKey};
use network::network::{
    connection::NetworkConnectionId,
    plugins::{ConnectionEvent, NetworkPluginConnection},
};
use x509_cert::der::Encode;

/// plugin to only allow connections that have an account
#[derive(Debug)]
pub struct AccountsOnly {
    account_server_certs_downloader: Arc<CertsDownloader>,
}

impl AccountsOnly {
    pub fn new(account_server_certs_downloader: Arc<CertsDownloader>) -> Self {
        Self {
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
}

#[async_trait]
impl NetworkPluginConnection for AccountsOnly {
    #[must_use]
    async fn on_incoming(&self, _remote_addr: &SocketAddr) -> bool {
        // This plugin prefers proper error messages instead
        // of ignoring connections
        true
    }
    #[must_use]
    async fn on_connect(
        &self,
        _id: &NetworkConnectionId,
        _remote_addr: &SocketAddr,
        cert: &x509_cert::Certificate,
    ) -> ConnectionEvent {
        let user_id = Self::user_id(&self.account_server_certs_downloader.public_keys(), cert);

        if user_id.account_id.is_none() {
            ConnectionEvent::Kicked("an account is required on this server".to_string())
        } else {
            ConnectionEvent::Allow
        }
    }
    async fn on_disconnect(
        &self,
        _id: &NetworkConnectionId,
        _remote_addr: &SocketAddr,
        _cert: &x509_cert::Certificate,
    ) {
    }
}
