use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{Context, ensure};
use network::network::{
    errors::ConnectionErrorCode,
    proxy::create_proxy_certificate,
    quinnminimal::{configure_client, make_server_endpoint},
    types::{
        NetworkClientCertCheckMode, NetworkClientCertMode, NetworkClientInitOptions,
        NetworkServerCertAndKey, NetworkServerCertMode, NetworkServerInitOptions,
    },
};
use quinn::{Connection, ConnectionError, Endpoint, RecvStream, SendStream};
use tokio::task::JoinSet;
use x509_cert::{Certificate, der::Decode};

use crate::config::{Config, parse_hash};

fn shutdown(connection: &Connection, reason: &[u8]) {
    connection.close((ConnectionErrorCode::Shutdown as u32).into(), reason);
}

fn propagate_close(target: &Connection, reason: ConnectionError) {
    if let ConnectionError::ApplicationClosed(close) = reason {
        target.close(close.error_code, &close.reason);
    } else {
        shutdown(target, b"proxy peer disconnected");
    }
}

async fn copy_stream(mut recv: RecvStream, mut send: SendStream) -> anyhow::Result<()> {
    loop {
        // Observe STOP_SENDING even while the source stream is idle.
        let read = tokio::select! {
            result = recv.read_chunk(64 * 1024, true) => result,
            result = send.stopped() => {
                if let Some(code) = result? { let _ = recv.stop(code); }
                return Ok(());
            }
        };
        match read {
            Ok(Some(chunk)) => {
                if let Err(err) = send.write_all(&chunk.bytes).await {
                    if let quinn::WriteError::Stopped(code) = err {
                        let _ = recv.stop(code);
                        return Ok(());
                    }
                    return Err(err.into());
                }
            }
            Ok(None) => {
                send.finish()?;
                // Retain the stream until delivery is acknowledged or stopped.
                if let Some(code) = send.stopped().await? {
                    let _ = recv.stop(code);
                }
                return Ok(());
            }
            Err(quinn::ReadError::Reset(code)) => {
                let _ = send.reset(code);
                return Ok(());
            }
            Err(err) => return Err(err.into()),
        }
    }
}

async fn unidirectional(source: &Connection, target: &Connection) -> anyhow::Result<()> {
    let mut streams = JoinSet::new();
    loop {
        tokio::select! {
            incoming = source.accept_uni(), if streams.len() < 32 => {
                let recv = incoming?;
                let send = target.open_uni().await?;
                streams.spawn(copy_stream(recv, send));
            }
            result = streams.join_next(), if !streams.is_empty() => { result.unwrap()??; }
        }
    }
}

async fn bidirectional(source: &Connection, target: &Connection) -> anyhow::Result<()> {
    let mut streams = JoinSet::new();
    loop {
        tokio::select! {
            incoming = source.accept_bi(), if streams.len() < 16 => {
                let (source_send, source_recv) = incoming?;
                let (target_send, target_recv) = target.open_bi().await?;
                streams.spawn(async move {
                    tokio::try_join!(copy_stream(source_recv, target_send), copy_stream(target_recv, source_send))?;
                    anyhow::Ok(())
                });
            }
            result = streams.join_next(), if !streams.is_empty() => { result.unwrap()??; }
        }
    }
}

async fn datagrams(source: &Connection, target: &Connection) -> anyhow::Result<()> {
    loop {
        let data = source.read_datagram().await?;
        match target.send_datagram(data) {
            Ok(()) | Err(quinn::SendDatagramError::TooLarge) => {}
            Err(err) => return Err(err.into()),
        }
    }
}

/// Relay encrypted-transport payloads verbatim; framing and compression stay end-to-end.
/// Each stream keeps its byte order, FIN, reset and stop semantics.
pub async fn relay(front: Connection, back: Connection) -> anyhow::Result<()> {
    let result = tokio::select! {
        reason = front.closed() => { propagate_close(&back, reason); Ok(()) }
        reason = back.closed() => { propagate_close(&front, reason); Ok(()) }
        result = async {
            tokio::try_join!(
                unidirectional(&front, &back), unidirectional(&back, &front),
                bidirectional(&front, &back), bidirectional(&back, &front),
                datagrams(&front, &back), datagrams(&back, &front),
            )?;
            anyhow::Ok(())
        } => {
            // A stream operation can observe a connection close before closed() wins select.
            if let Some(reason) = front.close_reason() { propagate_close(&back, reason); }
            else if let Some(reason) = back.close_reason() { propagate_close(&front, reason); }
            else {
                shutdown(&front, b"proxy relay failed");
                shutdown(&back, b"proxy relay failed");
            }
            result
        }
    };
    result
}

