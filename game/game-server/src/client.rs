use std::{
    collections::{BTreeMap, HashMap, HashSet},
    net::IpAddr,
    sync::Arc,
    time::Duration,
};

use base::linked_hash_map_view::FxLinkedHashMap;
use game_interface::types::{
    id_types::PlayerId, network_stats::PlayerNetworkStats, player_info::PlayerDropReason,
};
use game_network::messages::MsgSvInputAck;
use network::network::connection::NetworkConnectionId;
use pool::{datatypes::PoolFxLinkedHashMap, pool::Pool};
use game_base::network::messages::PlayerInputChainable;

use crate::server_game::ClientAuth;

/// A network queued client is a client that isn't actually part of the game,
/// but e.g. waiting for a slot.
#[derive(Debug)]
pub struct ServerNetworkQueuedClient {
    pub connect_timestamp: Duration,
    pub ip: IpAddr,
    pub auth: ClientAuth,
    pub network_stats: PlayerNetworkStats,
}

impl ServerNetworkQueuedClient {
    pub fn new(
        connect_timestamp: &Duration,
        ip: IpAddr,
        auth: ClientAuth,
        network_stats: PlayerNetworkStats,
    ) -> Self {
        Self {
            connect_timestamp: *connect_timestamp,
            ip,
            auth,
            network_stats,
        }
    }
}

/// A network client is a client that will be part of the game, but is not yet ready,
/// e.g. downloading the map etc.
#[derive(Debug)]
pub struct ServerNetworkClient {
    pub connect_timestamp: Duration,
    pub ip: IpAddr,
    pub auth: ClientAuth,
    pub network_stats: PlayerNetworkStats,
}

impl ServerNetworkClient {
    pub fn new(
        connect_timestamp: &Duration,
        ip: IpAddr,
        cert: Arc<x509_cert::Certificate>,
        network_stats: PlayerNetworkStats,
    ) -> Self {
        Self {
            connect_timestamp: *connect_timestamp,
            ip,
            auth: ClientAuth {
                cert,
                level: Default::default(),
            },
            network_stats,
        }
    }
}

#[derive(Debug)]
pub struct ServerClientPlayer {
    /// last (few) inputs the server uses for diffs.
    pub input_storage: BTreeMap<u64, PlayerInputChainable>,

    /// The client given id for this player.
    pub id: u64,
}

#[derive(Debug)]
pub struct ClientSnapshotForDiff {
    pub snap_id: u64,
    pub snapshot: Vec<u8>,
    pub monotonic_tick: u64,
}

#[derive(Debug)]
pub struct ClientSnapshotStorage {
    pub snapshot: Vec<u8>,
    pub monotonic_tick: u64,
}

/// A server client is a client that is part of the game.
#[derive(Debug)]
pub struct ServerClient {
    pub players: PoolFxLinkedHashMap<PlayerId, ServerClientPlayer>,
    pub connect_timestamp: Duration,

    pub snap_id: u64,

    /// latest snap id the client knows about
    pub latest_client_snap: Option<ClientSnapshotForDiff>,
    pub client_snap_storage: BTreeMap<u64, ClientSnapshotStorage>,

    pub inputs_to_ack: Vec<MsgSvInputAck>,

    pub network_stats: PlayerNetworkStats,

    pub loaded_map_votes: bool,
    pub loaded_misc_votes: bool,

    pub ip: IpAddr,
    pub auth: ClientAuth,

    pub requested_account_rename: bool,
    pub requested_account_details: bool,

    /// alternative drop reason if the network signalled a disconnect
    pub drop_reason: Option<PlayerDropReason>,
}

impl ServerClient {
    pub fn new(
        connect_timestamp: &Duration,
        pool: &Pool<FxLinkedHashMap<PlayerId, ServerClientPlayer>>,
        ip: IpAddr,
        auth: ClientAuth,
        network_stats: PlayerNetworkStats,
    ) -> Self {
        Self {
            players: pool.new(),
            connect_timestamp: *connect_timestamp,

            snap_id: 0,

            latest_client_snap: None,
            client_snap_storage: Default::default(),

            inputs_to_ack: Default::default(),

            loaded_map_votes: false,
            loaded_misc_votes: false,

            ip,
            auth,

            network_stats,

            requested_account_rename: false,
            requested_account_details: false,

            drop_reason: None,
        }
    }
}

#[derive(Debug)]
pub struct Clients {
    pub network_queued_clients: FxLinkedHashMap<NetworkConnectionId, ServerNetworkQueuedClient>,
    pub network_clients: HashMap<NetworkConnectionId, ServerNetworkClient>,
    pub clients: HashMap<NetworkConnectionId, ServerClient>,

    // pools
    players_pool: Pool<FxLinkedHashMap<PlayerId, ServerClientPlayer>>,
}

impl Clients {
    pub fn new(max_clients: usize, max_players_per_ip: usize) -> Self {
        Self {
            players_pool: Pool::with_sized(max_clients.min(512), || {
                FxLinkedHashMap::with_capacity_and_hasher(
                    max_players_per_ip.min(8),
                    rustc_hash::FxBuildHasher,
                )
            }),
            network_queued_clients: Default::default(),
            network_clients: Default::default(),
            clients: Default::default(),
        }
    }

    pub fn try_client_ready(&mut self, con_id: &NetworkConnectionId) -> Option<&mut ServerClient> {
        // check if the client can be part of the game
        self.network_clients.remove(con_id).and_then(|net_client| {
            log::debug!(target: "server", "client ready");
            self.clients.insert(
                *con_id,
                ServerClient::new(
                    &net_client.connect_timestamp.clone(),
                    &self.players_pool,
                    net_client.ip,
                    net_client.auth,
                    net_client.network_stats,
                ),
            );
            self.clients.get_mut(con_id)
        })
    }

    pub fn allowed_to_vote_count(&self) -> usize {
        let mut ips: HashSet<IpAddr> = Default::default();

        for client in self.clients.values() {
            ips.insert(client.ip);
        }

        ips.len()
    }
}
