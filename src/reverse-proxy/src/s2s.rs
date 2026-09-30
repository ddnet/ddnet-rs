use std::{sync::Arc, time::Duration};

use anyhow::{Context, ensure};
use ed25519_dalek::pkcs8::EncodePrivateKey;
use game_base::s2s::ServerInfo;
use master_server_types::response::RegisterResponse;
use network::network::quinnminimal::CertHashServerVerification;
use rand::RngExt;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use x509_cert::der::Encode;

use crate::config::{Config, parse_hash};

fn client(config: &Config) -> anyhow::Result<reqwest::Client> {
    let (cert, key) = config.identity()?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(CertHashServerVerification::new(
            provider,
            parse_hash(&config.backend_public_key_hash)?,
        ))
        .with_client_auth_cert(
            vec![CertificateDer::from(cert.to_der()?)],
            PrivatePkcs8KeyDer::from(key.to_pkcs8_der()?.as_bytes().to_vec()).into(),
        )?;
    Ok(reqwest::Client::builder()
        .tls_backend_preconfigured(tls)
        .http1_only()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(config.connect_timeout_seconds))
        .build()?)
}

async fn fetch(client: &reqwest::Client, url: &str) -> anyhow::Result<ServerInfo> {
    let mut response = client.get(url).send().await?.error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() + chunk.len() <= 32 * 1024,
            "S2S server info too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).context("invalid S2S server info")
}

pub async fn discover(config: &Config) -> anyhow::Result<ServerInfo> {
    let info = fetch(
        &client(config)?,
        &format!("https://{}/server-info", config.backend_s2s),
    )
    .await?;
    ensure!(
        info.game_port_v4 > 0 && info.game_port_v6 > 0,
        "backend did not report active game ports"
    );
    Ok(info)
}

async fn register(
    client: &reqwest::Client,
    master: &str,
    info: &str,
    address: (u16, &str),
    secret: &str,
    challenge: &str,
    serial: u64,
) -> anyhow::Result<()> {
    let (port, protocol) = address;
    let mut response = client
        .post(master)
        .header(
            "Address",
            format!("{protocol}://connecting-address.invalid:{port}"),
        )
        .header("Secret", secret)
        .header("Challenge-Secret", challenge)
        .header("Info-Serial", serial.to_string())
        .header("content-type", "application/json")
        .body(info.to_owned())
        .send()
        .await?
        .error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() + chunk.len() <= 16 * 1024,
            "master response too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    match serde_json::from_slice::<RegisterResponse>(&bytes)? {
        RegisterResponse::Success => Ok(()),
        RegisterResponse::NeedChallenge => anyhow::bail!("master challenges are not supported"),
        RegisterResponse::NeedInfo => anyhow::bail!("master requested info despite supplied body"),
        RegisterResponse::Error(err) => anyhow::bail!("{}", err.message),
    }
}

pub async fn run(
    config: &Config,
    initial: &ServerInfo,
    masters: &[url::Url],
) -> anyhow::Result<()> {
    run_with_info(config, initial, masters, "ddrs-0.1+quic", |info| {
        Ok(serde_json::to_string(&info.browser_info)?)
    })
    .await
}

