pub mod connection;
pub mod connection_ban;
pub mod connection_limit;
pub mod connection_per_ip;
pub mod connections;
pub mod errors;
pub mod event;
pub mod event_generator;
pub mod network;
pub mod network_async;
pub mod networks;
pub mod notifier;
pub mod packet_compressor;
pub mod packet_dict;
pub mod plugins;
pub mod quinn_network;
pub mod quinnminimal;
pub mod traits;
pub mod tungstenite_network;
pub mod types;
pub mod utils;

#[cfg(test)]
pub mod tests {
    use std::{
        borrow::Cow,
        num::NonZeroUsize,
        sync::{
            atomic::{AtomicBool, AtomicUsize},
            Arc,
        },
        thread::JoinHandle,
        time::Duration,
    };

    use async_trait::async_trait;
    use base::system::{System, SystemTimeInterface};
    use serde::{Deserialize, Serialize};
    use spki::der::Encode;
    use tokio::sync::Mutex;

    use crate::network::{
        network::Network,
        packet_compressor::DefaultNetworkPacketCompressor,
        plugins::{NetworkPluginPacket, NetworkPlugins},
        quinn_network::{
            QuinnEndpointWrapper, QuinnNetworkConnectingWrapper, QuinnNetworkConnectionWrapper,
        },
        types::{
            NetworkClientCertCheckMode, NetworkClientCertMode, NetworkClientInitOptions,
            NetworkInOrderChannel, NetworkServerCertAndKey, NetworkServerCertMode,
            NetworkServerInitOptions,
        },
        utils::create_certifified_keys,
    };

    use super::{
        connection::NetworkConnectionId,
        event::NetworkEvent,
        event_generator::NetworkEventToGameEventGenerator,
        quinn_network::{QuinnNetwork, QuinnNetworkIncomingWrapper},
        traits::{
            NetworkConnectingInterface, NetworkConnectionInterface, NetworkEndpointInterface,
            NetworkIncomingInterface,
        },
        tungstenite_network::{
            TungsteniteEndpointWrapper, TungsteniteNetworkConnectingWrapper,
            TungsteniteNetworkConnectionWrapper, TungsteniteNetworkIncomingWrapper,
        },
    };

    #[derive(Serialize, Deserialize)]
    enum TestGameMessage {
        UnreliableUnordered(u32),
        ReliableUnordered(u32),
        ReliableOrdered(u32),
        ReliableOrderedChannel1(u32),
        ReliableOrderedChannel2(u32),
        ReliableOrderedChannel1Con { order: u32, id: usize },
        ReliableOrderedChannel2Con { order: u32, id: usize },
        AnyPacket(Vec<u8>),
        Bench(Vec<u8>),
        BenchMulti { msg: Vec<u8>, id: usize },
    }

    #[derive(Debug, Default)]
    pub struct TestGameEventGenerator {
        is_shutdown: AtomicBool,
        should_log_ping: AtomicBool,
        is_connected: AtomicBool,
        unordered_unreliable_sum: AtomicUsize,
        unordered_reliable_sum: AtomicUsize,
        ordered_reliable_check: AtomicUsize,
        ordered_reliable_c1_check: AtomicUsize,
        ordered_reliable_c2_check: AtomicUsize,
        ordered_reliable_c1_check_con: [AtomicUsize; 3],
        ordered_reliable_c2_check_con: [AtomicUsize; 3],
        any_packet_gotten: AtomicUsize,
        cur_test_name: Mutex<String>,
        bench_start: Arc<AtomicUsize>,
        bench: AtomicUsize,
        bench_total: Arc<AtomicUsize>,
        bench_total_multi: [Arc<AtomicUsize>; 32],
    }

    impl TestGameEventGenerator {
        pub fn new() -> Self {
            Self {
                ..Default::default()
            }
        }
    }

    const BENCH_TIME_MS: u128 = 8000;

