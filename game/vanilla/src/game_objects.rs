pub mod game_objects {
    use game_interface::types::{emoticons::EnumCount, weapons::WeaponType};
    use hiarc::Hiarc;
    use legacy_map::mapdef_06::{DdraceEntityTiles, EntityTiles};
    use map::map::groups::{
        MapGroupPhysics,
        layers::{
            physics::MapLayerPhysics,
            tiles::{TileBase, TileFlags},
        },
    };
    use math::math::vector::ivec2;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Hiarc, Default)]
    pub struct GameObjectsPickupDefinitions<V> {
        pub hearts: Vec<V>,
        pub shields: Vec<V>,

        pub red_flags: Vec<V>,
        pub blue_flags: Vec<V>,

        pub weapons: [Vec<V>; WeaponType::COUNT],
        pub weapon_shields: [Vec<V>; WeaponType::COUNT],

        pub ninjas: Vec<V>,
        pub ninja_shields: Vec<V>,
    }

    #[derive(Debug, Hiarc, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum DdraceMapEntityKind {
        LaserFastCcw,
        LaserNormalCcw,
        LaserSlowCcw,
        LaserStop,
        LaserSlowCw,
        LaserNormalCw,
        LaserFastCw,
        LaserShort,
        LaserMedium,
        LaserLong,
        LaserCSlow,
        LaserCNormal,
        LaserCFast,
        LaserOSlow,
        LaserONormal,
        LaserOFast,
        PlasmaExplosive,
        PlasmaFreeze,
        PlasmaFreezeExplosive,
        Plasma,
        CrazyShotgunExplosive,
        CrazyShotgun,
        DraggerWeak,
        DraggerNormal,
        DraggerStrong,
        DraggerWeakNoWalls,
        DraggerNormalNoWalls,
        DraggerStrongNoWalls,
        Door,
    }

    impl DdraceMapEntityKind {
        pub fn from_tile_index(index: u8) -> Option<Self> {
            Some(match index {
                index if index == DdraceEntityTiles::LaserFastCcw as u8 => Self::LaserFastCcw,
                index if index == DdraceEntityTiles::LaserNormalCcw as u8 => Self::LaserNormalCcw,
                index if index == DdraceEntityTiles::LaserSlowCcw as u8 => Self::LaserSlowCcw,
                index if index == DdraceEntityTiles::LaserStop as u8 => Self::LaserStop,
                index if index == DdraceEntityTiles::LaserSlowCw as u8 => Self::LaserSlowCw,
                index if index == DdraceEntityTiles::LaserNormalCw as u8 => Self::LaserNormalCw,
                index if index == DdraceEntityTiles::LaserFastCw as u8 => Self::LaserFastCw,
                index if index == DdraceEntityTiles::LaserShort as u8 => Self::LaserShort,
                index if index == DdraceEntityTiles::LaserMedium as u8 => Self::LaserMedium,
                index if index == DdraceEntityTiles::LaserLong as u8 => Self::LaserLong,
                index if index == DdraceEntityTiles::LaserCSlow as u8 => Self::LaserCSlow,
                index if index == DdraceEntityTiles::LaserCNormal as u8 => Self::LaserCNormal,
                index if index == DdraceEntityTiles::LaserCFast as u8 => Self::LaserCFast,
                index if index == DdraceEntityTiles::LaserOSlow as u8 => Self::LaserOSlow,
                index if index == DdraceEntityTiles::LaserONormal as u8 => Self::LaserONormal,
                index if index == DdraceEntityTiles::LaserOFast as u8 => Self::LaserOFast,
                index if index == DdraceEntityTiles::PlasmaE as u8 => Self::PlasmaExplosive,
                index if index == DdraceEntityTiles::PlasmaF as u8 => Self::PlasmaFreeze,
                index if index == DdraceEntityTiles::Plasma as u8 => Self::PlasmaFreezeExplosive,
                index if index == DdraceEntityTiles::PlasmaU as u8 => Self::Plasma,
                index if index == DdraceEntityTiles::CrazyShotgunEx as u8 => {
                    Self::CrazyShotgunExplosive
                }
                index if index == DdraceEntityTiles::CrazyShotgun as u8 => Self::CrazyShotgun,
                index if index == DdraceEntityTiles::DraggerWeak as u8 => Self::DraggerWeak,
                index if index == DdraceEntityTiles::DraggerNormal as u8 => Self::DraggerNormal,
                index if index == DdraceEntityTiles::DraggerStrong as u8 => Self::DraggerStrong,
                index if index == DdraceEntityTiles::DraggerWeakNw as u8 => {
                    Self::DraggerWeakNoWalls
                }
                index if index == DdraceEntityTiles::DraggerNormalNw as u8 => {
                    Self::DraggerNormalNoWalls
                }
                index if index == DdraceEntityTiles::DraggerStrongNw as u8 => {
                    Self::DraggerStrongNoWalls
                }
                index if index == DdraceEntityTiles::Door as u8 => Self::Door,
                _ => return None,
            })
        }

        pub fn laser_angular_speed(self) -> Option<f32> {
            let pi = std::f32::consts::PI;
            Some(match self {
                Self::LaserFastCcw => pi / 90.0,
                Self::LaserNormalCcw => pi / 180.0,
                Self::LaserSlowCcw => pi / 360.0,
                Self::LaserStop => 0.0,
                Self::LaserSlowCw => -pi / 360.0,
                Self::LaserNormalCw => -pi / 180.0,
                Self::LaserFastCw => -pi / 90.0,
                _ => return None,
            })
        }

        pub fn laser_length(self) -> Option<f32> {
            Some(match self {
                Self::LaserShort => 96.0,
                Self::LaserMedium => 192.0,
                Self::LaserLong => 288.0,
                _ => return None,
            })
        }

        pub fn laser_curve_speed(self) -> Option<f32> {
            Some(match self {
                Self::LaserCSlow | Self::LaserOSlow => 1.0,
                Self::LaserCNormal | Self::LaserONormal => 2.0,
                Self::LaserCFast | Self::LaserOFast => 3.0,
                _ => return None,
            })
        }

        pub fn laser_curve_starts_open(self) -> bool {
            matches!(
                self,
                Self::LaserOSlow | Self::LaserONormal | Self::LaserOFast
            )
        }

        pub fn dragger_strength(self) -> Option<f32> {
            Some(match self {
                Self::DraggerWeak | Self::DraggerWeakNoWalls => 1.0,
                Self::DraggerNormal | Self::DraggerNormalNoWalls => 2.0,
                Self::DraggerStrong | Self::DraggerStrongNoWalls => 3.0,
                _ => return None,
            })
        }

        pub fn dragger_ignores_walls(self) -> bool {
            matches!(
                self,
                Self::DraggerWeakNoWalls | Self::DraggerNormalNoWalls | Self::DraggerStrongNoWalls
            )
        }

        pub fn plasma_freezes(self) -> bool {
            matches!(self, Self::PlasmaFreeze | Self::PlasmaFreezeExplosive)
        }

        pub fn plasma_unfreezes(self) -> bool {
            matches!(self, Self::Plasma)
        }

        pub fn plasma_explodes(self) -> bool {
            matches!(self, Self::PlasmaExplosive | Self::PlasmaFreezeExplosive)
        }

        pub fn is_crazy_shotgun(self) -> bool {
            matches!(self, Self::CrazyShotgun | Self::CrazyShotgunExplosive)
        }

        pub fn crazy_shotgun_explodes(self) -> bool {
            matches!(self, Self::CrazyShotgunExplosive)
        }
    }

    #[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
    pub struct DdraceMapEntityDefinition<V> {
        pub pos: V,
        pub kind: DdraceMapEntityKind,
        pub flags: TileFlags,
        pub number: Option<u8>,
    }

    /// definitions of game objects, like their spawn position or flags etc.
    #[derive(Debug, Hiarc)]
    pub struct GameObjectDefinitionsBase<V> {
        pub pickups: GameObjectsPickupDefinitions<V>,
        pub ddrace_entities: Vec<DdraceMapEntityDefinition<V>>,
    }

    impl GameObjectDefinitionsBase<ivec2> {
        pub fn new(physics_group: &MapGroupPhysics, race: bool) -> Self {
            let width = physics_group.attr.width.get() as u32;
            let height = physics_group.attr.height.get() as u32;
            let mut pickups = GameObjectsPickupDefinitions::<ivec2>::default();
            let mut ddrace_entities = Vec::new();

            let mut collect_tile = |tile: &TileBase, x: u32, y: u32, number: Option<u8>| {
                if let Some(kind) = DdraceMapEntityKind::from_tile_index(tile.index) {
                    ddrace_entities.push(DdraceMapEntityDefinition {
                        pos: ivec2::new(x as i32, y as i32),
                        kind,
                        flags: tile.flags,
                        number,
                    });
                }

                if number.is_some() {
                    return;
                }

                match tile.index {
                    i if i == EntityTiles::Health as u8 => {
                        pickups.hearts.push(ivec2::new(x as i32, y as i32));
                    }
                    i if i == EntityTiles::Armor as u8 => {
                        pickups.shields.push(ivec2::new(x as i32, y as i32));
                    }
                    i if i == EntityTiles::FlagSpawnRed as u8 => {
                        pickups.red_flags.push(ivec2::new(x as i32, y as i32));
                    }
                    i if i == EntityTiles::FlagSpawnBlue as u8 => {
                        pickups.blue_flags.push(ivec2::new(x as i32, y as i32));
                    }
                    i if i == EntityTiles::WeaponGrenade as u8 => {
                        pickups.weapons[WeaponType::Grenade as usize]
                            .push(ivec2::new(x as i32, y as i32));
                    }
                    i if i == EntityTiles::WeaponLaser as u8 => {
                        pickups.weapons[WeaponType::Laser as usize]
                            .push(ivec2::new(x as i32, y as i32));
                    }
                    i if i == EntityTiles::WeaponShotgunOrPullerInRace as u8 => {
                        let weapon = if race {
                            WeaponType::Puller
                        } else {
                            WeaponType::Shotgun
                        };
                        pickups.weapons[weapon as usize].push(ivec2::new(x as i32, y as i32));
                    }
                    i if i == EntityTiles::PowerupNinja as u8 => {
                        pickups.ninjas.push(ivec2::new(x as i32, y as i32));
                    }
                    i if i == DdraceEntityTiles::ArmorPuller as u8 => {
                        pickups.weapon_shields[WeaponType::Puller as usize]
                            .push(ivec2::new(x as i32, y as i32));
                    }
                    i if i == DdraceEntityTiles::ArmorGrenade as u8 => {
                        pickups.weapon_shields[WeaponType::Grenade as usize]
                            .push(ivec2::new(x as i32, y as i32));
                    }
                    i if i == DdraceEntityTiles::ArmorNinja as u8 => {
                        pickups.ninja_shields.push(ivec2::new(x as i32, y as i32));
                    }
                    i if i == DdraceEntityTiles::ArmorLaser as u8 => {
                        pickups.weapon_shields[WeaponType::Laser as usize]
                            .push(ivec2::new(x as i32, y as i32));
                    }
                    _ => {}
                }
            };

            for layer in &physics_group.layers {
                match layer {
                    MapLayerPhysics::Game(layer) => {
                        for y in 0..height {
                            for x in 0..width {
                                let index = (y * width + x) as usize;
                                collect_tile(&layer.tiles[index], x, y, None);
                            }
                        }
                    }
                    MapLayerPhysics::Front(layer) => {
                        for y in 0..height {
                            for x in 0..width {
                                let index = (y * width + x) as usize;
                                collect_tile(&layer.tiles[index], x, y, None);
                            }
                        }
                    }
                    MapLayerPhysics::Switch(layer) => {
                        for y in 0..height {
                            for x in 0..width {
                                let index = (y * width + x) as usize;
                                let tile = &layer.base.tiles[index];
                                collect_tile(&tile.base, x, y, Some(tile.number));
                            }
                        }
                    }
                    _ => {}
                }
            }
            Self {
                pickups,
                ddrace_entities,
            }
        }
    }

    pub type GameObjectDefinitions = GameObjectDefinitionsBase<ivec2>;
}
