pub mod core;
pub mod hook;
pub mod player;
pub mod pos;
pub mod score;

pub mod character {
    use std::{
        collections::VecDeque,
        marker::PhantomData,
        num::{NonZeroI64, NonZeroU64},
    };

    use crate::reusable::{CloneWithCopyableElements, ReusableCore};
    use base::linked_hash_map_view::{
        FxLinkedHashMap, FxLinkedHashSet, LinkedHashMapView, LinkedHashMapViewMut,
    };
    use game_base::mapdef_06::DdraceTileNum;
    use game_interface::{
        events::{
            GameBuffNinjaEventSound, GameBuffSoundEvent, GameCharacterEffectEvent,
            GameCharacterEventEffect, GameCharacterEventSound, GameCharacterSoundEvent,
            GameWorldAction, GameWorldActionKillWeapon, GameWorldEntityEffectEvent,
            GameWorldEntitySoundEvent, GameWorldEvent, GameWorldNotificationEvent, KillFlags,
        },
        pooling::GamePooling,
        types::{
            emoticons::{EmoticonType, EnumCount},
            game::{GameTickCooldown, GameTickCooldownAndLastActionCounter, GameTickType},
            id_types::{CharacterId, StageId},
            input::{cursor::CharacterInputCursor, CharacterInput, CharacterInputConsumableDiff},
            network_stats::PlayerNetworkStats,
            render::{
                character::{CharacterBuff, CharacterDebuff, TeeEye},
                game::game_match::MatchSide,
                projectiles::WeaponWithProjectile,
            },
            weapons::WeaponType,
        },
    };
    use hiarc::{hiarc_safer_rc_refcell, Hiarc};
    use map::map::groups::layers::tiles::Tile;
    use pool::{datatypes::PoolFxLinkedHashMap, mt_pool::Pool as MtPool};
    use rustc_hash::FxHashSet;

    use super::{
        core::character_core::{Core, CoreEvents, CorePipe, CoreReusable, PHYSICAL_SIZE},
        hook::character_hook::{CharacterHook, Hook, HookedCharacters},
        player::player::{PlayerInfo, Players, SpectatorPlayer, SpectatorPlayers},
        pos::character_pos::{CharacterPos, CharacterPositionPlayfield},
        score::character_score::{CharacterScore, CharacterScores},
    };
    use crate::{
        collision::collision::{Collision, CollisionTile, CollisionTypes, HitTile},
        entities::entity::entity::{DropMode, Entity, EntityInterface, EntityTickResult},
        events::events::{CharacterDespawnType, CharacterEvent, CharacterTickEvent},
        simulation_pipe::simulation_pipe::{
            GameWorldPendingEvents, SimulationEventWorldEntityType, SimulationPipeCharacter,
            SimulationWorldEvents,
        },
        state::state::TICKS_PER_SECOND,
        types::types::GameOptions,
        weapons::definitions::weapon_def::Weapon,
    };

    use math::math::{
        angle, distance_squared, length, lerp, mix, normalize,
        vector::{ivec2, vec2},
        PI,
    };
    use pool::{mt_datatypes::PoolVec, pool::Pool, recycle::Recycle, traits::Recyclable};
    use serde::{Deserialize, Serialize};

    use super::player::player::Player;

    pub const TICKS_UNTIL_RECOIL_ENDED: GameTickType = 7;

    pub enum DamageTypes<'a> {
        Character(&'a CharacterId),
        CharacterInMatchSide {
            char_id: &'a CharacterId,
            side: MatchSide,
        },
    }

    pub enum DamageBy {
        Ninja,
        Weapon {
            weapon: WeaponType,
            flags: KillFlags,
        },
    }

    #[derive(Debug, Hiarc, Serialize, Deserialize, Copy, Clone)]
    pub struct BuffProps {
        pub remaining_tick: GameTickCooldown,
        pub interact_tick: GameTickCooldown,
        pub interact_cursor_dir: vec2,
        pub interact_val: f32,
    }

    #[derive(Debug, Hiarc, Default, Serialize, Deserialize, Copy, Clone)]
    pub struct CharacterCoreMod {}

    #[derive(Debug, Hiarc, Default, Serialize, Deserialize, Copy, Clone)]
    pub struct CharacterCore {
        pub core: Core,
        // vanilla
        pub active_weapon: WeaponType,
        pub prev_weapon: WeaponType,
        pub queued_weapon: Option<WeaponType>,
        pub health: u32,
        pub armor: u32,
        pub attack_recoil: GameTickCooldownAndLastActionCounter,
        pub no_ammo_sound: GameTickCooldown,
        pub last_dmg_indicator: GameTickCooldown,
        pub last_dmg_angle: f32,

        pub emoticon_tick: GameTickCooldownAndLastActionCounter,
        pub cur_emoticon: Option<EmoticonType>,

        pub side: Option<MatchSide>,

        pub eye: TeeEye,
        pub normal_eye_in: GameTickCooldown,

        /// The default eyes from the player info
        pub default_eye: TeeEye,
        pub default_eye_reset_in: GameTickCooldown,

        pub input: CharacterInput,

        /// is timeout e.g. by a network disconnect.
        /// this is a hint, not a logic variable.
        pub is_timeout: bool,

        pub modifications: CharacterCoreMod,
    }

    impl CharacterCore {
        fn get_core_mut_and_input(
            &mut self,
            _reusable_core: &CharacterReusableCore,
        ) -> (&mut Core, &CharacterInput) {
            (&mut self.core, &self.input)
        }
    }

    #[derive(Debug, Hiarc, Serialize, Deserialize, Clone)]
    pub struct CharacterReusableCore {
        pub core: CoreReusable,
        pub weapons: FxLinkedHashMap<WeaponType, Weapon>,
        pub buffs: FxLinkedHashMap<CharacterBuff, BuffProps>,
        pub debuffs: FxLinkedHashMap<CharacterDebuff, BuffProps>,

        pub queued_emoticon: VecDeque<(EmoticonType, GameTickCooldown)>,

        pub interactions: FxLinkedHashSet<CharacterId>,
    }

    impl CloneWithCopyableElements for CharacterReusableCore {
        fn copy_clone_from(&mut self, other: &Self) {
            self.core.copy_clone_from(&other.core);
            self.weapons.copy_clone_from(&other.weapons);
            self.buffs.copy_clone_from(&other.buffs);
            self.debuffs.copy_clone_from(&other.debuffs);
            self.queued_emoticon.clone_from(&other.queued_emoticon);
            self.interactions.clone_from(&other.interactions);
        }
    }

    impl Recyclable for CharacterReusableCore {
        fn new() -> Self {
            Self {
                core: CoreReusable::new(),
                weapons: Default::default(),
                buffs: Default::default(),
                debuffs: Default::default(),
                interactions: Default::default(),
                queued_emoticon: Default::default(),
            }
        }
        fn reset(&mut self) {
            self.core.reset();
            self.weapons.reset();
            self.buffs.reset();
            self.debuffs.reset();
            self.interactions.reset();
        }
    }

    impl ReusableCore for CharacterReusableCore {}

    pub type PoolCharacterReusableCore = Recycle<CharacterReusableCore>;

    #[derive(Debug, Hiarc, Clone)]
    pub struct CharacterPool {
        pub(crate) character_pool: Pool<PoolCharacters>,
        pub(crate) character_reusable_cores_pool: Pool<CharacterReusableCore>,
    }

