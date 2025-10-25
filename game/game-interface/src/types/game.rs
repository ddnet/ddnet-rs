use std::{
    num::NonZeroU64,
    ops::{Deref, DerefMut},
};

use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

use super::id_gen::IdGeneratorIdType;

/// The game element id is a unique identifier to help
/// creating a connecting from a network object and the actual game object
/// it should be unique per type
pub type GameEntityId = IdGeneratorIdType;

pub type GameTickType = u64;
pub type NonZeroGameTickType = NonZeroU64;

/// A counter that helps counting tick events down
/// E.g. if a character has a recoil cooldown
#[derive(Debug, Hiarc, Default, Clone, Copy, Serialize, Deserialize)]
pub struct GameTickCooldown(Option<NonZeroGameTickType>);

impl GameTickCooldown {
    /// if `ticks` is zero that basically means that there is no cooldown,
    /// which also is the default.
    pub fn new(ticks: GameTickType) -> Self {
        Self((ticks > 0).then(|| NonZeroGameTickType::new(ticks).unwrap()))
    }

    /// Returns `Some` if there were ticks left, where the inner value
    /// indicates if the ticks just fell to zero.
    /// Returns `None` if there were was no cooldown in first place.
    /// Since it's mostly interesting if the cooldown fell to zero,
    /// you can use:
    /// ```no_run
    /// if cooldown.tick().unwrap_or_default() {
    ///     // you logic because cooldown fell to zero
    /// }
    /// ```
    pub fn tick(&mut self) -> Option<bool> {
        if let Some(ticks) = &mut self.0 {
            let in_ticks = ticks.get() - 1;
            if in_ticks == 0 {
                self.0 = None;
                Some(true)
            } else {
                *ticks = NonZeroGameTickType::new(in_ticks).unwrap();
                Some(false)
            }
        } else {
            None
        }
    }

    /// is a cooldown active
    pub fn is_some(&self) -> bool {
        self.0.is_some()
    }

    /// is NO cooldown active
    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }

    /// the raw cooldown value, or `None`
    pub fn get(&self) -> Option<NonZeroGameTickType> {
        self.0
    }
}

impl From<GameTickType> for GameTickCooldown {
    fn from(value: GameTickType) -> Self {
        Self::new(value)
    }
}

/// Like [`GameTickCooldown`] but also stores the original countdown start length.
#[derive(Debug, Hiarc, Default, Clone, Copy, Serialize, Deserialize)]
pub struct GameTickCooldownAndLength {
    countdown: GameTickCooldown,
    length: Option<NonZeroGameTickType>,
}

impl GameTickCooldownAndLength {
    /// if `ticks` is zero that basically means that there is no cooldown,
    /// which also is the default.
    pub fn new(ticks: GameTickType) -> Self {
        let length = (ticks > 0).then(|| NonZeroGameTickType::new(ticks).unwrap());
        Self {
            countdown: ticks.into(),
            length,
        }
    }

    /// Like [`Self::new`], but takes the length explictly.
    pub fn new_with_length(ticks: GameTickType, length: GameTickType) -> Self {
        let length = (length > 0).then(|| NonZeroGameTickType::new(length).unwrap());
        Self {
            countdown: ticks.into(),
            length,
        }
    }

    pub fn length(&self) -> Option<NonZeroGameTickType> {
        self.length
    }
}

impl From<GameTickType> for GameTickCooldownAndLength {
    fn from(value: GameTickType) -> Self {
        Self::new(value)
    }
}

impl Deref for GameTickCooldownAndLength {
    type Target = GameTickCooldown;

    fn deref(&self) -> &Self::Target {
        &self.countdown
    }
}

impl DerefMut for GameTickCooldownAndLength {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.countdown
    }
}

/// An extension to [GameTickCooldown] that additionally to
/// tracking a cooldown also counts how many ticks passed
/// since the last cooldown was __activated/created__.
#[derive(Debug, Hiarc, Serialize, Deserialize, Copy, Clone)]
pub enum GameTickCooldownAndLastActionCounter {
    None,
    Cooldown {
        ticks_left: NonZeroGameTickType,
        ticks_passed: GameTickType,
        initial_cooldown_len: NonZeroGameTickType,
    },
    LastActionCounter {
        ticks_passed: GameTickType,
        last_cooldown_len: NonZeroGameTickType,
    },
}

impl Default for GameTickCooldownAndLastActionCounter {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Hiarc)]
pub enum GameTickCooldownAndLastActionCounterTickRes {
    None,
    CooldownFellToZero,
    CooldownActive,
    CounterActive,
}

