use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
};

use anyhow::anyhow;
use num_traits::FromPrimitive;
use pool::mt_datatypes::PoolVec;
use quinn::{ConnectError, ConnectionError, SendDatagramError};
use spki::der::Decode;
use tokio::io::AsyncWriteExt;

use super::{
    connection::ConnectionStats,
    errors::{BanType, Banned, ConnectionErrorCode},
    event::{NetworkEventConnectingClosed, NetworkEventConnectingFailed, NetworkEventDisconnect},
    network::Network,
    network_async::NetworkAsync,
    networks::Networks,
    quinnminimal::{make_client_endpoint, make_server_endpoint},
    traits::{
        NetworkConnectingInterface, NetworkConnectionInterface, NetworkEndpointInterface,
        NetworkIncomingInterface, UnreliableUnorderedError, NUM_BIDI_STREAMS,
    },
    types::{
        NetworkClientInitOptions, NetworkInOrderChannel, NetworkServerCertMode,
        NetworkServerCertModeResult, NetworkServerInitOptions,
    },
};

#[derive(Default)]
pub struct QuinnNetworkConnectingWrapperChannel {
    in_order_packets: VecDeque<PoolVec<u8>>,
    open_bi: Option<(quinn::SendStream, quinn::RecvStream)>,
}

#[derive(Clone)]
pub struct QuinnNetworkConnectionWrapper {
    con: quinn::Connection,
    channels: Arc<
        std::sync::Mutex<
            HashMap<
                NetworkInOrderChannel,
                Arc<tokio::sync::Mutex<QuinnNetworkConnectingWrapperChannel>>,
            >,
        >,
    >,
    stream_window: usize,
}

impl QuinnNetworkConnectionWrapper {
    async fn write_bytes_chunked(
        send_stream: &mut quinn::SendStream,
        packet: PoolVec<u8>,
    ) -> anyhow::Result<()> {
        let packet_len = packet.len() as u64;
        let send_buffer = [packet_len.to_le_bytes().to_vec(), packet.take()].concat();
        let written_bytes = send_stream.write_all(send_buffer.as_slice()).await;
        if let Err(err) = written_bytes {
            Err(anyhow!(format!("packet write failed: {}", err.to_string())))
        } else {
            match send_stream.flush().await {
                Ok(_) => Ok(()),
                Err(err) => Err(anyhow!(format!("packet flush failed: {}", err.to_string()))),
            }
        }
    }
}

#[async_trait::async_trait]
impl NetworkConnectionInterface for QuinnNetworkConnectionWrapper {
    async fn send_unreliable_unordered(
        &self,
        data: PoolVec<u8>,
    ) -> anyhow::Result<(), (PoolVec<u8>, UnreliableUnorderedError)> {
        let pack_bytes = bytes::Bytes::copy_from_slice(&data[..]);
        let res = self
            .con
            .send_datagram(pack_bytes)
            .map_err(|err| match err {
                SendDatagramError::Disabled | SendDatagramError::UnsupportedByPeer => {
                    (data, UnreliableUnorderedError::Disabled)
                }
                SendDatagramError::ConnectionLost(err) => {
                    (data, UnreliableUnorderedError::ConnectionClosed(err.into()))
                }
                SendDatagramError::TooLarge => (data, UnreliableUnorderedError::TooLarge),
            })?;
        Ok(res)
    }

    async fn read_unreliable_unordered(&self) -> anyhow::Result<Vec<u8>> {
        let res = self.con.read_datagram().await;
        match res {
            Ok(res) => Ok(res.to_vec()),
            Err(err) => Err(anyhow!(err.to_string())),
        }
    }

    async fn send_unordered_reliable(&self, data: PoolVec<u8>) -> anyhow::Result<()> {
        let uni = self.con.open_uni().await;
        if let Ok(mut stream) = uni {
            let written_bytes = stream.write_all(data.as_slice()).await;
            if let Err(_written_bytes) = written_bytes {
                Err(anyhow!("packet write failed."))
            } else {
                let finish_res = stream.finish();
                if let Err(err) = finish_res {
                    Err(anyhow!(format!(
                        "packet finish failed: {}",
                        err.to_string()
                    )))
                } else {
                    Ok(())
                }
            }
        } else {
            Err(anyhow!(format!(
                "sent stream err: {}",
                uni.unwrap_err().to_string()
            )))
        }
    }

