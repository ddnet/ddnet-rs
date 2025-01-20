use std::{
    collections::{HashMap, VecDeque},
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use async_trait::async_trait;
use base::hash::Hash;
use math::math::vector::vec2;
use network::network::{
    connection::{ConnectionStats, NetworkConnectionId},
    event::NetworkEvent,
    event_generator::NetworkEventToGameEventGenerator,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::actions::actions::EditorActionGroup;

/// An editor command is the way the user expresses to
/// issue a certain state change.
///
/// E.g. an undo command means that the server should try to
/// undo the last action.
/// It's basically the logic of the editor ui which does not diretly affect
/// the state of the map.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EditorCommand {
    Undo,
    Redo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorEventOverwriteMap {
    pub map: Vec<u8>,
    pub resources: HashMap<Hash, Vec<u8>>,
}

/// The client props the server knows about.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ClientProps {
    pub mapper_name: String,
    pub color: [u8; 3],

    /// Cursor position in the world coordinates
    pub cursor_world: vec2,

    /// unique id on the server
    pub server_id: u64,

    pub stats: Option<ConnectionStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminConfigState {
    pub auto_save: Option<Duration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminChangeConfig {
    pub password: String,
    pub state: AdminConfigState,
}

/// editor events are a collection of either actions or commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EditorEventClientToServer {
    Action(EditorActionGroup),
    Auth {
        password: String,
        // if not local user
        is_local_client: bool,
        mapper_name: String,
        color: [u8; 3],
    },
    Command(EditorCommand),
    Info(ClientProps),
    Chat {
        msg: String,
    },
    AdminAuth {
        password: String,
    },
    AdminChangeConfig(AdminChangeConfig),
}

/// editor events are a collection of either actions or commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EditorEventServerToClient {
    RedoAction {
        action: EditorActionGroup,
        redo_label: Option<String>,
        undo_label: Option<String>,
    },
    UndoAction {
        action: EditorActionGroup,
        redo_label: Option<String>,
        undo_label: Option<String>,
    },
    Error(String),
    Map(EditorEventOverwriteMap),
    Infos(Vec<ClientProps>),
    Info {
        server_id: u64,
        /// Allows remotely controlled administration (e.g. changing config)
        allows_remote_admin: bool,
    },
    Chat {
        from: String,
        msg: String,
    },
    AdminAuthed,
    AdminState {
        cur_state: AdminConfigState,
    },
}

/// editor events are a collection of either actions or commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EditorEvent {
    Client(EditorEventClientToServer),
    Server(EditorEventServerToClient),
}

pub enum EditorNetEvent {
    Editor(EditorEvent),
    NetworkEvent(NetworkEvent),
}

pub struct EditorEventGenerator {
    pub events: Arc<Mutex<VecDeque<(NetworkConnectionId, Duration, EditorNetEvent)>>>,
    pub has_events: Arc<AtomicBool>,
}

impl EditorEventGenerator {
    pub fn new(has_events: Arc<AtomicBool>) -> Self {
        EditorEventGenerator {
            events: Default::default(),
            has_events,
        }
    }

    pub fn take(&self) -> VecDeque<(NetworkConnectionId, Duration, EditorNetEvent)> {
        std::mem::take(&mut self.events.blocking_lock())
    }

    pub fn push(&self, event: (NetworkConnectionId, Duration, EditorNetEvent)) {
        self.events.blocking_lock().push_back(event);
    }
}

#[async_trait]
impl NetworkEventToGameEventGenerator for EditorEventGenerator {
    async fn generate_from_binary(
        &self,
        timestamp: Duration,
        con_id: &NetworkConnectionId,
        bytes: &[u8],
    ) {
        let msg = bincode::serde::decode_from_slice::<EditorEvent, _>(
            bytes,
            bincode::config::standard().with_limit::<{ 1024 * 1024 * 1024 }>(),
        );
        if let Ok((msg, _)) = msg {
            self.events
                .lock()
                .await
                .push_back((*con_id, timestamp, EditorNetEvent::Editor(msg)));
            self.has_events
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    async fn generate_from_network_event(
        &self,
        timestamp: Duration,
        con_id: &NetworkConnectionId,
        network_event: &NetworkEvent,
    ) -> bool {
        {
            let mut events = self.events.lock().await;
            // network stats are not vital, so drop them if the queue gets too big
            if !matches!(network_event, NetworkEvent::NetworkStats(_)) || events.len() < 200 {
                events.push_back((
                    *con_id,
                    timestamp,
                    EditorNetEvent::NetworkEvent(network_event.clone()),
                ));
            }
        }
        self.has_events
            .store(true, std::sync::atomic::Ordering::Relaxed);
        true
    }
}
