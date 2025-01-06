use std::sync::{atomic::AtomicBool, Arc};

use base::{join_thread::JoinThread, system::System};
use config::config::ConfigEngine;
use game_base::local_server_info::{LocalServerInfo, LocalServerState, LocalServerThread};
use game_config::config::ConfigGame;
use network::network::utils::create_certifified_keys;

use crate::server::ddnet_server_main;

pub fn start_local_server(
    sys: &System,
    shared_info: Arc<LocalServerInfo>,
    config_engine: ConfigEngine,
    config_game: ConfigGame,
) {
    let (cert, private_key) = create_certifified_keys();
    let server_cert_hash = cert
        .tbs_certificate
        .subject_public_key_info
        .fingerprint_bytes()
        .unwrap();

    let server_is_open = Arc::new(AtomicBool::new(true));
    let server_is_open_clone = server_is_open.clone();

    let sys_clone = sys.clone();

    let mut state = shared_info.state.lock().unwrap();

    let shared_info_thread = shared_info.clone();
    let t = std::thread::Builder::new()
        .name("server".into())
        .spawn(move || {
            ddnet_server_main::<true>(
                sys_clone,
                (cert, private_key),
                server_is_open_clone,
                shared_info_thread,
                Default::default(),
                Some((config_engine, config_game)),
            )
        })
        .unwrap();

    *state = LocalServerState::Starting {
        server_cert_hash,
        thread: LocalServerThread {
            server_is_open,
            thread: JoinThread::new(t),
        },
    };
}
