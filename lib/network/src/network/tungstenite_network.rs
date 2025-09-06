use std::{
    collections::VecDeque,
    future::Future,
    net::SocketAddr,
    pin::{Pin, pin},
    sync::Arc,
    time::Duration,
};

use anyhow::anyhow;
use pool::mt_datatypes::PoolVec;
use spki::der::Decode;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
    task::JoinHandle,
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};

use super::{
    connection::ConnectionStats,
    errors::ConnectionErrorCode,
    event::{NetworkEventConnectingFailed, NetworkEventDisconnect},
    network::Network,
    traits::{
        NetworkConnectingInterface, NetworkConnectionInterface, NetworkEndpointInterface,
        NetworkIncomingInterface, UnreliableUnorderedError,
    },
    types::{
        NetworkClientInitOptions, NetworkInOrderChannel, NetworkServerCertMode,
        NetworkServerCertModeResult, NetworkServerInitOptions,
    },
    utils::create_certifified_keys,
};

use pollster::FutureExt as _;

use futures_util::{
    StreamExt,
    sink::SinkExt,
    stream::{SplitSink, SplitStream},
};

type ConnectionTypeClient = (
    Mutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>,
    Mutex<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>,
);

type ConnectionTypeServer = (
    Mutex<SplitSink<WebSocketStream<TcpStream>, Message>>,
    Mutex<SplitStream<WebSocketStream<TcpStream>>>,
);

enum ConnectionType {
    Client(ConnectionTypeClient),
    Server(ConnectionTypeServer),
}

#[derive(Clone)]
pub struct TungsteniteNetworkConnectionWrapper {
    con: Arc<ConnectionType>,
    in_order_packets: Arc<tokio::sync::Mutex<VecDeque<PoolVec<u8>>>>,
    addr: SocketAddr,
}

#[async_trait::async_trait]
impl NetworkConnectionInterface for TungsteniteNetworkConnectionWrapper {
    async fn send_unreliable_unordered(
        &self,
        data: PoolVec<u8>,
    ) -> anyhow::Result<(), (PoolVec<u8>, UnreliableUnorderedError)> {
        match self.con.as_ref() {
            ConnectionType::Client((send, _)) => {
                send.lock()
                    .await
                    .send(Message::Binary(data.take().into()))
                    .await
                    .map_err(|err| {
                        (
                            PoolVec::new_without_pool(),
                            UnreliableUnorderedError::ConnectionClosed(err.into()),
                        )
                    })?;
                Ok(())
            }
            ConnectionType::Server((send, _)) => {
                send.lock()
                    .await
                    .send(Message::Binary(data.take().into()))
                    .await
                    .map_err(|err| {
                        (
                            PoolVec::new_without_pool(),
                            UnreliableUnorderedError::ConnectionClosed(err.into()),
                        )
                    })?;
                Ok(())
            }
        }
    }

    async fn read_unreliable_unordered(&self) -> anyhow::Result<Vec<u8>> {
        match self.con.as_ref() {
            ConnectionType::Client((_, recv)) => {
                if let Some(Ok(Message::Binary(pkt))) = recv.lock().await.next().await {
                    Ok(pkt.into())
                } else {
                    Err(anyhow!("TODO:"))
                }
            }
            ConnectionType::Server((_, recv)) => {
                if let Some(Ok(Message::Binary(pkt))) = recv.lock().await.next().await {
                    Ok(pkt.into())
                } else {
                    Err(anyhow!("TODO:"))
                }
            }
        }
    }

    async fn send_unordered_reliable(&self, data: PoolVec<u8>) -> anyhow::Result<()> {
        // only reliable packets in websocket,
        // they might still come out of order because of the
        // overlaying network implementation
        Ok(self
            .send_unreliable_unordered(data)
            .await
            .map_err(|(_, err)| err)?)
    }

    async fn read_unordered_reliable<
        F: FnOnce(anyhow::Result<Vec<u8>>) -> JoinHandle<()> + Send + 'static,
    >(
        &self,
        _on_data: F,
    ) -> anyhow::Result<()> {
        // read_unreliable_unordered handles all recvs
        loop {
            tokio::time::sleep(Duration::MAX).await
        }
    }

    async fn push_ordered_reliable_packet_in_order(
        &self,
        data: PoolVec<u8>,
        _channel: NetworkInOrderChannel,
    ) {
        self.in_order_packets.lock().await.push_back(data);
    }

    async fn send_one_ordered_reliable(
        &self,
        _channel: NetworkInOrderChannel,
    ) -> anyhow::Result<()> {
        let mut packet_guard = self.in_order_packets.lock().await;
        let packet_res = packet_guard.pop_front();
        if let Some(packet) = packet_res {
            // lock guarantees order here
            self.send_unordered_reliable(packet).await
        } else {
            Err(anyhow!("No packet was queued."))
        }
    }

    async fn read_ordered_reliable<
        F: FnOnce(anyhow::Result<Vec<u8>>) -> JoinHandle<()> + Send + 'static,
    >(
        &self,
        _on_data: F,
    ) -> anyhow::Result<()> {
        // read_unreliable_unordered handles all recvs
        loop {
            tokio::time::sleep(Duration::MAX).await
        }
    }

