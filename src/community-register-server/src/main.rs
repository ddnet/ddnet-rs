use std::{
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use axum::{Json, Router, extract::ConnectInfo};
use clap::Parser;
use community::{Info, Register};
use tower_http::trace::TraceLayer;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The port this server should listen on
    #[arg(short, long)]
    port: Option<u16>,
    /// Password for community servers to register
    #[arg(short, long)]
    register_password: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let port = args.port.unwrap_or(3003);
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;

    let info = Arc::new(RwLock::new(Info {
        servers: Default::default(),
    }));
    let info_query = info.clone();

    let app = Router::new()
        .route(
            "/info",
            axum::routing::get(|| async move { Json(info_query.read().unwrap().clone()) }),
        )
        .route(
            "/register",
            axum::routing::post(
                move |ConnectInfo(mut client_ip): ConnectInfo<SocketAddr>,
                      Json(q): Json<Register>| async move {
                    if q.password == args.register_password {
                        client_ip.set_port(q.port);
                        info.write().unwrap().servers.insert(client_ip, q.info);
                    }
                },
            ),
        );
    axum::serve(
        listener,
        app.layer(TraceLayer::new_for_http())
            .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
