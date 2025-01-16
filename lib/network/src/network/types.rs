use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    sync::{mpsc::SyncSender, Arc},
    time::Duration,
};

use base::hash::Hash;
use ed25519_dalek::SigningKey;
use pool::mt_datatypes::PoolVec;

use tokio::sync::Mutex as TokioMutex;

use super::{
    connection::NetworkConnectionId, connections::NetworkConnectionIdCounter, errors::KickType,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum NetworkInOrderChannel {
    Global,
    Custom(usize),
}

pub(crate) enum NetworkEventSendType {
    /// packet loss possible, out of order possible
    UnreliableUnordered,
    /// packet loss **not** possible, out of order possible
    ReliableUnordered,
    /// Tries to send as unrealible first, if unsupported
    /// or packet too big for a single packet, falls back
    /// to reliable.
    UnorderedAuto,
    /// packet loss **not** possible, **in-order**
    ReliableOrdered(NetworkInOrderChannel),
}

pub(crate) enum NetworkLogicEvent {
    Connect {
        con_id: NetworkConnectionId,
        addr: String,
        cancel: Arc<tokio::sync::Notify>,
    },
    Disconnect {
        connection_id: NetworkConnectionId,
        sender: SyncSender<()>,
    },
    Send((NetworkConnectionId, PoolVec<u8>, NetworkEventSendType)),
    Kick {
        connection_id: NetworkConnectionId,
        ty: KickType,
    },
}

pub(crate) type NetworkPacket = PoolVec<u8>;

pub(crate) type NetworkInOrderPackets = HashMap<
    NetworkConnectionId,
    HashMap<NetworkInOrderChannel, Arc<TokioMutex<VecDeque<NetworkPacket>>>>,
>;

#[derive(Debug)]
pub enum NetworkClientCertCheckMode<'a> {
    CheckByCert { cert: Cow<'a, [u8]> },
    CheckByPubKeyHash { hash: Cow<'a, Hash> },
    // not recommended, only useful for debugging
    DisableCheck,
}

pub enum NetworkClientCertMode {
    FromCertAndPrivateKey {
        cert: x509_cert::Certificate,
        private_key: SigningKey,
    },
}

#[derive(Debug, Clone)]
pub enum NetworkServerCertModeResult {
    Cert { cert: Box<x509_cert::Certificate> },
    PubKeyHash { hash: Hash },
}

#[derive(Debug, Clone)]
pub struct NetworkServerCertAndKey {
    pub cert: x509_cert::Certificate,
    pub private_key: SigningKey,
}

#[derive(Debug, Clone)]
pub enum NetworkServerCertMode {
    FromCertAndPrivateKey(Box<NetworkServerCertAndKey>),
}

#[derive(Debug, Default, Clone)]
pub struct NetworkSharedInitOptions {
    pub debug_printing: Option<bool>,
    /// Max idle time before timing out
    pub timeout: Option<Duration>,
    /// Id generator that can be shared if multiple network implementations are required.
    pub id_generator: Arc<NetworkConnectionIdCounter>,
    /// How many packets at most should be in a pool instead of a heap (usually slower).
    pub max_packets_pooled: Option<usize>,
    /// How many packets should the backend assume to be used.
    pub packet_capacity: Option<usize>,
    /// How big should a single packet be assumed.
    /// __Caution__: this value is multiplied with
    /// [`NetworkSharedInitOptions::packet_capacity`].
    pub packet_size: Option<usize>,
    /// The size in bytes of the receive window
    /// per stream.
    pub stream_receive_window: Option<u32>,
    /// The size in bytes of the receive window
    /// of all streams.
    pub receive_window: Option<u32>,
    /// Maximum number of bytes to transmit to a peer without acknowledgment.
    pub send_window: Option<u32>,
    /// Max reordering of packets before it's considered lost.
    /// Should not be less than 3, per RFC5681.
    /// Note: ignored if not supported.
    pub packet_reorder_threshold: Option<u32>,
    /// Maximum reordering in time space before time based loss detection
    /// considers a packet lost, as a factor of RTT.
    /// Note: ignored if not supported.
    pub packet_time_threshold: Option<f32>,
    /// This threshold represents the number of ack-eliciting packets an endpoint
    /// may receive without immediately sending an ACK.
    pub ack_eliciting_threshold: Option<u32>,
    /// This parameter represents the maximum amount of time that an endpoint waits
    /// before sending an ACK when the ack-eliciting threshold hasn’t been reached.
    /// The effective max_ack_delay will be clamped to be at least the peer’s min_ack_delay
    /// transport parameter, and at most the greater of the current path RTT or 25ms.
    pub max_ack_delay: Option<Duration>,
    /// This threshold represents the amount of out-of-order packets that will trigger
    /// an endpoint to send an ACK, without waiting for ack_eliciting_threshold
    /// to be exceeded or for max_ack_delay to be elapsed.
    pub ack_reordering_threshold: Option<u32>,
}

impl NetworkSharedInitOptions {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn with_debug_priting(mut self, debug_printing: bool) -> Self {
        self.debug_printing = Some(debug_printing);
        self
    }
}

#[derive(Debug, Default, Clone)]
pub struct NetworkServerInitOptions {
    pub base: NetworkSharedInitOptions,
    pub max_thread_count: Option<usize>,
    /// disallow QUICs 0.5-RTT fast connection
    pub disallow_05_rtt: Option<bool>,
    /// disable that the connecting clients have
    /// to prove their connection.
    /// enabling this config makes connecting to
    /// the server faster, but might give more
    /// attack surface for DoS attacks
    pub disable_retry_on_connect: bool,
    /// Keep alive interval
    pub keep_alive: Option<Duration>,
}

impl NetworkServerInitOptions {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn with_max_thread_count(mut self, max_thread_count: usize) -> Self {
        self.max_thread_count = Some(max_thread_count);
        self
    }

    pub fn with_disallow_05_rtt(mut self, disallow_05_rtt: bool) -> Self {
        self.disallow_05_rtt = Some(disallow_05_rtt);
        self
    }

    pub fn with_disable_retry_on_connect(mut self, disable_retry_on_connect: bool) -> Self {
        self.disable_retry_on_connect = disable_retry_on_connect;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.base = self.base.with_timeout(timeout);
        self
    }

    pub fn with_keep_alive(mut self, keep_alive: Duration) -> Self {
        self.keep_alive = Some(keep_alive);
        self
    }

    pub fn with_stream_receive_window(mut self, stream_receive_window: u32) -> Self {
        self.base.stream_receive_window = Some(stream_receive_window);
        self
    }

    pub fn with_receive_window(mut self, receive_window: u32) -> Self {
        self.base.receive_window = Some(receive_window);
        self
    }

    pub fn with_send_window(mut self, send_window: u32) -> Self {
        self.base.send_window = Some(send_window);
        self
    }

    pub fn with_debug_priting(mut self, debug_printing: bool) -> Self {
        self.base = self.base.with_debug_priting(debug_printing);
        self
    }

    pub fn with_packet_capacity_and_size(mut self, capacity: usize, size: usize) -> Self {
        self.base.packet_capacity = Some(capacity);
        self.base.packet_size = Some(size);
        self
    }

    /// See [`NetworkSharedInitOptions::packet_reorder_threahold`] and
    /// [`NetworkSharedInitOptions::packet_time_threshold`]
    pub fn with_loss_detection_cfg(
        mut self,
        max_packet_reorder: u32,
        max_time_factor_reorder: f32,
    ) -> Self {
        self.base.packet_reorder_threshold = Some(max_packet_reorder);
        self.base.packet_time_threshold = Some(max_time_factor_reorder);
        self
    }

    /// See [`NetworkSharedInitOptions::ack_eliciting_threshold`],
    /// [`NetworkSharedInitOptions::max_ack_delay`] and
    /// [`NetworkSharedInitOptions::ack_reordering_threshold`] for more
    /// information.
    pub fn with_ack_config(
        mut self,
        ack_eliciting_threshold: u32,
        max_ack_delay: Duration,
        ack_reordering_threshold: u32,
    ) -> Self {
        self.base.ack_eliciting_threshold = Some(ack_eliciting_threshold);
        self.base.max_ack_delay = Some(max_ack_delay);
        self.base.ack_reordering_threshold = Some(ack_reordering_threshold);
        self
    }
}

pub struct NetworkClientInitOptions<'a> {
    pub base: NetworkSharedInitOptions,
    pub cert_check: NetworkClientCertCheckMode<'a>,
    pub cert: NetworkClientCertMode,
}

impl<'a> NetworkClientInitOptions<'a> {
    pub fn new(cert_check: NetworkClientCertCheckMode<'a>, cert: NetworkClientCertMode) -> Self {
        Self {
            base: Default::default(),
            cert_check,
            cert,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.base = self.base.with_timeout(timeout);
        self
    }

    pub fn with_stream_receive_window(mut self, stream_receive_window: u32) -> Self {
        self.base.stream_receive_window = Some(stream_receive_window);
        self
    }

    pub fn with_receive_window(mut self, receive_window: u32) -> Self {
        self.base.receive_window = Some(receive_window);
        self
    }

    pub fn with_send_window(mut self, send_window: u32) -> Self {
        self.base.send_window = Some(send_window);
        self
    }

    pub fn with_debug_priting(mut self, debug_printing: bool) -> Self {
        self.base = self.base.with_debug_priting(debug_printing);
        self
    }

    /// See [`NetworkSharedInitOptions::packet_reorder_threahold`] and
    /// [`NetworkSharedInitOptions::packet_time_threshold`]
    pub fn with_loss_detection_cfg(
        mut self,
        max_packet_reorder: u32,
        max_time_factor_reorder: f32,
    ) -> Self {
        self.base.packet_reorder_threshold = Some(max_packet_reorder);
        self.base.packet_time_threshold = Some(max_time_factor_reorder);
        self
    }

    /// See [`NetworkSharedInitOptions::ack_eliciting_threshold`],
    /// [`NetworkSharedInitOptions::max_ack_delay`] and
    /// [`NetworkSharedInitOptions::ack_reordering_threshold`] for more
    /// information.
    pub fn with_ack_config(
        mut self,
        ack_eliciting_threshold: u32,
        max_ack_delay: Duration,
        ack_reordering_threshold: u32,
    ) -> Self {
        self.base.ack_eliciting_threshold = Some(ack_eliciting_threshold);
        self.base.max_ack_delay = Some(max_ack_delay);
        self.base.ack_reordering_threshold = Some(ack_reordering_threshold);
        self
    }
}
