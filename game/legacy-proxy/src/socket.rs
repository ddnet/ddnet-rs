use anyhow::anyhow;
use base_io::{io::Io, runtime::IoRuntimeTask};
use futures::FutureExt;
use libtw2_net::{Timestamp, net::Callback};
use rand::RngCore as _;
use std::{error, fmt, future, io, net::SocketAddr, sync::Arc, time::Instant};
use tokio::{
    net::UdpSocket,
    sync::{
        Mutex,
        mpsc::{Receiver, Sender, channel, error::TryRecvError},
    },
};

#[derive(Debug)]
pub struct NoAddressFamiliesSupported(());

impl error::Error for NoAddressFamiliesSupported {}

impl fmt::Display for NoAddressFamiliesSupported {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("neither IPv4 nor IPv6 supported on this system")
    }
}

type ThreadedReceiver = Arc<Mutex<Receiver<(Vec<u8>, SocketAddr)>>>;

struct AsyncSocket {
    sender: Sender<Option<(Vec<u8>, SocketAddr)>>,
    receiver: ThreadedReceiver,
    _recv_task: IoRuntimeTask<()>,
    _send_task: IoRuntimeTask<()>,
}

impl Drop for AsyncSocket {
    fn drop(&mut self) {
        // The sender is more important and gets special logic that
        // makes it more likely that a disconnect pkt is send just in time.
        let _ = self.sender.blocking_send(None);
        let _ = self.sender.blocking_send(None);
    }
}

pub struct Socket {
    start: Instant,
    v4: Option<AsyncSocket>,
    v6: Option<AsyncSocket>,
}

async fn udp_socket(ipv4: bool) -> io::Result<Option<UdpSocket>> {
    let socket = if ipv4 {
        UdpSocket::bind("0.0.0.0:0")
    } else {
        UdpSocket::bind("[::]:0")
    };
    let socket = match socket.await {
        Err(_) => {
            // Assume address family not supported.
            return Ok(None);
        }
        b => b?,
    };
    Ok(Some(socket))
}

impl Socket {
    pub fn new(io: &Io) -> anyhow::Result<Socket> {
        let v4 = io.rt.spawn(async { Ok(udp_socket(true).await?) }).get()?;
        let v6 = io.rt.spawn(async { Ok(udp_socket(false).await?) }).get()?;

        if v4.is_none() && v6.is_none() {
            return Err(io::Error::other(NoAddressFamiliesSupported(())).into());
        }
        let spawn_socket = |s: Arc<UdpSocket>| {
            // recv
            let socket = s.clone();
            let (sender, receiver) = channel::<(Vec<u8>, SocketAddr)>(4096);
            let recv_task = io
                .rt
                .spawn(
                    async move {
                        let mut b = Vec::with_capacity(4096);

                        while let Ok((_, addr)) = s.recv_buf_from(&mut b).await {
                            sender.send((b.clone(), addr)).await?;
                            b.clear();
                        }

                        anyhow::Ok(())
                    }
                    .inspect(|r| match r {
                        Ok(()) => {
                            log::info!("Proxy socket recv part closed");
                        }
                        Err(err) => {
                            log::error!("recv failed: {err}");
                        }
                    }),
                )
                .abortable();
            // send
            let (sender, mut receiver_task) = channel::<Option<(Vec<u8>, SocketAddr)>>(4096);
            let send_task = io.rt.spawn(
                async move {
                    while let Some(Some((pkt, addr))) = receiver_task.recv().await {
                        socket.send_to(&pkt, addr).await.map_err(|err| {
                            anyhow!("Failed to send {} bytes to {addr}: {err}", pkt.len())
                        })?;
                    }
                    anyhow::Ok(())
                }
                .inspect(|r| match r {
                    Ok(()) => {
                        log::info!("Proxy socket sending part closed");
                    }
                    Err(err) => {
                        log::error!("sending failed: {err}");
                    }
                }),
            );
            AsyncSocket {
                sender,
                receiver: Arc::new(Mutex::new(receiver)),
                _recv_task: recv_task,
                _send_task: send_task,
            }
        };
        let v4 = v4.map(|v4| spawn_socket(Arc::new(v4)));
        let v6 = v6.map(|v6| spawn_socket(Arc::new(v6)));

        Ok(Socket {
            start: Instant::now(),
            v4,
            v6,
        })
    }
    pub fn try_recv(&self) -> Result<(SocketAddr, Vec<u8>), TryRecvError> {
        match self
            .v4
            .as_ref()
            .ok_or(TryRecvError::Empty)
            .and_then(|v| v.receiver.blocking_lock().try_recv())
        {
            Ok((buf, addr)) => Ok((addr, buf)),
            Err(err) => {
                if matches!(err, TryRecvError::Empty) {
                    match self
                        .v6
                        .as_ref()
                        .ok_or(TryRecvError::Empty)
                        .and_then(|v| v.receiver.blocking_lock().try_recv())
                    {
                        Ok((buf, addr)) => Ok((addr, buf)),
                        Err(err) => Err(err),
                    }
                } else {
                    Err(err)
                }
            }
        }
    }
    pub async fn recv_from(
        v4: Option<ThreadedReceiver>,
        v6: Option<ThreadedReceiver>,
    ) -> Option<(Vec<u8>, SocketAddr)> {
        tokio::select! {
            v = async {
                if let Some(v4) = v4 {
                    v4.lock().await.recv().await
                }
                else {
                   future::pending().await
                }
            } => v,
            v = async {
                if let Some(v6) = v6 {
                    v6.lock().await.recv().await
                }
                else {
                   future::pending().await
                }
            } => v,
        }
    }
    pub fn receivers(&self) -> (Option<ThreadedReceiver>, Option<ThreadedReceiver>) {
        (
            self.v4.as_ref().map(|v| v.receiver.clone()),
            self.v6.as_ref().map(|v| v.receiver.clone()),
        )
    }
}

impl Callback<SocketAddr> for Socket {
    type Error = io::Error;
    fn secure_random(&mut self, buffer: &mut [u8]) {
        rand::rng().fill_bytes(buffer)
    }
    fn send(&mut self, addr: SocketAddr, data: &[u8]) -> Result<(), io::Error> {
        match &addr {
            SocketAddr::V4(_) => {
                if let Some(v4) = &mut self.v4 {
                    v4.sender
                        .blocking_send(Some((data.to_vec(), addr)))
                        .map_err(|e| io::Error::new(io::ErrorKind::NotConnected, e))
                } else {
                    Err(io::Error::new(io::ErrorKind::NotFound, ""))
                }
            }
            SocketAddr::V6(_) => {
                if let Some(v6) = &mut self.v6 {
                    v6.sender
                        .blocking_send(Some((data.to_vec(), addr)))
                        .map_err(|e| io::Error::new(io::ErrorKind::NotConnected, e))
                } else {
                    Err(io::Error::new(io::ErrorKind::NotFound, ""))
                }
            }
        }
    }
    fn time(&mut self) -> Timestamp {
        Timestamp::from_secs_since_epoch(0) + self.start.elapsed()
    }
}
