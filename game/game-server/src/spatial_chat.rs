use std::collections::{BTreeMap, HashMap, HashSet};

use game_interface::types::{id_types::PlayerId, player_info::PlayerUniqueId};
use network::network::{connection::NetworkConnectionId, quinn_network::QuinnNetworks};
use game_network::messages::{MsgSvSpatialChatOfEntitity, ServerToClientMessage};

const MAX_ID_REORDER: u64 = 2;

#[derive(Debug)]
pub struct SpatialClient {
    pending_opus_frames: BTreeMap<u64, Vec<Vec<u8>>>,
    handled_id: Option<u64>,
    main_player_id: PlayerId,
    player_unique_id: PlayerUniqueId,
}

#[derive(Debug, Default)]
pub struct SpatialWorld {
    clients: HashMap<NetworkConnectionId, SpatialClient>,
}

impl SpatialWorld {
    pub fn chat_sound(
        &mut self,
        client: NetworkConnectionId,
        main_player_id: PlayerId,
        player_unique_id: PlayerUniqueId,
        id: u64,
        opus_frames: Vec<Vec<u8>>,
    ) {
        let client = self.clients.entry(client).or_insert_with(|| SpatialClient {
            pending_opus_frames: Default::default(),
            handled_id: Default::default(),
            main_player_id,
            player_unique_id,
        });
        if client
            .handled_id
            .is_none_or(|handled_id| handled_id < id + MAX_ID_REORDER)
        {
            client.pending_opus_frames.insert(id, opus_frames);
        }
    }

    pub fn update(&mut self, network: &QuinnNetworks) {
        let all_clients = self
            .clients
            .keys()
            .copied()
            .collect::<HashSet<NetworkConnectionId>>();
        for client_id in all_clients {
            let mut entities: HashMap<PlayerId, MsgSvSpatialChatOfEntitity> = Default::default();
            for (_, client) in self.clients.iter_mut().filter(|(&id, _)| id != client_id) {
                entities.insert(
                    client.main_player_id,
                    MsgSvSpatialChatOfEntitity {
                        latest_opus_frames: client.pending_opus_frames.clone(),
                        player_unique_id: client.player_unique_id,
                    },
                );
            }
            network.send_unordered_auto_to(
                &ServerToClientMessage::SpatialChat { entities },
                &client_id,
            );
        }

        // clear all pending
        self.clients.values_mut().for_each(|c| {
            // check if empty here, else the handled id gets None
            if !c.pending_opus_frames.is_empty() {
                c.handled_id = c.pending_opus_frames.keys().max().copied();
            }
            c.pending_opus_frames.clear();
        });
    }

    pub fn on_client_drop(&mut self, con_id: &NetworkConnectionId) {
        self.clients.remove(con_id);
    }
}