async fn connect_backend(
    front: &Connection,
    endpoint: &Endpoint,
    config: &Config,
    backend_game: SocketAddr,
) -> anyhow::Result<Connection> {
    let cert = {
        let identity = front
            .peer_identity()
            .context("missing client certificate")?;
        let chain = identity
            .downcast_ref::<Vec<rustls::pki_types::CertificateDer<'static>>>()
            .context("unsupported client identity")?;
        Certificate::from_der(chain.first().context("empty client certificate chain")?)?
    };
    let options = backend_options(config, front.remote_address(), &cert)?;
    let connecting = endpoint.connect_with(
        configure_client(&options)?,
        backend_game,
        &config.backend_server_name,
    )?;
    tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(config.connect_timeout_seconds), connecting) => Ok(result??),
        _ = front.closed() => anyhow::bail!("client disconnected before backend connected"),
    }
}

/// Shared authenticated backend setup for native and legacy frontends.
pub fn backend_options(
    config: &Config,
    original_addr: SocketAddr,
    cert: &Certificate,
) -> anyhow::Result<NetworkClientInitOptions<'static>> {
    let (_, key) = config.identity()?;
    let forwarded = create_proxy_certificate(&key, original_addr, cert)?;
    let hash = parse_hash(&config.backend_public_key_hash)?;
    let options = NetworkClientInitOptions::new(
        NetworkClientCertCheckMode::CheckByPubKeyHash {
            hash: std::borrow::Cow::Owned(hash),
        },
        NetworkClientCertMode::FromCertAndPrivateKey {
            cert: forwarded,
            private_key: key,
        },
    )
    .with_timeout(Duration::from_secs(config.idle_timeout_seconds));
    Ok(options)
}

fn make_frontends(config: &Config) -> anyhow::Result<(Endpoint, Endpoint)> {
    let (cert, private_key) = config.identity()?;
    let cert_mode =
        NetworkServerCertMode::FromCertAndPrivateKey(Box::new(NetworkServerCertAndKey {
            cert,
            private_key,
        }));
    let options = NetworkServerInitOptions::new()
        .with_timeout(Duration::from_secs(config.idle_timeout_seconds));
    let (v4, _) = make_server_endpoint(config.listen_game_v4.into(), cert_mode.clone(), &options)?;
    let (v6, _) = make_server_endpoint(config.listen_game_v6.into(), cert_mode, &options)?;
    Ok((v4, v6))
}

