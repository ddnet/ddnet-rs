use std::{
    net::SocketAddr,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use base::{join_thread::JoinThread, linked_hash_map_view::FxLinkedHashMap};
use game_interface::types::game::GameTickType;

use crate::server_browser::ServerBrowserInfo;

#[derive(Debug)]
pub struct ServerDbgGame {
    pub time: Instant,
    pub tick_time: Duration,
    pub players: String,
    pub projectiles: String,
    pub inputs: String,
    pub caller: String,
}

#[derive(Debug)]
pub struct LocalServerThread {
    pub server_is_open: Arc<AtomicBool>,
    // must be the last entry
    pub thread: JoinThread<anyhow::Result<()>>,
}

impl Drop for LocalServerThread {
    fn drop(&mut self) {
        self.server_is_open
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

#[derive(Debug)]
pub struct LocalServerConnectInfo {
    pub sock_addr: SocketAddr,
    pub dbg_games: FxLinkedHashMap<GameTickType, ServerDbgGame>,
    pub rcon_secret: [u8; 32],
    pub server_cert_hash: [u8; 32],
}

#[derive(Debug)]
pub struct LocalServerStateReady {
    pub connect_info: LocalServerConnectInfo,
    pub browser_info: Option<ServerBrowserInfo>,
    // must be last
    pub thread: LocalServerThread,
}

#[derive(Debug, Default)]
pub enum LocalServerState {
    #[default]
    None,
    Starting {
        server_cert_hash: [u8; 32],
        // must be last
        thread: LocalServerThread,
    },
    Ready(Box<LocalServerStateReady>),
}

#[derive(Debug, Default)]
pub struct LocalServerInfo {
    pub state: std::sync::Mutex<LocalServerState>,
    /// client internal server,
    /// this server should only be reachable in LAN configurations
    pub is_internal_server: bool,
}

impl LocalServerInfo {
    pub fn new(is_internal_server: bool) -> Self {
        Self {
            is_internal_server,
            ..Default::default()
        }
    }
}