    #[derive(Debug, Hiarc, PartialEq, Eq)]
    pub enum CharacterDamageResult {
        None,
        Damage,
        Death,
    }

    #[derive(Debug, Hiarc)]
    pub enum CharacterPlayerTy {
        /// e.g. server side dummy
        None,
        /// usually a normal human player
        Player {
            /// keep a reference to the players, the client automatically deletes the player if
            /// it is destroyed
            players: Players,
            /// same as `players`
            spectator_players: SpectatorPlayers,
            /// the network stats for this player.
            network_stats: PlayerNetworkStats,
            /// The stage this character is in
            stage_id: StageId,
        },
    }

    #[derive(Debug, Clone, Copy)]
    pub enum FriendlyFireTy {
        Dmg,
        DmgSelf,
        DmgTeam,
        NoDmgTeam,
    }

    #[derive(Debug, Hiarc)]
    pub struct CharacterPhaseDead {
        pub respawn_in_ticks: GameTickCooldown,

        phased: PhasedCharacters,
        id: CharacterId,

        /// Please use the constructor
        _dont_construct: PhantomData<()>,
    }

    impl CharacterPhaseDead {
        fn push_character_event(
            simulation_events: &SimulationWorldEvents,
            id: CharacterId,
            killer_id: Option<CharacterId>,
            weapon: GameWorldActionKillWeapon,
        ) {
            simulation_events.push_world(SimulationEventWorldEntityType::Character {
                ev: CharacterEvent::Despawn {
                    id,
                    killer_id,
                    weapon,
                },
            });
        }

        pub fn new(
            id: CharacterId,
            respawn_in_ticks: GameTickCooldown,
            pos: vec2,
            phased: PhasedCharacters,
            killer_id: Option<CharacterId>,
            weapon: GameWorldActionKillWeapon,
            flags: KillFlags,
            simulation_events: &SimulationWorldEvents,
            game_pending_events: &GameWorldPendingEvents,
            character_id_pool: &MtPool<Vec<CharacterId>>,
            silent: bool,
        ) -> Self {
            if !silent {
                Self::push_character_event(simulation_events, id, killer_id, weapon);
                game_pending_events.push(GameWorldEvent::Notification(
                    GameWorldNotificationEvent::Action(GameWorldAction::Kill {
                        killer: killer_id,
                        // TODO:
                        assists: PoolVec::new_without_pool(),
                        victims: {
                            let mut victims: pool::mt_recycle::Recycle<Vec<CharacterId>> =
                                character_id_pool.new();
                            victims.push(id);
                            victims
                        },
                        weapon,
                        flags,
                    }),
                ));

                game_pending_events.push_effect(
                    Some(id),
                    pos,
                    GameWorldEntityEffectEvent::Character(GameCharacterEffectEvent::Effect(
                        GameCharacterEventEffect::Death,
                    )),
                );
                game_pending_events.push_sound(
                    Some(id),
                    Some(pos),
                    GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Sound(
                        GameCharacterEventSound::Death,
                    )),
                );
            }

