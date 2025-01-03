use hiarc::Hiarc;
use math::math::vector::vec2;
use serde::{Deserialize, Serialize};

use crate::types::{id_types::CharacterId, weapons::WeaponType};

#[derive(
    Debug, Hiarc, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord,
)]
pub enum WeaponWithProjectile {
    Gun,
    Shotgun,
    Grenade,
}

impl From<WeaponWithProjectile> for WeaponType {
    fn from(value: WeaponWithProjectile) -> Self {
        match value {
            WeaponWithProjectile::Gun => Self::Gun,
            WeaponWithProjectile::Shotgun => Self::Shotgun,
            WeaponWithProjectile::Grenade => Self::Grenade,
        }
    }
}

/// The ingame metric is 1 tile = 1.0 float units
#[derive(Debug, Hiarc, Copy, Clone, Serialize, Deserialize)]
pub struct ProjectileRenderInfo {
    pub ty: WeaponWithProjectile,
    pub pos: vec2,
    pub vel: vec2,

    /// If this entity is owned by a character, this should be `Some` and
    /// include the characters id.
    pub owner_id: Option<CharacterId>,

    /// Whether the entity is phased, e.g. cannot hit any entitiy
    /// except the owner.
    ///
    /// In ddrace this is solo.
    #[doc(alias = "solo")]
    pub phased: bool,
}
