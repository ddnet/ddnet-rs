pub mod queries;
pub mod server;

use std::{
    net::IpAddr,
    sync::{Arc, RwLock},
    time::Duration,
};

use base::steady_clock::SteadyClock;
use clap::Parser;
use community::{Register, ServerInfo};
use network::network::{
    errors::KickType,
    packet_compressor::DefaultNetworkPacketCompressor,
    plugins::NetworkPlugins,
    quinn_network::QuinnNetworkAsync,
    types::{
        NetworkServerCertAndKey, NetworkServerCertMode, NetworkServerCertModeResult,
        NetworkServerInitOptions,
    },
    utils::create_certifified_keys,
};
use server::CommunityServer;
use sql::database::DatabaseDetails;
use tokio::sync::mpsc::channel;
use url::Url;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Password for community servers to register
    /// to the main server.
    #[arg(short, long)]
    register_password: String,
    /// Address of the community main server.
    #[arg(short, long)]
    main_server_addresses: Vec<Url>,
    /// Max clients allowed to connect to this
    /// instance.
    #[arg(short, long)]
    max_clients: u64,
    /// Should this server be ipv6, otherwise it
    /// is ipv4
    #[arg(short, long)]
    ipv6: bool,

    #[arg(short, long)]
    pub host: String,
    #[arg(short, long)]
    pub database_port: u16,
    #[arg(short, long)]
    pub database: String,
    #[arg(short, long)]
    pub username: String,
    #[arg(short, long)]
    pub password: String,
    #[arg(short, long)]
    pub ca_cert_path: String,
    #[arg(short, long)]
    pub connection_count: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let addr = "0.0.0.0:0";

    let (cert, private_key) = create_certifified_keys();

    let (sender, mut receiver) = channel(4096);

    let (network_server, cert, addr, _) = QuinnNetworkAsync::init_server(
        addr,
        Arc::new(
            CommunityServer::new(
                sender,
                DatabaseDetails {
                    host: args.host,
                    port: args.database_port,
                    database: args.database,
                    username: args.username,
                    password: args.password,
                    ca_cert_path: args.ca_cert_path,
                    connection_count: args.connection_count,
                },
            )
            .await?,
        ),
        NetworkServerCertMode::FromCertAndPrivateKey(Box::new(NetworkServerCertAndKey {
            cert,
            private_key,
        })),
        &SteadyClock::start(),
        NetworkServerInitOptions::new()
            .with_keep_alive(Duration::from_secs(60))
            .with_timeout(Duration::from_secs(120)),
        NetworkPlugins {
            connection_plugins: Default::default(),
            packet_plugins: Arc::new(vec![Arc::new(DefaultNetworkPacketCompressor::new())]),
        },
    )?;

    let register_info = Arc::new(RwLock::new(Register {
        password: args.register_password,
        info: ServerInfo {
            cert_hash: match cert {
                NetworkServerCertModeResult::PubKeyHash { hash } => hash,
                NetworkServerCertModeResult::Cert { cert } => cert
                    .tbs_certificate
                    .subject_public_key_info
                    .fingerprint_bytes()?,
            },
            cur_load: 0,
            max_load: args.max_clients,
        },
        port: addr.port(),
    }));

    let addr: IpAddr = if args.ipv6 { "[::0]:0" } else { "0.0.0.0:0" }.parse()?;
    let http = reqwest::ClientBuilder::new().local_address(addr).build()?;

    tokio::spawn(async move {
        while let Some(ev) = receiver.recv().await {
            match ev {
                server::Event::Kick(id) => {
                    network_server
                        .kick(
                            id,
                            KickType::Kick(
                                "community server does not allow this connection, \
                                usually because the account is not valid"
                                    .to_string(),
                            ),
                        )
                        .await;
                }
            }
        }
    });

    // register server every minute
    loop {
        let info: Register = register_info.read().unwrap().clone();
        for main_server_address in &args.main_server_addresses {
            http.post(main_server_address.clone())
                .body(serde_json::to_string(&info)?)
                .send()
                .await?;
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
