use std::{
    marker::PhantomData,
    net::{SocketAddr, ToSocketAddrs},
    num::NonZeroUsize,
    ops::DerefMut,
    sync::{mpsc::sync_channel, Arc},
    time::Duration,
};

use anyhow::anyhow;
use base::system::System;
use pool::mt_pool::Pool;
use serde::Serialize;

use std::sync::mpsc::{Receiver, SyncSender as Sender};

use super::{
    connection::NetworkConnectionId,
    connections::NetworkConnections,
    errors::{ConnectionErrorCode, KickType},
    event_generator::{InternalGameEventGenerator, NetworkEventToGameEventGenerator},
    network_async::NetworkAsync,
    notifier::NetworkEventNotifier,
    plugins::NetworkPlugins,
    traits::{
        NetworkConnectingInterface, NetworkConnectionInterface, NetworkEndpointInterface,
        NetworkIncomingInterface,
    },
    types::{
        NetworkClientInitOptions, NetworkEventSendType, NetworkInOrderChannel, NetworkLogicEvent,
        NetworkServerCertMode, NetworkServerCertModeResult, NetworkServerInitOptions,
    },
};

pub struct Network<E, C, Z, I, const TY: u32>
where
    C: NetworkConnectionInterface,
    Z: NetworkConnectingInterface<C>,
    I: NetworkIncomingInterface<Z>,
    E: NetworkEndpointInterface<Z, I>,
{
    // some attributes are shared with the NetworkAsync struct
    // so that the endpoint can be closed without requiring
    // an additional lock
    is_server: bool,
    endpoint: E,
    thread: Arc<NetworkAsync<E, C, Z, I, TY>>,
    events_send: Sender<NetworkLogicEvent>,
    run_thread: Option<std::thread::JoinHandle<anyhow::Result<()>>>,

    // for the client to remember the last server it connected to
    connecting_connection_id: NetworkConnectionId,
    connecting_cancel_notifier: Option<Arc<tokio::sync::Notify>>,
    packet_pool: Pool<Vec<u8>>,

    _connecting: PhantomData<Z>,
    _incoming: PhantomData<I>,
}