    #[async_trait]
    impl NetworkEventToGameEventGenerator for TestGameEventGenerator {
        async fn generate_from_binary(
            &self,
            timestamp: Duration,
            con_id: &NetworkConnectionId,
            bytes: &[u8],
        ) {
            let msg = bincode::serde::decode_from_slice::<TestGameMessage, _>(
                bytes,
                bincode::config::standard(),
            );
            let do_print = if let Ok((msg, _)) = &msg {
                if let TestGameMessage::Bench(_) = msg {
                    false
                } else {
                    !matches!(msg, TestGameMessage::BenchMulti { .. })
                }
            } else {
                true
            };
            if do_print {
                if bytes.len() < 1024 {
                    println!(
                        "for {} -- {:?} at {:?}: {:?}",
                        self.cur_test_name.lock().await,
                        con_id,
                        timestamp,
                        bytes
                    );
                } else {
                    println!(
                        "for {} -- {:?} at {:?}: len: {:?}",
                        self.cur_test_name.lock().await,
                        con_id,
                        timestamp,
                        bytes.len()
                    );
                }
            }
            if let Ok((msg, _)) = msg {
                match msg {
                    TestGameMessage::UnreliableUnordered(num) => {
                        self.unordered_unreliable_sum
                            .fetch_add(num as usize, std::sync::atomic::Ordering::SeqCst);
                    }
                    TestGameMessage::ReliableUnordered(num) => {
                        self.unordered_reliable_sum
                            .fetch_add(num as usize, std::sync::atomic::Ordering::SeqCst);
                    }
                    TestGameMessage::ReliableOrdered(num) => {
                        self.ordered_reliable_check
                            .compare_exchange(
                                (num - 1) as usize,
                                num as usize,
                                std::sync::atomic::Ordering::SeqCst,
                                std::sync::atomic::Ordering::SeqCst,
                            )
                            .unwrap_or_else(|_| {
                                println!("out of order detected");
                                0
                            });
                    }
                    TestGameMessage::ReliableOrderedChannel1(num) => {
                        self.ordered_reliable_c1_check
                            .compare_exchange(
                                (num - 1) as usize,
                                num as usize,
                                std::sync::atomic::Ordering::SeqCst,
                                std::sync::atomic::Ordering::SeqCst,
                            )
                            .unwrap_or_else(|_| {
                                println!("out of order detected");
                                0
                            });
                    }
                    TestGameMessage::ReliableOrderedChannel2(num) => {
                        self.ordered_reliable_c2_check
                            .compare_exchange(
                                (num - 1) as usize,
                                num as usize,
                                std::sync::atomic::Ordering::SeqCst,
                                std::sync::atomic::Ordering::SeqCst,
                            )
                            .unwrap_or_else(|_| {
                                println!("out of order detected");
                                0
                            });
                    }
                    TestGameMessage::ReliableOrderedChannel1Con { order: num, id } => {
                        self.ordered_reliable_c1_check_con[id]
                            .compare_exchange(
                                (num - 1) as usize,
                                num as usize,
                                std::sync::atomic::Ordering::SeqCst,
                                std::sync::atomic::Ordering::SeqCst,
                            )
                            .unwrap_or_else(|_| {
                                println!("out of order detected");
                                0
                            });
                    }
                    TestGameMessage::ReliableOrderedChannel2Con { order: num, id } => {
                        self.ordered_reliable_c2_check_con[id]
                            .compare_exchange(
                                (num - 1) as usize,
                                num as usize,
                                std::sync::atomic::Ordering::SeqCst,
                                std::sync::atomic::Ordering::SeqCst,
                            )
                            .unwrap_or_else(|_| {
                                println!("out of order detected");
                                0
                            });
                    }
                    TestGameMessage::AnyPacket(data) => {
                        self.any_packet_gotten
                            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        println!("got any packet, with size: {}", data.len());
                    }
                    TestGameMessage::Bench(_) => {
                        if (timestamp.as_nanos() as usize
                            - self.bench_start.load(std::sync::atomic::Ordering::Relaxed))
                            < (BENCH_TIME_MS * 1000000) as usize
                        {
                            self.bench
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        self.bench_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    TestGameMessage::BenchMulti { id, .. } => {
                        if (timestamp.as_nanos() as usize
                            - self.bench_start.load(std::sync::atomic::Ordering::Relaxed))
                            < (BENCH_TIME_MS * 1000000) as usize
                        {
                            self.bench
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        self.bench_total_multi[id]
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
        }

        async fn generate_from_network_event(
            &self,
            timestamp: Duration,
            con_id: &NetworkConnectionId,
            network_event: &NetworkEvent,
        ) -> bool {
            if self
                .should_log_ping
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                println!(
                    "network_event: {:?} at {:?}: {:?}",
                    con_id, timestamp, network_event
                );
            }
            match network_event {
                NetworkEvent::Disconnected { .. } => {
                    self.is_shutdown
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    true
                }
                NetworkEvent::Connected { .. } => {
                    self.is_connected
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    true
                }
                NetworkEvent::NetworkStats(_) => false, // don't notify because of a ping event
                NetworkEvent::ConnectingFailed(err) => {
                    println!("connecting failed: {}", err);
                    false
                }
            }
        }
    }

    fn it_works_impl<E, C, Z, I, const TY: u32>()
    where
        C: NetworkConnectionInterface,
        Z: NetworkConnectingInterface<C>,
        I: NetworkIncomingInterface<Z>,
        E: NetworkEndpointInterface<Z, I>,
    {
        let (client_cert, client_private_key) = create_certifified_keys();
        let server_cert = client_cert.to_der().unwrap().to_vec();
        let server_pub_key_hash = client_cert
            .tbs_certificate
            .subject_public_key_info
            .fingerprint_bytes()
            .unwrap();
        let sys = System::new();
        let compressor: Arc<Vec<Arc<dyn NetworkPluginPacket>>> =
            Arc::new(vec![Arc::new(DefaultNetworkPacketCompressor::new())]);
        let game_event_generator_server = Arc::new(TestGameEventGenerator::new());
        let game_event_generator_client = Arc::new(TestGameEventGenerator::new());
        let game_event_generator_client2 = Arc::new(TestGameEventGenerator::new());
        let game_event_generator_client3 = Arc::new(TestGameEventGenerator::new());
        let (network_server, _, addr, notifier_server) = Network::<E, C, Z, I, TY>::init_server(
            "0.0.0.0:0",
            game_event_generator_server.clone(),
            NetworkServerCertMode::FromCertAndPrivateKey(Box::new(NetworkServerCertAndKey {
                cert: client_cert,
                private_key: client_private_key,
            })),
            &sys,
            NetworkServerInitOptions::new()
                .with_debug_priting(true)
                .with_max_thread_count(4)
                .with_disallow_05_rtt(true),
            NetworkPlugins {
                packet_plugins: compressor.clone(),
                connection_plugins: Default::default(),
            },
        )
        .unwrap();
        let (client_cert, client_private_key) = create_certifified_keys();
        let (mut network_client, notifier) = Network::<E, C, Z, I, TY>::init_client(
            None,
            game_event_generator_client.clone(),
            &sys,
            NetworkClientInitOptions::new(
                NetworkClientCertCheckMode::CheckByPubKeyHash {
                    hash: Cow::Borrowed(&server_pub_key_hash),
                },
                NetworkClientCertMode::FromCertAndPrivateKey {
                    cert: client_cert,
                    private_key: client_private_key,
                },
            )
            .with_debug_priting(true),
            NetworkPlugins {
                packet_plugins: compressor.clone(),
                connection_plugins: Default::default(),
            },
            &format!("127.0.0.1:{}", addr.port()),
        )
        .unwrap();
        let (client_cert, client_private_key) = create_certifified_keys();
        let (mut network_client2, _notifier2) = Network::<E, C, Z, I, TY>::init_client(
            None,
            game_event_generator_client2.clone(),
            &sys,
            NetworkClientInitOptions::new(
                NetworkClientCertCheckMode::CheckByCert {
                    cert: server_cert.clone().into(),
                },
                NetworkClientCertMode::FromCertAndPrivateKey {
                    cert: client_cert,
                    private_key: client_private_key,
                },
            )
            .with_debug_priting(true),
            NetworkPlugins {
                packet_plugins: compressor.clone(),
                connection_plugins: Default::default(),
            },
            &format!("127.0.0.1:{}", addr.port()),
        )
        .unwrap();
        let (client_cert, client_private_key) = create_certifified_keys();
        let (mut network_client3, _notifier3) = Network::<E, C, Z, I, TY>::init_client(
            None,
            game_event_generator_client3.clone(),
            &sys,
            NetworkClientInitOptions::new(
                NetworkClientCertCheckMode::CheckByCert {
                    cert: server_cert.into(),
                },
                NetworkClientCertMode::FromCertAndPrivateKey {
                    cert: client_cert,
                    private_key: client_private_key,
                },
            )
            .with_debug_priting(true),
            NetworkPlugins {
                packet_plugins: compressor.clone(),
                connection_plugins: Default::default(),
            },
            &format!("127.0.0.1:{}", addr.port()),
        )
        .unwrap();

        while !game_event_generator_server
            .is_connected
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            notifier_server.wait_for_event(None);
        }
        while !game_event_generator_client
            .is_connected
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            notifier.wait_for_event(None);
        }

        const MAX_ORDERED_EV: usize = 1000;
        let test_case = |network_client: &mut Network<E, C, Z, I, TY>| {
            // reliable in order
            *game_event_generator_server.cur_test_name.blocking_lock() =
                "reliable in order".to_string();
            game_event_generator_server
                .ordered_reliable_check
                .store(0, std::sync::atomic::Ordering::SeqCst);
            for i in 1..=MAX_ORDERED_EV {
                network_client.send_in_order_to_server(
                    &TestGameMessage::ReliableOrdered(i as u32),
                    NetworkInOrderChannel::Global,
                );
            }
            while game_event_generator_server
                .ordered_reliable_check
                .load(std::sync::atomic::Ordering::SeqCst)
                != MAX_ORDERED_EV
            {
                notifier_server.wait_for_event(None);
            }
            // reliable in order a on different channel
            *game_event_generator_server.cur_test_name.blocking_lock() =
                "reliable in order on different channel".to_string();
            for channel in 10..12 {
                game_event_generator_server
                    .ordered_reliable_check
                    .store(0, std::sync::atomic::Ordering::SeqCst);
                for i in 1..=MAX_ORDERED_EV {
                    network_client.send_in_order_to_server(
                        &TestGameMessage::ReliableOrdered(i as u32),
                        NetworkInOrderChannel::Custom(channel),
                    );
                }
                while game_event_generator_server
                    .ordered_reliable_check
                    .load(std::sync::atomic::Ordering::SeqCst)
                    != MAX_ORDERED_EV
                {
                    notifier_server.wait_for_event(None);
                }
            }
            *game_event_generator_server.cur_test_name.blocking_lock() =
                "reliable in order on two channels at once".to_string();
            game_event_generator_server
                .ordered_reliable_c1_check
                .store(0, std::sync::atomic::Ordering::SeqCst);
            game_event_generator_server
                .ordered_reliable_c2_check
                .store(0, std::sync::atomic::Ordering::SeqCst);
            for i in 1..=MAX_ORDERED_EV {
                network_client.send_in_order_to_server(
                    &TestGameMessage::ReliableOrderedChannel1(i as u32),
                    NetworkInOrderChannel::Custom(1),
                );
                network_client.send_in_order_to_server(
                    &TestGameMessage::ReliableOrderedChannel2(i as u32),
                    NetworkInOrderChannel::Custom(2),
                );
            }
            while game_event_generator_server
                .ordered_reliable_c1_check
                .load(std::sync::atomic::Ordering::SeqCst)
                != MAX_ORDERED_EV
            {
                notifier_server.wait_for_event(None);
            }
            while game_event_generator_server
                .ordered_reliable_c2_check
                .load(std::sync::atomic::Ordering::SeqCst)
                != MAX_ORDERED_EV
            {
                notifier_server.wait_for_event(None);
            }
            // reliable out of order
            *game_event_generator_server.cur_test_name.blocking_lock() =
                "reliable out of order".to_string();
            game_event_generator_server
                .unordered_reliable_sum
                .store(0, std::sync::atomic::Ordering::SeqCst);
            let mut sum_i = 0;
            for i in 1..=MAX_ORDERED_EV {
                network_client
                    .send_unordered_to_server(&TestGameMessage::ReliableUnordered(i as u32));
                sum_i += i;
            }
            while game_event_generator_server
                .unordered_reliable_sum
                .load(std::sync::atomic::Ordering::SeqCst)
                != sum_i
            {
                notifier_server.wait_for_event(None);
            }
            // unreliable out of order
            *game_event_generator_server.cur_test_name.blocking_lock() =
                "unreliable out of order".to_string();
            game_event_generator_server
                .unordered_unreliable_sum
                .store(0, std::sync::atomic::Ordering::SeqCst);
            let mut sum_i = 0;
            for i in 1..=MAX_ORDERED_EV {
                network_client
                    .send_unreliable_to_server(&TestGameMessage::UnreliableUnordered(i as u32));
                sum_i += i;
            }
            while game_event_generator_server
                .unordered_unreliable_sum
                .load(std::sync::atomic::Ordering::SeqCst)
                != sum_i
            {
                if !notifier_server.wait_for_event(Some(Duration::from_secs(7))) {
                    println!("info: unreliable packet probably lost (this is not a bug)");
                    break;
                }
            }
        };
        test_case(&mut network_client);
        // try all packet orders with multiple connections
        println!("try all packet orders with multiple connections");
        test_case(&mut network_client2);
        test_case(&mut network_client3);

        // try some test with multiple connection at the same time
        // reliable out of order
        *game_event_generator_server.cur_test_name.blocking_lock() =
            "reliable out of order multiple clients".to_string();
        game_event_generator_server
            .unordered_reliable_sum
            .store(0, std::sync::atomic::Ordering::SeqCst);
        let mut sum_i = 0;
        for i in 1..=MAX_ORDERED_EV {
            network_client.send_unordered_to_server(&TestGameMessage::ReliableUnordered(i as u32));
            network_client2.send_unordered_to_server(&TestGameMessage::ReliableUnordered(i as u32));
            network_client3.send_unordered_to_server(&TestGameMessage::ReliableUnordered(i as u32));
            sum_i += i * 3;
        }
        while game_event_generator_server
            .unordered_reliable_sum
            .load(std::sync::atomic::Ordering::SeqCst)
            != sum_i
        {
            notifier_server.wait_for_event(None);
        }
        // unreliable out of order
        *game_event_generator_server.cur_test_name.blocking_lock() =
            "unreliable out of order multiple clients".to_string();
        game_event_generator_server
            .unordered_unreliable_sum
            .store(0, std::sync::atomic::Ordering::SeqCst);
        let mut sum_i = 0;
        for i in 1..=MAX_ORDERED_EV {
            network_client
                .send_unreliable_to_server(&TestGameMessage::UnreliableUnordered(i as u32));
            network_client2
                .send_unreliable_to_server(&TestGameMessage::UnreliableUnordered(i as u32));
            network_client3
                .send_unreliable_to_server(&TestGameMessage::UnreliableUnordered(i as u32));
            sum_i += i * 3;
        }
        while game_event_generator_server
            .unordered_unreliable_sum
            .load(std::sync::atomic::Ordering::SeqCst)
            != sum_i
        {
            if !notifier_server.wait_for_event(Some(Duration::from_secs(10))) {
                println!("info: unreliable packet probably lost (this is not a bug)");
                break;
            }
        }

        // try in order test with multiple clients sending
        *game_event_generator_server.cur_test_name.blocking_lock() =
            "reliable in order multiple clients, multiple channels".to_string();
        game_event_generator_server
            .ordered_reliable_c1_check_con
            .iter()
            .for_each(|f| f.store(0, std::sync::atomic::Ordering::SeqCst));
        game_event_generator_server
            .ordered_reliable_c2_check_con
            .iter()
            .for_each(|f| f.store(0, std::sync::atomic::Ordering::SeqCst));
        for i in 1..=MAX_ORDERED_EV {
            network_client.send_in_order_to_server(
                &TestGameMessage::ReliableOrderedChannel1Con {
                    order: i as u32,
                    id: 0,
                },
                NetworkInOrderChannel::Custom(1),
            );
            network_client2.send_in_order_to_server(
                &TestGameMessage::ReliableOrderedChannel1Con {
                    order: i as u32,
                    id: 1,
                },
                NetworkInOrderChannel::Custom(1),
            );
            network_client3.send_in_order_to_server(
                &TestGameMessage::ReliableOrderedChannel1Con {
                    order: i as u32,
                    id: 2,
                },
                NetworkInOrderChannel::Custom(1),
            );
            network_client.send_in_order_to_server(
                &TestGameMessage::ReliableOrderedChannel2Con {
                    order: i as u32,
                    id: 0,
                },
                NetworkInOrderChannel::Custom(2),
            );
            network_client2.send_in_order_to_server(
                &TestGameMessage::ReliableOrderedChannel2Con {
                    order: i as u32,
                    id: 1,
                },
                NetworkInOrderChannel::Custom(2),
            );
            network_client3.send_in_order_to_server(
                &TestGameMessage::ReliableOrderedChannel2Con {
                    order: i as u32,
                    id: 2,
                },
                NetworkInOrderChannel::Custom(2),
            );
        }
        while game_event_generator_server
            .ordered_reliable_c1_check_con
            .iter()
            .any(|f| f.load(std::sync::atomic::Ordering::SeqCst) != MAX_ORDERED_EV)
        {
            notifier_server.wait_for_event(None);
        }
        while game_event_generator_server
            .ordered_reliable_c2_check_con
            .iter()
            .any(|f| f.load(std::sync::atomic::Ordering::SeqCst) != MAX_ORDERED_EV)
        {
            notifier_server.wait_for_event(None);
        }

        *game_event_generator_server.cur_test_name.blocking_lock() = "huge packet".to_string();
        let mut arbitrary_packet: Vec<u8> = Vec::new();
        // for a reliable stream the size should not matter, try 10 MBytes of data
        println!("preparing 10 MBytes data");
        arbitrary_packet.resize(1024 * 1024 * 10_usize, Default::default());
        network_client.send_unordered_to_server(&TestGameMessage::AnyPacket(arbitrary_packet));
        println!("pushed 10 MBytes data on network stack");

        while game_event_generator_server
            .any_packet_gotten
            .load(std::sync::atomic::Ordering::SeqCst)
            == 0
        {
            notifier_server.wait_for_event(None);
        }

        drop(network_client);
        drop(network_client2);
        drop(network_client3);

        drop(network_server);

        assert_eq!(
            game_event_generator_server
                .ordered_reliable_check
                .load(std::sync::atomic::Ordering::SeqCst),
            MAX_ORDERED_EV,
            "ordered reliable messages were wrong"
        );
    }

    #[test]
    fn it_works() {
        it_works_impl::<
            QuinnEndpointWrapper,
            QuinnNetworkConnectionWrapper,
            QuinnNetworkConnectingWrapper,
            QuinnNetworkIncomingWrapper,
            0,
        >();
    }

    #[test]
    fn it_works_websockets() {
        it_works_impl::<
            TungsteniteEndpointWrapper,
            TungsteniteNetworkConnectionWrapper,
            TungsteniteNetworkConnectingWrapper,
            TungsteniteNetworkIncomingWrapper,
            0,
        >();
    }

    #[test]
    fn max_datagram_size_tests() {
        let (client_cert, client_private_key) = create_certifified_keys();
        let server_cert = client_cert.to_der().unwrap().to_vec();
        let sys = System::new();
        let game_event_generator_server = Arc::new(TestGameEventGenerator::new());
        let game_event_generator_client = Arc::new(TestGameEventGenerator::new());
        let (network_server, _, addr, notifier_server) = QuinnNetwork::init_server(
            "0.0.0.0:0",
            game_event_generator_server.clone(),
            NetworkServerCertMode::FromCertAndPrivateKey(Box::new(NetworkServerCertAndKey {
                cert: client_cert,
                private_key: client_private_key,
            })),
            &sys,
            NetworkServerInitOptions::new()
                .with_debug_priting(true)
                .with_max_thread_count(2)
                .with_disallow_05_rtt(true),
            Default::default(),
        )
        .unwrap();
        let (client_cert, client_private_key) = create_certifified_keys();
        let (network_client, notifier) = QuinnNetwork::init_client(
            None,
            game_event_generator_client.clone(),
            &sys,
            NetworkClientInitOptions::new(
                NetworkClientCertCheckMode::CheckByCert {
                    cert: server_cert.into(),
                },
                NetworkClientCertMode::FromCertAndPrivateKey {
                    cert: client_cert,
                    private_key: client_private_key,
                },
            )
            .with_debug_priting(true),
            Default::default(),
            &format!("127.0.0.1:{}", addr.port()),
        )
        .unwrap();

        while !game_event_generator_server
            .is_connected
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            notifier_server.wait_for_event(None);
        }
        while !game_event_generator_client
            .is_connected
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            notifier.wait_for_event(None);
        }

        let mut arbitrary_packet: Vec<u8> = Vec::new();
        // the maximum datagram size for a quinn datagram is 1200 before MTU detection
        // however this isn't the maximum safe packet size. For simplicity assume around
        // 512 bytes
        arbitrary_packet.resize(512_usize, Default::default());
        network_client.send_unreliable_to_server(&TestGameMessage::AnyPacket(arbitrary_packet));

        while game_event_generator_server
            .any_packet_gotten
            .load(std::sync::atomic::Ordering::SeqCst)
            == 0
        {
            if !notifier_server.wait_for_event(Some(Duration::from_secs(10))) {
                println!("info: unreliable packet probably lost (this is not a bug)");
                break;
            }
        }

        drop(network_client);

        drop(network_server);
    }

