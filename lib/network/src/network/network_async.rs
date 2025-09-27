use std::{marker::PhantomData, net::SocketAddr, sync::Arc};

use anyhow::anyhow;
use base::steady_clock::SteadyClock;
use tracing::instrument;

use crate::network::{
    errors::ConnectionErrorCode,
    event::{NetworkEvent, NetworkEventConnectingFailed},
};

use super::{
    connection::NetworkConnectionId,
    connections::NetworkConnections,
    errors::KickType,
    event_generator::{InternalGameEventGenerator, NetworkEventToGameEventGenerator},
    notifier::NetworkEventNotifier,
    plugins::NetworkPlugins,
    traits::{
        NetworkConnectingInterface, NetworkConnectionInterface, NetworkEndpointInterface,
        NetworkIncomingInterface, UnreliableUnorderedError,
    },
    types::{
        NetworkInOrderChannel, NetworkInOrderPackets, NetworkServerCertMode,
        NetworkServerCertModeResult, NetworkServerInitOptions,
    },
};
use pool::{mt_datatypes::PoolVec, mt_pool::Pool};
use tokio::sync::Mutex as TokioMutex;

pub struct NetworkAsync<E, C: Send + Sync, Z, I, const TY: u32> {
    pub(crate) is_server: bool,
    pub(crate) endpoint: E,
    pub(crate) connections: NetworkConnections<C, TY>,
    pub(crate) all_in_order_packets: Arc<TokioMutex<NetworkInOrderPackets>>,
    pub(crate) game_event_generator: InternalGameEventGenerator,
    pub(crate) time: SteadyClock,
    pub(crate) is_debug: bool,
    pub(crate) packet_pool: Pool<Vec<u8>>,

    pub(crate) stream_receive_window: Option<u32>,

    // plugins
    pub(crate) plugins: NetworkPlugins,

    pub(crate) _z: PhantomData<Z>,
    pub(crate) _i: PhantomData<I>,
}

