use std::net::SocketAddr;

use game_config::config::Config;
use hiarc::{hiarc_safer_rc_refcell, Hiarc};

use crate::events::UiEvents;

#[derive(Debug, Clone, Hiarc)]
pub enum ConnectModes {
    Connecting { addr: SocketAddr },
    Queue { msg: String },
    ConnectingErr { msg: String },
    DisconnectErr { msg: String },
}

#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc)]
pub struct ConnectMode {
    mode: ConnectModes,
}

#[hiarc_safer_rc_refcell]
impl ConnectMode {
    pub fn new(mode: ConnectModes) -> Self {
        Self { mode }
    }

    pub fn set(&mut self, mode: ConnectModes) {
        self.mode = mode;
    }

    pub fn get(&self) -> ConnectModes {
        self.mode.clone()
    }
}

pub struct UserData<'a> {
    pub mode: &'a ConnectMode,
    pub config: &'a mut Config,
    pub events: &'a UiEvents,
}