    #[test]
    pub fn test_benchmark() {
        let (client_cert, client_private_key) = create_certifified_keys();
        let server_pub_key_hash = client_cert
            .tbs_certificate
            .subject_public_key_info
            .fingerprint_bytes()
            .unwrap();
        let sys = System::new();
        let game_event_generator_server = Arc::new(TestGameEventGenerator::new());
        let game_event_generator_client = Arc::new(TestGameEventGenerator::new());
        let (network_server, _, addr, notifier_server) = QuinnNetwork::init_server(
            "0.0.0.0:0",
            game_event_generator_server.clone(),
            NetworkServerCertMode::FromCertAndPrivateKey(Box::new(NetworkServerCertAndKey {
                cert: client_cert,
                private_key: client_private_key,
            })),
            &sys,
            Default::default(),
            Default::default(),
        )
        .unwrap();

        game_event_generator_server
            .should_log_ping
            .store(false, std::sync::atomic::Ordering::Relaxed);

        let total_packets = Arc::new(AtomicUsize::new(0));

        let total_packets_thread = total_packets.clone();
        let sys_thread = sys.clone();

        let (client_cert, client_private_key) = create_certifified_keys();
        let (network_client, notifier) = QuinnNetwork::init_client(
            None,
            game_event_generator_client.clone(),
            &sys,
            NetworkClientInitOptions::new(
                NetworkClientCertCheckMode::CheckByPubKeyHash {
                    hash: Cow::Borrowed(&server_pub_key_hash),
                },
                NetworkClientCertMode::FromCertAndPrivateKey {
                    cert: client_cert,
                    private_key: client_private_key,
                },
            ),
            Default::default(),
            &format!("127.0.0.1:{}", addr.port()),
        )
        .unwrap();
        network_client.send_in_order_to_server(
            &TestGameMessage::AnyPacket(vec![]),
            NetworkInOrderChannel::Global,
        );

        while !game_event_generator_client
            .is_connected
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            notifier.wait_for_event(None);
        }

