use std::sync::{Arc, atomic::AtomicBool};

use base::system::System;
use game_base::local_server_info::LocalServerInfo;
use game_server::server::ddnet_server_main;
use network::network::utils::create_certifified_keys;

fn main() {
    let sys = System::new();
    if std::env::var("RUST_LOG").is_err() {
        unsafe { std::env::set_var("RUST_LOG", "info") };
    }
    env_logger::init();

    let mut args: Vec<_> = std::env::args().collect();
    // TODO: don't rely on first arg being executable
    if !args.is_empty() {
        args.remove(0);
    }

    let cert = create_certifified_keys();

    let server_is_open = Arc::new(AtomicBool::new(true));
    let server_is_open_clone = server_is_open.clone();

    let sys_clone = sys.clone();

    let shared_info = Arc::new(LocalServerInfo::new(false));
    ddnet_server_main::<false>(
        sys_clone,
        cert,
        server_is_open_clone,
        shared_info,
        args,
        None,
    )
    .unwrap();
    server_is_open.store(false, std::sync::atomic::Ordering::Relaxed);
}
