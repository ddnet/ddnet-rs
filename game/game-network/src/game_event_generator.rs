use std::{
    collections::VecDeque,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use async_trait::async_trait;
use network::network::{
    connection::NetworkConnectionId, event::NetworkEvent,
    event_generator::NetworkEventToGameEventGenerator,
};
use serde::de::DeserializeOwned;
use tokio::sync::Mutex;

pub enum GameEvents<E> {
    NetworkEvent(NetworkEvent),
    NetworkMsg(E),
}

pub type Events<E> = Arc<Mutex<VecDeque<(NetworkConnectionId, Duration, GameEvents<E>)>>>;

pub struct GameEventGenerator<E: DeserializeOwned> {
    pub events: Events<E>,
    pub has_events: Arc<AtomicBool>,
}

impl<E: DeserializeOwned> GameEventGenerator<E> {
    pub fn new(has_events: Arc<AtomicBool>) -> Self {
        GameEventGenerator {
            events: Default::default(),
            has_events,
        }
    }
}

#[async_trait]
impl<E: DeserializeOwned + Sync + Send> NetworkEventToGameEventGenerator for GameEventGenerator<E> {
    async fn generate_from_binary(
        &self,
        timestamp: Duration,
        con_id: &NetworkConnectionId,
        bytes: &[u8],
    ) {
        let msg = bincode::serde::decode_from_slice::<E, _>(
            bytes,
            bincode::config::standard().with_limit::<{ 1024 * 1024 * 4 }>(),
        );
        match msg {
            Ok((msg, _)) => {
                self.events.lock().await.push_back((
                    *con_id,
                    timestamp,
                    GameEvents::NetworkMsg(msg),
                ));
                self.has_events
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
            Err(err) => {
                log::debug!("failed to decode msg {err}");
            }
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
                    GameEvents::NetworkEvent(network_event.clone()),
                ));
            }
        }
        self.has_events
            .store(true, std::sync::atomic::Ordering::Relaxed);
        true
    }
}