        let msg = vec![0; 64];
        let start_time = sys_thread.time_get();
        game_event_generator_server.bench_start.store(
            start_time.as_nanos() as usize,
            std::sync::atomic::Ordering::SeqCst,
        );
        loop {
            network_client.send_in_order_to_server(
                &TestGameMessage::Bench(msg.clone()),
                NetworkInOrderChannel::Global,
            );

            total_packets_thread.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            while game_event_generator_server
                .bench_total
                .load(std::sync::atomic::Ordering::Relaxed)
                + 500
                < total_packets_thread.load(std::sync::atomic::Ordering::Relaxed)
            {
                std::thread::yield_now();
            }

            if (sys_thread.time_get() - start_time).as_millis() > BENCH_TIME_MS {
                break;
            }
        }

        while game_event_generator_server
            .bench_total
            .load(std::sync::atomic::Ordering::SeqCst)
            != total_packets.load(std::sync::atomic::Ordering::SeqCst)
        {
            notifier_server.wait_for_event(Some(Duration::from_millis(10000)));
            let cur_events = game_event_generator_server
                .bench_total
                .load(std::sync::atomic::Ordering::SeqCst);
            if cur_events % 1000 == 0 {
                println!(
                    "waiting for server: {}/{}",
                    cur_events,
                    total_packets.load(std::sync::atomic::Ordering::SeqCst)
                );
            }
        }