impl<E, C, Z, I, const TY: u32> Network<E, C, Z, I, TY>
where
    C: NetworkConnectionInterface,
    Z: NetworkConnectingInterface<C>,
    I: NetworkIncomingInterface<Z>,
    E: NetworkEndpointInterface<Z, I>,
{
    fn run(thread: &Arc<NetworkAsync<E, C, Z, I, TY>>, events: Receiver<NetworkLogicEvent>) {
        while let Ok(event) = events.recv() {
            match event {
                NetworkLogicEvent::Connect {
                    con_id,
                    addr,
                    cancel,
                } => {
                    if let Err(err) = thread.connect(con_id, addr, cancel) {
                        log::error!("{err}");
                    }
                }
                NetworkLogicEvent::Disconnect {
                    connection_id,
                    sender,
                } => {
                    thread.disconnect(connection_id);
                    sender.send(()).unwrap();
                    // the end of this connection
                    return;
                }
                NetworkLogicEvent::Kick { connection_id, ty } => {
                    let thread = thread.clone();
                    tokio::spawn(async move { thread.kick(connection_id, ty).await });
                }
                NetworkLogicEvent::Send((con_id, packet, packet_order)) => {
                    let thread = thread.clone();
                    match packet_order {
                        NetworkEventSendType::ReliableOrdered(channel) => {
                            thread.send_ordered_to(con_id, packet, channel);
                        }
                        NetworkEventSendType::UnreliableUnordered => {
                            tokio::spawn(async move {
                                thread.send_unordered_unreliable_to(con_id, packet).await
                            });
                        }
                        NetworkEventSendType::ReliableUnordered => {
                            tokio::spawn(async move {
                                thread.send_unordered_reliable_to(con_id, packet).await
                            });
                        }
                        NetworkEventSendType::UnorderedAuto => {
                            tokio::spawn(async move {
                                thread.send_unordered_auto_to(con_id, packet).await
                            });
                        }
                    }
                }
            }
        }
    }

    /// Returns a tuple of:
    /// Self, server_cert, server_addr, net_event_notifier
    pub fn init_server(
        addr: &str,
        game_event_generator: Arc<dyn NetworkEventToGameEventGenerator + Send + Sync>,
        cert_mode: NetworkServerCertMode,
        sys: &System,
        options: NetworkServerInitOptions,
        plugins: NetworkPlugins,
    ) -> anyhow::Result<(
        Self,
        NetworkServerCertModeResult,
        SocketAddr,
        NetworkEventNotifier,
    )> {
        let thread_count = options
            .max_thread_count
            .unwrap_or(
                std::thread::available_parallelism()
                    .unwrap_or(NonZeroUsize::new(2).unwrap()) // at least two
                    .into(),
            )
            .max(2); // at least two
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("network-server")
                .worker_threads(thread_count)
                .build()?,
        );
        let runtime_guard = runtime.enter();

        let (network, server_cert, sock_addr, event_notifier) =
            <NetworkAsync<E, C, Z, I, TY>>::init_server(
                addr,
                game_event_generator,
                cert_mode,
                sys,
                options,
                plugins,
            )?;

        let (send, recv) = std::sync::mpsc::sync_channel(1024);

        let mut res = Network {
            is_server: true,
            endpoint: network.endpoint.clone(),
            packet_pool: network.packet_pool.clone(),
            connecting_connection_id: network.connections.id_gen.get_next(TY),
            connecting_cancel_notifier: None,
            thread: Arc::new(network),
            events_send: send,
            run_thread: None,
            _connecting: Default::default(),
            _incoming: Default::default(),
        };
        drop(runtime_guard);
        res.init(runtime, recv)?;
        Ok((res, server_cert, sock_addr, event_notifier))
    }

    pub fn init_client(
        forced_port: Option<u16>,
        game_event_generator: Arc<dyn NetworkEventToGameEventGenerator + Send + Sync>,
        sys: &System,
        options: NetworkClientInitOptions,
        plugins: NetworkPlugins,
        connect_addr: &str,
    ) -> anyhow::Result<(Self, NetworkEventNotifier)> {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .thread_name("network-client")
                .worker_threads(2) // at least 2
                .enable_all()
                .build()?,
        );
        let runtime_guard = runtime.enter();

        let event_notifier = NetworkEventNotifier {
            rt: runtime.handle().clone(),
            notifiers: [Some(Default::default()), None],
        };

        let connect_addr_resolve = connect_addr.to_socket_addrs();
        // prefers ipv4 right now
        let client_addr = if connect_addr_resolve
            .ok()
            .map(|mut r| r.any(|addr| matches!(addr, SocketAddr::V4(_))))
            .unwrap_or_default()
        {
            format!("0.0.0.0:{}", forced_port.unwrap_or(0))
                .parse()
                .unwrap()
        } else {
            format!("[::0]:{}", forced_port.unwrap_or(0))
                .parse()
                .unwrap()
        };
        let endpoint = E::make_client_endpoint(client_addr, &options)?;

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
            .build_sized(options.base.packet_capacity.unwrap_or(8), || {
                Vec::with_capacity(options.base.packet_size.unwrap_or(256))
            });

        let (send, recv) = std::sync::mpsc::sync_channel(1024);

        let mut res = Self {
            is_server: false,
            endpoint,
            thread: Arc::new(NetworkAsync::<E, C, Z, I, TY> {
                is_server: false,
                endpoint: endpoint_thread,
                connections: NetworkConnections::new(counter.clone()),
                all_in_order_packets: Default::default(),
                game_event_generator: InternalGameEventGenerator {
                    game_event_generator,
                    game_event_notifier: event_notifier.clone(),
                },
                sys: sys.time.clone(),
                is_debug: debug_printing,
                packet_pool: pool.clone(),

                stream_receive_window: options.base.stream_receive_window,

                plugins,

                _z: PhantomData,
                _i: PhantomData,
            }),
            events_send: send,
            run_thread: None,
            connecting_connection_id: counter.get_next(TY),
            connecting_cancel_notifier: None,
            packet_pool: pool,
            _connecting: Default::default(),
            _incoming: Default::default(),
        };

        drop(runtime_guard);
        res.init(runtime, recv)?;
        res.connect(connect_addr)?;
        Ok((res, event_notifier))
    }

    fn init(
        &mut self,
        runtime: Arc<tokio::runtime::Runtime>,
        events: Receiver<NetworkLogicEvent>,
    ) -> anyhow::Result<()> {
        let network_thread = self.thread.clone();

        let pre_defined_id = if self.is_server {
            None
        } else {
            Some(self.connecting_connection_id)
        };
        self.run_thread = Some(std::thread::Builder::new().name("network".into()).spawn(
            move || {
                let _runtime_guard = runtime.enter();
                let endpoint = network_thread.endpoint.clone();
                let connections = network_thread.connections.clone();
                let game_event_generator = network_thread.game_event_generator.clone();
                let sys = network_thread.sys.clone();
                let all_packets_in_order = network_thread.all_in_order_packets.clone();

                let is_server = network_thread.is_server;
                let is_debug = network_thread.is_debug;
                let packet_plugins = network_thread.plugins.packet_plugins.clone();
                let connection_plugins = network_thread.plugins.connection_plugins.clone();

                let stream_receive_window = network_thread.stream_receive_window;

                if is_server {
                    tokio::spawn(async move {
                        log::debug!("server: starting to accept connections");
                        while let Some(conn) = endpoint.accept().await {
                            let mut should_accept = true;

                            for plugin in connection_plugins.iter() {
                                should_accept &= plugin.on_incoming(&conn.remote_addr()).await;
                            }

                            if let Ok(conn) = conn.accept().and_then(|conn| {
                                should_accept
                                    .then_some(conn)
                                    .ok_or_else(|| anyhow!("connection refused"))
                            }) {
                                log::debug!("server: accepted a connection");
                                NetworkConnections::handle_connection(
                                    &connections,
                                    &game_event_generator,
                                    conn,
                                    pre_defined_id.as_ref(),
                                    sys.clone(),
                                    &all_packets_in_order,
                                    is_debug,
                                    &packet_plugins,
                                    &connection_plugins,
                                    stream_receive_window,
                                )
                                .await;
                            }
                        }
                    });
                }
                Self::run(&network_thread, events);
                Arc::try_unwrap(runtime)
                    .map_err(|_| anyhow!("failed to unwrap runtime"))?
                    .shutdown_timeout(Duration::from_secs(2));
                anyhow::Ok(())
            },
        )?);
        Ok(())
    }

    fn close(&mut self) {
        let (sender, receiver) = sync_channel(1);
        self.events_send
            .send(NetworkLogicEvent::Disconnect {
                connection_id: self.connecting_connection_id,
                sender,
            })
            .unwrap();

        if let Some(notifier) = self.connecting_cancel_notifier.take() {
            notifier.notify_waiters();
        }

        if receiver.recv_timeout(Duration::from_secs(1)).is_err() {
            self.endpoint
                .close(ConnectionErrorCode::Shutdown, "partially graceful shutdown");
        }

        let run_thread = self.run_thread.take().unwrap();
        if run_thread.join().is_err() {
            log::info!("failed to close/join network thread");
        }
        self.endpoint
            .close(ConnectionErrorCode::Shutdown, "graceful shutdown");
    }

    fn connect(&mut self, connect_addr: &str) -> anyhow::Result<()> {
        let notifier: Arc<tokio::sync::Notify> = Default::default();
        self.events_send.send(NetworkLogicEvent::Connect {
            con_id: self.connecting_connection_id,
            addr: connect_addr.to_string(),
            cancel: notifier.clone(),
        })?;
        self.connecting_cancel_notifier = Some(notifier);
        Ok(())
    }

    pub fn kick(&self, connection_id: &NetworkConnectionId, ty: KickType) {
        self.events_send
            .send(NetworkLogicEvent::Kick {
                connection_id: *connection_id,
                ty,
            })
            .unwrap();
    }

    fn send_to_impl<T>(
        &self,
        msg: &T,
        connection_id: &NetworkConnectionId,
        send_type: NetworkEventSendType,
    ) where
        T: Serialize,
    {
        let mut packet = self.packet_pool.new();
        bincode::serde::encode_into_std_write(msg, packet.deref_mut(), bincode::config::standard())
            .unwrap();
        self.events_send
            .send(NetworkLogicEvent::Send((*connection_id, packet, send_type)))
            .unwrap();
    }

    /// Tries to send as unrealible first, if unsupported
    /// or packet too big for a single packet, falls back
    /// to reliable.
    pub fn send_unordered_auto_to<T>(&self, msg: &T, connection_id: &NetworkConnectionId)
    where
        T: Serialize,
    {
        self.send_to_impl(msg, connection_id, NetworkEventSendType::UnorderedAuto);
    }

    pub fn send_unordered_to<T>(&self, msg: &T, connection_id: &NetworkConnectionId)
    where
        T: Serialize,
    {
        self.send_to_impl(msg, connection_id, NetworkEventSendType::ReliableUnordered);
    }

    pub fn send_in_order_to<T>(
        &self,
        msg: &T,
        connection_id: &NetworkConnectionId,
        channel: NetworkInOrderChannel,
    ) where
        T: Serialize,
    {
        self.send_to_impl(
            msg,
            connection_id,
            NetworkEventSendType::ReliableOrdered(channel),
        );
    }

    pub fn send_unreliable_to<T>(&self, msg: &T, connection_id: &NetworkConnectionId)
    where
        T: Serialize,
    {
        self.send_to_impl(
            msg,
            connection_id,
            NetworkEventSendType::UnreliableUnordered,
        );
    }

    /// Tries to send as unrealible first, if unsupported
    /// or packet too big for a single packet, falls back
    /// to reliable.
    pub fn send_unordered_auto_to_server<T>(&self, msg: &T)
    where
        T: Serialize,
    {
        self.send_unordered_auto_to(msg, &self.connecting_connection_id.clone());
    }

    /// Only use this if `connect` was used
    pub fn send_unordered_to_server<T>(&self, msg: &T)
    where
        T: Serialize,
    {
        self.send_unordered_to(msg, &self.connecting_connection_id.clone());
    }

    /// Only use this if `connect` was used
    pub fn send_in_order_to_server<T>(&self, msg: &T, channel: NetworkInOrderChannel)
    where
        T: Serialize,
    {
        self.send_in_order_to(msg, &self.connecting_connection_id.clone(), channel);
    }

    /// Only use this if `connect` was used
    pub fn send_unreliable_to_server<T>(&self, msg: &T)
    where
        T: Serialize,
    {
        self.send_unreliable_to(msg, &self.connecting_connection_id.clone());
    }
}

impl<E, C, Z, I, const TY: u32> Drop for Network<E, C, Z, I, TY>
where
    C: NetworkConnectionInterface,
    Z: NetworkConnectingInterface<C>,
    I: NetworkIncomingInterface<Z>,
    E: NetworkEndpointInterface<Z, I>,
{
    fn drop(&mut self) {
        self.close()
    }
}
