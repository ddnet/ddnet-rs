pub mod snapshot {
    use std::{num::NonZeroU16, rc::Rc};

    use crate::{
        entities::character::character::CharacterSpectateMode, reusable::CloneWithCopyableElements,
    };
    use base::{
        linked_hash_map_view::FxLinkedHashMap,
        network_string::{NetworkStringPool, PoolNetworkString},
    };
    use game_interface::{
        client_commands::MAX_TEAM_NAME_LEN,
        events::GameWorldActionKillWeapon,
        pooling::GamePooling,
        types::{
            emoticons::EnumCount,
            game::{GameEntityId, GameTickCooldown},
            id_gen::IdGenerator,
            id_types::{
                CharacterId, CtfFlagId, LaserId, PickupId, PlayerId, ProjectileId, StageId,
            },
            network_stats::PlayerNetworkStats,
            snapshot::{SnapshotClientInfo, SnapshotLocalPlayer, SnapshotLocalPlayers},
            weapons::WeaponType,
        },
    };
    use hiarc::{hi_closure, Hiarc};
    use math::math::vector::{ubvec4, vec2};
    use rustc_hash::FxHashSet;

    use crate::{
        collision::collision::Tunings,
        entities::{
            character::{
                character::{
                    CharacterPhaseDead, CharacterPhaseNormal, CharacterPhasedState,
                    CharacterPlayerTy,
                },
                hook::character_hook::Hook,
                player::player::{
                    PlayerCharacterInfo, PlayerInfo, Players, SpectatorPlayer, SpectatorPlayers,
                },
            },
            entity::entity::{DropMode, EntityInterface},
            flag::flag::{Flag, FlagCore, FlagReusableCore, Flags, PoolFlagReusableCore},
            laser::laser::{LaserCore, LaserReusableCore, PoolLaserReusableCore},
            pickup::pickup::{PickupCore, PickupReusableCore, PoolPickupReusableCore},
            projectile::projectile::{
                PoolProjectileReusableCore, ProjectileCore, ProjectileReusableCore,
            },
        },
        game_objects::game_objects::GameObjectDefinitions,
        match_state::match_state::Match,
        simulation_pipe::simulation_pipe::GamePendingEvents,
        spawns::GameSpawns,
        stage::stage::Stages,
        types::types::GameOptions,
        world::world::{GameObjectWorld, WorldPool},
    };

    use super::super::{
        entities::character::character::{
            CharacterCore, CharacterReusableCore, PoolCharacterReusableCore,
        },
        state::state::GameState,
    };
    use pool::{
        datatypes::{PoolFxHashSet, PoolFxLinkedHashMap, PoolVec},
        pool::Pool,
    };
    use serde::{Deserialize, Serialize};

    pub enum SnapshotFor {
        Client(SnapshotClientInfo),
        Hotreload,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub enum SnapshotCharacterPlayerTy {
        None,
        Player(PlayerNetworkStats),
    }

    #[derive(Debug, Hiarc, Serialize, Deserialize)]
    pub enum SnapshotCharacterSpectateMode {
        Free(vec2),
        Follows {
            ids: PoolFxHashSet<CharacterId>,
            /// Disallow zooming if forced
            /// zoom level is set.
            locked_zoom: bool,
        },
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub enum SnapshotCharacterPhasedState {
        Normal {
            hook: (Hook, Option<CharacterId>),
            ingame_spectate: Option<SnapshotCharacterSpectateMode>,
        },
        Dead {
            respawn_in_ticks: GameTickCooldown,
        },
        PhasedSpectate(SnapshotCharacterSpectateMode),
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct SnapshotCharacter {
        pub core: CharacterCore,
        pub reusable_core: PoolCharacterReusableCore,
        pub player_info: PlayerInfo,
        pub ty: SnapshotCharacterPlayerTy,
        pub pos: vec2,
        pub phased: SnapshotCharacterPhasedState,
        pub score: i64,

        pub game_el_id: CharacterId,
    }

    pub type PoolSnapshotCharacters = FxLinkedHashMap<CharacterId, SnapshotCharacter>;
    pub type SnapshotCharacters = PoolFxLinkedHashMap<CharacterId, SnapshotCharacter>;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct SnapshotProjectile {
        pub core: ProjectileCore,
        pub reusable_core: PoolProjectileReusableCore,

        pub game_el_id: ProjectileId,
        pub owner_game_el_id: CharacterId,
    }

    pub type PoolSnapshotProjectiles = FxLinkedHashMap<ProjectileId, SnapshotProjectile>;
    pub type SnapshotProjectiles = PoolFxLinkedHashMap<ProjectileId, SnapshotProjectile>;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct SnapshotLaser {
        pub core: LaserCore,
        pub reusable_core: PoolLaserReusableCore,

        pub game_el_id: LaserId,
        pub owner_game_el_id: CharacterId,
    }

    pub type PoolSnapshotLasers = FxLinkedHashMap<LaserId, SnapshotLaser>;
    pub type SnapshotLasers = PoolFxLinkedHashMap<LaserId, SnapshotLaser>;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct SnapshotPickup {
        pub core: PickupCore,
        pub reusable_core: PoolPickupReusableCore,

        pub game_el_id: PickupId,
    }

    pub type PoolSnapshotPickups = FxLinkedHashMap<PickupId, SnapshotPickup>;
    pub type SnapshotPickups = PoolFxLinkedHashMap<PickupId, SnapshotPickup>;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct SnapshotFlag {
        pub core: FlagCore,
        pub reusable_core: PoolFlagReusableCore,

        pub game_el_id: CtfFlagId,
    }

    pub type PoolSnapshotFlags = FxLinkedHashMap<CtfFlagId, SnapshotFlag>;
    pub type SnapshotFlags = PoolFxLinkedHashMap<CtfFlagId, SnapshotFlag>;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct SnapshotInactiveObject {
        pub hearts: PoolVec<GameObjectWorld>,
        pub shields: PoolVec<GameObjectWorld>,

        pub red_flags: PoolVec<GameObjectWorld>,
        pub blue_flags: PoolVec<GameObjectWorld>,

        pub weapons: [PoolVec<GameObjectWorld>; WeaponType::COUNT],

        pub ninjas: PoolVec<GameObjectWorld>,
    }

    pub type PoolSnapshotInactiveObjects = Vec<GameObjectWorld>;
    pub type SnapshotInactiveObjects = PoolVec<GameObjectWorld>;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct SnapshotWorld {
        pub characters: SnapshotCharacters,
        pub projectiles: SnapshotProjectiles,
        pub lasers: SnapshotLasers,
        pub pickups: SnapshotPickups,
        pub red_flags: SnapshotFlags,
        pub blue_flags: SnapshotFlags,

        pub inactive_objects: SnapshotInactiveObject,
    }

    impl SnapshotWorld {
        pub fn new(world_pool: &SnapshotWorldPool) -> Self {
            Self {
                characters: world_pool.characters_pool.new(),
                projectiles: world_pool.projectiles_pool.new(),
                lasers: world_pool.lasers_pool.new(),
                pickups: world_pool.pickups_pool.new(),
                red_flags: world_pool.flags_pool.new(),
                blue_flags: world_pool.flags_pool.new(),
                inactive_objects: SnapshotInactiveObject {
                    hearts: world_pool.inactive_objects.new(),
                    shields: world_pool.inactive_objects.new(),
                    red_flags: world_pool.inactive_objects.new(),
                    blue_flags: world_pool.inactive_objects.new(),
                    weapons: [
                        world_pool.inactive_objects.new(),
                        world_pool.inactive_objects.new(),
                        world_pool.inactive_objects.new(),
                        world_pool.inactive_objects.new(),
                        world_pool.inactive_objects.new(),
                    ],
                    ninjas: world_pool.inactive_objects.new(),
                },
            }
        }
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct SnapshotMatchManager {
        pub game_match: Match,
    }

    impl SnapshotMatchManager {
        pub fn new(game_match: Match) -> Self {
            Self { game_match }
        }
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct SnapshotStage {
        pub world: SnapshotWorld,
        pub match_manager: SnapshotMatchManager,

        pub game_el_id: StageId,
        pub stage_name: PoolNetworkString<MAX_TEAM_NAME_LEN>,
        pub stage_color: ubvec4,
    }

    #[derive(Serialize, Deserialize)]
    pub struct SnapshotPlayer {
        pub game_el_id: PlayerId,
        pub character_info: PlayerCharacterInfo,
    }

    #[derive(Debug, Hiarc, Serialize, Deserialize)]
    pub struct SnapshotSpectatorPlayer {
        pub player: SpectatorPlayer,
    }

    pub struct SnapshotWorldPool {
        characters_pool: Pool<PoolSnapshotCharacters>,
        pub character_reusable_cores_pool: Pool<CharacterReusableCore>,
        projectiles_pool: Pool<PoolSnapshotProjectiles>,
        pub projectile_reusable_cores_pool: Pool<ProjectileReusableCore>,
        lasers_pool: Pool<PoolSnapshotLasers>,
        pub laser_reusable_cores_pool: Pool<LaserReusableCore>,
        pickups_pool: Pool<PoolSnapshotPickups>,
        pub pickup_reusable_cores_pool: Pool<PickupReusableCore>,
        flags_pool: Pool<PoolSnapshotFlags>,
        pub flag_reusable_cores_pool: Pool<FlagReusableCore>,
        inactive_objects: Pool<PoolSnapshotInactiveObjects>,
        pub character_ids_pool: Pool<FxHashSet<CharacterId>>,
    }

    impl SnapshotWorldPool {
        pub fn new(max_characters: usize) -> Self {
            Self {
                characters_pool: Pool::with_capacity(max_characters),
                // multiply by 2, because every character has two cores of this type
                character_reusable_cores_pool: Pool::with_capacity(max_characters * 2),
                projectiles_pool: Pool::with_capacity(1024), // TODO: no random number
                // multiply by 2, because every projectile has two cores of this type
                projectile_reusable_cores_pool: Pool::with_capacity(1024 * 2), // TODO: no random number
                lasers_pool: Pool::with_capacity(1024), // TODO: no random number
                // multiply by 2, because every laser has two cores of this type
                laser_reusable_cores_pool: Pool::with_capacity(1024 * 2), // TODO: no random number
                pickups_pool: Pool::with_capacity(1024),                  // TODO: no random number
                // multiply by 2, because every pickup has two cores of this type
                pickup_reusable_cores_pool: Pool::with_capacity(1024 * 2), // TODO: no random number
                flags_pool: Pool::with_capacity(16),                       // TODO: no random number
                // multiply by 2, because every flag has two cores of this type
                flag_reusable_cores_pool: Pool::with_capacity(16 * 2), // TODO: no random number
                inactive_objects: Pool::with_capacity(16 * 2),         // TODO: no random number
                character_ids_pool: Pool::with_capacity(16 * 2),
            }
        }
    }

    pub struct SnapshotPool {
        pub(crate) stages_pool: Pool<FxLinkedHashMap<StageId, SnapshotStage>>,
        spectator_players_pool: Pool<FxLinkedHashMap<PlayerId, SnapshotSpectatorPlayer>>,
        local_players_pool: Pool<FxLinkedHashMap<PlayerId, SnapshotLocalPlayer>>,
        string_pool: NetworkStringPool<MAX_TEAM_NAME_LEN>,
    }

    impl SnapshotPool {
        pub fn new(max_characters: usize, max_local_players: usize) -> Self {
            Self {
                stages_pool: Pool::with_capacity(max_characters),
                spectator_players_pool: Pool::with_capacity(max_characters),
                local_players_pool: Pool::with_capacity(max_local_players),
                string_pool: Pool::with_capacity(8),
            }
        }
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Snapshot {
        pub stages: PoolFxLinkedHashMap<StageId, SnapshotStage>,
        pub spectator_players: PoolFxLinkedHashMap<PlayerId, SnapshotSpectatorPlayer>,

        pub local_players: PoolFxLinkedHashMap<PlayerId, SnapshotLocalPlayer>,

        pub id_generator_id: GameEntityId,

        pub voted_player: Option<PlayerId>,

        pub global_tune_zone: Tunings,
    }

    impl Snapshot {
        pub fn new(
            pool: &SnapshotPool,
            id_generator_id: GameEntityId,
            voted_player: Option<PlayerId>,
            global_tune_zone: Tunings,
        ) -> Self {
            Self {
                stages: pool.stages_pool.new(),
                spectator_players: pool.spectator_players_pool.new(),
                local_players: pool.local_players_pool.new(),
                id_generator_id,
                voted_player,
                global_tune_zone,
            }
        }
    }

    /// this is closely build like the type [`GameStateCreateOptions`]
    #[derive(Debug, Default)]
    pub struct SnapshotManagerCreateOptions {
        hint_max_characters: Option<usize>,
        hint_max_local_players: Option<usize>,
    }

    pub struct SnapshotManager {
        // pools
        pub(crate) snapshot_pool: SnapshotPool,
        world_pool: SnapshotWorldPool,
    }

    impl SnapshotManager {
        pub fn new(options: &SnapshotManagerCreateOptions) -> Self {
            Self {
                snapshot_pool: SnapshotPool::new(
                    options.hint_max_characters.unwrap_or(64),
                    options.hint_max_local_players.unwrap_or(4),
                ),
                world_pool: SnapshotWorldPool::new(options.hint_max_local_players.unwrap_or(64)),
            }
        }

        pub(crate) fn build_stages(
            &self,
            stages: &mut PoolFxLinkedHashMap<StageId, SnapshotStage>,
            game: &GameState,
        ) {
            game.game.stages.values().for_each(|stage| {
                let mut characters = self.world_pool.characters_pool.new();
                stage.world.characters.iter().for_each(|(id, char)| {
                    let mode_to_snap_mode = |s: &CharacterSpectateMode| match s {
                        &CharacterSpectateMode::Free(pos) => {
                            SnapshotCharacterSpectateMode::Free(pos)
                        }
                        CharacterSpectateMode::Follows { ids, locked_zoom } => {
                            SnapshotCharacterSpectateMode::Follows {
                                ids: {
                                    let mut res_ids = self.world_pool.character_ids_pool.new();
                                    res_ids.extend(ids);
                                    res_ids
                                },
                                locked_zoom: *locked_zoom,
                            }
                        }
                    };
                    let mut snap_char = SnapshotCharacter {
                        core: char.core,
                        reusable_core: self.world_pool.character_reusable_cores_pool.new(),
                        pos: *char.pos.pos(),
                        phased: match &char.phased {
                            CharacterPhasedState::Normal(normal) => {
                                SnapshotCharacterPhasedState::Normal {
                                    hook: normal.hook.get(),
                                    ingame_spectate: normal
                                        .ingame_spectate
                                        .as_ref()
                                        .map(mode_to_snap_mode),
                                }
                            }
                            CharacterPhasedState::Dead(dead) => {
                                SnapshotCharacterPhasedState::Dead {
                                    respawn_in_ticks: dead.respawn_in_ticks,
                                }
                            }
                            CharacterPhasedState::PhasedSpectate(mode) => {
                                SnapshotCharacterPhasedState::PhasedSpectate(mode_to_snap_mode(
                                    mode,
                                ))
                            }
                        },
                        score: char.score.get(),
                        game_el_id: char.base.game_element_id,
                        ty: if let Some(network_stats) = char.is_player_character() {
                            SnapshotCharacterPlayerTy::Player(network_stats)
                        } else {
                            SnapshotCharacterPlayerTy::None
                        },
                        player_info: char.player_info.clone(),
                    };
                    snap_char.reusable_core.copy_clone_from(&char.reusable_core);
                    characters.insert(*id, snap_char);
                });
                let mut projectiles = self.world_pool.projectiles_pool.new();
                stage.world.get_projectiles().iter().for_each(|(id, proj)| {
                    let mut snap_proj = SnapshotProjectile {
                        core: proj.projectile.core,
                        reusable_core: self.world_pool.projectile_reusable_cores_pool.new(),
                        game_el_id: proj.projectile.base.game_element_id,
                        owner_game_el_id: proj.character_id,
                    };
                    snap_proj
                        .reusable_core
                        .copy_clone_from(&proj.projectile.reusable_core);
                    projectiles.insert(*id, snap_proj);
                });
                let mut lasers = self.world_pool.lasers_pool.new();
                stage.world.get_lasers().iter().for_each(|(id, laser)| {
                    let mut snap_laser = SnapshotLaser {
                        core: laser.laser.core,
                        reusable_core: self.world_pool.laser_reusable_cores_pool.new(),
                        game_el_id: laser.laser.base.game_element_id,
                        owner_game_el_id: laser.character_id,
                    };
                    snap_laser
                        .reusable_core
                        .copy_clone_from(&laser.laser.reusable_core);
                    lasers.insert(*id, snap_laser);
                });
                let mut pickups = self.world_pool.pickups_pool.new();
                stage.world.get_pickups().iter().for_each(|(id, pickup)| {
                    let mut snap_pickup = SnapshotPickup {
                        core: pickup.core,
                        reusable_core: self.world_pool.pickup_reusable_cores_pool.new(),
                        game_el_id: pickup.base.game_element_id,
                    };
                    snap_pickup
                        .reusable_core
                        .copy_clone_from(&pickup.reusable_core);
                    pickups.insert(*id, snap_pickup);
                });
                let mut red_flags = self.world_pool.flags_pool.new();
                let mut blue_flags = self.world_pool.flags_pool.new();
                let prepare_flags = |flags: &Flags, snap_flags: &mut PoolSnapshotFlags| {
                    flags.iter().for_each(|(id, flag)| {
                        let mut snap_flag = SnapshotFlag {
                            core: flag.core,
                            reusable_core: self.world_pool.flag_reusable_cores_pool.new(),
                            game_el_id: flag.base.game_element_id,
                        };
                        snap_flag.reusable_core.copy_clone_from(&flag.reusable_core);
                        snap_flags.insert(*id, snap_flag);
                    })
                };
                prepare_flags(stage.world.get_red_flags(), &mut red_flags);
                prepare_flags(stage.world.get_blue_flags(), &mut blue_flags);
                let add_inactive_obj =
                    |objs: &Vec<GameObjectWorld>, cont: &mut PoolSnapshotInactiveObjects| {
                        cont.extend(objs.iter().copied());
                    };

                let mut hearts = self.world_pool.inactive_objects.new();
                add_inactive_obj(
                    &stage.world.inactive_game_objects.pickups.hearts,
                    &mut hearts,
                );
                let mut shields = self.world_pool.inactive_objects.new();
                add_inactive_obj(
                    &stage.world.inactive_game_objects.pickups.shields,
                    &mut shields,
                );
                let mut inactive_red_flags = self.world_pool.inactive_objects.new();
                add_inactive_obj(
                    &stage.world.inactive_game_objects.pickups.red_flags,
                    &mut inactive_red_flags,
                );
                let mut inactive_blue_flags = self.world_pool.inactive_objects.new();
                add_inactive_obj(
                    &stage.world.inactive_game_objects.pickups.blue_flags,
                    &mut inactive_blue_flags,
                );
                let mut weapons: [_; WeaponType::COUNT] = [
                    self.world_pool.inactive_objects.new(),
                    self.world_pool.inactive_objects.new(),
                    self.world_pool.inactive_objects.new(),
                    self.world_pool.inactive_objects.new(),
                    self.world_pool.inactive_objects.new(),
                ];
                for (i, weapon) in weapons.iter_mut().enumerate() {
                    add_inactive_obj(
                        &stage.world.inactive_game_objects.pickups.weapons[i],
                        weapon,
                    );
                }
                let mut ninjas = self.world_pool.inactive_objects.new();
                add_inactive_obj(
                    &stage.world.inactive_game_objects.pickups.ninjas,
                    &mut ninjas,
                );

                stages.insert(
                    stage.game_element_id,
                    SnapshotStage {
                        world: SnapshotWorld {
                            characters,
                            projectiles,
                            lasers,
                            pickups,
                            red_flags,
                            blue_flags,
                            inactive_objects: SnapshotInactiveObject {
                                hearts,
                                shields,
                                red_flags: inactive_red_flags,
                                blue_flags: inactive_blue_flags,
                                weapons,
                                ninjas,
                            },
                        },
                        match_manager: SnapshotMatchManager::new(stage.match_manager.game_match),
                        game_el_id: stage.game_element_id,
                        stage_name: {
                            let mut name = self.snapshot_pool.string_pool.new();
                            (*name).clone_from(&stage.stage_name);
                            name
                        },
                        stage_color: stage.stage_color,
                    },
                );
            });
        }

        pub fn snapshot_for(&self, game: &GameState, snap_for: SnapshotFor) -> Snapshot {
            let mut res = Snapshot::new(
                &self.snapshot_pool,
                game.id_generator.peek_next_id(),
                game.game.voted_player,
                game.collision.tune_zones[0],
            );
            if let SnapshotFor::Client(client) = snap_for {
                match client {
                    SnapshotClientInfo::ForPlayerIds(ids)
                    | SnapshotClientInfo::OtherStagesForPlayerIds(ids) => {
                        res.local_players.reserve(ids.len());
                        ids.iter().for_each(|id| {
                            if let Some(p) = game.game.players.player(id).and_then(|p| {
                                game.game
                                    .stages
                                    .get(&p.stage_id())
                                    .and_then(|stage| stage.world.characters.get(id))
                            }) {
                                res.local_players.insert(
                                    *id,
                                    SnapshotLocalPlayer {
                                        id: p.player_info.id,
                                    },
                                );
                            } else if let Some(p) =
                                game.game.spectator_players.to_snapshot_local_player(id)
                            {
                                res.local_players.insert(*id, p);
                            }
                        });
                    }
                    SnapshotClientInfo::Everything => {
                        // nothing to do
                    }
                }
            }
            self.build_stages(&mut res.stages, game);

            let mut spectator_players = game.spectator_player_clone_pool.new();
            game.game
                .spectator_players
                .pooled_clone_into(&mut spectator_players);
            for (_, spectator_player) in spectator_players.drain() {
                res.spectator_players.insert(
                    spectator_player.id,
                    SnapshotSpectatorPlayer {
                        player: spectator_player,
                    },
                );
            }

            res
        }

        pub(crate) fn convert_to_game_stages(
            mut snap_stages: PoolFxLinkedHashMap<StageId, SnapshotStage>,
            stages: &mut Stages,
            world_pool: &WorldPool,
            game_object_definitions: &Rc<GameObjectDefinitions>,
            spawns: &Rc<GameSpawns>,
            id_gen: Option<&IdGenerator>,
            game_options: &GameOptions,
            players: &Players,
            spectator_players: &SpectatorPlayers,
            width: NonZeroU16,
            height: NonZeroU16,
            game_pending_events: &mut GamePendingEvents,
            game_pool: &GamePooling,
        ) {
            // drop all missing stages, we don't need the order here, since it will later be sorted anyway
            stages.retain(|id, stage| {
                // every stage that is not in the snapshot must be removed
                if let Some(snap_stage) = snap_stages.get(id) {
                    // same for characters
                    stage.world.characters.retain(|id, ent| {
                        if snap_stage.world.characters.contains_key(id) {
                            true
                        } else {
                            ent.drop_mode(DropMode::Silent);
                            false
                        }
                    });
                    // same for projectiles
                    stage.world.projectiles.retain(|id, ent| {
                        if snap_stage.world.projectiles.contains_key(id) {
                            true
                        } else {
                            ent.projectile.drop_mode(DropMode::Silent);
                            false
                        }
                    });
                    // same for lasers
                    stage.world.lasers.retain(|id, ent| {
                        if snap_stage.world.lasers.contains_key(id) {
                            true
                        } else {
                            ent.laser.drop_mode(DropMode::Silent);
                            false
                        }
                    });
                    // same for pickups
                    stage.world.pickups.retain(|id, ent| {
                        if snap_stage.world.pickups.contains_key(id) {
                            true
                        } else {
                            ent.drop_mode(DropMode::Silent);
                            false
                        }
                    });
                    // same for flags
                    let retain_flags = |flags: &mut Flags, snap_flags: &SnapshotFlags| {
                        flags.retain(|id, ent| {
                            if snap_flags.contains_key(id) {
                                true
                            } else {
                                ent.drop_mode(DropMode::Silent);
                                false
                            }
                        });
                    };
                    retain_flags(&mut stage.world.red_flags, &snap_stage.world.red_flags);
                    retain_flags(&mut stage.world.blue_flags, &snap_stage.world.blue_flags);

                    true
                } else {
                    false
                }
            });

            // go through stages, add missing ones stages
            snap_stages.drain().for_each(|(snap_stage_id, snap_stage)| {
                // if the stage is new, add it to our list
                if !stages.contains_key(&snap_stage_id) {
                    GameState::insert_new_stage(
                        stages,
                        snap_stage_id,
                        (*snap_stage.stage_name).clone(),
                        snap_stage.stage_color,
                        world_pool,
                        game_object_definitions,
                        spawns,
                        width,
                        height,
                        id_gen,
                        game_options.clone(),
                        game_pending_events,
                        false,
                    );
                }

                // sorting by always moving the entry to the end (all entries will do this)
                let state_stage = stages.to_back(&snap_stage_id).unwrap();

                let match_manager = &mut state_stage.match_manager;
                match_manager.game_match = snap_stage.match_manager.game_match;

                // go through all characters of the stage, add missing ones
                snap_stage.world.characters.values().for_each(|char| {
                    // if the character does not exist, add it
                    if !state_stage.world.characters.contains_key(&char.game_el_id) {
                        // make sure the player is not still existing in other lists
                        spectator_players.remove(&char.game_el_id);

                        state_stage.world.add_character(
                            char.game_el_id,
                            &snap_stage_id,
                            char.player_info.clone(),
                            Default::default(),
                            char.core.side,
                            match char.ty {
                                SnapshotCharacterPlayerTy::None => CharacterPlayerTy::None,
                                SnapshotCharacterPlayerTy::Player(network_stats) => {
                                    CharacterPlayerTy::Player {
                                        players: players.clone(),
                                        spectator_players: spectator_players.clone(),
                                        network_stats,
                                        stage_id: snap_stage_id,
                                    }
                                }
                            },
                            char.pos,
                            game_pool,
                        );

                        // sort
                        players.move_to_back(&char.game_el_id);
                    }

                    // sorting by always moving the entry to the end (all entries will do this)
                    let stage_char = state_stage
                        .world
                        .characters
                        .to_back(&char.game_el_id)
                        .unwrap();

                    stage_char.update_player_ty(
                        &snap_stage_id,
                        match char.ty {
                            SnapshotCharacterPlayerTy::None => CharacterPlayerTy::None,
                            SnapshotCharacterPlayerTy::Player(network_stats) => {
                                CharacterPlayerTy::Player {
                                    players: players.clone(),
                                    spectator_players: spectator_players.clone(),
                                    network_stats,
                                    stage_id: snap_stage_id,
                                }
                            }
                        },
                    );
                    stage_char.core = char.core;
                    stage_char
                        .reusable_core
                        .copy_clone_from(&char.reusable_core);
                    stage_char.player_info.clone_from(&char.player_info);
                });
                snap_stage.world.characters.values().for_each(|char| {
                    let stage_char = state_stage
                        .world
                        .characters
                        .get_mut(&char.game_el_id)
                        .unwrap();
                    // update hook & position in an extra loop, since they might
                    // depent on other characters (e.g. hooking an existing
                    // character only).
                    stage_char.pos.move_pos(char.pos);
                    let snap_mode_to_mode = |m: &SnapshotCharacterSpectateMode| match m {
                        &SnapshotCharacterSpectateMode::Free(pos) => {
                            CharacterSpectateMode::Free(pos)
                        }
                        SnapshotCharacterSpectateMode::Follows { ids, locked_zoom } => {
                            CharacterSpectateMode::Follows {
                                ids: (**ids).clone(),
                                locked_zoom: *locked_zoom,
                            }
                        }
                    };
                    match &char.phased {
                        SnapshotCharacterPhasedState::Normal {
                            hook: snap_hook,
                            ingame_spectate,
                        } => match &mut stage_char.phased {
                            CharacterPhasedState::Normal(normal) => {
                                normal.hook.set(snap_hook.0, snap_hook.1);
                                normal.ingame_spectate =
                                    ingame_spectate.as_ref().map(snap_mode_to_mode);
                            }
                            CharacterPhasedState::Dead { .. }
                            | CharacterPhasedState::PhasedSpectate(_) => {
                                stage_char.phased =
                                    CharacterPhasedState::Normal(CharacterPhaseNormal::new(
                                        char.game_el_id,
                                        char.pos,
                                        &state_stage.world.game_pending_events,
                                        {
                                            let mut hook = state_stage
                                                .world
                                                .hooks
                                                .get_new_hook(stage_char.base.game_element_id);
                                            hook.set(snap_hook.0, snap_hook.1);
                                            hook
                                        },
                                        ingame_spectate.as_ref().map(snap_mode_to_mode),
                                        true,
                                    ));
                            }
                        },
                        &SnapshotCharacterPhasedState::Dead {
                            respawn_in_ticks: snap_respawn_in_ticks,
                        } => match &mut stage_char.phased {
                            CharacterPhasedState::Normal { .. }
                            | CharacterPhasedState::PhasedSpectate(_) => {
                                stage_char.phased =
                                    CharacterPhasedState::Dead(CharacterPhaseDead::new(
                                        char.game_el_id,
                                        snap_respawn_in_ticks,
                                        char.pos,
                                        state_stage.world.phased_characters.clone(),
                                        None,
                                        GameWorldActionKillWeapon::World,
                                        Default::default(),
                                        &state_stage.world.simulation_events,
                                        &state_stage.world.game_pending_events,
                                        &stage_char.character_id_pool,
                                        true,
                                    ));
                            }
                            CharacterPhasedState::Dead(dead) => {
                                dead.respawn_in_ticks = snap_respawn_in_ticks;
                            }
                        },
                        SnapshotCharacterPhasedState::PhasedSpectate(mode) => {
                            stage_char.phased =
                                CharacterPhasedState::PhasedSpectate(snap_mode_to_mode(mode));
                        }
                    }
                    stage_char.score.set(char.score);
                });

                // go through all projectiles of the stage, add missing ones
                snap_stage.world.projectiles.values().for_each(|proj| {
                    // if the projectile does not exist, add it
                    if !state_stage.world.projectiles.contains_key(&proj.game_el_id) {
                        state_stage.world.insert_new_projectile(
                            proj.game_el_id,
                            proj.owner_game_el_id,
                            &proj.core.pos,
                            &proj.core.vel,
                            proj.core.life_span,
                            proj.core.damage,
                            proj.core.force,
                            proj.core.is_explosive,
                            proj.core.ty,
                            proj.core.side,
                        );
                    }

                    // sorting by always moving the entry to the end (all entries will do this)
                    let stage_proj = state_stage
                        .world
                        .projectiles
                        .to_back(&proj.game_el_id)
                        .unwrap();
                    stage_proj.projectile.core = proj.core;
                    stage_proj
                        .projectile
                        .reusable_core
                        .copy_clone_from(&proj.reusable_core);
                });
                // go through all lasers of the stage, add missing ones
                snap_stage.world.lasers.values().for_each(|laser| {
                    // if the laser does not exist, add it
                    if !state_stage.world.lasers.contains_key(&laser.game_el_id) {
                        state_stage.world.insert_new_laser(
                            laser.game_el_id,
                            laser.owner_game_el_id,
                            &laser.core.pos,
                            &laser.core.dir,
                            laser.core.energy,
                            laser.core.can_hit_others,
                            laser.core.can_hit_own,
                            laser.core.side,
                        );
                    }

                    // sorting by always moving the entry to the end (all entries will do this)
                    let stage_proj = state_stage.world.lasers.to_back(&laser.game_el_id).unwrap();
                    stage_proj.laser.core = laser.core;
                    stage_proj
                        .laser
                        .reusable_core
                        .copy_clone_from(&laser.reusable_core);
                });
                // go through all pickups of the stage, add missing ones
                snap_stage.world.pickups.values().for_each(|proj| {
                    // if the pickup does not exist, add it
                    if !state_stage.world.pickups.contains_key(&proj.game_el_id) {
                        state_stage.world.insert_new_pickup(
                            proj.game_el_id,
                            &proj.core.pos,
                            proj.core.ty,
                        );
                    }

                    // sorting by always moving the entry to the end (all entries will do this)
                    let stage_proj = state_stage.world.pickups.to_back(&proj.game_el_id).unwrap();
                    stage_proj.core = proj.core;
                    stage_proj
                        .reusable_core
                        .copy_clone_from(&proj.reusable_core);
                });
                // go through all flags of the stage, add missing ones
                let collect_flags = |flags: &mut Flags, snap_flags: &SnapshotFlags| {
                    snap_flags.values().for_each(|flag| {
                        // if the flag does not exist, add it
                        if !flags.contains_key(&flag.game_el_id) {
                            flags.insert(
                                flag.game_el_id,
                                Flag::new(
                                    &flag.game_el_id,
                                    &flag.core.pos,
                                    flag.core.ty,
                                    &state_stage.world.world_pool.flag_pool,
                                    &state_stage.world.game_pending_events,
                                    &state_stage.world.simulation_events,
                                ),
                            );
                        }

                        // sorting by always moving the entry to the end (all entries will do this)
                        let stage_proj = flags.to_back(&flag.game_el_id).unwrap();
                        stage_proj.core = flag.core;
                        stage_proj
                            .reusable_core
                            .copy_clone_from(&flag.reusable_core);
                    });
                };
                collect_flags(
                    &mut state_stage.world.red_flags,
                    &snap_stage.world.red_flags,
                );
                collect_flags(
                    &mut state_stage.world.blue_flags,
                    &snap_stage.world.blue_flags,
                );

                state_stage
                    .world
                    .inactive_game_objects
                    .pickups
                    .hearts
                    .clone_from(&snap_stage.world.inactive_objects.hearts);
                state_stage
                    .world
                    .inactive_game_objects
                    .pickups
                    .shields
                    .clone_from(&snap_stage.world.inactive_objects.shields);
                state_stage
                    .world
                    .inactive_game_objects
                    .pickups
                    .red_flags
                    .clone_from(&snap_stage.world.inactive_objects.red_flags);
                state_stage
                    .world
                    .inactive_game_objects
                    .pickups
                    .blue_flags
                    .clone_from(&snap_stage.world.inactive_objects.blue_flags);
                state_stage
                    .world
                    .inactive_game_objects
                    .pickups
                    .weapons
                    .iter_mut()
                    .enumerate()
                    .for_each(|(index, weapon)| {
                        weapon.clone_from(&snap_stage.world.inactive_objects.weapons[index])
                    });
                state_stage
                    .world
                    .inactive_game_objects
                    .pickups
                    .ninjas
                    .clone_from(&snap_stage.world.inactive_objects.ninjas);
            });
        }

        /// Writes a snapshot into a game state.
        /// It uses a mutable reference to reuse vector capacity, heap objects etc.
        #[must_use]
        pub fn build_from_snapshot(
            snapshot: Snapshot,
            write_game_state: &mut GameState,
        ) -> SnapshotLocalPlayers {
            // retain spectator players
            let spectator_players = &snapshot.spectator_players;
            write_game_state
                .game
                .spectator_players
                .retain_with_order(hi_closure!(
                    [spectator_players: &PoolFxLinkedHashMap<PlayerId, SnapshotSpectatorPlayer>],
                    |id: &PlayerId, _: &mut SpectatorPlayer| -> bool {
                        spectator_players.contains_key(id)
                    }
                ));

            let mut spectator_players = snapshot.spectator_players;
            spectator_players.drain().for_each(|(id, p)| {
                // make sure the player doesn't exist anywhere else
                if let Some(player) = write_game_state.game.players.player(&p.player.id) {
                    let stage = write_game_state
                        .game
                        .stages
                        .get_mut(&player.stage_id())
                        .unwrap();
                    stage
                        .world
                        .characters
                        .get_mut(&id)
                        .unwrap()
                        .drop_mode(DropMode::Silent);
                    stage.world.characters.remove(&id);
                }

                write_game_state.game.spectator_players.insert(id, p.player);

                // sort
                write_game_state.game.spectator_players.move_to_back(&id);
            });

            Self::convert_to_game_stages(
                snapshot.stages,
                &mut write_game_state.game.stages,
                &write_game_state.world_pool,
                &write_game_state.game_objects_definitions,
                &write_game_state.spawns,
                Some(&write_game_state.id_generator),
                &write_game_state.game_options,
                &write_game_state.game.players,
                &write_game_state.game.spectator_players,
                NonZeroU16::new(write_game_state.collision.get_playfield_width() as u16).unwrap(),
                NonZeroU16::new(write_game_state.collision.get_playfield_height() as u16).unwrap(),
                &mut write_game_state.game.game_pending_events,
                &write_game_state.game_pools,
            );

            write_game_state
                .id_generator
                .reset_id_for_client(snapshot.id_generator_id);

            write_game_state.game.voted_player = snapshot.voted_player;

            write_game_state.collision.tune_zones[0] = snapshot.global_tune_zone;

            snapshot.local_players
        }
    }
}
