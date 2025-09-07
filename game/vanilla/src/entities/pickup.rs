pub mod pickup {
    use crate::{
        entities::character::character::WeaponsExt,
        reusable::{CloneWithCopyableElements, ReusableCore},
    };
    use base::linked_hash_map_view::FxLinkedHashMap;
    use game_interface::{
        events::{
            GameBuffNinjaEventSound, GameBuffSoundEvent, GameCharacterSoundEvent,
            GameGrenadeEventSound, GameLaserEventSound, GamePickupArmorEventSound,
            GamePickupHeartEventSound, GamePickupSoundEvent, GameShotgunEventSound,
            GameWorldEntitySoundEvent,
        },
        types::{id_types::PickupId, pickup::PickupType, weapons::WeaponType},
    };
    use hiarc::Hiarc;
    use math::math::{lerp, vector::vec2};
    use pool::{datatypes::PoolFxLinkedHashMap, pool::Pool, recycle::Recycle, traits::Recyclable};
    use serde::{Deserialize, Serialize};

    use crate::{
        entities::entity::entity::{DropMode, Entity, EntityInterface, EntityTickResult},
        events::events::PickupEvent,
        simulation_pipe::simulation_pipe::{
            GameWorldPendingEvents, SimulationEventWorldEntityType, SimulationPipePickup,
            SimulationWorldEvents,
        },
        weapons::definitions::weapon_def::Weapon,
        world::world::GameWorld,
    };

    #[derive(Debug, Hiarc, Default, Serialize, Deserialize)]
    pub struct PickupReusableCore {}

    impl Recyclable for PickupReusableCore {
        fn new() -> Self {
            Self {}
        }

        fn reset(&mut self) {}
    }

    impl CloneWithCopyableElements for PickupReusableCore {
        fn copy_clone_from(&mut self, _other: &Self) {}
    }

    impl ReusableCore for PickupReusableCore {}

    pub type PoolPickupReusableCore = Recycle<PickupReusableCore>;

    #[derive(Debug, Hiarc, Copy, Clone, Serialize, Deserialize)]
    pub struct PickupCore {
        pub pos: vec2,
        pub ty: PickupType,
    }

    #[derive(Debug, Hiarc)]
    pub struct Pickup {
        pub(crate) base: Entity<PickupId>,
        pub(crate) core: PickupCore,
        pub(crate) reusable_core: PoolPickupReusableCore,

        game_pending_events: GameWorldPendingEvents,
        simulation_events: SimulationWorldEvents,
    }

    impl Pickup {
        pub fn new(
            game_el_id: &PickupId,
            pos: &vec2,
            ty: PickupType,
            pool: &PickupPool,
            game_pending_events: &GameWorldPendingEvents,
            simulation_events: &SimulationWorldEvents,
        ) -> Self {
            let spawn_effect = || {
                game_pending_events.push_sound(
                    None,
                    Some(*pos),
                    match ty {
                        PickupType::PowerupHealth => return,
                        PickupType::PowerupArmor => return,
                        PickupType::PowerupNinja => return,
                        PickupType::PowerupWeapon(weapon_type) => match weapon_type {
                            WeaponType::Hammer => return,
                            WeaponType::Gun => return,
                            WeaponType::Shotgun => {
                                GameWorldEntitySoundEvent::Shotgun(GameShotgunEventSound::Spawn)
                            }
                            WeaponType::Grenade => {
                                GameWorldEntitySoundEvent::Grenade(GameGrenadeEventSound::Spawn)
                            }
                            WeaponType::Laser => {
                                GameWorldEntitySoundEvent::Laser(GameLaserEventSound::Spawn)
                            }
                        },
                    },
                )
            };
            spawn_effect();

            Self {
                base: Entity::new(game_el_id),
                core: PickupCore { pos: *pos, ty },
                reusable_core: pool.pickup_reusable_cores_pool.new(),

                game_pending_events: game_pending_events.clone(),
                simulation_events: simulation_events.clone(),
            }
        }

        pub fn lerped_pos(pickup1: &Pickup, pickup2: &Pickup, ratio: f64) -> vec2 {
            lerp(&pickup1.core.pos, &pickup2.core.pos, ratio as f32)
        }
    }

    impl EntityInterface<PickupCore, PickupReusableCore, SimulationPipePickup<'_>> for Pickup {
        fn pre_tick(&mut self, _pipe: &mut SimulationPipePickup) -> EntityTickResult {
            todo!()
        }

        fn tick(&mut self, pipe: &mut SimulationPipePickup) -> EntityTickResult {
            let intersection = GameWorld::intersect_character(
                pipe.field,
                pipe.characters.characters_mut(),
                &self.core.pos,
                20,
            );

            if let Some(char) = intersection {
                // player picked us up, is someone was hooking us, let them go
                // TODO: magic constants
                match self.core.ty {
                    PickupType::PowerupHealth => {
                        if char.core.health < 10 {
                            char.core.health += 1;
                            self.game_pending_events.push_sound(
                                Some(char.base.game_element_id),
                                Some(self.core.pos),
                                GameWorldEntitySoundEvent::Pickup(GamePickupSoundEvent::Heart(
                                    GamePickupHeartEventSound::Collect,
                                )),
                            );
                            self.simulation_events.push_world(
                                SimulationEventWorldEntityType::Pickup {
                                    id: self.base.game_element_id,
                                    ev: PickupEvent::Pickup {
                                        pos: self.core.pos,
                                        by: char.base.game_element_id,
                                        ty: PickupType::PowerupHealth,
                                    },
                                },
                            );
                            EntityTickResult::RemoveEntity
                        } else {
                            EntityTickResult::None
                        }
                    }
                    PickupType::PowerupArmor => {
                        if char.core.armor < 10 {
                            char.core.armor += 1;
                            self.game_pending_events.push_sound(
                                Some(char.base.game_element_id),
                                Some(self.core.pos),
                                GameWorldEntitySoundEvent::Pickup(GamePickupSoundEvent::Armor(
                                    GamePickupArmorEventSound::Collect,
                                )),
                            );
                            self.simulation_events.push_world(
                                SimulationEventWorldEntityType::Pickup {
                                    id: self.base.game_element_id,
                                    ev: PickupEvent::Pickup {
                                        pos: self.core.pos,
                                        by: char.base.game_element_id,
                                        ty: PickupType::PowerupArmor,
                                    },
                                },
                            );
                            EntityTickResult::RemoveEntity
                        } else {
                            EntityTickResult::None
                        }
                    }
                    PickupType::PowerupWeapon(weapon) => {
                        let res = if let Some(weapon) = char.reusable_core.weapons.get_mut(&weapon)
                        {
                            // check if ammo can be refilled
                            if weapon.cur_ammo.is_some_and(|val| val < 10) {
                                weapon.cur_ammo = Some(10);
                                EntityTickResult::RemoveEntity
                            } else {
                                EntityTickResult::None
                            }
                        }
                        // else add the weapon
                        else {
                            char.reusable_core.weapons.insert_sorted(
                                weapon,
                                Weapon {
                                    cur_ammo: Some(10),
                                    next_ammo_regeneration_tick: 0.into(),
                                },
                            );
                            EntityTickResult::RemoveEntity
                        };

                        if res == EntityTickResult::RemoveEntity {
                            if let Some(ev) = match weapon {
                                WeaponType::Hammer | WeaponType::Gun => None,
                                WeaponType::Shotgun => Some(GameWorldEntitySoundEvent::Shotgun(
                                    GameShotgunEventSound::Collect,
                                )),
                                WeaponType::Grenade => Some(GameWorldEntitySoundEvent::Grenade(
                                    GameGrenadeEventSound::Collect,
                                )),
                                WeaponType::Laser => Some(GameWorldEntitySoundEvent::Laser(
                                    GameLaserEventSound::Collect,
                                )),
                            } {
                                self.game_pending_events.push_sound(
                                    Some(char.base.game_element_id),
                                    Some(self.core.pos),
                                    ev,
                                );
                            }
                            self.simulation_events.push_world(
                                SimulationEventWorldEntityType::Pickup {
                                    id: self.base.game_element_id,
                                    ev: PickupEvent::Pickup {
                                        pos: self.core.pos,
                                        by: char.base.game_element_id,
                                        ty: PickupType::PowerupWeapon(weapon),
                                    },
                                },
                            );
                        }
                        res
                    }
                    PickupType::PowerupNinja => {
                        // activate ninja on target player
                        self.game_pending_events.push_sound(
                            Some(char.base.game_element_id),
                            Some(self.core.pos),
                            GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Buff(
                                GameBuffSoundEvent::Ninja(GameBuffNinjaEventSound::Collect),
                            )),
                        );
                        self.simulation_events
                            .push_world(SimulationEventWorldEntityType::Pickup {
                                id: self.base.game_element_id,
                                ev: PickupEvent::Pickup {
                                    pos: self.core.pos,
                                    by: char.base.game_element_id,
                                    ty: PickupType::PowerupNinja,
                                },
                            });
                        char.give_ninja();
                        EntityTickResult::RemoveEntity
                    }
                }
            } else {
                EntityTickResult::None
            }
        }

        fn tick_deferred(&mut self, _pipe: &mut SimulationPipePickup) -> EntityTickResult {
            EntityTickResult::None
        }

        fn drop_mode(&mut self, mode: DropMode) {
            self.base.drop_mode = mode;
        }
    }

    impl Drop for Pickup {
        fn drop(&mut self) {
            if matches!(self.base.drop_mode, DropMode::None) {
                self.simulation_events
                    .push_world(SimulationEventWorldEntityType::Pickup {
                        id: self.base.game_element_id,
                        ev: PickupEvent::Despawn {
                            pos: self.core.pos,
                            ty: self.core.ty,
                            respawns_in_ticks: 0.into(),
                        },
                    });
            }
        }
    }

    #[derive(Debug, Hiarc, Clone)]
    pub struct PickupPool {
        pub(crate) pickup_pool: Pool<PoolPickups>,
        pub(crate) pickup_reusable_cores_pool: Pool<PickupReusableCore>,
    }

    pub type PoolPickups = FxLinkedHashMap<PickupId, Pickup>;
    pub type Pickups = PoolFxLinkedHashMap<PickupId, Pickup>;
}
