use std::{fmt::Display, str::FromStr};

use hiarc::{Hiarc, hiarc_safer_rc_refcell};
use serde::{Deserialize, Serialize};

/// This represents the id of any kind of identifiable resource
/// be it characters, stages, projectiles etc.
///
/// Note: It is purposely not copyable for debug reasons, even if it release it is
/// a simple u64. Just let the compiler optimize this
#[derive(
    Debug, Hiarc, Serialize, Deserialize, PartialEq, Eq, Copy, Clone, Hash, PartialOrd, Ord,
)]
pub struct IdGeneratorIdType(u64);

impl Display for IdGeneratorIdType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for IdGeneratorIdType {
    type Err = <u64 as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse()?))
    }
}

#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc)]
pub struct IdGenerator {
    cur_id: IdGeneratorIdType,
    rev: bool,
}

#[hiarc_safer_rc_refcell]
impl Default for IdGenerator {
    fn default() -> Self {
        Self {
            cur_id: IdGeneratorIdType(0),
            rev: false,
        }
    }
}

#[hiarc_safer_rc_refcell]
impl IdGenerator {
    pub fn new() -> Self {
        Self::default()
    }

    /// This should only be used for components in the client
    /// to generate fake ids.
    pub fn reverse(&mut self) {
        self.rev = true;
        self.cur_id = IdGeneratorIdType(u64::MAX);
    }

    /// generate the next unique id of this generator
    pub fn next_id<T: From<IdGeneratorIdType>>(&mut self) -> T {
        let cur = self.cur_id;
        if self.rev {
            self.cur_id.0 -= 1;
        } else {
            self.cur_id.0 += 1;
        }
        cur.into()
    }

    /// Reset the id of the id generator to this id.
    /// This should only be called if syncing the id
    /// is explicitly requested, most commonly by the client.
    pub fn reset_id_for_client(&mut self, next_id: IdGeneratorIdType) {
        self.cur_id = next_id;
    }

    /// Get the next unique id without
    /// advancing the internal id tracker.
    /// This is useful to sync the id
    /// with the client over
    /// [`IdGenerator::reset_id_for_client`]
    #[must_use]
    pub fn peek_next_id(&self) -> IdGeneratorIdType {
        self.cur_id
    }
}
