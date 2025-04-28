#![allow(clippy::multiple_bound_locations)]

use std::{collections::VecDeque, net::SocketAddr};

use hiarc::{hiarc_safer_arc_mutex, Hiarc};

#[derive(Debug, Clone, Hiarc)]
pub enum ConnectModes {
    Connecting { addr: SocketAddr },
    Queue { msg: String },
    ConnectingErr { msg: String },
    DisconnectErr { msg: String },
}

#[hiarc_safer_arc_mutex]
#[derive(Debug, Default, Hiarc)]
pub struct ConnectingLog {
    log: VecDeque<String>,
    mode: Option<ConnectModes>,
}

#[hiarc_safer_arc_mutex]
impl ConnectingLog {
    pub fn log<S>(&mut self, s: S)
    where
        S: Into<String>,
    {
        self.log.truncate(200);
        self.log.push_front(s.into());
    }

    /// Latest logs starting with the oldest log entry
    pub fn logs(&self) -> Vec<String> {
        self.log.iter().cloned().rev().collect()
    }

    pub fn clear(&mut self) {
        self.log.clear();
        self.mode = None;
    }

    pub fn set_mode(&mut self, mode: ConnectModes) {
        self.mode = Some(mode);
    }

    pub fn mode(&self) -> Option<ConnectModes> {
        self.mode.clone()
    }
}