/// Share authenticated discovery and registration with translating frontends.
pub async fn run_with_info(
    config: &Config,
    initial: &ServerInfo,
    masters: &[url::Url],
    protocol: &str,
    prepare: impl Fn(&ServerInfo) -> anyhow::Result<String>,
) -> anyhow::Result<()> {
    let s2s = client(config)?;
    let url = format!("https://{}/server-info", config.backend_s2s);
    let hash = config.hash()?;
    let public_client = |ip| {
        reqwest::Client::builder()
            .local_address(ip)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
    };
    let v4 = public_client(std::net::IpAddr::V4(*config.listen_game_v4.ip()))?;
    let v6 = public_client(std::net::IpAddr::V6(*config.listen_game_v6.ip()))?;
    let secret = base::hash::fmt_hash(&rand::rng().random::<[u8; 32]>());
    let challenge = base::hash::fmt_hash(&rand::rng().random::<[u8; 32]>());
    let mut serial = 0;
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match fetch(&s2s, &url).await {
            Ok(mut info) => {
                ensure!(
                    (info.game_port_v4, info.game_port_v6)
                        == (initial.game_port_v4, initial.game_port_v6),
                    "backend game ports changed; restart the proxy to rediscover them"
                );
                info.browser_info.cert_sha256_fingerprint = hash;
                let info = prepare(&info)?;
                serial += 1;
                let register_family = |client, port, family| {
                    let info = &info;
                    let secret = &secret;
                    let challenge = &challenge;
                    async move {
                        for master in masters {
                            let master = master.join("register")?;
                            match register(
                                client,
                                master.as_str(),
                                info,
                                (port, protocol),
                                secret,
                                challenge,
                                serial,
                            )
                            .await
                            {
                                Ok(()) => break,
                                Err(err) => eprintln!("proxy browser registration {family}: {err}"),
                            }
                        }
                        Ok::<_, anyhow::Error>(())
                    }
                };
                tokio::try_join!(
                    register_family(&v4, config.listen_game_v4.port(), "IPv4"),
                    register_family(&v6, config.listen_game_v6.port(), "IPv6"),
                )?;
            }
            Err(err) => eprintln!("proxy S2S unavailable, skipping registration: {err}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_base::server_browser::ServerBrowserInfo;
    use game_server::s2s::Server;
    use network::network::{proxy::TrustedProxies, utils::create_certifified_keys};

    #[tokio::test]
    async fn authenticated_info_and_registration() {
        let (cert, key) = create_certifified_keys();
        let hash = cert
            .tbs_certificate
            .subject_public_key_info
            .fingerprint_bytes()
            .unwrap();
        let mut config = Config::create(
            "127.0.0.1:8310".parse().unwrap(),
            "[::1]:8311".parse().unwrap(),
            base::hash::fmt_hash(&hash),
        )
        .unwrap();
        let proxy_hash = config.hash().unwrap();
        let info = Arc::new(arc_swap::ArcSwap::from_pointee(ServerBrowserInfo::default()));
        let routes = game_server::s2s::router(
            info.clone(),
            game_server::s2s::Ports {
                game_v4: 8312,
                game_v6: 8317,
            },
        );
        let server = tokio::task::spawn_blocking(move || {
            Server::new(
                &["127.0.0.1:0".parse().unwrap()],
                &cert,
                &key,
                TrustedProxies {
                    public_key_hashes: [proxy_hash].into(),
                },
                routes,
            )
            .unwrap()
            .unwrap()
        })
        .await
        .unwrap();
        config.backend_s2s = server.local_addresses()[0];
        let url = format!("https://{}/server-info", config.backend_s2s);
        let http = client(&config).unwrap();
        let browser: ServerBrowserInfo = serde_json::from_value(serde_json::json!({
            "name": "test backend", "max_players": 32,
            "resource_server_url": "http://assets.example:3400/",
            "cert_sha256_fingerprint": hash,
        }))
        .unwrap();
        info.store(Arc::new(browser.clone()));
        let discovered = discover(&config).await.unwrap();
        assert_eq!(discovered.game_port_v4, 8312);
        assert_eq!(discovered.game_port_v6, 8317);
        assert_eq!(
            discovered.browser_info.resource_server_url,
            browser.resource_server_url
        );

        let mut fetched = fetch(&http, &url).await.unwrap();
        fetched.browser_info.cert_sha256_fingerprint = proxy_hash;
        let json = serde_json::to_string(&fetched.browser_info).unwrap();
        let result: ServerBrowserInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(result.cert_sha256_fingerprint, proxy_hash);
        assert_eq!(result.resource_server_url, browser.resource_server_url);
        assert_eq!(result.name, browser.name);
        assert_eq!(result.max_players, 32);

        let expected = json.clone();
        let routes = axum::Router::new().route(
            "/register",
            axum::routing::post(move |headers: axum::http::HeaderMap, body: String| {
                let expected = expected.clone();
                async move {
                    assert_eq!(
                        headers["Address"],
                        "ddrs-0.1+quic://connecting-address.invalid:8310"
                    );
                    assert_eq!(headers["Secret"], "test-secret");
                    assert_eq!(headers["Info-Serial"], "7");
                    assert_eq!(body, expected);
                    axum::Json(RegisterResponse::Success)
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let master_url = format!("http://{}/register", listener.local_addr().unwrap());
        let master = tokio::spawn(async move { axum::serve(listener, routes).await.unwrap() });
        register(
            &reqwest::Client::new(),
            &master_url,
            &json,
            (8310, "ddrs-0.1+quic"),
            "test-secret",
            "challenge",
            7,
        )
        .await
        .unwrap();

        config.backend_public_key_hash = "00".repeat(32);
        assert!(fetch(&client(&config).unwrap(), &url).await.is_err());
        config.backend_public_key_hash = base::hash::fmt_hash(&hash);
        let (other_cert, other_key) = create_certifified_keys();
        config.certificate = hex::encode(other_cert.to_der().unwrap());
        config.private_key = hex::encode(other_key.to_bytes());
        assert!(fetch(&client(&config).unwrap(), &url).await.is_err());
        tokio::task::spawn_blocking(move || drop(server))
            .await
            .unwrap();
        assert!(fetch(&http, &url).await.is_err());
        master.abort();
    }
}