        drop(network_client);
        drop(network_server);

        println!(
            "send {} packets in per second ({} total packets sent)",
            game_event_generator_server
                .bench
                .load(std::sync::atomic::Ordering::SeqCst)
                / (BENCH_TIME_MS / 1000) as usize,
            game_event_generator_server
                .bench_total
                .load(std::sync::atomic::Ordering::SeqCst)
                / (BENCH_TIME_MS / 1000) as usize
        );
    }

    #[test]
    pub fn test_benchmark_multi() {
        let (client_cert, client_private_key) = create_certifified_keys();
        let server_cert = client_cert.to_der().unwrap().to_vec();
        let sys = System::new();
        let game_event_generator_server = Arc::new(TestGameEventGenerator::new());
        let (network_server, _, addr, notifier_server) = QuinnNetwork::init_server(
            "0.0.0.0:0",
            game_event_generator_server.clone(),
            NetworkServerCertMode::FromCertAndPrivateKey(Box::new(NetworkServerCertAndKey {
                cert: client_cert,
                private_key: client_private_key,
            })),
            &sys,
            Default::default(),
            Default::default(),
        )
        .unwrap();

        game_event_generator_server
            .should_log_ping
            .store(false, std::sync::atomic::Ordering::Relaxed);

        let total_packets = Arc::new(AtomicUsize::new(0));

        let mut thread_joins: Vec<JoinHandle<()>> = Default::default();
        let bench_start = game_event_generator_server.bench_start.clone();
        let start_time_ = sys.time_get();
        bench_start.store(
            start_time_.as_nanos() as usize,
            std::sync::atomic::Ordering::SeqCst,
        );
        let finished_networks = Arc::new(Mutex::new(Vec::new()));
        let threads = std::thread::available_parallelism()
            .unwrap_or(NonZeroUsize::new(2).unwrap())
            .min(NonZeroUsize::new(32).unwrap());
        for i in 0..threads.get() {
            let game_event_generator_client = Arc::new(TestGameEventGenerator::new());

            let total_packets_thread = total_packets.clone();
            let sys_thread = sys.clone();
            let total_count = game_event_generator_server.bench_total_multi[i].clone();
            let server_cert = server_cert.clone();

            let (client_cert, client_private_key) = create_certifified_keys();
            let (network_client, notifier) = QuinnNetwork::init_client(
                None,
                game_event_generator_client.clone(),
                &sys_thread,
                NetworkClientInitOptions::new(
                    NetworkClientCertCheckMode::CheckByCert {
                        cert: server_cert.into(),
                    },
                    NetworkClientCertMode::FromCertAndPrivateKey {
                        cert: client_cert,
                        private_key: client_private_key,
                    },
                ),
                Default::default(),
                &format!("127.0.0.1:{}", addr.port()),
            )
            .unwrap();

            while !game_event_generator_client
                .is_connected
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                notifier.wait_for_event(None);
            }

            let finished_networks = finished_networks.clone();
            let t = std::thread::Builder::new()
                .name("network-test".into())
                .spawn(move || {
                    let total_packets_this_thread = Arc::new(AtomicUsize::new(0));
                    let msg = vec![0; 64];
                    let start_time = sys_thread.time_get();
                    loop {
                        network_client.send_in_order_to_server(
                            &TestGameMessage::BenchMulti {
                                msg: msg.clone(),
                                id: i,
                            },
                            NetworkInOrderChannel::Custom(
                                total_packets_this_thread
                                    .load(std::sync::atomic::Ordering::Relaxed)
                                    % 5,
                            ),
                        );

                        total_packets_thread.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        total_packets_this_thread
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                        while total_count.load(std::sync::atomic::Ordering::Relaxed) + 2000
                            < total_packets_this_thread.load(std::sync::atomic::Ordering::Relaxed)
                        {
                            std::thread::yield_now();
                        }

                        if (sys_thread.time_get() - start_time).as_millis() > BENCH_TIME_MS {
                            break;
                        }
                    }

                    finished_networks.blocking_lock().push(network_client);
                })
                .unwrap();
            thread_joins.push(t);
        }

