use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
};

use anyhow::anyhow;
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    extract::{Path as AxumPath, State},
    http::{HeaderValue, StatusCode},
    response::Response,
    routing::get,
};
use base::hash::{fmt_hash, generate_hash_for};
use base_io::{io::Io, runtime::IoRuntimeTask};
use base_io_traits::fs_traits::FileSystemInterface;
use client_ui::main_menu::{
    communities::{CommunityServers, IconUrlHash},
    ddnet_info::{DdnetInfo, DdnetInfoRequest},
};
use reqwest::{Client, header::CONTENT_TYPE};
use serde::Deserialize;
use tokio::sync::{Mutex, oneshot};
use tracing::error;
use url::Url;

const DEFAULT_UPSTREAM: &str = "https://info.ddnet.org/info";

#[derive(Deserialize)]
struct DdnetInfoQuery {
    name: String,
}

pub struct ProxyState {
    client: Client,
    fs: Arc<dyn FileSystemInterface>,
    upstream: Url,
    base_url: Url,

    icons: Mutex<HashMap<String, Vec<u8>>>,
}

impl ProxyState {
    async fn fetch_and_prepare(&self, player_name: &str) -> anyhow::Result<DdnetInfo> {
        let mut upstream = self.upstream.clone();
        {
            let mut pairs = upstream.query_pairs_mut();
            pairs.clear();
            pairs.append_pair("name", player_name);
        }

        let response = self
            .client
            .get(upstream.clone())
            .send()
            .await?
            .error_for_status()?;

        let mut info: DdnetInfo = response.json().await?;

        let icons = self.rewrite_communities(&mut info).await?;
        *self.icons.lock().await = icons;

        Ok(info)
    }

    async fn rewrite_communities(
        &self,
        info: &mut DdnetInfo,
    ) -> anyhow::Result<HashMap<String, Vec<u8>>> {
        info.community_icons_download_url = Some(self.base_url.clone());

        let mut icons: HashMap<String, Vec<u8>> = Default::default();
        for community in info.communities.values_mut() {
            // servers bug workaround
            if let Some(servers) = community.icon.servers_for_ddnet_bug_workaround() {
                community.servers = CommunityServers::new(servers);
            }
            if community.id == "ddnet" {
                community.servers =
                    CommunityServers::new(info.workaround_servers.take_ddnet_servers());
            } else if community.id == "kog" {
                community.servers =
                    CommunityServers::new(info.workaround_servers.take_kog_servers());
            }

            let icon = &mut community.icon;

            let Some(source_url) = &icon.url else {
                continue;
            };
            let IconUrlHash::Sha256 { sha256 } = &icon.hash else {
                continue;
            };

            let icon_file = self.get_or_download_icon(sha256, source_url).await?;

            let hash = generate_hash_for(&icon_file);
            let hash_str = fmt_hash(&hash);
            let icon_path = Self::icon_download_path(&community.id, &hash_str);
            let icon_url = self.icon_url(&icon_path)?;

            icon.hash = IconUrlHash::Blake3 {
                blake3: hash_str.to_string(),
            };
            icon.url = Some(icon_url);

            icons.insert(icon_path, icon_file);
        }

        Ok(icons)
    }

    async fn get_or_download_icon(
        &self,
        sha256: &str,
        source_url: &Url,
    ) -> anyhow::Result<Vec<u8>> {
        let path = icon_path(sha256);

        let bytes = if self.fs.file_exists(&path).await {
            self.fs.read_file(&path).await?
        } else {
            let response = self
                .client
                .get(source_url.clone())
                .send()
                .await?
                .error_for_status()?;

            let body = response.bytes().await?;

            let dir = icon_dir();
            self.fs.create_dir(&dir).await?;

            let data = body.to_vec();
            self.fs.write_file(&path, data.clone()).await?;
            data
        };

        Ok(bytes)
    }

    fn icon_download_path(community_name: &str, blake3: &str) -> String {
        format!("{community_name}_{blake3}.png")
    }

    fn icon_url(&self, icon_path: &str) -> anyhow::Result<Url> {
        Ok(self.base_url.join("thumbnails/")?.join(icon_path)?)
    }
}

pub struct DdnetInfoProxy {
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<IoRuntimeTask<()>>,
    pub state: Arc<ProxyState>,
}

impl Drop for DdnetInfoProxy {
    fn drop(&mut self) {
        let tx = self.shutdown.take().unwrap();
        let _ = tx.send(());
        if let Err(err) = self.task.take().unwrap().get() {
            log::info!("ddnet-info-proxy task join failed: {err}");
        }
    }
}

pub fn spawn(io: &Io) -> anyhow::Result<DdnetInfoProxy> {
    spawn_with_upstream(io, DEFAULT_UPSTREAM.parse()?)
}

fn spawn_with_upstream(io: &Io, upstream: Url) -> anyhow::Result<DdnetInfoProxy> {
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let listener = std::net::TcpListener::bind(bind_addr)?;
    listener.set_nonblocking(true)?;
    let addr = listener.local_addr()?;

    let base_url = Url::parse(&format!("http://127.0.0.1:{}/", addr.port()))?;

    let client = Client::builder().user_agent("ddnet-info-proxy").build()?;

    let state = Arc::new(ProxyState {
        client,
        fs: io.fs.clone(),
        upstream,
        base_url,
        icons: Default::default(),
    });

    let state_app = state.clone();
    let app = Router::new()
        .route("/thumbnails/{file}", get(icon_handler))
        .with_state(state_app);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let task = io
        .rt
        .spawn(async move {
            let server = axum::serve(tokio::net::TcpListener::from_std(listener)?, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                });
            Ok(server.await?)
        })
        .cancelable();

    Ok(DdnetInfoProxy {
        shutdown: Some(shutdown_tx),
        task: Some(task),
        state,
    })
}

async fn ddnet_info_handler(
    state: &ProxyState,
    query: DdnetInfoQuery,
) -> anyhow::Result<DdnetInfo> {
    let name = query.name;
    if name.is_empty() {
        return Err(anyhow!("Name is missing"));
    }

    match state.fetch_and_prepare(&name).await {
        Ok(info) => Ok(info),
        Err(err) => Err(err),
    }
}

async fn icon_handler(
    State(state): State<Arc<ProxyState>>,
    AxumPath(file): AxumPath<String>,
) -> Result<Response, StatusCode> {
    let file = file
        .strip_suffix(".png")
        .ok_or(StatusCode::NOT_ACCEPTABLE)?;
    let (community_name, hash) = file.rsplit_once("_").ok_or(StatusCode::NOT_ACCEPTABLE)?;

    let icon = state
        .icons
        .lock()
        .await
        .get(&ProxyState::icon_download_path(community_name, hash))
        .cloned();
    let bytes = match icon {
        Some(bytes) => bytes,
        None => {
            error!("failed to read icon {community_name} with hash {hash}");
            return Err(StatusCode::NOT_FOUND);
        }
    };

    let mut response = Response::new(Body::from(bytes));
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("image/png"));
    Ok(response)
}

fn icon_dir() -> PathBuf {
    PathBuf::from("downloaded").join("community_icons")
}

fn icon_path(sha256: &str) -> PathBuf {
    icon_dir().join(format!("{sha256}.png"))
}

#[async_trait]
impl DdnetInfoRequest for ProxyState {
    async fn get(&self, name: &str) -> anyhow::Result<DdnetInfo> {
        ddnet_info_handler(
            self,
            DdnetInfoQuery {
                name: name.to_string(),
            },
        )
        .await
    }
    fn url(&self) -> &Url {
        &self.base_url
    }
}