    async fn read_unordered_reliable<
        F: FnOnce(anyhow::Result<Vec<u8>>) -> tokio::task::JoinHandle<()> + Send + 'static,
    >(
        &self,
        on_data: F,
    ) -> anyhow::Result<()> {
        let uni = self.con.accept_uni().await;
        let stream_window = self.stream_window;
        match uni {
            Ok(mut recv_stream) => {
                tokio::spawn(async move {
                    match recv_stream.read_to_end(stream_window).await {
                        Ok(read_res) => {
                            // ignore error
                            let _ = on_data(Ok(read_res)).await;
                        }
                        Err(read_err) => {
                            on_data(Err(anyhow!(format!(
                                "connection stream acception failed {}",
                                read_err
                            ))))
                            .await?;
                        }
                    }
                    anyhow::Ok(())
                });
                anyhow::Ok(())
            }
            Err(recv_err) => Err(anyhow!(format!(
                "connection stream acception failed {}",
                recv_err
            ))),
        }
    }

    async fn push_ordered_reliable_packet_in_order(
        &self,
        data: PoolVec<u8>,
        channel: NetworkInOrderChannel,
    ) {
        let cur_channel = {
            let mut channels = self.channels.lock().unwrap();
            let has_global = channels.contains_key(&NetworkInOrderChannel::Global);
            let reserved_channels = if has_global { 0 } else { 1 };
            let channel = if channels.len() >= NUM_BIDI_STREAMS as usize - reserved_channels {
                // always fall back to the global channel if limit is reached
                NetworkInOrderChannel::Global
            } else {
                channel
            };
            channels.entry(channel).or_default();
            let cur_channel = channels.get_mut(&channel).unwrap().clone();
            drop(channels);
            cur_channel
        };
        cur_channel.lock().await.in_order_packets.push_back(data);
    }

    async fn send_one_ordered_reliable(
        &self,
        channel: NetworkInOrderChannel,
    ) -> anyhow::Result<()> {
        let cur_channel = {
            let mut channels = self.channels.lock().unwrap();
            let cur_channel = channels
                .get_mut(&channel)
                .cloned()
                .or_else(|| channels.get_mut(&NetworkInOrderChannel::Global).cloned());
            cur_channel
        };
        if let Some(cur_channel) = cur_channel {
            let mut cur_channel = cur_channel.lock().await;
            let packet_res = cur_channel.in_order_packets.pop_front();
            if let Some(packet) = packet_res {
                if let Some((send_stream, _)) = cur_channel.open_bi.as_mut() {
                    Self::write_bytes_chunked(send_stream, packet).await
                } else {
                    match self.con.open_bi().await {
                        Ok((send, recv)) => {
                            cur_channel.open_bi = Some((send, recv));
                            Self::write_bytes_chunked(
                                &mut cur_channel.open_bi.as_mut().unwrap().0,
                                packet,
                            )
                            .await
                        }
                        Err(err) => Err(anyhow!(err.to_string())),
                    }
                }
            } else {
                Err(anyhow!("No packet was queued."))
            }
        } else {
            Err(anyhow!("Channel did not exist."))
        }
    }

