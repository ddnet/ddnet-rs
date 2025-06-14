use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    num::NonZeroU64,
    sync::Mutex,
};

use async_trait::async_trait;

use super::{
    connection::NetworkConnectionId,
    plugins::{ConnectionEvent, NetworkPluginConnection},
};

#[derive(Debug, Default)]
pub struct LimitState {
    active_connections: HashMap<IpAddr, u64>,
}

/// plugin to disallow/ban certain connections
#[derive(Debug)]
pub struct ConnectionLimitPerIp {
    state: Mutex<LimitState>,
    max_per_ip: NonZeroU64,
}

impl ConnectionLimitPerIp {
    pub fn new(max_per_ip: NonZeroU64) -> Self {
        Self {
            max_per_ip,
            state: Default::default(),
        }
    }
}

#[async_trait]
impl NetworkPluginConnection for ConnectionLimitPerIp {
    async fn on_incoming(&self, _remote_addr: &SocketAddr) -> bool {
        // This plugin prefers proper error messages instead
        // of ignoring connections
        true
    }
    async fn on_connect(
        &self,
        _id: &NetworkConnectionId,
        remote_addr: &SocketAddr,
        _cert: &x509_cert::Certificate,
    ) -> ConnectionEvent {
        let mut state = self.state.lock().unwrap();

        let count = state
            .active_connections
            .entry(remote_addr.ip())
            .or_default();
        if *count < self.max_per_ip.get() {
            *count += 1;
            ConnectionEvent::Allow
        } else {
            ConnectionEvent::Kicked(format!(
                "a maximum of {} connections per ip are allowed",
                self.max_per_ip.get()
            ))
        }
    }
    async fn on_disconnect(
        &self,
        _id: &NetworkConnectionId,
        remote_addr: &SocketAddr,
        _cert: &x509_cert::Certificate,
    ) {
        let mut state = self.state.lock().unwrap();

        let count = state
            .active_connections
            .entry(remote_addr.ip())
            .or_default();

        *count = count.saturating_sub(1);

        if *count == 0 {
            state.active_connections.remove(&remote_addr.ip());
        }
    }
}