impl<E, C, Z, I, const TY: u32> NetworkAsync<E, C, Z, I, TY>
where
    C: NetworkConnectionInterface,
    Z: NetworkConnectingInterface<C>,
    I: NetworkIncomingInterface<Z>,
    E: NetworkEndpointInterface<Z, I>,
{
    /// Returns a tuple of:
    /// Self, server_cert, server_addr, net_event_notifier
    #[instrument(level = "trace", skip_all)]
    pub fn init_server(
        addr: &str,
        game_event_generator: Arc<dyn NetworkEventToGameEventGenerator + Send + Sync>,
        cert_mode: NetworkServerCertMode,
        time: &SteadyClock,
        options: NetworkServerInitOptions,
        plugins: NetworkPlugins,
    ) -> anyhow::Result<(
        Self,
        NetworkServerCertModeResult,
        SocketAddr,
        NetworkEventNotifier,
    )> {
        let event_notifier = NetworkEventNotifier {
            rt: tokio::runtime::Handle::current(),
            notifiers: [Some(Default::default()), None],
        };

        let server_addr = addr.parse()?;
        let server = E::make_server_endpoint(server_addr, cert_mode, &options);
        if let Err(err) = &server {
            log::info!("{err}");
        }
        let (endpoint, server_cert) = server?;

        let sock_addr = endpoint.sock_addr()?;

        let counter = options.base.id_generator;

        let debug_printing = options.base.debug_printing.unwrap_or(false);

        let endpoint_thread = endpoint.clone();
        let pool = Pool::builder()
            .with_limit(
                options
                    .base
                    .max_packets_pooled
                    .and_then(|p| p.try_into().ok())
                    .unwrap_or(512.try_into().unwrap()),
            )
            .build_sized(options.base.packet_capacity.unwrap_or(64), || {
                Vec::with_capacity(options.base.packet_size.unwrap_or(256))
            });

        let res = NetworkAsync::<E, C, Z, I, TY> {
            is_server: true,
            endpoint: endpoint_thread,
            connections: NetworkConnections::new(counter.clone()),
            all_in_order_packets: Default::default(),
            game_event_generator: InternalGameEventGenerator {
                game_event_generator,
                game_event_notifier: event_notifier.clone(),
            },
            time: time.clone(),
            is_debug: debug_printing,
            packet_pool: pool.clone(),

            stream_receive_window: options.base.stream_receive_window,

            plugins,

            _z: PhantomData,
            _i: PhantomData,
        };
        Ok((res, server_cert, sock_addr, event_notifier))
    }

    #[instrument(level = "trace", skip_all, parent = None, fields(bench_skip_render = true))]
    pub fn connect(
        &self,
        con_id: NetworkConnectionId,
        addr: String,
        cancel: Arc<tokio::sync::Notify>,
    ) -> anyhow::Result<()> {
        log::debug!(target: "network", "connecting to {addr}");
        let conn_res = self.endpoint.connect(addr.as_str().parse()?, "localhost");
        match conn_res {
            Ok(conn) => {
                let connections = self.connections.clone();
                let game_event_generator = self.game_event_generator.clone();
                let time = self.time.clone();
                let all_in_order_packets = self.all_in_order_packets.clone();
                let is_debug = self.is_debug;
                let packet_plugins = self.plugins.packet_plugins.clone();
                let connection_plugins = self.plugins.connection_plugins.clone();
                let stream_receive_window = self.stream_receive_window;
                // handle the connect sync (since it's client side only)
                if let Err(err) =
                    tokio::runtime::Handle::current().block_on(tokio::spawn(async move {
                        tokio::select! {
                            res = NetworkConnections::handle_connection(
                                &connections,
                                &game_event_generator,
                                conn,
                                Some(&con_id),
                                time,
                                &all_in_order_packets,
                                is_debug,
                                &packet_plugins,
                                &connection_plugins,
                                stream_receive_window
                            )
                            .await => Err(anyhow!("{:?}", res)),
                            res = async move {
                                cancel.notified().await;
                                Err::<(), anyhow::Error>(anyhow!("connecting was cancelled"))
                            } => res,
                        }
                    }))
                {
                    let game_event_generator_clone = self.game_event_generator.clone();
                    let timestamp = self.time.now();
                    tokio::spawn(async move {
                        game_event_generator_clone
                            .generate_from_network_event(
                                timestamp,
                                &con_id,
                                &NetworkEvent::ConnectingFailed(
                                    NetworkEventConnectingFailed::Other(err.to_string()),
                                ),
                            )
                            .await;
                    });
                }
            }
            Err(conn) => {
                let game_event_generator_clone = self.game_event_generator.clone();
                let timestamp = self.time.now();
                tokio::spawn(async move {
                    game_event_generator_clone
                        .generate_from_network_event(
                            timestamp,
                            &con_id,
                            &NetworkEvent::ConnectingFailed(conn),
                        )
                        .await;
                });
            }
        }
        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub(crate) fn disconnect(&self, connection_id: NetworkConnectionId) {
        log::debug!("disconnecting");
        let connections_ = self.connections.clone();
        let con_id = connection_id;
        // handle the disconnect sync (since it's client side only)
        // ignore error here, nobody cares about it anyway
        let _ = tokio::runtime::Handle::current().block_on(tokio::spawn(async move {
            let mut connections_guard = connections_.connections.lock().await;
            let connections = &mut *connections_guard;
            // remove the connection if it exists
            let con = connections.remove(&con_id);
            drop(connections_guard);
            if let Some(conn) = con {
                conn.conn
                    .close(ConnectionErrorCode::Shutdown, "client graceful disconnect")
                    .await;
            }
        }));
    }

    #[instrument(level = "trace", skip_all)]
    pub async fn kick(&self, connection_id: NetworkConnectionId, ty: KickType) {
        log::debug!("kick {connection_id:?}");
        let connections_ = self.connections.clone();
        let con_id = connection_id;
        // try to kick the connection, if exists
        let mut connections_guard = connections_.connections.lock().await;
        let connections = &mut *connections_guard;
        // remove the connection if it exists
        let con = connections.remove(&con_id);
        drop(connections_guard);
        if let Some(conn) = con {
            conn.conn
                .close(
                    if matches!(ty, KickType::Ban(_)) {
                        ConnectionErrorCode::Banned
                    } else {
                        ConnectionErrorCode::Kicked
                    },
                    &match ty {
                        KickType::Ban(banned) => serde_json::to_string(&banned).unwrap_or_default(),
                        KickType::Kick(reason) => reason,
                    },
                )
                .await;
        }
    }

    /// This is a sync call but spawns an async thread.
    ///
    /// The order is only reliable if ALL ordered packets
    /// use the same thread or do a call to this function
    /// in a queued way.
    pub fn send_ordered_to(
        &self,
        con_id: NetworkConnectionId,
        packet: PoolVec<u8>,
        channel: NetworkInOrderChannel,
    ) {
        let mut in_order = self.all_in_order_packets.blocking_lock();
        let channels = in_order.entry(con_id).or_default();
        let channel_packets = channels.entry(channel).or_default().clone();
        drop(in_order);
        channel_packets.blocking_lock().push_back(packet);

        let pool = self.packet_pool.clone();
        let packet_plugins = self.plugins.packet_plugins.clone();
        let is_debug = self.is_debug;
        let connections = self.connections.clone();

        tokio::spawn(async move {
            let connection = connections.get_connection_impl_clone_by_id(&con_id).await;
            let mut in_order = channel_packets.lock().await;
            let packet_to_send = in_order.pop_front();
            if let Some(con_clone) = connection
                && let Some(packet) = packet_to_send
            {
                let write_packet = NetworkConnections::<C, TY>::prepare_write_packet(
                    &con_id,
                    &packet,
                    &pool,
                    &packet_plugins,
                )
                .await;
                if let Ok(write_packet) = write_packet {
                    con_clone
                        .push_ordered_reliable_packet_in_order(write_packet, channel)
                        .await;
                    drop(in_order);
                    match con_clone.send_one_ordered_reliable(channel).await {
                        Ok(_) => {}
                        Err(err) => {
                            if is_debug {
                                log::debug!("error: send ordered packet failed: {err}");
                            }
                        }
                    }
                }
            }
        });
    }

    pub async fn send_unordered_unreliable_to(
        &self,
        con_id: NetworkConnectionId,
        packet: PoolVec<u8>,
    ) {
        let pool = self.packet_pool.clone();
        let packet_plugins = self.plugins.packet_plugins.clone();

        let connection = self
            .connections
            .get_connection_impl_clone_by_id(&con_id)
            .await;
        if let Some(con_clone) = connection {
            let write_packet = NetworkConnections::<C, TY>::prepare_write_packet(
                &con_id,
                &packet,
                &pool,
                &packet_plugins,
            )
            .await;
            if let Ok(write_packet) = write_packet {
                match con_clone.send_unreliable_unordered(write_packet).await {
                    Ok(_) => {}
                    Err((_, err)) => {
                        if self.is_debug {
                            log::debug!("error: send unreliable unordered packet failed: {err}");
                        }
                    }
                }
            }
        }
    }

    pub async fn send_unordered_reliable_to(
        &self,
        con_id: NetworkConnectionId,
        packet: PoolVec<u8>,
    ) {
        let pool = self.packet_pool.clone();
        let packet_plugins = self.plugins.packet_plugins.clone();

        let connection = self
            .connections
            .get_connection_impl_clone_by_id(&con_id)
            .await;
        if let Some(con_clone) = connection {
            let write_packet = NetworkConnections::<C, TY>::prepare_write_packet(
                &con_id,
                &packet,
                &pool,
                &packet_plugins,
            )
            .await;
            if let Ok(write_packet) = write_packet {
                match con_clone.send_unordered_reliable(write_packet).await {
                    Ok(_) => {}
                    Err(err) => {
                        if self.is_debug {
                            log::debug!("error: send reliable unordered packet failed: {err}");
                        }
                    }
                }
            }
        }
    }

    pub async fn send_unordered_auto_to(&self, con_id: NetworkConnectionId, packet: PoolVec<u8>) {
        let pool = self.packet_pool.clone();
        let packet_plugins = self.plugins.packet_plugins.clone();

        let connection = self
            .connections
            .get_connection_impl_clone_by_id(&con_id)
            .await;
        if let Some(con_clone) = connection {
            let write_packet = NetworkConnections::<C, TY>::prepare_write_packet(
                &con_id,
                &packet,
                &pool,
                &packet_plugins,
            )
            .await;
            if let Ok(write_packet) = write_packet {
                match con_clone.send_unreliable_unordered(write_packet).await {
                    Ok(_) => {}
                    Err((write_packet, err)) => {
                        match err {
                            UnreliableUnorderedError::ConnectionClosed(err) => {
                                if self.is_debug {
                                    log::debug!("error: send auto unordered packet failed: {err}");
                                }
                            }
                            UnreliableUnorderedError::Disabled
                            | UnreliableUnorderedError::TooLarge => {
                                // try unordered reliable
                                if let Err(err) =
                                    con_clone.send_unordered_reliable(write_packet).await
                                    && self.is_debug
                                {
                                    log::debug!("error: send auto unordered packet failed: {err}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