    async fn read_ordered_reliable<
        F: Fn(anyhow::Result<Vec<u8>>) -> tokio::task::JoinHandle<()> + Send + Sync + 'static,
    >(
        &self,
        on_data: F,
    ) -> anyhow::Result<()> {
        let stream_window = self.stream_window;
        match self.con.accept_bi().await {
            Ok((_, mut recv_stream)) => {
                tokio::spawn(async move {
                    let mut len_buff: [u8; std::mem::size_of::<u64>()] = Default::default();
                    'read_loop: loop {
                        match recv_stream.read_exact(&mut len_buff).await {
                            Ok(_) => {
                                let read_buff_len = u64::from_le_bytes(len_buff);
                                if read_buff_len > stream_window as u64 {
                                    on_data(Err(anyhow!("read size exceeded max length."))).await?;
                                    break 'read_loop;
                                } else {
                                    let mut read_buff: Vec<u8> = Vec::new();
                                    read_buff.resize(read_buff_len as usize, Default::default());

                                    match recv_stream.read_exact(read_buff.as_mut()).await {
                                        Ok(_) => {
                                            on_data(Ok(read_buff)).await?;
                                        }
                                        Err(err) => {
                                            on_data(Err(anyhow!(err.to_string()))).await?;
                                            break 'read_loop;
                                        }
                                    }
                                }
                            }
                            Err(err) => {
                                on_data(Err(anyhow!(err.to_string()))).await?;
                                break 'read_loop;
                            }
                        }
                    }

                    anyhow::Ok(())
                });
                Ok(())
            }
            Err(err) => {
                return Err(anyhow!(err.to_string()));
            }
        }
    }

    async fn close(&self, error_code: ConnectionErrorCode, reason: &str) {
        self.con
            .close((error_code as u32).into(), reason.as_bytes());
        self.con.closed().await;
    }

    fn close_reason(&self) -> Option<NetworkEventDisconnect> {
        self.con.close_reason().map(|err| match err {
            ConnectionError::VersionMismatch
            | ConnectionError::CidsExhausted
            | ConnectionError::TransportError(_) => NetworkEventDisconnect::Other(err.to_string()),
            ConnectionError::ConnectionClosed(connection_close) => {
                NetworkEventDisconnect::ConnectionClosed(NetworkEventConnectingClosed::Other(
                    connection_close.to_string(),
                ))
            }
            ConnectionError::ApplicationClosed(application_close) => {
                let reason = String::from_utf8_lossy(&application_close.reason).into();
                NetworkEventDisconnect::ConnectionClosed(
                    match ConnectionErrorCode::from_u64(application_close.error_code.into_inner()) {
                        Some(code) => match code {
                            ConnectionErrorCode::Kicked => {
                                NetworkEventConnectingClosed::Kicked(reason)
                            }
                            ConnectionErrorCode::Banned => NetworkEventConnectingClosed::Banned(
                                serde_json::from_str(&reason).unwrap_or_else(|_| Banned {
                                    msg: BanType::Custom("unknown reason".to_string()),
                                    until: None,
                                }),
                            ),
                            ConnectionErrorCode::Shutdown => {
                                NetworkEventConnectingClosed::Shutdown(reason)
                            }
                        },
                        None => NetworkEventConnectingClosed::Other(reason),
                    },
                )
            }
            ConnectionError::Reset => {
                NetworkEventDisconnect::ConnectionClosed(NetworkEventConnectingClosed::Reset)
            }
            ConnectionError::TimedOut => NetworkEventDisconnect::TimedOut,
            ConnectionError::LocallyClosed => NetworkEventDisconnect::LocallyClosed,
        })
    }

    fn remote_addr(&self) -> SocketAddr {
        self.con.remote_address()
    }

    fn peer_identity(&self) -> x509_cert::Certificate {
        let certs = self.con.peer_identity().unwrap();
        let certs: &Vec<rustls::pki_types::CertificateDer> = certs.downcast_ref().unwrap();
        x509_cert::Certificate::from_der(&certs[0]).unwrap()
    }

    fn stats(&self) -> ConnectionStats {
        let mut stats = self.con.stats();

        stats.path.rtt = self.con.rtt();

        ConnectionStats {
            ping: stats.path.rtt,
            packets_lost: stats.path.lost_packets,
            packets_sent: stats.path.sent_packets,
            bytes_sent: stats.udp_tx.bytes,
            bytes_recv: stats.udp_rx.bytes,

            last_keep_alive_id: stats.frame_tx.acks,
        }
    }
}

pub struct QuinnNetworkConnectingWrapper {
    connecting: quinn::Connecting,
    stream_window: usize,
}

impl Future for QuinnNetworkConnectingWrapper {
    type Output = Result<QuinnNetworkConnectionWrapper, NetworkEventConnectingFailed>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let con = Pin::new(&mut self.connecting).poll(cx);
        con.map(|f| match f {
            Ok(connection) => Ok(QuinnNetworkConnectionWrapper {
                con: connection,
                channels: Default::default(),
                stream_window: self.stream_window,
            }),
            Err(err) => Err(match err {
                ConnectionError::VersionMismatch
                | ConnectionError::CidsExhausted
                | ConnectionError::TransportError(_) => {
                    NetworkEventConnectingFailed::Other(err.to_string())
                }
                ConnectionError::ConnectionClosed(connection_close) => {
                    NetworkEventConnectingFailed::ConnectionClosed(
                        NetworkEventConnectingClosed::Other(connection_close.to_string()),
                    )
                }
                ConnectionError::ApplicationClosed(application_close) => {
                    let reason = String::from_utf8_lossy(&application_close.reason).into();
                    NetworkEventConnectingFailed::ConnectionClosed(
                        match ConnectionErrorCode::from_u64(
                            application_close.error_code.into_inner(),
                        ) {
                            Some(code) => match code {
                                ConnectionErrorCode::Kicked => {
                                    NetworkEventConnectingClosed::Kicked(reason)
                                }
                                ConnectionErrorCode::Banned => {
                                    NetworkEventConnectingClosed::Banned(
                                        serde_json::from_str(&reason).unwrap_or_else(|_| Banned {
                                            msg: BanType::Custom("unknown reason".to_string()),
                                            until: None,
                                        }),
                                    )
                                }
                                ConnectionErrorCode::Shutdown => {
                                    NetworkEventConnectingClosed::Shutdown(reason)
                                }
                            },
                            None => NetworkEventConnectingClosed::Other(reason),
                        },
                    )
                }
                ConnectionError::Reset => NetworkEventConnectingFailed::ConnectionClosed(
                    NetworkEventConnectingClosed::Reset,
                ),
                ConnectionError::TimedOut => NetworkEventConnectingFailed::TimedOut,
                ConnectionError::LocallyClosed => NetworkEventConnectingFailed::LocallyClosed,
            }),
        })
    }
}