impl GameTickCooldownAndLastActionCounterTickRes {
    /// cooldown fell to zero or neither cooldown nor
    /// last action counter are active
    pub fn cooldown_fell_to_zero_or_none(&self) -> bool {
        matches!(
            self,
            GameTickCooldownAndLastActionCounterTickRes::None
                | GameTickCooldownAndLastActionCounterTickRes::CooldownFellToZero
        )
    }
    /// cooldown fell to zero
    pub fn cooldown_fell_to_zero(&self) -> bool {
        matches!(
            self,
            GameTickCooldownAndLastActionCounterTickRes::CooldownFellToZero
        )
    }
}

impl GameTickCooldownAndLastActionCounter {
    /// If `ticks` is zero that means that there is no cooldown
    /// and no action was performed yet, which also is the default.
    /// Otherwise a cooldown is created and last action ticks are counted.
    pub fn new(ticks: GameTickType) -> Self {
        if ticks > 0 {
            let cooldown = NonZeroGameTickType::new(ticks).unwrap();
            Self::Cooldown {
                ticks_left: cooldown,
                ticks_passed: 0,
                initial_cooldown_len: cooldown,
            }
        } else {
            Self::None
        }
    }

    /// Returns [GameTickCooldownAndLastActionCounterTickRes].
    /// Since it's mostly interesting if the cooldown fell to zero,
    /// or no cooldown or action counter was active, you can use:
    /// ```no_run
    /// if cooldown.tick().cooldown_fell_to_zero_or_none() {
    ///     // you logic because cooldown fell to zero
    /// }
    /// ```
    pub fn tick(&mut self) -> GameTickCooldownAndLastActionCounterTickRes {
        match self {
            GameTickCooldownAndLastActionCounter::None => {
                GameTickCooldownAndLastActionCounterTickRes::None
            }
            GameTickCooldownAndLastActionCounter::Cooldown {
                ticks_left,
                ticks_passed,
                initial_cooldown_len,
            } => {
                *ticks_passed += 1;

                let in_ticks = ticks_left.get() - 1;
                if in_ticks == 0 {
                    *self = Self::LastActionCounter {
                        ticks_passed: *ticks_passed,
                        last_cooldown_len: *initial_cooldown_len,
                    };
                    GameTickCooldownAndLastActionCounterTickRes::CooldownFellToZero
                } else {
                    *ticks_left = NonZeroGameTickType::new(in_ticks).unwrap();
                    GameTickCooldownAndLastActionCounterTickRes::CooldownActive
                }
            }
            GameTickCooldownAndLastActionCounter::LastActionCounter { ticks_passed, .. } => {
                *ticks_passed += 1;
                GameTickCooldownAndLastActionCounterTickRes::CounterActive
            }
        }
    }

    /// is a cooldown active
    pub fn is_some(&self) -> bool {
        matches!(self, Self::Cooldown { .. })
    }

    /// is neither cooldown nor last action counter active
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// if an last action counter is active
    /// returns the amount of ticks passed
    /// since then
    pub fn action_ticks(self) -> Option<u64> {
        match self {
            Self::None => None,
            Self::Cooldown { ticks_passed, .. } | Self::LastActionCounter { ticks_passed, .. } => {
                Some(ticks_passed)
            }
        }
    }

    /// if a tick counter is active, advance this counter
    /// to the current given count without modifying
    /// the cooldown counter.
    /// This is useful if a forced weapon switch happend
    /// (e.g. ninja collected / lost)
    pub fn advance_ticks_passed_to(&mut self, to: u64) {
        match self {
            GameTickCooldownAndLastActionCounter::None => {}
            GameTickCooldownAndLastActionCounter::Cooldown { ticks_passed, .. }
            | GameTickCooldownAndLastActionCounter::LastActionCounter { ticks_passed, .. } => {
                if *ticks_passed < to {
                    *ticks_passed = to;
                }
            }
        }
    }

    /// How many ticks passed relative and the cooldown length
    pub fn action_ticks_and_cooldown_len(self) -> Option<(GameTickType, NonZeroGameTickType)> {
        match self {
            Self::None => None,
            Self::Cooldown {
                ticks_passed,
                initial_cooldown_len: cooldown_len,
                ..
            }
            | Self::LastActionCounter {
                ticks_passed,
                last_cooldown_len: cooldown_len,
                ..
            } => Some((ticks_passed, cooldown_len)),
        }
    }
}

impl From<GameTickType> for GameTickCooldownAndLastActionCounter {
    fn from(value: GameTickType) -> Self {
        Self::new(value)
    }
}
