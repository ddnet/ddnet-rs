use std::{collections::BTreeMap, time::Duration};

use game_interface::types::{id_types::PlayerId, player_info::PlayerUniqueId};
use hiarc::{Hiarc, hiarc_safer_rc_refcell};
use serde::{Deserialize, Serialize};

#[derive(Debug, Hiarc, Default, Clone, Serialize, Deserialize)]
pub struct MicrophoneDevices {
    pub devices: Vec<String>,
    pub default: Option<String>,
}

#[derive(Debug, Hiarc, Default, Clone, Serialize, Deserialize)]
pub struct MicrophoneHosts {
    pub hosts: BTreeMap<String, MicrophoneDevices>,
    pub default: String,
}

#[derive(Debug, Hiarc, Clone, Copy)]
pub struct SliderValue {
    pub val: f64,
    pub changed_at: Duration,
}

impl Default for SliderValue {
    fn default() -> Self {
        Self {
            val: 0.0,
            changed_at: Duration::MAX,
        }
    }
}

#[derive(Debug, Hiarc, Clone, Copy)]
pub enum EntitiesEvent {
    Mute(PlayerId),
    Unmute(PlayerId),
}

#[derive(Debug, Hiarc, Clone)]
pub struct SpatialChatEntity {
    pub unique_id: PlayerUniqueId,
    pub name: String,
}

#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc, Default)]
pub struct SpatialChat {
    is_active: bool,
    changed: bool,

    hosts: MicrophoneHosts,

    server_support: bool,

    loudest: f32,

    attenuation: SliderValue,
    processing_threshold: SliderValue,
    boost: SliderValue,

    gate_open: SliderValue,
    gate_close: SliderValue,

    entities: BTreeMap<PlayerId, SpatialChatEntity>,
    entities_events: Vec<EntitiesEvent>,
}

#[hiarc_safer_rc_refcell]
impl SpatialChat {
    /// For the ui part
    pub fn set_active(&mut self) {
        self.is_active = true;
    }

    /// Once per frame this should be called exactly once.
    /// Calling it twice or more per frame is a bug.
    pub fn is_active(&mut self) -> bool {
        std::mem::replace(&mut self.is_active, false)
    }

    pub fn set_changed(&mut self) {
        self.changed = true;
    }

    pub fn support(&mut self, server_support: bool) {
        self.server_support = server_support;
    }

    pub fn get_support(&self) -> bool {
        self.server_support
    }

    /// Settings changed and the stream should be reinitialized.
    /// Only call once per frame, else bug.
    pub fn has_changed(&mut self) -> bool {
        std::mem::replace(&mut self.changed, false)
    }

    pub fn should_fill_hosts(&mut self) -> bool {
        self.hosts.hosts.is_empty()
    }

    pub fn fill_hosts(&mut self, hosts: MicrophoneHosts) {
        self.hosts = hosts;
    }

    pub fn get_hosts(&mut self) -> MicrophoneHosts {
        self.hosts.clone()
    }

    pub fn set_loudest(&mut self, db: f32) {
        self.loudest = db;
    }
    pub fn get_loudest(&mut self) -> f32 {
        self.loudest
    }

    pub fn set_gate_open_slider(&mut self, val: SliderValue) {
        self.gate_open = val;
    }
    pub fn get_gate_open_slider(&mut self) -> SliderValue {
        self.gate_open
    }
    pub fn set_gate_close_slider(&mut self, val: SliderValue) {
        self.gate_close = val;
    }
    pub fn get_gate_close_slider(&mut self) -> SliderValue {
        self.gate_close
    }

    pub fn set_attenuation_slider(&mut self, val: SliderValue) {
        self.attenuation = val;
    }
    pub fn get_attenuation_slider(&mut self) -> SliderValue {
        self.attenuation
    }
    pub fn set_processing_threshold_slider(&mut self, val: SliderValue) {
        self.processing_threshold = val;
    }
    pub fn get_processing_threshold_slider(&mut self) -> SliderValue {
        self.processing_threshold
    }
    pub fn set_boost_slider(&mut self, val: SliderValue) {
        self.boost = val;
    }
    pub fn get_boost_slider(&mut self) -> SliderValue {
        self.boost
    }

    pub fn update_entities(&mut self, entities: BTreeMap<PlayerId, SpatialChatEntity>) {
        self.entities = entities;
    }

    pub fn get_entities(&self) -> BTreeMap<PlayerId, SpatialChatEntity> {
        self.entities.clone()
    }

    pub fn push_entity_event(&mut self, ev: EntitiesEvent) {
        self.entities_events.push(ev);
    }

    pub fn take_entities_events(&mut self) -> Vec<EntitiesEvent> {
        std::mem::take(&mut self.entities_events)
    }
}
