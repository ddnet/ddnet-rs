use std::{fmt::Display, time::Duration};

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct NetworkConnectionId {
    id: u64,
    ty: u32,
}

impl Display for NetworkConnectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl NetworkConnectionId {
    pub(crate) fn new(id: u64, ty: u32) -> Self {
        Self { id, ty }
    }

    pub(crate) fn ty(&self) -> u32 {
        self.ty
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConnectionStats {
    pub ping: Duration,
    pub packets_lost: u64,
    pub packets_sent: u64,
    pub bytes_sent: u64,
    pub bytes_recv: u64,
}

#[derive(Debug)]
pub(crate) struct NetworkConnection<C: Send + Sync> {
    pub(crate) conn: C,
}