    async fn close(&self, _error_code: ConnectionErrorCode, _reason: &str) {
        match self.con.as_ref() {
            ConnectionType::Client((send, _)) => {
                send.lock().await.close().await.unwrap();
            }
            ConnectionType::Server((send, _)) => {
                send.lock().await.close().await.unwrap();
            }
        }
    }

    fn close_reason(&self) -> Option<NetworkEventDisconnect> {
        None
    }

    fn remote_addr(&self) -> SocketAddr {
        self.addr
    }

    fn peer_identity(&self) -> x509_cert::Certificate {
        x509_cert::Certificate::from_der(&vec![
            48, 130, 1, 202, 48, 130, 1, 111, 160, 3, 2, 1, 2, 2, 1, 42, 48, 10, 6, 8, 42, 134, 72,
            206, 61, 4, 3, 2, 48, 83, 49, 11, 48, 9, 6, 3, 85, 4, 6, 19, 2, 85, 83, 49, 29, 48, 27,
            6, 3, 85, 4, 10, 12, 20, 87, 111, 114, 108, 100, 32, 100, 111, 109, 105, 110, 97, 116,
            105, 111, 110, 32, 73, 110, 99, 49, 37, 48, 35, 6, 3, 85, 4, 3, 12, 28, 87, 111, 114,
            108, 100, 32, 100, 111, 109, 105, 110, 97, 116, 105, 111, 110, 32, 99, 111, 114, 112,
            111, 114, 97, 116, 105, 111, 110, 48, 30, 23, 13, 50, 52, 48, 54, 50, 50, 50, 48, 53,
            57, 53, 55, 90, 23, 13, 50, 52, 48, 54, 50, 50, 50, 49, 53, 57, 53, 55, 90, 48, 83, 49,
            11, 48, 9, 6, 3, 85, 4, 6, 19, 2, 85, 83, 49, 29, 48, 27, 6, 3, 85, 4, 10, 12, 20, 87,
            111, 114, 108, 100, 32, 100, 111, 109, 105, 110, 97, 116, 105, 111, 110, 32, 73, 110,
            99, 49, 37, 48, 35, 6, 3, 85, 4, 3, 12, 28, 87, 111, 114, 108, 100, 32, 100, 111, 109,
            105, 110, 97, 116, 105, 111, 110, 32, 99, 111, 114, 112, 111, 114, 97, 116, 105, 111,
            110, 48, 42, 48, 5, 6, 3, 43, 101, 112, 3, 33, 0, 106, 193, 239, 244, 131, 220, 95,
            250, 115, 38, 114, 38, 242, 183, 35, 84, 191, 149, 88, 47, 118, 51, 208, 64, 134, 162,
            0, 57, 119, 198, 65, 34, 163, 99, 48, 97, 48, 29, 6, 3, 85, 29, 14, 4, 22, 4, 20, 201,
            210, 56, 181, 223, 0, 42, 236, 255, 205, 81, 31, 74, 217, 80, 92, 113, 152, 87, 153,
            48, 15, 6, 3, 85, 29, 19, 1, 1, 255, 4, 5, 48, 3, 1, 1, 255, 48, 14, 6, 3, 85, 29, 15,
            1, 1, 255, 4, 4, 3, 2, 1, 6, 48, 31, 6, 12, 43, 6, 1, 4, 1, 0, 68, 68, 45, 65, 99, 99,
            4, 15, 48, 13, 48, 11, 2, 1, 1, 2, 6, 1, 144, 65, 191, 160, 224, 48, 10, 6, 8, 42, 134,
            72, 206, 61, 4, 3, 2, 3, 73, 0, 48, 70, 2, 33, 0, 146, 67, 139, 219, 91, 173, 71, 240,
            201, 231, 84, 56, 186, 103, 168, 189, 154, 149, 182, 84, 28, 249, 35, 163, 201, 2, 86,
            71, 49, 213, 167, 248, 2, 33, 0, 146, 54, 104, 106, 181, 151, 37, 24, 36, 100, 152, 56,
            138, 214, 45, 255, 89, 96, 96, 108, 49, 209, 144, 151, 77, 139, 149, 135, 180, 64, 86,
            50,
        ])
        .unwrap()
    }

    fn stats(&self) -> ConnectionStats {
        // TODO:
        ConnectionStats {
            ping: Duration::from_millis(10),
            packets_lost: Default::default(),
            packets_sent: Default::default(),
            bytes_sent: 0,
            bytes_recv: 0,

            last_keep_alive_id: 0,
        }
    }
}

enum ConnectingTypes {
    Client(Box<std::future::Ready<WebSocketStream<MaybeTlsStream<TcpStream>>>>),
    Server(
        Pin<
            Box<
                dyn Future<
                        Output = Result<
                            WebSocketStream<TcpStream>,
                            tokio_tungstenite::tungstenite::error::Error,
                        >,
                    > + Send
                    + Sync,
            >,
        >,
    ),
}

