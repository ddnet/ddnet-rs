use super::*;
use network::network::{
    proxy::TrustedProxies,
    types::{NetworkServerCertAndKey, NetworkServerCertMode, NetworkServerInitOptions},
};

fn socket(rt: &base_io::runtime::IoRuntime) -> Socket {
    Socket::bind(
        rt,
        "127.0.0.1:0".parse().unwrap(),
        "[::]:0".parse().unwrap(),
    )
    .unwrap()
}

fn encoded(msg: impl Into<System<'static>>) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(2048);
    with_packer(&mut bytes, |p| msg.into().encode(p)).unwrap();
    bytes
}

#[test]
fn legacy_handshake_and_input_reach_authenticated_backend() {
    let rt = base_io::io::create_runtime();
    let (cert, key) = network::network::utils::create_certifified_keys();
    let hash = cert
        .tbs_certificate
        .subject_public_key_info
        .fingerprint_bytes()
        .unwrap();
    let config = Config::create(
        "127.0.0.1:0".parse().unwrap(),
        "[::]:0".parse().unwrap(),
        base::hash::fmt_hash(&hash),
    )
    .unwrap();
    let backend_events = Arc::new(GameEventGenerator::<ClientMsg<'static>>::new(Arc::new(
        AtomicBool::new(false),
    )));
    let (_backend, _, backend_addr, _) = QuinnNetwork::init_server(
        "127.0.0.1:0",
        backend_events.clone(),
        NetworkServerCertMode::FromCertAndPrivateKey(Box::new(NetworkServerCertAndKey {
            cert,
            private_key: key,
        })),
        &SteadyClock::start(),
        NetworkServerInitOptions::new().with_trusted_proxies(TrustedProxies {
            public_key_hashes: [config.hash().unwrap()].into(),
        }),
        NetworkPlugins {
            packet_plugins: Arc::new(vec![Arc::new(DefaultNetworkPacketCompressor::new())]),
            ..Default::default()
        },
    )
    .unwrap();
    let socket_rt = base_io::runtime::IoRuntime::new(base_io::io::create_runtime());
    let mut front = socket(&socket_rt);
    let mut old_socket = socket(&socket_rt);
    let original = old_socket.local_addr(true).unwrap();
    let mut net = Net::server();
    let mut old_net = Net::client();
    let (old_pid, res) = old_net.connect(&mut old_socket, front.local_addr(true).unwrap());
    res.unwrap();
    let mut pid = None;
    let mut online = false;
    let start = Instant::now();
    while !online {
        assert!(start.elapsed() < Duration::from_secs(3));
        if let Ok((addr, buf)) = front.try_recv() {
            let mut scratch = Vec::with_capacity(4096);
            let (events, result) = net.feed(&mut front, &mut Ignore, addr, &buf, &mut scratch);
            result.unwrap();
            for event in events {
                match event {
                    ChunkOrEvent::Connect(id) => {
                        net.accept(&mut front, id).unwrap();
                        pid = Some(id);
                    }
                    ChunkOrEvent::Chunk(_) => online = true,
                    _ => {}
                }
            }
        }
        if let Ok((addr, buf)) = old_socket.try_recv() {
            let mut scratch = Vec::with_capacity(4096);
            let (events, result) =
                old_net.feed(&mut old_socket, &mut Ignore, addr, &buf, &mut scratch);
            result.unwrap();
            for event in events {
                if let ChunkOrEvent::Ready(_) = event {
                    sends(
                        &mut old_net,
                        &mut old_socket,
                        old_pid,
                        system::Info {
                            version: b"0.6 626fce9a778df4d4",
                            password: Some(b""),
                        },
                    )
                    .unwrap();
                    old_net.flush(&mut old_socket, old_pid).unwrap();
                }
            }
        }
        for _ in net.tick(&mut front) {}
        for _ in old_net.tick(&mut old_socket) {}
        std::thread::sleep(Duration::from_millis(1));
    }
    let pid = pid.unwrap();
    let mut client = connect_client(&config, original, backend_addr, &None).unwrap();
    // Both backend notifications and completed map tasks wake an otherwise idle loop.
    client.notifier.notify_one();
    rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(1), client.wait_for_work())
            .await
            .unwrap();
    });
    client.map_task = Some(rt.spawn(async { anyhow::bail!("test map completion") }));
    rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(1), async {
            while client.map_result.is_none() {
                client.wait_for_work().await;
            }
        })
        .await
        .unwrap();
    });
    assert!(client.map_result.take().unwrap().is_err());
    assert!(client.map_task.is_none());
    client.map = Some(Arc::new(maps::LegacyMap {
        name: "test".into(),
        bytes: vec![0; 900],
        crc: 1,
        tuning: None,
    }));
    // StartInfo alone must not enter the backend before the legacy Ready.
    let mut bytes = Vec::with_capacity(2048);
    with_packer(&mut bytes, |p| {
        Game::from(game::ClStartInfo {
            name: b"legacy tee",
            clan: b"",
            country: -1,
            skin: b"default",
            use_custom_color: false,
            color_body: 0,
            color_feet: 0,
        })
        .encode(p)
    })
    .unwrap();
    chunk(&mut client, &mut net, &mut front, pid, &bytes, true).unwrap();
    assert!(!client.ready_sent);
    chunk(
        &mut client,
        &mut net,
        &mut front,
        pid,
        &encoded(system::Ready),
        true,
    )
    .unwrap();
    assert!(client.ready_sent);
    let start = Instant::now();
    let mut received_ready = false;
    let mut identity_checked = false;
    while !received_ready {
        assert!(start.elapsed() < Duration::from_secs(5));
        while let Some((_, _, event)) = backend_events.events.blocking_lock().pop_front() {
            match event {
                GameEvents::NetworkEvent(NetworkEvent::Connected { addr, .. }) => {
                    assert_eq!(addr, original);
                    identity_checked = true;
                }
                GameEvents::NetworkMsg(ClientMsg::Ready(ready)) => {
                    assert_eq!(ready.players[0].player_info.name.as_str(), "legacy tee");
                    received_ready = true;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(identity_checked);
    let (snapshot, local) = snapshot::tests::fixture();
    client.player = Some(local);
    client.entered = true;
    client.tick_origin = Some(0);
    let bytes = bincode::serde::encode_to_vec(&snapshot, bincode::config::standard()).unwrap();
    client.snapshots.decode(&bytes, None, 1, 100, true).unwrap();
    let input = libtw2_gamenet_ddnet::snap_obj::PlayerInput {
        direction: 1,
        target_x: 320,
        target_y: 64,
        fire: 1,
        ..Default::default()
    };
    chunk(
        &mut client,
        &mut net,
        &mut front,
        pid,
        &encoded(system::Input {
            ack_snapshot: -1,
            intended_tick: 102,
            input_size: 40,
            input,
        }),
        false,
    )
    .unwrap();
    let start = Instant::now();
    loop {
        assert!(start.elapsed() < Duration::from_secs(5));
        let event = backend_events.events.blocking_lock().pop_front();
        if let Some((_, _, GameEvents::NetworkMsg(ClientMsg::Inputs { inputs, .. }))) = event {
            let chain = &inputs[&local];
            let cfg = bincode::config::standard().with_fixed_int_encoding();
            let baseline =
                bincode::serde::encode_to_vec(PlayerInputChainable::default(), cfg).unwrap();
            let mut bytes = Vec::new();
            bin_patch::patch_exact_size(&baseline, &chain.data, &mut bytes).unwrap();
            let (translated, _) =
                bincode::serde::decode_from_slice::<PlayerInputChainable, _>(&bytes, cfg).unwrap();
            assert_eq!(translated.for_monotonic_tick, 102);
            assert_eq!(*translated.inp.inp.state.dir, 1);
            assert_eq!(
                translated.inp.inp.cursor.to_vec2(),
                math::math::vector::dvec2::new(10.0, 2.0)
            );
            assert_eq!(
                translated
                    .inp
                    .inp
                    .consumable
                    .diff(&Default::default())
                    .fire
                    .unwrap()
                    .0
                    .get(),
                1
            );
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let fs: Arc<dyn FileSystemInterface> = Arc::new(
        base_fs::filesys::FileSystem::new(&rt, "org", "", "DDNet-Rs-Alpha", "DDNet-Accounts")
            .unwrap(),
    );
    let snapshot_bytes =
        bincode::serde::encode_to_vec(&snapshot, bincode::config::standard()).unwrap();
    let msg = ServerMsg::Snapshot {
        overhead_time: Duration::from_millis(5),
        snapshot: pool::mt_datatypes::PoolCow::from_without_pool(std::borrow::Cow::Owned(
            snapshot_bytes,
        )),
        diff_id: None,
        snap_id_diffed: 2,
        game_monotonic_tick_diff: 101,
        as_diff: true,
        input_ack: pool::mt_datatypes::PoolCow::from_without_pool(std::borrow::Cow::Owned(vec![
            game_network::messages::MsgSvInputAck {
                id: client.input_id,
                logic_overhead: Duration::from_millis(15),
            },
        ])),
    };
    server_message(
        &mut client,
        &mut net,
        &mut front,
        pid,
        msg,
        &fs,
        &config,
        rt.handle(),
        &Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(1)
                .build()
                .unwrap(),
        ),
        &Default::default(),
    )
    .unwrap();
    net.flush(&mut front, pid).unwrap();
    let mut manager = libtw2_snapshot::Manager::new();
    let start = Instant::now();
    let mut got_snapshot = false;
    let mut got_timing = false;
    while !got_snapshot || !got_timing {
        assert!(start.elapsed() < Duration::from_secs(3));
        if let Ok((addr, buf)) = old_socket.try_recv() {
            let mut scratch = Vec::with_capacity(4096);
            let (events, result) =
                old_net.feed(&mut old_socket, &mut Ignore, addr, &buf, &mut scratch);
            result.unwrap();
            for event in events {
                if let ChunkOrEvent::Chunk(data) = event {
                    let decoded = match System::decode(&mut Ignore, &mut Unpacker::new(data.data)) {
                        Ok(System::InputTiming(timing)) => {
                            assert_eq!(timing.input_pred_tick, 102);
                            assert_eq!(timing.time_left, 30);
                            got_timing = true;
                            None
                        }
                        Ok(System::SnapSingle(s)) => manager
                            .snap_single(&mut Ignore, libtw2_gamenet_ddnet::snap_obj::obj_size, s)
                            .unwrap(),
                        Ok(System::Snap(s)) => manager
                            .snap(&mut Ignore, libtw2_gamenet_ddnet::snap_obj::obj_size, s)
                            .unwrap(),
                        _ => None,
                    };
                    if let Some(snapshot) = decoded {
                        let character = snapshot
                            .item(
                                libtw2_gamenet_ddnet::snap_obj::TypeId::Ordinal(
                                    libtw2_gamenet_ddnet::snap_obj::CHARACTER,
                                ),
                                0,
                            )
                            .unwrap();
                        assert_eq!(&character[..3], &[101, 320, 640]);
                        got_snapshot = true;
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(client);
    drop(_backend);
    drop(rt);
}

#[test]
fn converts_repository_map_with_verified_resources() {
    let rt = base_io::io::create_runtime();
    let fs = Arc::new(
        base_fs::filesys::FileSystem::new(&rt, "org", "", "DDNet-Rs-Alpha", "DDNet-Accounts")
            .unwrap(),
    );
    let bytes = rt
        .block_on(fs.read_file("map/maps/dm1.twmap.tar".as_ref()))
        .unwrap();
    let info = MsgSvServerInfo {
        map: "dm1".try_into().unwrap(),
        map_blake3_hash: base::hash::generate_hash_for(&bytes),
        game_mod: GameModification::Native,
        render_mod: RenderModification::Native,
        mod_config: None,
        server_options: Default::default(),
        required_resources: Default::default(),
        resource_server_fallback: None,
        hint_start_camera_pos: Default::default(),
        spatial_chat: false,
        send_input_every_tick: false,
    };
    let converted = rt
        .block_on(maps::load(
            fs,
            "127.0.0.1".parse().unwrap(),
            info,
            Arc::new(
                rayon::ThreadPoolBuilder::new()
                    .num_threads(2)
                    .build()
                    .unwrap(),
            ),
            None,
        ))
        .unwrap();
    assert_eq!(&converted.bytes[..4], b"DATA");
    assert_eq!(converted.crc, crc32fast::hash(&converted.bytes) as i32);
}

#[test]
fn async_socket_wakes_for_both_families_and_sends_queued_packets() {
    let rt = base_io::io::create_runtime();
    let socket_rt = base_io::runtime::IoRuntime::new(base_io::io::create_runtime());
    let mut socket = socket(&socket_rt);
    for ipv4 in [true, false] {
        let peer = match rt.block_on(async {
            tokio::net::UdpSocket::bind(if ipv4 { "127.0.0.1:0" } else { "[::1]:0" }).await
        }) {
            Ok(peer) => peer,
            Err(e) if !ipv4 && e.kind() == std::io::ErrorKind::AddrNotAvailable => {
                eprintln!("IPv6 loopback unavailable; skipping IPv6 socket check");
                continue;
            }
            Err(e) => panic!("bind test peer: {e}"),
        };
        rt.block_on(async {
            let (v4, v6) = socket.receivers();
            assert!(
                tokio::time::timeout(Duration::from_millis(10), Socket::recv_from(v4, v6))
                    .await
                    .is_err()
            );
            let mut address = socket.local_addr(ipv4).unwrap();
            if !ipv4 {
                address.set_ip(std::net::Ipv6Addr::LOCALHOST.into());
            }
            peer.send_to(b"request", address).await.unwrap();
            let (v4, v6) = socket.receivers();
            let (data, addr) =
                tokio::time::timeout(Duration::from_secs(1), Socket::recv_from(v4, v6))
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(data, b"request");
            assert_eq!(addr, peer.local_addr().unwrap());
        });
        socket.send(peer.local_addr().unwrap(), b"reply").unwrap();
        rt.block_on(async {
            let mut data = [0; 32];
            let (len, _) = tokio::time::timeout(Duration::from_secs(1), peer.recv_from(&mut data))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(&data[..len], b"reply");
        });
    }
}