            phased.insert(id);
            Self {
                id,
                respawn_in_ticks,
                phased,
                _dont_construct: PhantomData,
            }
        }
    }

    impl Drop for CharacterPhaseDead {
        fn drop(&mut self) {
            self.phased.remove(&self.id)
        }
    }

    #[derive(Debug, Hiarc)]
    pub struct CharacterPhaseNormal {
        pub hook: CharacterHook,
        /// Please use the constructor
        _dont_construct: PhantomData<()>,
    }

    impl CharacterPhaseNormal {
        pub fn new(
            id: CharacterId,
            pos: vec2,
            game_pending_events: &GameWorldPendingEvents,
            hook: CharacterHook,
            silent: bool,
        ) -> Self {
            if !silent {
                game_pending_events.push_effect(
                    Some(id),
                    pos,
                    GameWorldEntityEffectEvent::Character(GameCharacterEffectEvent::Effect(
                        GameCharacterEventEffect::Spawn,
                    )),
                );
                game_pending_events.push_sound(
                    Some(id),
                    Some(pos),
                    GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Sound(
                        GameCharacterEventSound::Spawn,
                    )),
                );
            }
            Self {
                hook,
                _dont_construct: PhantomData,
            }
        }
    }

    #[derive(Debug, Hiarc)]
    pub enum CharacterPhasedState {
        Normal(CharacterPhaseNormal),
        Dead(CharacterPhaseDead),
    }

    impl CharacterPhasedState {
        /// Get the character hook mutably
        ///
        /// # Panics
        ///
        /// Panics if the character is not in a phased state that has a hook (e.g. dead).
        pub fn hook_mut(&mut self) -> &mut CharacterHook {
            match self {
                CharacterPhasedState::Normal(normal) => &mut normal.hook,
                CharacterPhasedState::Dead(_) => panic!("Called hook_mut on a dead character."),
            }
        }
        /// Get the character hook
        ///
        /// # Panics
        ///
        /// Panics if the character is not in a phased state that has a hook (e.g. dead).
        pub fn hook(&self) -> &CharacterHook {
            match self {
                CharacterPhasedState::Normal(normal) => &normal.hook,
                CharacterPhasedState::Dead(_) => panic!("Called hook_mut on a dead character."),
            }
        }
        /// Returns `true` if the character is dead.
        pub fn is_dead(&self) -> bool {
            match self {
                CharacterPhasedState::Normal(_) => false,
                CharacterPhasedState::Dead(_) => true,
            }
        }
    }

    #[derive(Debug, Hiarc)]
    pub struct Character {
        pub(crate) base: Entity<CharacterId>,
        pub(crate) core: CharacterCore,
        pub(crate) reusable_core: PoolCharacterReusableCore,
        pub(crate) player_info: PlayerInfo,
        pub(crate) pos: CharacterPos,
        pub(crate) phased: CharacterPhasedState,
        pub(crate) score: CharacterScore,

        game_pending_events: GameWorldPendingEvents,
        simulation_events: SimulationWorldEvents,
        phased_characters: PhasedCharacters,

        despawn_info: CharacterDespawnType,
        pub(crate) character_id_pool: MtPool<Vec<CharacterId>>,
        pub(crate) character_id_hash_pool: Pool<FxHashSet<CharacterId>>,

        pub(crate) game_options: GameOptions,

        ty: CharacterPlayerTy,
    }

    impl Character {
        pub fn new(
            id: &CharacterId,
            character_pool: &CharacterPool,
            player_info: PlayerInfo,
            player_input: CharacterInput,
            game_pending_events: &GameWorldPendingEvents,
            simulation_events: &SimulationWorldEvents,
            phased_characters: &PhasedCharacters,
            game_pool: &GamePooling,
            stage_id: &StageId,
            ty: CharacterPlayerTy,
            pos: vec2,
            field: &CharacterPositionPlayfield,
            hooks: &HookedCharacters,
            scores: &CharacterScores,
            side: Option<MatchSide>,
            game_options: GameOptions,
        ) -> Self {
            let (core, reusable_core, pos) =
                Self::respawn(None, character_pool, side, player_input, &player_info, pos);

            if let CharacterPlayerTy::Player { players, .. } = &ty {
                players.insert(
                    *id,
                    Player {
                        stage_id: *stage_id,
                    },
                );
            }

            Self {
                base: Entity::new(id),
                core,
                reusable_core,
                player_info,
                pos: field.get_character_pos(pos, *id),
                phased: CharacterPhasedState::Normal(CharacterPhaseNormal::new(
                    *id,
                    pos,
                    game_pending_events,
                    hooks.get_new_hook(*id),
                    false,
                )),
                score: scores.get_new_score(*id, 0),

                game_pending_events: game_pending_events.clone(),
                simulation_events: simulation_events.clone(),
                phased_characters: phased_characters.clone(),

                character_id_pool: game_pool.character_id_pool.clone(),
                character_id_hash_pool: game_pool.character_id_hashset_pool.clone(),
                despawn_info: Default::default(),

                ty,

                game_options,
            }
        }

        fn respawn_weapons(reusable_core: &mut CharacterReusableCore) {
            let gun = Weapon {
                cur_ammo: Some(10),
                next_ammo_regeneration_tick: 0.into(),
            };
            let hammer = Weapon {
                cur_ammo: None,
                next_ammo_regeneration_tick: 0.into(),
            };
            reusable_core.weapons.clear();
            reusable_core.weapons.insert(WeaponType::Hammer, hammer);
            reusable_core.weapons.insert(WeaponType::Gun, gun);
        }

        fn default_active_weapon() -> WeaponType {
            WeaponType::Gun
        }

        /// Call this and you can't forget to reset anything important
        #[must_use]
        pub(crate) fn respawn(
            prev_core: Option<&CharacterCore>,
            character_pool: &CharacterPool,
            side: Option<MatchSide>,
            player_input: CharacterInput,
            player_info: &PlayerInfo,
            pos: vec2,
        ) -> (CharacterCore, PoolCharacterReusableCore, vec2) {
            let mut core = CharacterCore {
                side,
                health: 10,
                armor: 0,
                input: player_input,
                active_weapon: Self::default_active_weapon(),
                ..Default::default()
            };
            let mut reusable_core = character_pool.character_reusable_cores_pool.new();

            Self::respawn_weapons(&mut reusable_core);

            core.default_eye = player_info.player_info.default_eyes;
            core.eye = core.default_eye;
            if let Some(prev_core) = prev_core {
                core.is_timeout = prev_core.is_timeout;
                core.default_eye = prev_core.default_eye;
                core.default_eye_reset_in = prev_core.default_eye_reset_in;
                core.eye = prev_core.default_eye;
                core.normal_eye_in = prev_core.normal_eye_in;
            }
            (core, reusable_core, pos)
        }

        /// Returns `Some` if character is a player's character.
        pub(crate) fn is_player_character(&self) -> Option<PlayerNetworkStats> {
            if let CharacterPlayerTy::Player { network_stats, .. } = &self.ty {
                Some(*network_stats)
            } else {
                None
            }
        }

        pub(crate) fn die(
            &mut self,
            killer_id: Option<CharacterId>,
            weapon: GameWorldActionKillWeapon,
            flags: KillFlags,
        ) {
            self.phased = CharacterPhasedState::Dead(CharacterPhaseDead::new(
                self.base.game_element_id,
                (TICKS_PER_SECOND / 2).into(),
                *self.pos.pos(),
                self.phased_characters.clone(),
                killer_id,
                weapon,
                flags,
                &self.simulation_events,
                &self.game_pending_events,
                &self.character_id_pool,
                false,
            ));
        }

        /// sets the despawn info to a silently drop the player from the game
        /// it won't be added to the spectators etc.
        /// pending simulation events are still processed.
        pub fn despawn_completely_silent(&mut self) {
            self.despawn_info = CharacterDespawnType::DropFromGame;
        }

        /// the user wants to respawn (a.k.a. kill)
        pub fn despawn_to_respawn(&mut self, create_events: bool) {
            self.phased = CharacterPhasedState::Dead(CharacterPhaseDead::new(
                self.base.game_element_id,
                (TICKS_PER_SECOND / 10).into(),
                *self.pos.pos(),
                self.phased_characters.clone(),
                None,
                GameWorldActionKillWeapon::World,
                Default::default(),
                &self.simulation_events,
                &self.game_pending_events,
                &self.character_id_pool,
                !create_events,
            ));
        }

        /// The character will be dropped and the player will join the spectators
        pub fn despawn_to_join_spectators(&mut self) {
            self.despawn_info = CharacterDespawnType::JoinsSpectator;
        }

        /// normally only useful for snapshot
        pub fn update_player_ty(&mut self, stage_id: &StageId, player_ty: CharacterPlayerTy) {
            match &mut self.ty {
                CharacterPlayerTy::None => {
                    if let CharacterPlayerTy::Player { players, .. } = &player_ty {
                        players.insert(
                            self.base.game_element_id,
                            Player {
                                stage_id: *stage_id,
                            },
                        );
                        self.ty = player_ty;
                    }
                }
                CharacterPlayerTy::Player {
                    players,
                    network_stats,
                    ..
                } => match player_ty {
                    CharacterPlayerTy::None => {
                        players.remove(&self.base.game_element_id);
                        self.ty = player_ty;
                    }
                    CharacterPlayerTy::Player {
                        network_stats: update_stats,
                        ..
                    } => {
                        *network_stats = update_stats;
                    }
                },
            }
        }

        pub fn give_ninja(&mut self) {
            let buff = self.reusable_core.buffs.entry(CharacterBuff::Ninja);
            let had_ninja = matches!(buff, hashlink::lru_cache::Entry::Occupied(_));
            let buff = buff.or_insert_with_keep_order(|| BuffProps {
                remaining_tick: 0.into(),
                interact_tick: 0.into(),
                interact_cursor_dir: vec2::default(),
                interact_val: 0.0,
            });
            buff.remaining_tick = (15 * TICKS_PER_SECOND).into();
            self.core.normal_eye_in = TICKS_PER_SECOND.into();
            self.core.eye = TeeEye::Angry;
            if !had_ninja {
                self.core
                    .attack_recoil
                    .advance_ticks_passed_to(TICKS_PER_SECOND);
            }
        }

        fn push_sound(&self, pos: vec2, ev: GameWorldEntitySoundEvent) {
            self.game_pending_events
                .push_sound(Some(self.base.game_element_id), Some(pos), ev);
        }

        fn push_effect(&self, pos: vec2, ev: GameWorldEntityEffectEvent) {
            self.game_pending_events
                .push_effect(Some(self.base.game_element_id), pos, ev);
        }

        /// Returns true if the tile was handled
        fn handle_game_front_tiles(
            &mut self,
            tile: &Tile,
            res: &mut CharacterDamageResult,
        ) -> bool {
            if tile.index == DdraceTileNum::Death as u8 {
                self.die(None, GameWorldActionKillWeapon::World, Default::default());
                *res = CharacterDamageResult::Death;
            } else if tile.index == DdraceTileNum::Freeze as u8 {
                // freeze
                self.reusable_core.debuffs.insert(
                    CharacterDebuff::Freeze,
                    BuffProps {
                        remaining_tick: (TICKS_PER_SECOND * 3).into(),
                        interact_tick: 0.into(),
                        interact_cursor_dir: Default::default(),
                        interact_val: 0.0,
                    },
                );
            } else if tile.index == DdraceTileNum::Unfreeze as u8 {
                // unfreeze
                self.reusable_core.debuffs.remove(&CharacterDebuff::Freeze);
            } else {
                return false;
            }
            true
        }

        fn handle_game_layer_tiles(&mut self, tile: &Tile, res: &mut CharacterDamageResult) {
            self.handle_game_front_tiles(tile, res);
        }

        fn handle_front_layer_tiles(&mut self, tile: &Tile, res: &mut CharacterDamageResult) {
            self.handle_game_front_tiles(tile, res);
        }

        #[must_use]
        fn handle_tiles(&mut self, old_pos: vec2, collision: &Collision) -> CharacterDamageResult {
            let mut res = CharacterDamageResult::None;
            let cur_pos = *self.pos.pos();
            collision.intersect_line_feedback(&old_pos, &cur_pos, |tile| match tile {
                HitTile::Game(tile) => {
                    self.handle_game_layer_tiles(tile, &mut res);
                }
                HitTile::Front(tile) => {
                    self.handle_front_layer_tiles(tile, &mut res);
                }
                HitTile::Tele(_) => {}
                HitTile::Speedup(_) => {}
                HitTile::Switch(_) => {}
                HitTile::Tune(_) => {
                    // tune tiles are handled on the fly where needed
                }
            });
            res
        }

        fn set_weapon(&mut self, new_weapon: WeaponType) {
            if self.core.active_weapon == new_weapon {
                return;
            }

            self.core.prev_weapon = self.core.active_weapon;
            self.core.queued_weapon = None;
            self.core.active_weapon = new_weapon;
            self.push_sound(
                *self.pos.pos(),
                GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Sound(
                    GameCharacterEventSound::WeaponSwitch { new_weapon },
                )),
            );

            if self.core.active_weapon as usize >= WeaponType::COUNT {
                self.core.active_weapon = Default::default(); // TODO: what is the idea behind this?
            }
            if self
                .reusable_core
                .weapons
                .get_mut(&self.core.active_weapon)
                .is_some()
            {
                // TODO: weapon.next_ammo_regeneration_tick
                //core.weapons[m_ActiveWeapon].m_AmmoRegenStart = -1;
            }
        }

        fn do_weapon_switch(&mut self) {
            // make sure we can switch
            if self.core.attack_recoil.is_some() || self.core.queued_weapon.is_none() {
                return;
            }

            // switch weapon
            self.set_weapon(self.core.queued_weapon.unwrap());
        }

        pub fn friendly_fire_no_dmg(
            characters: &dyn CharactersGetter,
            self_char_id: &CharacterId,
            attacker_char_id: &CharacterId,
            attacker_fallback_side: Option<MatchSide>,
        ) -> FriendlyFireTy {
            if self_char_id.eq(attacker_char_id) {
                return FriendlyFireTy::DmgSelf;
            }
            let self_side = characters.side(self_char_id);
            let other_side = characters.side(attacker_char_id).or(attacker_fallback_side);
            let Some((self_side, other_side)) = self_side.zip(other_side) else {
                return FriendlyFireTy::Dmg;
            };

            if characters
                .does_friendly_fire(self_char_id)
                .or_else(|| characters.does_friendly_fire(attacker_char_id))
                .unwrap_or_default()
            {
                return FriendlyFireTy::DmgTeam;
            }

            if self_side == other_side {
                FriendlyFireTy::NoDmgTeam
            } else {
                FriendlyFireTy::Dmg
            }
        }

        fn create_damage_indicators(&mut self, pos: &vec2, amount: usize) {
            self.core.last_dmg_indicator = (TICKS_PER_SECOND / 2).into();

            let start_offset = -PI * 3.0 / 4.0;
            for _ in 0..amount {
                let step = PI / 8.0;
                let angle = start_offset + self.core.last_dmg_angle + step;

                let dir = vec2::new(angle.cos(), angle.sin()) * -75.0 / 4.0;
                self.push_effect(
                    *pos,
                    GameWorldEntityEffectEvent::Character(GameCharacterEffectEvent::Effect(
                        GameCharacterEventEffect::DamageIndicator { vel: dir },
                    )),
                );
                self.core.last_dmg_angle += step;
            }
        }

        fn create_hammer_hit(&self, pos: &vec2) {
            self.push_effect(
                *pos,
                GameWorldEntityEffectEvent::Character(GameCharacterEffectEvent::Effect(
                    GameCharacterEventEffect::HammerHit,
                )),
            );
            self.push_sound(
                *pos,
                GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Sound(
                    GameCharacterEventSound::HammerHit,
                )),
            );
        }

        pub fn take_damage_from(
            self_char: &mut Character,
            self_char_id: &CharacterId,
            killer_id: CharacterId,
            force: &vec2,
            _source: &vec2,
            _friendly_fire_ty: FriendlyFireTy,
            mut dmg_amount: u32,
            from: DamageTypes,
            by: DamageBy,
        ) -> CharacterDamageResult {
            let core = &mut self_char.core;
            core.core.vel += *force;
            let old_health = core.health;
            let old_armor = core.armor;
            if dmg_amount > 0 {
                if core.armor > 0 {
                    if dmg_amount > 1 {
                        core.health -= 1;
                        dmg_amount -= 1;
                    }

                    if dmg_amount > core.armor {
                        dmg_amount -= core.armor;
                        core.armor = 0;
                    } else {
                        core.armor -= dmg_amount.min(core.armor);
                        dmg_amount = 0;
                    }
                }

                core.health -= dmg_amount.min(core.health);

                let indicator_amount =
                    ((old_health - core.health) + (old_armor - core.armor)) as usize;
                let pos = *self_char.pos.pos();
                self_char.create_damage_indicators(&pos, indicator_amount);
                let id = match from {
                    DamageTypes::Character(id) => id,
                    DamageTypes::CharacterInMatchSide { char_id, .. } => char_id,
                };

                if *id != *self_char_id {
                    self_char.push_sound(
                        *self_char.pos.pos(),
                        GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Sound(
                            GameCharacterEventSound::Hit { strong: false },
                        )),
                    );
                }

                let core = &mut self_char.core;
                // check for death
                if core.health == 0 {
                    let (weapon, flags) = match by {
                        DamageBy::Ninja => (GameWorldActionKillWeapon::Ninja, Default::default()),
                        DamageBy::Weapon { weapon, flags } => {
                            (GameWorldActionKillWeapon::Weapon { weapon }, flags)
                        }
                    };
                    self_char.die(Some(killer_id), weapon, flags);

                    return CharacterDamageResult::Death;
                }

                self_char.push_sound(
                    *self_char.pos.pos(),
                    GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Sound(
                        GameCharacterEventSound::Pain {
                            long: dmg_amount > 2,
                        },
                    )),
                );

                let core = &mut self_char.core;
                core.normal_eye_in = (TICKS_PER_SECOND / 2).into();
                core.eye = TeeEye::Pain;

                CharacterDamageResult::Damage
            } else {
                CharacterDamageResult::None
            }
        }

        pub fn take_damage(
            characters: &mut dyn CharactersGetter,
            self_char_id: &CharacterId,
            force: &vec2,
            source: &vec2,
            mut dmg_amount: u32,
            from: DamageTypes,
            by: DamageBy,
        ) -> CharacterDamageResult {
            let (killer_id, friendly_fire_ty) = match &from {
                DamageTypes::Character(&from_id) => {
                    let friendly_fire_ty =
                        Self::friendly_fire_no_dmg(characters, self_char_id, &from_id, None);
                    (from_id, friendly_fire_ty)
                }
                DamageTypes::CharacterInMatchSide {
                    char_id: &char_id,
                    side,
                } => {
                    let friendly_fire_ty =
                        Self::friendly_fire_no_dmg(characters, self_char_id, &char_id, Some(*side));
                    (char_id, friendly_fire_ty)
                }
            };
            match friendly_fire_ty {
                FriendlyFireTy::Dmg => {
                    // ignore
                }
                FriendlyFireTy::DmgSelf | FriendlyFireTy::DmgTeam => {
                    dmg_amount = 1.max(dmg_amount / 2);
                }
                FriendlyFireTy::NoDmgTeam => {
                    dmg_amount = 0;
                }
            }

            let self_char = characters.char_mut(self_char_id).unwrap();
            let res = Self::take_damage_from(
                self_char,
                self_char_id,
                killer_id,
                force,
                source,
                friendly_fire_ty,
                dmg_amount,
                from,
                by,
            );
            if let (CharacterDamageResult::Death, Some(killer)) =
                (&res, characters.char_mut(&killer_id))
            {
                if let FriendlyFireTy::Dmg = friendly_fire_ty {
                    killer.core.eye = TeeEye::Happy;
                    killer.core.normal_eye_in = (TICKS_PER_SECOND / 2).into();
                }
            }
            res
        }

        /// can fire at all (ninja or weapon)
        fn can_fire(&self) -> bool {
            !self.reusable_core.buffs.contains_key(&CharacterBuff::Ghost)
                && !self
                    .reusable_core
                    .debuffs
                    .contains_key(&CharacterDebuff::Freeze)
        }

        fn can_fire_weapon(&self) -> bool {
            !self.reusable_core.buffs.contains_key(&CharacterBuff::Ninja) && self.can_fire()
        }

        fn fire_weapon(
            &mut self,
            pipe: &mut SimulationPipeCharacter,
            fire: Option<(NonZeroU64, CharacterInputCursor)>,
        ) {
            if self.core.attack_recoil.is_some() {
                return;
            }

            self.do_weapon_switch();

            if !self.can_fire_weapon() {
                return;
            }

            let full_auto = self.core.active_weapon == WeaponType::Grenade
                || self.core.active_weapon == WeaponType::Shotgun
                || self.core.active_weapon == WeaponType::Laser;

            let auto_fired = full_auto && *self.core.input.state.fire;
            let fired = fire.is_some();

            let direction = normalize(&{
                let cursor_pos = if fired {
                    fire.as_ref().unwrap().1.to_vec2()
                } else {
                    self.core.input.cursor.to_vec2()
                };
                vec2::new(cursor_pos.x as f32, cursor_pos.y as f32)
            });

            // check if we gonna fire
            let will_fire = fired || auto_fired;

            if !will_fire {
                return;
            }

            // check for ammo
            let cur_weapon = self.reusable_core.weapons.get_mut(&self.core.active_weapon);
            if cur_weapon
                .as_ref()
                .is_none_or(|weapon| weapon.cur_ammo.is_some_and(|val| val == 0))
            {
                if fired && self.core.no_ammo_sound.is_none() {
                    self.push_sound(
                        *self.pos.pos(),
                        GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Sound(
                            GameCharacterEventSound::NoAmmo {
                                weapon: self.core.active_weapon,
                            },
                        )),
                    );
                    self.core.no_ammo_sound = TICKS_PER_SECOND.into();
                }
                return;
            }

            let proj_start_pos = *self.pos.pos() + direction * PHYSICAL_SIZE * 0.75;

            // TODO: check all branches. make sure no code/TODO comments are in, before removing this comment

            self.core.attack_recoil = match self.core.active_weapon {
                WeaponType::Hammer => {
                    // TODO: recheck
                    self.push_sound(
                        *self.pos.pos(),
                        GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Sound(
                            GameCharacterEventSound::HammerFire,
                        )),
                    );

                    let mut hits = 0;
                    let core_pos = *self.pos.pos();
                    pipe.characters.for_other_characters_in_range_mut(
                        &proj_start_pos,
                        PHYSICAL_SIZE * 0.5,
                        &mut |char| {
                            if pipe.collision.intersect_line(
                                &proj_start_pos,
                                char.pos.pos(),
                                &mut vec2::default(),
                                &mut vec2::default(),
                                CollisionTypes::SOLID,
                            ) != CollisionTile::None
                            {
                                return;
                            }

                            // set his velocity to fast upward (for now)
                            if length(&(*char.pos.pos() - proj_start_pos)) > 0.0 {
                                self.create_hammer_hit(
                                    &(*char.pos.pos()
                                        - normalize(&(*char.pos.pos() - proj_start_pos))
                                            * PHYSICAL_SIZE
                                            * 0.5),
                                );
                            } else {
                                self.create_hammer_hit(&proj_start_pos);
                            }

                            let dir = if length(&(*char.pos.pos() - core_pos)) > 0.0 {
                                normalize(&(*char.pos.pos() - core_pos))
                            } else {
                                vec2::new(0.0, -1.0)
                            };

                            let char_id = char.base.game_element_id;
                            let self_id = self.base.game_element_id;
                            Self::take_damage(
                                &mut (
                                    (self.base.game_element_id, &mut *self),
                                    (char_id, &mut *char),
                                ),
                                &char_id,
                                &(vec2::new(0.0, -1.0)
                                    + normalize(&(dir + vec2::new(0.0, -1.1))) * 10.0),
                                &(dir * -1.0),
                                3,
                                DamageTypes::Character(&self_id),
                                DamageBy::Weapon {
                                    weapon: WeaponType::Hammer,
                                    flags: Default::default(),
                                },
                            );
                            hits += 1;
                        },
                    );
                    let tune = pipe.collision.get_tune_at(&proj_start_pos);
                    let fire_delay = if hits > 0 {
                        tune.hammer_hit_fire_delay
                    } else {
                        tune.hammer_fire_delay
                    };
                    ((fire_delay * TICKS_PER_SECOND as f32 / 1000.0).ceil() as GameTickType).into()
                }
                WeaponType::Gun => {
                    let tunings = pipe.collision.get_tune_at(&proj_start_pos);
                    pipe.entity_events.push(CharacterTickEvent::Projectile {
                        pos: proj_start_pos,
                        dir: direction,
                        ty: WeaponWithProjectile::Gun,
                        lifetime: tunings.gun_lifetime,
                    });
                    self.push_sound(
                        *self.pos.pos(),
                        GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Sound(
                            GameCharacterEventSound::GunFire,
                        )),
                    );

                    let fire_delay = tunings.gun_fire_delay;
                    ((fire_delay * TICKS_PER_SECOND as f32 / 1000.0).ceil() as GameTickType).into()
                }
                WeaponType::Shotgun => {
                    let shot_spreed: i32 = 2;

                    for i in -shot_spreed..=shot_spreed {
                        let spreading = [-0.185, -0.070, 0.0, 0.070, 0.185];
                        let a = angle(&direction) + spreading[(i + 2) as usize];
                        let v = 1.0 - (i.abs() as f32 / (shot_spreed as f32));
                        let tunings = pipe.collision.get_tune_at(&proj_start_pos);
                        let speed = mix(&tunings.shotgun_speeddiff, &1.0, v);

                        pipe.entity_events.push(CharacterTickEvent::Projectile {
                            pos: proj_start_pos,
                            dir: vec2::new(a.cos(), a.sin()) * speed,
                            ty: WeaponWithProjectile::Shotgun,
                            lifetime: tunings.shotgun_lifetime,
                        });
                    }

                    self.push_sound(
                        *self.pos.pos(),
                        GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Sound(
                            GameCharacterEventSound::ShotgunFire,
                        )),
                    );

                    let fire_delay = pipe
                        .collision
                        .get_tune_at(&proj_start_pos)
                        .shotgun_fire_delay;
                    ((fire_delay * TICKS_PER_SECOND as f32 / 1000.0).ceil() as GameTickType).into()
                }
                WeaponType::Grenade => {
                    let tunings = pipe.collision.get_tune_at(&proj_start_pos);
                    pipe.entity_events.push(CharacterTickEvent::Projectile {
                        pos: proj_start_pos,
                        dir: direction,
                        ty: WeaponWithProjectile::Grenade,
                        lifetime: tunings.grenade_lifetime,
                    });
                    self.push_sound(
                        *self.pos.pos(),
                        GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Sound(
                            GameCharacterEventSound::GrenadeFire,
                        )),
                    );
                    let fire_delay = tunings.grenade_fire_delay;
                    ((fire_delay * TICKS_PER_SECOND as f32 / 1000.0).ceil() as GameTickType).into()
                }
                WeaponType::Laser => {
                    pipe.entity_events.push(CharacterTickEvent::Laser {
                        pos: *self.pos.pos(),
                        dir: direction,
                        energy: pipe.collision.get_tune_at(self.pos.pos()).laser_reach,
                        can_hit_own: self.game_options.laser_hit_self(),
                    });
                    self.push_sound(
                        *self.pos.pos(),
                        GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Sound(
                            GameCharacterEventSound::LaserFire,
                        )),
                    );

                    let fire_delay = pipe.collision.get_tune_at(&proj_start_pos).laser_fire_delay;
                    ((fire_delay * TICKS_PER_SECOND as f32 / 1000.0).ceil() as GameTickType).into()
                }
            };

            let cur_weapon = self
                .reusable_core
                .weapons
                .get_mut(&self.core.active_weapon)
                .unwrap();
            cur_weapon.cur_ammo = cur_weapon.cur_ammo.map(|val| val.saturating_sub(1));
        }

        fn fire_ninja(
            &mut self,
            fire: &Option<(NonZeroU64, CharacterInputCursor)>,
            collision: &Collision,
        ) {
            if !self.can_fire() {
                return;
            }
            if self.core.attack_recoil.is_some() {
                return;
            }
            let Some((_, cursor)) = fire else { return };
            let Some(buff) = self.reusable_core.buffs.get_mut(&CharacterBuff::Ninja) else {
                return;
            };

            let fire_delay = collision.get_tune_at(self.pos.pos()).ninja_fire_delay;
            self.core.attack_recoil =
                ((fire_delay * TICKS_PER_SECOND as f32 / 1000.0).ceil() as GameTickType).into();

            let cursor = cursor.to_vec2();
            buff.interact_cursor_dir = normalize(&vec2::new(cursor.x as f32, cursor.y as f32));
            buff.interact_tick = (TICKS_PER_SECOND / 5).into();
            buff.interact_val = length(&self.core.core.vel);
            self.reusable_core.interactions.clear();

            self.push_sound(
                *self.pos.pos(),
                GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Buff(
                    GameBuffSoundEvent::Ninja(GameBuffNinjaEventSound::Attack),
                )),
            );
        }

        fn handle_weapon_switch(
            &mut self,
            weapon_diff: Option<NonZeroI64>,
            weapon_req: Option<WeaponType>,
        ) {
            let wanted_weapon = if let Some(queued_weapon) = self.core.queued_weapon {
                queued_weapon
            } else {
                self.core.active_weapon
            };

            // select weapon
            let diff = weapon_diff.map(|diff| diff.get()).unwrap_or(0);

            let cur_weapon_count = self.reusable_core.weapons.len();
            let offset = diff as i32 % cur_weapon_count as i32;

            let (found_weapon_index, _) = self
                .reusable_core
                .weapons
                .keys()
                .enumerate()
                .find(|(_, weapon)| (*weapon).eq(&wanted_weapon))
                .unwrap();

            // move the offset to where the actual weapon is
            let mut new_index = (found_weapon_index as i32 + offset) % cur_weapon_count as i32;
            if new_index < 0 {
                new_index += cur_weapon_count as i32;
            }

            let mut next_weapon = self
                .reusable_core
                .weapons
                .keys()
                .enumerate()
                .find_map(|(index, weapon)| {
                    if index == new_index as usize {
                        Some(*weapon)
                    } else {
                        None
                    }
                })
                .unwrap();

            // Direct Weapon selection
            if let Some(ref weapon) = weapon_req {
                if self.reusable_core.weapons.contains_key(weapon) {
                    next_weapon = *weapon;
                }
            }

            // check for insane values
            if next_weapon != self.core.active_weapon {
                self.core.queued_weapon = Some(next_weapon);
            }

            self.do_weapon_switch();
        }

        fn handle_buffs_and_debuffs(&mut self, pipe: &mut SimulationPipeCharacter) {
            self.reusable_core.buffs.retain_with_order(|ty, buff| {
                if buff.remaining_tick.tick().unwrap_or_default()
                    && matches!(ty, CharacterBuff::Ninja)
                {
                    self.core
                        .attack_recoil
                        .advance_ticks_passed_to(TICKS_PER_SECOND);
                }
                buff.remaining_tick.is_some()
            });
            self.reusable_core.debuffs.retain_with_order(|_, buff| {
                buff.remaining_tick.tick();
                buff.remaining_tick.is_some()
            });

            self.handle_ninja(pipe);
        }

        fn handle_ninja(&mut self, pipe: &mut SimulationPipeCharacter) {
            let Some(buff) = self.reusable_core.buffs.get_mut(&CharacterBuff::Ninja) else {
                return;
            };
            if buff.interact_tick.is_none() {
                return;
            }
            if buff.interact_tick.tick().unwrap_or_default() {
                self.core.core.vel = buff.interact_cursor_dir * buff.interact_val;
            } else {
                // Set velocity
                let mut vel = buff.interact_cursor_dir * 50.0;
                let old_pos = *self.pos.pos();
                let mut new_pos = *self.pos.pos();
                pipe.collision.move_box(
                    &mut new_pos,
                    &mut vel,
                    &ivec2::new(PHYSICAL_SIZE as i32, PHYSICAL_SIZE as i32),
                    0.0,
                );
                self.pos.move_pos(new_pos);

                self.core.core.vel = vec2::new(0.0, 0.0);

                let dir = *self.pos.pos() - old_pos;
                let center = old_pos + dir * 0.5;
                pipe.characters.for_other_characters_in_range_mut(
                    &center,
                    PHYSICAL_SIZE * 2.0,
                    &mut |char| {
                        let char_id = char.base.game_element_id;
                        // make sure we haven't Hit this object before
                        if self.reusable_core.interactions.contains(&char_id) {
                            return;
                        }

                        // check so we are sufficiently close
                        if distance_squared(char.pos.pos(), self.pos.pos())
                            > (PHYSICAL_SIZE * 2.0).powf(2.0)
                        {
                            return;
                        }

                        self.reusable_core.interactions.insert(char_id);

                        let self_id = self.base.game_element_id;
                        let self_pos = *self.pos.pos();
                        Self::take_damage(
                            &mut (
                                (self.base.game_element_id, &mut *self),
                                (char_id, &mut *char),
                            ),
                            &char_id,
                            &vec2::new(0.0, -10.0),
                            &self_pos,
                            9,
                            DamageTypes::Character(&self_id),
                            DamageBy::Ninja,
                        );

                        self.push_sound(
                            *self.pos.pos(),
                            GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Buff(
                                GameBuffSoundEvent::Ninja(GameBuffNinjaEventSound::Hit),
                            )),
                        );
                    },
                );
            }
        }

        fn handle_weapons(&mut self, pipe: &mut SimulationPipeCharacter) {
            // don't handle weapon if ninja, ghost or freeze are active
            if self.reusable_core.buffs.contains_key(&CharacterBuff::Ninja)
                || self.reusable_core.buffs.contains_key(&CharacterBuff::Ghost)
                || self
                    .reusable_core
                    .debuffs
                    .contains_key(&CharacterDebuff::Freeze)
            {
                return;
            }

            // check reload timer
            if self.core.attack_recoil.is_some() {
                return;
            }

            // fire weapon, if wanted
            self.fire_weapon(pipe, None);

            // ammo regen
            let ammo_regen_time = match self.core.active_weapon {
                WeaponType::Hammer => None,
                WeaponType::Gun => Some(TICKS_PER_SECOND / 2),
                WeaponType::Shotgun => None,
                WeaponType::Grenade => None,
                WeaponType::Laser => None,
            };
            let weapon = self
                .reusable_core
                .weapons
                .get_mut(&self.core.active_weapon)
                .unwrap();
            if let Some(ammo_regen_time) = ammo_regen_time {
                if weapon.cur_ammo.is_some_and(|ammo| ammo >= 10) {
                    weapon.next_ammo_regeneration_tick = ammo_regen_time.into();
                } else if weapon
                    .next_ammo_regeneration_tick
                    .tick()
                    .unwrap_or_default()
                {
                    weapon.cur_ammo = weapon.cur_ammo.map(|ammo| ammo + 1);
                    weapon.next_ammo_regeneration_tick = ammo_regen_time.into();
                }
            }
        }

        pub fn handle_input_change(
            &mut self,
            pipe: &mut SimulationPipeCharacter,
            diff: CharacterInputConsumableDiff,
        ) -> EntityTickResult {
            self.core.core.jumps.queued = self
                .core
                .core
                .jumps
                .queued
                .saturating_add(diff.jump.map(|val| val.get()).unwrap_or_default());
            if let Some((hooks, cursor)) = diff.hook {
                self.core.core.queued_hooks.clicked = self
                    .core
                    .core
                    .queued_hooks
                    .clicked
                    .saturating_add(hooks.get());
                self.core.core.queued_hooks.cursor = cursor.to_vec2();
            }
            self.handle_weapon_switch(diff.weapon_diff, diff.weapon_req);
            self.fire_ninja(&diff.fire, pipe.collision);
            self.fire_weapon(pipe, diff.fire);
            EntityTickResult::None
        }

        fn handle_emoticon_queue(&mut self) {
            let core = &mut self.core;
            self.reusable_core
                .queued_emoticon
                .retain_mut(|(emoticon, cooldown)| {
                    if cooldown.tick().unwrap_or_default() {
                        core.eye = match emoticon {
                            EmoticonType::OOP | EmoticonType::SORRY => TeeEye::Pain,
                            EmoticonType::EXCLAMATION
                            | EmoticonType::GHOST
                            | EmoticonType::SUSHI
                            | EmoticonType::WTF
                            | EmoticonType::QUESTION => TeeEye::Surprised,
                            EmoticonType::HEARTS | EmoticonType::MUSIC | EmoticonType::EYES => {
                                TeeEye::Happy
                            }
                            EmoticonType::DROP | EmoticonType::DOTDOT | EmoticonType::ZZZ => {
                                TeeEye::Blink
                            }
                            EmoticonType::SPLATTEE
                            | EmoticonType::DEVILTEE
                            | EmoticonType::ZOMG => TeeEye::Angry,
                        };
                        core.normal_eye_in = (2 * TICKS_PER_SECOND).into();

                        core.emoticon_tick = (2 * TICKS_PER_SECOND).into();
                        core.cur_emoticon = Some(*emoticon);
                        false
                    } else {
                        true
                    }
                });
        }

        /// For modifications
        fn mod_tick(&mut self) {}

        fn handle_ticks(&mut self) {
            self.core.attack_recoil.tick();
            self.core.no_ammo_sound.tick();
            if self.core.last_dmg_indicator.tick().unwrap_or_default() {
                self.core.last_dmg_angle = 0.0;
            }
            self.core.emoticon_tick.tick();

            self.handle_emoticon_queue();
        }

        fn post_ddrace_tick(&mut self) {
            let core = &mut self.core.core;
            if core.has_endless {
                if let CharacterPhasedState::Normal(state) = &mut self.phased {
                    let (mut hook, hooked_char) = state.hook.get();
                    if let Hook::Active { hook_tick, .. } = &mut hook {
                        *hook_tick = 0;
                    }
                    state.hook.set(hook, hooked_char);
                }
            }

            // following jump rules can be overridden by tiles, like Refill Jumps, Stopper and Wall Jump
            if core.jumps.max == -1 {
                // The player has only one ground jump, so his feet are always dark
                core.jumps.flag |= 2;
            } else if core.jumps.max == 0 {
                // The player has no jumps at all, so his feet are always dark
                core.jumps.flag |= 2;
            } else if core.jumps.max == 1 && core.jumps.flag > 0 {
                // If the player has only one jump, each jump is the last one
                core.jumps.flag |= 2;
            } else if core.jumps.count < core.jumps.max - 1 && core.jumps.flag > 1 {
                // The player has not yet used up all his jumps, so his feet remain light
                core.jumps.flag = 1;
            }

            if (core.is_super || core.jumps.endless) && core.jumps.flag > 1 {
                // Super players and players with infinite jumps always have light feet
                core.jumps.flag = 1;
            }
        }
    }

    impl EntityInterface<CharacterCore, CharacterReusableCore, SimulationPipeCharacter<'_>>
        for Character
    {
        fn pre_tick(&mut self, _pipe: &mut SimulationPipeCharacter) -> EntityTickResult {
            if self.core.normal_eye_in.tick().unwrap_or_default() {
                self.core.eye = self.core.default_eye;
            }
            if self.core.default_eye_reset_in.tick().unwrap_or_default() {
                if self.core.default_eye == self.core.eye && self.core.normal_eye_in.is_none() {
                    self.core.eye = self.player_info.player_info.default_eyes;
                };
                self.core.default_eye = self.player_info.player_info.default_eyes;
            }

            if self
                .reusable_core
                .debuffs
                .contains_key(&CharacterDebuff::Freeze)
            {
                self.core.input.state.fire.set(false);
                self.core.input.state.hook.set(false);
                self.core.input.state.dir.set(0);
                self.core.input.state.jump.set(false);
                self.core.core.queued_hooks.clicked = 0;
                self.core.core.jumps.queued = 0;
            }

            EntityTickResult::None
        }

        fn tick(&mut self, pipe: &mut SimulationPipeCharacter) -> EntityTickResult {
            self.mod_tick();
            self.handle_ticks();

            self.handle_weapon_switch(None, None);

            let old_pos = *self.pos.pos();
            let (core, input) = self.core.get_core_mut_and_input(&self.reusable_core);
            let mut core_pipe = CorePipe {
                characters: pipe.characters,
                input,
            };
            core.physics_tick(
                &mut self.pos,
                self.phased.hook_mut(),
                true,
                true,
                &mut core_pipe,
                pipe.collision,
                CoreEvents {
                    character_id: &self.base.game_element_id,
                    game_pending_events: &self.game_pending_events,
                },
            );

            if Entity::<CharacterId>::outside_of_playfield(self.pos.pos(), pipe.collision) {
                self.die(None, GameWorldActionKillWeapon::World, Default::default());
                return EntityTickResult::RemoveEntity;
            }

            let tiles_res = self.handle_tiles(old_pos, pipe.collision);
            if matches!(tiles_res, CharacterDamageResult::Death) {
                return EntityTickResult::RemoveEntity;
            }

            self.handle_buffs_and_debuffs(pipe);
            self.handle_weapons(pipe);

            self.post_ddrace_tick();

            EntityTickResult::None
        }

        fn tick_deferred(&mut self, pipe: &mut SimulationPipeCharacter) -> EntityTickResult {
            let mut core_pipe = CorePipe {
                characters: pipe.characters,
                input: &self.core.input,
            };
            self.core
                .core
                .physics_move(&mut self.pos, &mut core_pipe, pipe.collision);
            self.core
                .core
                .physics_quantize(&mut self.pos, self.phased.hook_mut());

            EntityTickResult::None
        }

        fn drop_mode(&mut self, mode: DropMode) {
            self.base.drop_mode = mode;
        }
    }

    impl Drop for Character {
        fn drop(&mut self) {
            let (add_to_spectator_players, death_effect) = match &mut self.despawn_info {
                CharacterDespawnType::DropFromGame => (false, true),
                CharacterDespawnType::JoinsSpectator => (true, false),
            };

            let (add_to_spectator_players, death_effect) = (
                add_to_spectator_players
                    && matches!(self.base.drop_mode, DropMode::None | DropMode::NoEvents),
                death_effect && matches!(self.base.drop_mode, DropMode::None),
            );

            if death_effect {
                self.push_effect(
                    *self.pos.pos(),
                    GameWorldEntityEffectEvent::Character(GameCharacterEffectEvent::Effect(
                        GameCharacterEventEffect::Death,
                    )),
                );
                self.push_sound(
                    *self.pos.pos(),
                    GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Sound(
                        GameCharacterEventSound::Death,
                    )),
                );
            }

            if let CharacterPlayerTy::Player {
                players,
                spectator_players,
                network_stats,
                ..
            } = &self.ty
            {
                players.remove(&self.base.game_element_id);
                if add_to_spectator_players {
                    spectator_players.insert(
                        self.base.game_element_id,
                        SpectatorPlayer::new(
                            self.player_info.clone(),
                            self.core.input,
                            &self.base.game_element_id,
                            self.character_id_hash_pool.new(),
                            self.core.default_eye,
                            self.core.default_eye_reset_in,
                            *network_stats,
                        ),
                    );
                }
            }
        }
    }

    pub type PoolCharacters = FxLinkedHashMap<CharacterId, Character>;

    pub type CharactersViewMut<'a, F, FV> =
        LinkedHashMapViewMut<'a, CharacterId, Character, rustc_hash::FxBuildHasher, F, FV>;
    pub type CharactersView<'a, F, FV> =
        LinkedHashMapView<'a, CharacterId, Character, rustc_hash::FxBuildHasher, F, FV>;

    pub type Characters = PoolFxLinkedHashMap<CharacterId, Character>;

    pub trait CharactersGetter {
        fn char_mut(&mut self, char_id: &CharacterId) -> Option<&mut Character>;
        fn side(&self, char_id: &CharacterId) -> Option<MatchSide>;
        fn does_friendly_fire(&self, char_id: &CharacterId) -> Option<bool>;
    }

    impl CharactersGetter for Characters {
        fn char_mut(&mut self, char_id: &CharacterId) -> Option<&mut Character> {
            self.get_mut(char_id)
        }
        fn side(&self, char_id: &CharacterId) -> Option<MatchSide> {
            self.get(char_id).and_then(|c| c.core.side)
        }
        fn does_friendly_fire(&self, char_id: &CharacterId) -> Option<bool> {
            self.get(char_id).map(|c| c.game_options.friendly_fire())
        }
    }

    impl CharactersGetter for ((CharacterId, &mut Character), (CharacterId, &mut Character)) {
        fn char_mut(&mut self, char_id: &CharacterId) -> Option<&mut Character> {
            if self.0 .0 == *char_id {
                Some(self.0 .1)
            } else {
                Some(self.1 .1)
            }
        }
        fn side(&self, char_id: &CharacterId) -> Option<MatchSide> {
            if self.0 .0 == *char_id {
                &*self.0 .1
            } else {
                &*self.1 .1
            }
            .core
            .side
        }
        fn does_friendly_fire(&self, char_id: &CharacterId) -> Option<bool> {
            Some(
                if self.0 .0 == *char_id {
                    &*self.0 .1
                } else {
                    &*self.1 .1
                }
                .game_options
                .friendly_fire(),
            )
        }
    }

    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc)]
    pub struct PhasedCharacters {
        ids: PoolFxLinkedHashMap<CharacterId, u64>,
        pool: Pool<FxLinkedHashMap<CharacterId, u64>>,
    }

    #[hiarc_safer_rc_refcell]
    impl Default for PhasedCharacters {
        fn default() -> Self {
            let pool = Pool::with_capacity(2);
            let ids = pool.new();
            Self { ids, pool }
        }
    }

    #[hiarc_safer_rc_refcell]
    impl PhasedCharacters {
        pub(super) fn insert(&mut self, id: CharacterId) {
            let counter = self
                .ids
                .entry(id)
                .or_insert_with_keep_order(Default::default);
            *counter += 1;
        }
        pub(super) fn remove(&mut self, id: &CharacterId) {
            if let Some(counter) = self.ids.get_mut(id) {
                *counter -= 1;
                if *counter == 0 {
                    self.ids.remove(id);
                }
            }
        }
        pub fn is_empty(&self) -> bool {
            self.ids.is_empty()
        }
        pub fn contains(&self, id: &CharacterId) -> bool {
            self.ids.contains_key(id)
        }
        pub fn take(&mut self) -> PoolFxLinkedHashMap<CharacterId, u64> {
            let mut ids = self.pool.new();
            ids.extend(self.ids.iter());
            ids
        }
    }

    pub fn lerp_core_pos(char1: &Character, char2: &Character, amount: f64) -> vec2 {
        lerp(char1.pos.pos(), char2.pos.pos(), amount as f32)
    }

    pub fn lerp_core_vel(char1: &Character, char2: &Character, amount: f64) -> vec2 {
        lerp(&char1.core.core.vel, &char2.core.core.vel, amount as f32)
    }

    pub fn lerp_core_hook_pos(char1: &Character, char2: &Character, amount: f64) -> Option<vec2> {
        if let (Hook::Active { hook_pos: pos1, .. }, Hook::Active { hook_pos: pos2, .. }) =
            (char1.phased.hook().hook(), char2.phased.hook().hook())
        {
            Some(lerp(&pos1, &pos2, amount as f32))
        } else {
            None
        }
    }
}
