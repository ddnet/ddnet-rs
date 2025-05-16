use std::{net::SocketAddr, num::NonZeroU64, sync::atomic::AtomicU64};

use async_trait::async_trait;

use super::{
    connection::NetworkConnectionId,
    plugins::{ConnectionEvent, NetworkPluginConnection},
};

/// plugin to limit the max amount of connections totally
/// allowed concurrently.
#[derive(Debug)]
pub struct MaxConnections {
    count: AtomicU64,
    max: NonZeroU64,
}

impl MaxConnections {
    pub fn new(max: NonZeroU64) -> Self {
        Self {
            count: Default::default(),
            max,
        }
    }
}

#[async_trait]
impl NetworkPluginConnection for MaxConnections {
    async fn on_incoming(&self, _remote_addr: &SocketAddr) -> bool {
        // This plugin prefers proper error messages instead
        // of ignoring connections
        true
    }
    async fn on_connect(
        &self,
        _id: &NetworkConnectionId,
        _remote_addr: &SocketAddr,
        _cert: &x509_cert::Certificate,
    ) -> ConnectionEvent {
        if self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= self.max.get() {
            self.count.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            ConnectionEvent::Kicked("server is full".to_string())
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
        self.count.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}