        thread_joins.drain(..).for_each(|j| j.join().unwrap());
        while game_event_generator_server
            .bench_total_multi
            .iter()
            .map(|d| d.load(std::sync::atomic::Ordering::SeqCst))
            .sum::<usize>()
            != total_packets.load(std::sync::atomic::Ordering::SeqCst)
        {
            notifier_server.wait_for_event(Some(Duration::from_millis(10000)));
        }

        drop(finished_networks);

        drop(network_server);

        println!(
            "send {} packets in per second",
            game_event_generator_server
                .bench
                .load(std::sync::atomic::Ordering::SeqCst)
                / (BENCH_TIME_MS / 1000) as usize
        );
    }

    #[test]
    pub fn rapid_connect_disconnect() {
        let (client_cert, client_private_key) = create_certifified_keys();
        let server_cert = client_cert.to_der().unwrap().to_vec();
        let sys = System::new();
        let game_event_generator_server = Arc::new(TestGameEventGenerator::new());
        let (_network_server, _, addr, _) = QuinnNetwork::init_server(
            "0.0.0.0:0",
            game_event_generator_server.clone(),
            NetworkServerCertMode::FromCertAndPrivateKey(Box::new(NetworkServerCertAndKey {
                cert: client_cert,
                private_key: client_private_key,
            })),
            &sys,
            Default::default(),
            Default::default(),
        )
        .unwrap();

        game_event_generator_server
            .should_log_ping
            .store(false, std::sync::atomic::Ordering::Relaxed);

        let bench_start = game_event_generator_server.bench_start.clone();
        let start_time_ = sys.time_get();
        bench_start.store(
            start_time_.as_nanos() as usize,
            std::sync::atomic::Ordering::SeqCst,
        );

        let game_event_generator_client = Arc::new(TestGameEventGenerator::new());

        let sys_thread = sys.clone();
        let server_cert = server_cert.clone();

        let (client_cert, client_private_key) = create_certifified_keys();

        for _i in 0..20 {
            let (network_client, _) = QuinnNetwork::init_client(
                None,
                game_event_generator_client.clone(),
                &sys_thread,
                NetworkClientInitOptions::new(
                    NetworkClientCertCheckMode::CheckByCert {
                        cert: server_cert.clone().into(),
                    },
                    NetworkClientCertMode::FromCertAndPrivateKey {
                        cert: client_cert.clone(),
                        private_key: client_private_key.clone(),
                    },
                ),
                Default::default(),
                &format!("127.0.0.1:{}", addr.port()),
            )
            .unwrap();
            network_client
                .send_unordered_to_server(&TestGameMessage::AnyPacket(vec![0; 64 * 1024]));
            network_client.send_in_order_to_server(
                &TestGameMessage::AnyPacket(vec![0; 64 * 1024]),
                NetworkInOrderChannel::Global,
            );
        }
        for _i in 0..20 {
            let (client_cert, client_private_key) = create_certifified_keys();
            let (network_client, _) = QuinnNetwork::init_client(
                None,
                game_event_generator_client.clone(),
                &sys_thread,
                NetworkClientInitOptions::new(
                    NetworkClientCertCheckMode::CheckByCert {
                        cert: server_cert.clone().into(),
                    },
                    NetworkClientCertMode::FromCertAndPrivateKey {
                        cert: client_cert,
                        private_key: client_private_key,
                    },
                ),
                Default::default(),
                &format!("127.0.0.1:{}", addr.port()),
            )
            .unwrap();
            network_client
                .send_unordered_to_server(&TestGameMessage::AnyPacket(vec![0; 64 * 1024]));
            network_client.send_in_order_to_server(
                &TestGameMessage::AnyPacket(vec![0; 64 * 1024]),
                NetworkInOrderChannel::Global,
            );
        }
    }

    #[test]
    pub fn too_many_channels() {
        let (client_cert, client_private_key) = create_certifified_keys();
        let server_cert = client_cert.to_der().unwrap().to_vec();
        let sys = System::new();
        let game_event_generator_server = Arc::new(TestGameEventGenerator::new());
        let (_network_server, _, addr, notifier_server) = QuinnNetwork::init_server(
            "0.0.0.0:0",
            game_event_generator_server.clone(),
            NetworkServerCertMode::FromCertAndPrivateKey(Box::new(NetworkServerCertAndKey {
                cert: client_cert,
                private_key: client_private_key,
            })),
            &sys,
            Default::default(),
            Default::default(),
        )
        .unwrap();

        game_event_generator_server
            .should_log_ping
            .store(false, std::sync::atomic::Ordering::Relaxed);

        let bench_start = game_event_generator_server.bench_start.clone();
        let start_time_ = sys.time_get();
        bench_start.store(
            start_time_.as_nanos() as usize,
            std::sync::atomic::Ordering::SeqCst,
        );

        let game_event_generator_client = Arc::new(TestGameEventGenerator::new());

        let sys_thread = sys.clone();
        let server_cert = server_cert.clone();

        let (client_cert, client_private_key) = create_certifified_keys();

        let (network_client, _) = QuinnNetwork::init_client(
            None,
            game_event_generator_client.clone(),
            &sys_thread,
            NetworkClientInitOptions::new(
                NetworkClientCertCheckMode::CheckByCert {
                    cert: server_cert.clone().into(),
                },
                NetworkClientCertMode::FromCertAndPrivateKey {
                    cert: client_cert.clone(),
                    private_key: client_private_key.clone(),
                },
            ),
            Default::default(),
            &format!("127.0.0.1:{}", addr.port()),
        )
        .unwrap();
        for i in 0..100 {
            network_client.send_in_order_to_server(
                &TestGameMessage::AnyPacket(vec![0; 64 * 1024]),
                NetworkInOrderChannel::Custom(i),
            );
        }
        network_client.send_in_order_to_server(
            &TestGameMessage::AnyPacket(vec![0; 64 * 1024]),
            NetworkInOrderChannel::Global,
        );

        while game_event_generator_server
            .any_packet_gotten
            .load(std::sync::atomic::Ordering::SeqCst)
            < 101
        {
            if !notifier_server.wait_for_event(Some(Duration::from_secs(10))) {
                println!("info: unreliable packet probably lost (this is not a bug)");
                break;
            }
        }

        println!("test successful");
    }

    #[test]
    fn ip_tests_alt_works() {
        // ipv4
        let ip_range: prefix_trie::PrefixSet<ipnet::Ipv4Net> = [
            "10.0.0.0/8",
            "172.16.0.0/16",
            "192.168.1.0/24",
            "5.5.5.5/32",
        ]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();

        assert!(ip_range
            .get_lpm(&"172.16.32.1/32".parse::<ipnet::Ipv4Net>().unwrap())
            .is_some());
        assert!(ip_range
            .get_lpm(&"172.17.32.1/32".parse::<ipnet::Ipv4Net>().unwrap())
            .is_none());
        assert!(ip_range
            .get_lpm(&"192.168.1.1/32".parse::<ipnet::Ipv4Net>().unwrap())
            .is_some());
        assert!(ip_range
            .get_lpm(&"192.168.2.1/32".parse::<ipnet::Ipv4Net>().unwrap())
            .is_none());
        assert!(ip_range
            .get_lpm(&"10.5.5.5/32".parse::<ipnet::Ipv4Net>().unwrap())
            .is_some());
        assert!(ip_range
            .get_lpm(&"11.5.5.5/32".parse::<ipnet::Ipv4Net>().unwrap())
            .is_none());
        assert!(ip_range
            .get_lpm(&"5.5.5.5/32".parse::<ipnet::Ipv4Net>().unwrap())
            .is_some());
        assert!(ip_range
            .get_lpm(&"5.5.5.6/32".parse::<ipnet::Ipv4Net>().unwrap())
            .is_none());

        // ipv6
        let ip_range: prefix_trie::PrefixSet<ipnet::Ipv6Net> = [
            "2000::/16",
            "3000:FF00::/32",
            "FF00:1234:5432::/48",
            "A000:B000:C000:D000::/64",
            "F000:E000:D000:C000:B000:A000:9000:8000/128",
        ]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();

        assert!(ip_range
            .get_lpm(&"2000:FF::/128".parse::<ipnet::Ipv6Net>().unwrap())
            .is_some());
        assert!(ip_range
            .get_lpm(&"2001:FF::/128".parse::<ipnet::Ipv6Net>().unwrap())
            .is_none());
        assert!(ip_range
            .get_lpm(&"3000:FF00:FF00::/128".parse::<ipnet::Ipv6Net>().unwrap())
            .is_some());
        assert!(ip_range
            .get_lpm(&"3000:FF01:FF00::/128".parse::<ipnet::Ipv6Net>().unwrap())
            .is_none());
        assert!(ip_range
            .get_lpm(
                &"A000:B000:C000:D000:1::/128"
                    .parse::<ipnet::Ipv6Net>()
                    .unwrap()
            )
            .is_some());
        assert!(ip_range
            .get_lpm(
                &"A000:B000:C000:D001::/128"
                    .parse::<ipnet::Ipv6Net>()
                    .unwrap()
            )
            .is_none());
        assert!(ip_range
            .get_lpm(
                &"F000:E000:D000:C000:B000:A000:9000:8000/128"
                    .parse::<ipnet::Ipv6Net>()
                    .unwrap()
            )
            .is_some());
        assert!(ip_range
            .get_lpm(
                &"F000:E000:D000:C000:B000:A000:9000:8001/128"
                    .parse::<ipnet::Ipv6Net>()
                    .unwrap()
            )
            .is_none());
    }
}