pub async fn run(config: Config, masters: Vec<url::Url>) -> anyhow::Result<()> {
    ensure!(
        config.max_connections > 0,
        "max_connections must be positive"
    );
    ensure!(
        config.connect_timeout_seconds > 0 && config.idle_timeout_seconds > 0,
        "timeouts must be positive"
    );
    parse_hash(&config.backend_public_key_hash)?;
    let info = crate::s2s::discover(&config).await?;
    let backend_game = SocketAddr::new(
        config.backend_s2s.ip(),
        if config.backend_s2s.is_ipv4() {
            info.game_port_v4
        } else {
            info.game_port_v6
        },
    );
    let (front_v4, front_v6) = make_frontends(&config)?;
    let bind: SocketAddr = if backend_game.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    }
    .parse()?;
    let back = Endpoint::client(bind)?;
    eprintln!(
        "proxy listening on {} and {}, backend {}",
        front_v4.local_addr()?,
        front_v6.local_addr()?,
        backend_game
    );
    let config = Arc::new(config);
    let s2s = crate::s2s::run(&config, &info, &masters);
    tokio::pin!(s2s);
    let mut connections = JoinSet::new();
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    loop {
        tokio::select! {
            result = &mut s2s => { result?; break; }
            signal = &mut interrupt => { signal?; break; }
            incoming = async {
                tokio::select! {
                    incoming = front_v4.accept() => incoming,
                    incoming = front_v6.accept() => incoming,
                }
            } => {
                let Some(incoming) = incoming else { break; };
                if connections.len() >= config.max_connections { incoming.refuse(); continue; }
                if !incoming.remote_address_validated() { let _ = incoming.retry(); continue; }
                let back = back.clone();
                let config = config.clone();
                connections.spawn(async move {
                    let client = tokio::time::timeout(Duration::from_secs(config.connect_timeout_seconds), incoming).await??;
                    let backend = match connect_backend(&client, &back, &config, backend_game).await {
                        Ok(backend) => backend,
                        Err(err) => { shutdown(&client, b"proxy backend connection failed"); return Err(err); }
                    };
                    relay(client, backend).await
                });
            }
            result = connections.join_next(), if !connections.is_empty() => {
                match result.unwrap() {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => eprintln!("proxy connection ended: {err}"),
                    Err(err) => eprintln!("proxy connection task failed: {err}"),
                }
            }
        }
    }
    front_v4.close(
        (ConnectionErrorCode::Shutdown as u32).into(),
        b"proxy shutting down",
    );
    front_v6.close(
        (ConnectionErrorCode::Shutdown as u32).into(),
        b"proxy shutting down",
    );
    back.close(
        (ConnectionErrorCode::Shutdown as u32).into(),
        b"proxy shutting down",
    );
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(front_v4.wait_idle(), front_v6.wait_idle(), back.wait_idle());
    })
    .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    use network::network::{
        proxy::TrustedProxies,
        quinn_network::{QuinnEndpointWrapper, QuinnNetworkConnectionWrapper},
        traits::{NetworkConnectionInterface, NetworkEndpointInterface, NetworkIncomingInterface},
        types::NetworkInOrderChannel,
        utils::create_certifified_keys,
    };
    use x509_cert::der::Encode;

    async fn exercise_direction(
        sender: &QuinnNetworkConnectionWrapper,
        receiver: &QuinnNetworkConnectionWrapper,
    ) {
        let pool = pool::mt_pool::Pool::<Vec<u8>>::with_capacity(8);
        let packet = |value| {
            let mut packet = pool.new();
            packet.extend_from_slice(&[value; 64]);
            packet
        };
        sender.send_unreliable_unordered(packet(1)).await.unwrap();
        assert_eq!(receiver.read_unreliable_unordered().await.unwrap(), [1; 64]);
        sender.send_unordered_reliable(packet(2)).await.unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        receiver
            .read_unordered_reliable(move |data| {
                tokio::spawn(async move {
                    let _ = tx.send(data);
                })
            })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap().unwrap(), [2; 64]);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        for value in [3, 4] {
            sender
                .push_ordered_reliable_packet_in_order(packet(value), NetworkInOrderChannel::Global)
                .await;
            sender
                .send_one_ordered_reliable(NetworkInOrderChannel::Global)
                .await
                .unwrap();
        }
        receiver
            .read_ordered_reliable(move |data| {
                let tx = tx.clone();
                tokio::spawn(async move {
                    let _ = tx.send(data);
                })
            })
            .await
            .unwrap();
        assert_eq!(rx.recv().await.unwrap().unwrap(), [3; 64]);
        assert_eq!(rx.recv().await.unwrap().unwrap(), [4; 64]);
    }

    async fn exercise_proxy(trusted: bool, correct_pin: bool, ipv6: bool) {
        tokio::time::timeout(Duration::from_secs(10), async {
            let (server_cert, server_key) = create_certifified_keys();
            let server_hash = server_cert
                .tbs_certificate
                .subject_public_key_info
                .fingerprint_bytes()
                .unwrap();
            // Simulate the backend reloading its persistent key on restart.
            let pem = server_key
                .to_pkcs8_pem(x509_cert::der::pem::LineEnding::LF)
                .unwrap();
            let (server_cert, server_key) =
                network::network::utils::certified_keys_from_pem(&pem).unwrap();
            let config = Config::create(
                "127.0.0.1:0".parse().unwrap(),
                "[::]:0".parse().unwrap(),
                base::hash::fmt_hash(if correct_pin { &server_hash } else { &[0; 32] }),
            )
            .unwrap();
            let (backend_server, _) = QuinnEndpointWrapper::make_server_endpoint(
                "127.0.0.1:0".parse().unwrap(),
                NetworkServerCertMode::FromCertAndPrivateKey(Box::new(NetworkServerCertAndKey {
                    cert: server_cert,
                    private_key: server_key,
                })),
                &NetworkServerInitOptions::new().with_trusted_proxies(TrustedProxies {
                    public_key_hashes: if trusted {
                        [config.hash().unwrap()].into()
                    } else {
                        Default::default()
                    },
                }),
            )
            .unwrap();
            let backend_game = backend_server.sock_addr().unwrap();
            let (frontend_v4, frontend_v6) = make_frontends(&config).unwrap();
            let frontend = if ipv6 { &frontend_v6 } else { &frontend_v4 };
            let outgoing_endpoint = Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
            let (user_cert, user_key) = create_certifified_keys();
            let proxy_hash = config.hash().unwrap();
            let game_client = QuinnEndpointWrapper::make_client_endpoint(
                if ipv6 { "[::1]:0" } else { "127.0.0.1:0" }
                    .parse()
                    .unwrap(),
                &NetworkClientInitOptions::new(
                    NetworkClientCertCheckMode::CheckByPubKeyHash {
                        hash: std::borrow::Cow::Borrowed(&proxy_hash),
                    },
                    NetworkClientCertMode::FromCertAndPrivateKey {
                        cert: user_cert.clone(),
                        private_key: user_key,
                    },
                ),
            )
            .unwrap();
            let mut frontend_addr = frontend.local_addr().unwrap();
            if ipv6 {
                frontend_addr.set_ip(std::net::Ipv6Addr::LOCALHOST.into());
            }
            let (client, front) = tokio::join!(
                game_client.connect(frontend_addr, "localhost").unwrap(),
                async { frontend.accept().await.unwrap().await },
            );
            let client = client.unwrap();
            let front = front.unwrap();
            let (back, backend) = tokio::join!(
                connect_backend(&front, &outgoing_endpoint, &config, backend_game),
                async {
                    backend_server
                        .accept()
                        .await
                        .unwrap()
                        .accept()
                        .unwrap()
                        .await
                }
            );
            if !correct_pin {
                assert!(back.is_err(), "wrong backend key must fail authentication");
                return;
            }
            if !trusted {
                assert!(backend.is_err(), "backend must reject untrusted forwarding");
                return;
            }
            let backend = backend.unwrap();
            assert_eq!(backend.remote_addr(), game_client.sock_addr().unwrap());
            assert_eq!(
                backend.peer_identity().to_der().unwrap(),
                user_cert.to_der().unwrap()
            );
            let task = tokio::spawn(relay(front, back.unwrap()));
            exercise_direction(&client, &backend).await;
            exercise_direction(&backend, &client).await;
            client
                .close(ConnectionErrorCode::Shutdown, "test close reason")
                .await;
            assert!(backend.read_unreliable_unordered().await.is_err());
            assert!(
                backend
                    .close_reason()
                    .unwrap()
                    .to_string()
                    .contains("test close reason")
            );
            let _ = task.await.unwrap();
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn forwards_identity_all_packet_types_and_close() {
        exercise_proxy(true, true, false).await;
    }

    #[tokio::test]
    async fn rejects_untrusted_proxy() {
        exercise_proxy(false, true, false).await;
    }

    #[tokio::test]
    async fn rejects_wrong_backend_key() {
        exercise_proxy(true, false, false).await;
    }
    #[tokio::test]
    async fn forwards_ipv6_identity_and_traffic() {
        match std::net::UdpSocket::bind("[::1]:0") {
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::AddrNotAvailable => {
                eprintln!("skipping IPv6 traffic test: IPv6 loopback is unavailable");
                return;
            }
            Err(err) => panic!("IPv6 test socket failed: {err}"),
        }
        exercise_proxy(true, true, true).await;
    }
}
