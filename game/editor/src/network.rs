use std::{collections::HashSet, sync::Arc, time::Duration};

use base::system::System;
use network::network::{
    connection::NetworkConnectionId,
    event::NetworkEvent,
    packet_compressor::{types::DecompressionByteLimit, DefaultNetworkPacketCompressor},
    plugins::NetworkPlugins,
    quinn_network::QuinnNetwork,
    types::{
        NetworkClientCertCheckMode, NetworkClientCertMode, NetworkClientInitOptions,
        NetworkInOrderChannel, NetworkServerCertAndKey, NetworkServerCertMode,
        NetworkServerCertModeResult, NetworkServerInitOptions,
    },
    utils::create_certifified_keys,
};

use crate::event::{EditorEvent, EditorEventGenerator};

/// small wrapper around network for needs of editor
pub struct EditorNetwork {
    network: QuinnNetwork,

    is_server: bool,

    connections: HashSet<NetworkConnectionId>,
}

impl EditorNetwork {
    pub fn new_server(
        sys: &System,
        event_generator: Arc<EditorEventGenerator>,
        cert: Option<NetworkServerCertMode>,
        port: Option<u16>,
    ) -> (Self, NetworkServerCertModeResult, u16) {
        let (network, server_cert, addr, _) = QuinnNetwork::init_server(
            &format!("0.0.0.0:{}", port.unwrap_or_default()),
            event_generator.clone(),
            cert.unwrap_or_else(|| {
                let (cert, private_key) = create_certifified_keys();
                NetworkServerCertMode::FromCertAndPrivateKey(Box::new(NetworkServerCertAndKey {
                    cert,
                    private_key,
                }))
            }),
            sys,
            NetworkServerInitOptions::new()
                .with_max_thread_count(6)
                .with_timeout(Duration::from_secs(120))
                .with_stream_receive_window(1024 * 1024 * 1024)
                .with_receive_window(1024 * 1024 * 1024)
                .with_send_window(1024 * 1024 * 1024),
            NetworkPlugins {
                packet_plugins: Arc::new(vec![Arc::new(
                    DefaultNetworkPacketCompressor::new()
                        .with_limit(DecompressionByteLimit::OneGigaByte),
                )]),
                ..Default::default()
            },
        )
        .unwrap();
        let port = addr.port();
        (
            Self {
                network,
                is_server: true,
                connections: Default::default(),
            },
            server_cert,
            port,
        )
    }

    pub fn new_client(
        sys: &System,
        event_generator: Arc<EditorEventGenerator>,
        server_addr: &str,
        server_info: NetworkClientCertCheckMode,
    ) -> Self {
        let (client_cert, client_private_key) = create_certifified_keys();
        let network = QuinnNetwork::init_client(
            None,
            event_generator.clone(),
            sys,
            NetworkClientInitOptions::new(
                server_info,
                NetworkClientCertMode::FromCertAndPrivateKey {
                    cert: client_cert,
                    private_key: client_private_key,
                },
            )
            .with_timeout(Duration::from_secs(120))
            .with_stream_receive_window(1024 * 1024 * 1024)
            .with_receive_window(1024 * 1024 * 1024)
            .with_send_window(1024 * 1024 * 1024),
            NetworkPlugins {
                packet_plugins: Arc::new(vec![Arc::new(
                    DefaultNetworkPacketCompressor::new()
                        .with_limit(DecompressionByteLimit::OneGigaByte),
                )]),
                ..Default::default()
            },
            server_addr,
        )
        .unwrap()
        .0;

        Self {
            network,
            is_server: false,
            connections: Default::default(),
        }
    }

    pub fn send(&self, ev: EditorEvent) {
        if self.is_server {
            for connection in &self.connections {
                self.network
                    .send_in_order_to(&ev, connection, NetworkInOrderChannel::Global);
            }
        } else {
            self.network
                .send_in_order_to_server(&ev, NetworkInOrderChannel::Global);
        }
    }

    pub fn send_to(&self, id: &NetworkConnectionId, ev: EditorEvent) {
        if self.is_server {
            self.network
                .send_in_order_to(&ev, id, NetworkInOrderChannel::Global);
        } else {
            self.network
                .send_in_order_to_server(&ev, NetworkInOrderChannel::Global);
        }
    }

    pub fn handle_network_ev(&mut self, id: NetworkConnectionId, ev: NetworkEvent) {
        match ev {
            NetworkEvent::Connected { .. } => {
                self.connections.insert(id);
            }
            NetworkEvent::Disconnected { .. } => {
                self.connections.remove(&id);
            }
            NetworkEvent::ConnectingFailed(_) => {}
            NetworkEvent::NetworkStats(_) => {}
        }
    }
}