pub struct TungsteniteNetworkConnectingWrapper {
    connecting: ConnectingTypes,
    addr: SocketAddr,
}

impl Future for TungsteniteNetworkConnectingWrapper {
    type Output = Result<TungsteniteNetworkConnectionWrapper, NetworkEventConnectingFailed>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match &mut self.connecting {
            ConnectingTypes::Client(connection) => {
                let ws_stream = pin!(connection).poll(cx);
                ws_stream.map(|f| {
                    let (send, recv) = f.split();
                    Ok(TungsteniteNetworkConnectionWrapper {
                        con: Arc::new(ConnectionType::Client((Mutex::new(send), Mutex::new(recv)))),
                        in_order_packets: Default::default(),
                        addr: self.addr,
                    })
                })
            }
            ConnectingTypes::Server(connection) => {
                let t = Pin::new(connection);
                let ws_stream = t.poll(cx);
                ws_stream.map(|f| match f {
                    Ok(connection) => {
                        let (send, recv) = connection.split();
                        Ok(TungsteniteNetworkConnectionWrapper {
                            con: Arc::new(ConnectionType::Server((
                                Mutex::new(send),
                                Mutex::new(recv),
                            ))),
                            in_order_packets: Default::default(),
                            addr: self.addr,
                        })
                    }
                    Err(err) => Err(NetworkEventConnectingFailed::Other(err.to_string())),
                })
            }
        }
    }
}

impl NetworkConnectingInterface<TungsteniteNetworkConnectionWrapper>
    for TungsteniteNetworkConnectingWrapper
{
    fn remote_addr(&self) -> SocketAddr {
        self.addr
    }
}

pub struct TungsteniteNetworkIncomingWrapper {
    incoming: TcpStream,
    addr: SocketAddr,
}

impl NetworkIncomingInterface<TungsteniteNetworkConnectingWrapper>
    for TungsteniteNetworkIncomingWrapper
{
    fn remote_addr(&self) -> SocketAddr {
        self.addr
    }
    fn accept(self) -> anyhow::Result<TungsteniteNetworkConnectingWrapper> {
        Ok(TungsteniteNetworkConnectingWrapper {
            connecting: ConnectingTypes::Server(Box::pin(tokio_tungstenite::accept_async(
                self.incoming,
            ))),
            addr: self.addr,
        })
    }
}

#[derive(Clone)]
pub struct TungsteniteEndpointWrapper {
    endpoint: Arc<TcpListener>,
}

#[async_trait::async_trait]
impl
    NetworkEndpointInterface<TungsteniteNetworkConnectingWrapper, TungsteniteNetworkIncomingWrapper>
    for TungsteniteEndpointWrapper
{
    fn connect(
        &self,
        addr: std::net::SocketAddr,
        _server_name: &str,
    ) -> anyhow::Result<TungsteniteNetworkConnectingWrapper, NetworkEventConnectingFailed> {
        let (res, _) = tokio_tungstenite::connect_async_tls_with_config(
            &format!("ws://{addr}"),
            None,
            false,
            Some(tokio_tungstenite::Connector::Plain),
        )
        .block_on()
        .map_err(|err| NetworkEventConnectingFailed::Other(err.to_string()))?;
        Ok(TungsteniteNetworkConnectingWrapper {
            connecting: ConnectingTypes::Client(Box::new(std::future::ready(res))),
            addr,
        })
    }

    fn close(&self, _error_code: ConnectionErrorCode, _reason: &str) {
        // TODO: mhh self.endpoint.close();
    }

    fn make_server_endpoint(
        bind_addr: std::net::SocketAddr,
        _cert_mode: NetworkServerCertMode,
        _options: &NetworkServerInitOptions,
    ) -> anyhow::Result<(Self, NetworkServerCertModeResult)> {
        let (endpoint, (cert, _)) = (
            TcpListener::bind(&bind_addr).block_on()?,
            create_certifified_keys(),
        );
        Ok((
            Self {
                endpoint: Arc::new(endpoint),
            },
            NetworkServerCertModeResult::Cert {
                cert: Box::new(cert),
            },
        ))
    }

    fn make_client_endpoint(
        bind_addr: std::net::SocketAddr,
        _options: &NetworkClientInitOptions,
    ) -> anyhow::Result<Self> {
        let res = TcpListener::bind(&bind_addr).block_on()?;
        Ok(Self {
            endpoint: Arc::new(res),
        })
    }

    async fn accept(&self) -> Option<TungsteniteNetworkIncomingWrapper> {
        let res = self.endpoint.accept().await;
        match res {
            Ok((stream, addr)) => Some(TungsteniteNetworkIncomingWrapper {
                incoming: stream,
                addr,
            }),
            Err(_) => None,
        }
    }

    fn sock_addr(&self) -> anyhow::Result<SocketAddr> {
        Ok(self.endpoint.local_addr()?)
    }
}

pub type TungsteniteNetwork<const TY: u32> = Network<
    TungsteniteEndpointWrapper,
    TungsteniteNetworkConnectionWrapper,
    TungsteniteNetworkConnectingWrapper,
    TungsteniteNetworkIncomingWrapper,
    TY,
>;