impl NetworkConnectingInterface<QuinnNetworkConnectionWrapper> for QuinnNetworkConnectingWrapper {
    fn remote_addr(&self) -> SocketAddr {
        self.connecting.remote_address()
    }
}

pub struct QuinnNetworkIncomingWrapper {
    inc: quinn::Incoming,
    stream_window: usize,
}

impl NetworkIncomingInterface<QuinnNetworkConnectingWrapper> for QuinnNetworkIncomingWrapper {
    fn remote_addr(&self) -> SocketAddr {
        self.inc.remote_address()
    }

    fn accept(self) -> anyhow::Result<QuinnNetworkConnectingWrapper> {
        Ok(QuinnNetworkConnectingWrapper {
            connecting: self.inc.accept()?,
            stream_window: self.stream_window,
        })
    }
}

#[derive(Clone)]
pub struct QuinnEndpointWrapper {
    endpoint: quinn::Endpoint,
    must_retry_inc: bool,
    stream_window: usize,
}

#[async_trait::async_trait]
impl NetworkEndpointInterface<QuinnNetworkConnectingWrapper, QuinnNetworkIncomingWrapper>
    for QuinnEndpointWrapper
{
    fn connect(
        &self,
        addr: std::net::SocketAddr,
        server_name: &str,
    ) -> anyhow::Result<QuinnNetworkConnectingWrapper, NetworkEventConnectingFailed> {
        let res = self
            .endpoint
            .connect(addr, server_name)
            .map_err(|err| match err {
                ConnectError::CidsExhausted
                | ConnectError::EndpointStopping
                | ConnectError::NoDefaultClientConfig
                | ConnectError::UnsupportedVersion => {
                    NetworkEventConnectingFailed::Other(err.to_string())
                }
                ConnectError::InvalidServerName(name) => {
                    NetworkEventConnectingFailed::InvalidServerName(name)
                }
                ConnectError::InvalidRemoteAddress(socket_addr) => {
                    NetworkEventConnectingFailed::InvalidRemoteAddress(socket_addr)
                }
            })?;
        Ok(QuinnNetworkConnectingWrapper {
            connecting: res,
            stream_window: self.stream_window,
        })
    }

    fn close(&self, error_code: ConnectionErrorCode, reason: &str) {
        self.endpoint
            .close((error_code as u32).into(), reason.as_bytes());
    }

    fn make_server_endpoint(
        bind_addr: std::net::SocketAddr,
        cert_mode: NetworkServerCertMode,
        options: &NetworkServerInitOptions,
    ) -> anyhow::Result<(Self, NetworkServerCertModeResult)> {
        let (endpoint, cert) = make_server_endpoint(bind_addr, cert_mode, options)?;
        Ok((
            Self {
                endpoint,
                must_retry_inc: !options.disable_retry_on_connect,
                stream_window: options
                    .base
                    .stream_receive_window
                    .unwrap_or(1024u32 * 64u32) as usize,
            },
            cert,
        ))
    }

    fn make_client_endpoint(
        bind_addr: std::net::SocketAddr,
        options: &NetworkClientInitOptions,
    ) -> anyhow::Result<Self> {
        let res = make_client_endpoint(bind_addr, options)?;
        Ok(Self {
            endpoint: res,
            must_retry_inc: false,
            stream_window: options
                .base
                .stream_receive_window
                .unwrap_or(1024u32 * 1024u32) as usize,
        })
    }

    async fn accept(&self) -> Option<QuinnNetworkIncomingWrapper> {
        while let Some(inc) = self.endpoint.accept().await {
            if self.must_retry_inc && !inc.remote_address_validated() {
                inc.retry().unwrap();
            } else {
                return Some(QuinnNetworkIncomingWrapper {
                    inc,
                    stream_window: self.stream_window,
                });
            }
        }
        None
    }

    fn sock_addr(&self) -> anyhow::Result<SocketAddr> {
        Ok(self.endpoint.local_addr()?)
    }
}

pub type QuinnNetworks = Networks<
    QuinnEndpointWrapper,
    QuinnNetworkConnectionWrapper,
    QuinnNetworkConnectingWrapper,
    QuinnNetworkIncomingWrapper,
>;

pub type QuinnNetwork = Network<
    QuinnEndpointWrapper,
    QuinnNetworkConnectionWrapper,
    QuinnNetworkConnectingWrapper,
    QuinnNetworkIncomingWrapper,
    0,
>;

pub type QuinnNetworkAsync = NetworkAsync<
    QuinnEndpointWrapper,
    QuinnNetworkConnectionWrapper,
    QuinnNetworkConnectingWrapper,
    QuinnNetworkIncomingWrapper,
    0,
>;
