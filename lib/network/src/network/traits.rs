use std::{future::Future, net::SocketAddr};

use pool::mt_datatypes::PoolVec;
use thiserror::Error;
use tokio::task::JoinHandle;

use super::{
    connection::ConnectionStats,
    errors::ConnectionErrorCode,
    event::{NetworkEventConnectingFailed, NetworkEventDisconnect},
    types::{
        NetworkClientInitOptions, NetworkInOrderChannel, NetworkServerCertMode,
        NetworkServerCertModeResult, NetworkServerInitOptions,
    },
};

#[async_trait::async_trait]
pub trait NetworkEndpointInterface<Z, I>: Clone + Send + Sync + 'static
where
    Self: Sized,
{
    fn close(&self, error_code: ConnectionErrorCode, reason: &str);
    fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
    ) -> anyhow::Result<Z, NetworkEventConnectingFailed>;
    async fn accept(&self) -> Option<I>;
    fn sock_addr(&self) -> anyhow::Result<SocketAddr>;

    fn make_server_endpoint(
        bind_addr: SocketAddr,
        cert_mode: NetworkServerCertMode,
        options: &NetworkServerInitOptions,
    ) -> anyhow::Result<(Self, NetworkServerCertModeResult)>;

    fn make_client_endpoint(
        bind_addr: SocketAddr,
        options: &NetworkClientInitOptions,
    ) -> anyhow::Result<Self>;
}

/// The result of a [`NetworkConnectionInterface::send_unreliable_unordered`] request.
#[derive(Error, Debug)]
pub enum UnreliableUnorderedError {
    /// A http like error occurred.
    #[error("connection was closed: {0}")]
    ConnectionClosed(anyhow::Error),
    #[error("unreliable unordered packets are not supported")]
    Disabled,
    #[error("packet too large for a single unreliable unordered packet.")]
    TooLarge,
}

/// Number of bidirectional streams that the implementations should support
/// (if bidi streams are enabled).
/// For better compatibility this value shouldn't be changed, and especially
/// not lowered.
pub const NUM_BIDI_STREAMS: u32 = 10;

/// the interface for connections. This includes sending receiving etc.
/// If a function returns an error, this usually results into a drop of the connection
#[async_trait::async_trait]
pub trait NetworkConnectionInterface: Clone + Send + Sync + 'static {
    async fn close(&self, error_code: ConnectionErrorCode, reason: &str);

    /// If known, return the close reason
    fn close_reason(&self) -> Option<NetworkEventDisconnect>;

    async fn send_unreliable_unordered(
        &self,
        data: PoolVec<u8>,
    ) -> anyhow::Result<(), (PoolVec<u8>, UnreliableUnorderedError)>;
    async fn read_unreliable_unordered(&self) -> anyhow::Result<Vec<u8>>;

    async fn send_unordered_reliable(&self, data: PoolVec<u8>) -> anyhow::Result<()>;
    async fn read_unordered_reliable<
        F: FnOnce(anyhow::Result<Vec<u8>>) -> JoinHandle<()> + Send + 'static,
    >(
        &self,
        on_data: F,
    ) -> anyhow::Result<()>;

    // this function guarantees that the packet was given to the implementation
    // in order. it should not block the network implementation more than necessary
    async fn push_ordered_reliable_packet_in_order(
        &self,
        data: PoolVec<u8>,
        channel: NetworkInOrderChannel,
    );
    async fn send_one_ordered_reliable(&self, channel: NetworkInOrderChannel)
    -> anyhow::Result<()>;
    async fn read_ordered_reliable<
        F: Fn(anyhow::Result<Vec<u8>>) -> JoinHandle<()> + Send + Sync + 'static,
    >(
        &self,
        on_data: F,
    ) -> anyhow::Result<()>;

    fn remote_addr(&self) -> SocketAddr;
    fn peer_identity(&self) -> x509_cert::Certificate;
    fn stats(&self) -> ConnectionStats;
}

pub trait NetworkConnectingInterface<C>:
    Send + Sync + 'static + Future<Output = Result<C, NetworkEventConnectingFailed>> + Unpin
where
    Self: Sized,
{
    fn remote_addr(&self) -> SocketAddr;
}

pub trait NetworkIncomingInterface<Z>: Send + Sync + 'static
where
    Self: Sized,
{
    fn remote_addr(&self) -> SocketAddr;
    fn accept(self) -> anyhow::Result<Z>;
}
