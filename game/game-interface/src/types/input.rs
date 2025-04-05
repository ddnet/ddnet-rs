pub mod cursor;
pub mod dyn_cam;
pub mod viewport;

use std::{
    marker::PhantomData,
    num::{NonZeroI64, NonZeroU64},
    ops::{AddAssign, Deref},
};

use bitflags::bitflags;
use cursor::CharacterInputCursor;
use dyn_cam::CharacterInputDynCamOffset;
use either::Either;
use hiarc::Hiarc;
use serde::{Deserialize, Serialize};
use viewport::CharacterInputViewport;

use super::weapons::WeaponType;

#[derive(Debug, Hiarc, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputVarConsumable<V> {
    val: V,
}

impl<V: PartialEq + AddAssign<V>> InputVarConsumable<V> {
    pub fn add(&mut self, val: V) {
        self.val += val;
    }
}

/// Some input is positioned by a cursor
#[derive(Debug, Hiarc, Copy, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PositionedInputVarConsumable<V> {
    val: InputVarConsumable<V>,
    cursor: CharacterInputCursor,
}

impl<V: PartialEq + AddAssign<V>> PositionedInputVarConsumable<V> {
    pub fn add(&mut self, val: V, at_cursor: CharacterInputCursor) {
        self.val.add(val);
        self.cursor = at_cursor;
    }
}

#[derive(Debug, Hiarc, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputVarState<V> {
    val: V,
}

impl<V: PartialEq> InputVarState<V> {
    pub fn set(&mut self, val: V) {
        if val != self.val {
            self.val = val;
        }
    }
}

impl<V> Deref for InputVarState<V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        &self.val
    }
}

#[derive(Debug, Hiarc, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterInputConsumableDiff {
    pub jump: Option<NonZeroU64>,
    pub fire: Option<(NonZeroU64, CharacterInputCursor)>,
    pub hook: Option<(NonZeroU64, CharacterInputCursor)>,
    pub weapon_req: Option<WeaponType>,
    pub weapon_diff: Option<NonZeroI64>,

    // don't allow contructing outside of this file
    _prevent: PhantomData<()>,
}

/// To get const size for the weapon request,
/// use a wrapper that serializes it to the "same"
/// thing.
#[derive(Debug, Hiarc, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct WeaponReq(pub Option<WeaponType>);

impl Serialize for WeaponReq {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let val = match self.0 {
            Some(val) => Either::Right(val),
            None => Either::Left(WeaponType::default()),
        };

        val.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WeaponReq {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match <Either<WeaponType, WeaponType>>::deserialize(deserializer) {
            Ok(val) => Ok(match val {
                Either::Left(_) => Self(None),
                Either::Right(val) => Self(Some(val)),
            }),
            Err(err) => Err(err),
        }
    }
}

impl Deref for WeaponReq {
    type Target = Option<WeaponType>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Option<WeaponType>> for WeaponReq {
    fn from(value: Option<WeaponType>) -> Self {
        Self(value)
    }
}

#[derive(Debug, Hiarc, Copy, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CharacterInputConsumable {
    pub jump: InputVarConsumable<u64>,
    pub fire: PositionedInputVarConsumable<u64>,
    pub hook: PositionedInputVarConsumable<u64>,
    /// Weapon requests are versioned and stateful at the same time.
    /// This is why direct access to it is not given.
    weapon_req: InputVarState<(u64, WeaponReq)>,
    pub weapon_diff: InputVarConsumable<i64>,
}

impl CharacterInputConsumable {
    /// Create the difference between two consumable input states.
    /// The difference means, the amount of clicks that happened etc.
    pub fn diff(&self, other: &Self) -> CharacterInputConsumableDiff {
        let jump = self.jump.val.saturating_sub(other.jump.val);
        let fire = self.fire.val.val.saturating_sub(other.fire.val.val);
        let hook = self.hook.val.val.saturating_sub(other.hook.val.val);
        let weapon_req = self.weapon_req.val != other.weapon_req.val;
        let weapon_diff = self.weapon_diff.val.saturating_sub(other.weapon_diff.val);

        CharacterInputConsumableDiff {
            jump: if jump == 0 {
                None
            } else {
                Some(NonZeroU64::new(jump).unwrap())
            },
            fire: if fire == 0 {
                None
            } else {
                Some((NonZeroU64::new(fire).unwrap(), self.fire.cursor))
            },
            hook: if hook == 0 {
                None
            } else {
                Some((NonZeroU64::new(hook).unwrap(), self.hook.cursor))
            },
            weapon_req: weapon_req.then_some(*self.weapon_req.val.1).flatten(),
            weapon_diff: if weapon_diff == 0 {
                None
            } else {
                Some(NonZeroI64::new(weapon_diff).unwrap())
            },

            _prevent: Default::default(),
        }
    }

    pub fn set_weapon_req(&mut self, val: Option<WeaponType>) {
        let (version, _) = *self.weapon_req;
        self.weapon_req.set((version + 1, val.into()))
    }

    /// weapon diff also needs special treatment to prevent sending too much input.
    pub fn only_weapon_diff_changed(&mut self, other: &Self) -> bool {
        let diff = self.diff(other);

        diff.jump.is_none()
            && diff.fire.is_none()
            && diff.hook.is_none()
            && diff.weapon_req.is_none()
            && diff.weapon_diff.is_some()
    }
}

#[derive(Debug, Hiarc, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterInputFlags(u64);
bitflags! {
    impl CharacterInputFlags: u64 {
        const HOOK_COLLISION_LINE = (1 << 0);
        const CHATTING = (1 << 1);
        const SCOREBOARD = (1 << 2);
        const MENU_UI = (1 << 3);
    }
}

#[derive(Debug, Hiarc, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterInputMethodFlags(u64);
bitflags! {
    impl CharacterInputMethodFlags: u64 {
        const MOUSE_KEYBOARD = (1 << 0);
        const CONTROLLER = (1 << 1);
        const TOUCHSCREEN = (1 << 2);
        /// Generated by dummy input
        const DUMMY = (1 << 3);
    }
}

#[derive(Debug, Hiarc, Copy, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CharacterInputState {
    pub dir: InputVarState<i32>,
    pub hook: InputVarState<bool>,
    pub fire: InputVarState<bool>,
    pub jump: InputVarState<bool>,
    pub flags: InputVarState<CharacterInputFlags>,
    pub input_method_flags: InputVarState<CharacterInputMethodFlags>,
}

/// character input splits into two categories:
/// - consumable input: these inputs are private and can only be queried by
///   comparing it to another input. They represent an input event
///   (was fired, has jumped, was weapon changed etc.)
/// - stateful input: these inputs are like a current state of the input and
///   can be queried all the time (current cursor, hold hook button, hold fire button etc.)
#[derive(Debug, Hiarc, Copy, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CharacterInput {
    pub cursor: InputVarState<CharacterInputCursor>,
    pub viewport: InputVarState<CharacterInputViewport>,
    pub dyn_cam_offset: InputVarState<CharacterInputDynCamOffset>,

    pub state: CharacterInputState,
    pub consumable: CharacterInputConsumable,
}

/// When a the character input is overriden, this is the object
/// the client & server passes in.
#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct CharacterInputInfo {
    /// The current character's input
    pub inp: CharacterInput,
    /// The difference compared to the previous input,
    /// which are the actions that happened compared to the previous input
    /// (e.g. how often the player fired)
    pub diff: CharacterInputConsumableDiff,
}
