pub mod state {
    use std::num::{NonZero, NonZeroU16, NonZeroU64};
    use std::rc::Rc;
    use std::sync::Arc;
    use std::time::Duration;

    use anyhow::anyhow;
    use base::hash::{fmt_hash, Hash};
    use base::linked_hash_map_view::FxLinkedHashMap;
    use base::network_string::{NetworkReducedAsciiString, NetworkString};
    use base_io::runtime::{IoRuntime, IoRuntimeTask};
    use command_parser::parser::{self, CommandArg, CommandArgType, CommandType, ParserCache, Syn};
    use config::parsing::parse_conf_values_as_str_list;
    use config::traits::ConfigInterface;
    use ddnet_accounts_types::account_id::AccountId;
    use game_base::config_helper::handle_config_variable_cmd;
    use game_database::traits::DbInterface;
    use game_interface::account_info::MAX_ACCOUNT_NAME_LEN;
    use game_interface::chat_commands::ChatCommands;
    use game_interface::client_commands::{
        ClientCameraMode, ClientCommand, JoinStage, MAX_TEAM_NAME_LEN,
    };
    use game_interface::events::{
        EventClientInfo, EventId, EventIdGenerator, GameEvents, GameWorldEvent, GameWorldEvents,
        GameWorldNotificationEvent, GameWorldSystemMessage,
    };
    use game_interface::ghosts::GhostResult;
    use game_interface::pooling::GamePooling;
    use game_interface::rcon_entries::{AuthLevel, ExecRconInput, RconEntries, RconEntry};
    use game_interface::settings::GameStateSettings;
    use game_interface::tick_result::TickResult;
    use game_interface::types::character_info::{
        NetworkCharacterInfo, NetworkLaserInfo, NetworkSkinInfo, MAX_ASSET_NAME_LEN,
        MAX_CHARACTER_NAME_LEN,
    };
    use game_interface::types::emoticons::EmoticonType;
    use game_interface::types::fixed_zoom_level::FixedZoomLevel;
    use game_interface::types::game::{GameTickCooldown, GameTickType};
    use game_interface::types::id_gen::{IdGenerator, IdGeneratorIdType};
    use game_interface::types::id_types::{
        CharacterId, CtfFlagId, LaserId, PickupId, PlayerId, ProjectileId, StageId,
    };
    use game_interface::types::input::{
        CharacterInput, CharacterInputConsumableDiff, CharacterInputFlags, CharacterInputInfo,
    };
    use game_interface::types::network_stats::PlayerNetworkStats;
    use game_interface::types::player_info::{PlayerClientInfo, PlayerDropReason, PlayerUniqueId};
    use game_interface::types::render::game::game_match::{
        FlagCarrierCharacter, LeadingCharacter, MatchSide, MatchStandings,
    };
    use game_interface::types::render::game::GameRenderInfo;
    use game_interface::types::render::stage::StageRenderInfo;
    use game_interface::types::render::world::WorldRenderInfo;
    use game_interface::types::resource_key::NetworkResourceKey;
    use game_interface::types::ticks::TickOptions;
    use game_interface::types::weapons::WeaponType;
    use game_interface::vote_commands::{VoteCommand, VoteCommandResult};
    use hiarc::hi_closure;
    use map::map::config::ConfigVariables;
    use map::map::Map;
    use math::math::lerp;
    use math::math::vector::{ubvec4, vec2};
    use pool::datatypes::{PoolFxHashMap, PoolFxLinkedHashMap, PoolVec};
    use pool::mt_datatypes::{PoolCow as MtPoolCow, PoolFxLinkedHashMap as MtPoolFxLinkedHashMap};
    use pool::pool::Pool;

    use game_base::mapdef_06::EntityTiles;
    use game_interface::interface::{
        GameStateCreate, GameStateCreateOptions, GameStateInterface, GameStateServerOptions,
        GameStateStaticInfo, MAX_MAP_NAME_LEN, MAX_PHYSICS_GAME_TYPE_NAME_LEN,
    };
    use game_interface::types::render::character::{
        CharacterBuff, CharacterBuffInfo, CharacterDebuff, CharacterDebuffInfo,
        CharacterHookRenderInfo, CharacterInfo, CharacterPlayerInfo, CharacterRenderInfo,
        LocalCharacterRenderInfo, LocalCharacterVanilla, PlayerCameraMode, PlayerIngameMode,
        TeeEye,
    };
    use game_interface::types::render::flag::FlagRenderInfo;
    use game_interface::types::render::laser::LaserRenderInfo;
    use game_interface::types::render::pickup::PickupRenderInfo;
    use game_interface::types::render::projectiles::ProjectileRenderInfo;
    use game_interface::types::render::scoreboard::{
        Scoreboard, ScoreboardCharacterInfo, ScoreboardConnectionType, ScoreboardGameOptions,
        ScoreboardGameType, ScoreboardGameTypeOptions, ScoreboardPlayerSpectatorInfo,
        ScoreboardScoreType, ScoreboardStageInfo,
    };
    use game_interface::types::snapshot::{SnapshotClientInfo, SnapshotLocalPlayers};
    use pool::rc::PoolRc;
    use rustc_hash::FxHashMap;

    use crate::collision::collision::Tunings;
    use crate::command_chain::{Command, CommandChain};
    use crate::config::config::{ConfigGameType, ConfigVanilla, ConfigVanillaWrapper};
    use crate::entities::character::character::{self, CharacterPlayerTy};
    use crate::entities::character::core::character_core::Core;
    use crate::entities::character::player::player::{
        Player, PlayerInfo, Players, SpectatorPlayer, SpectatorPlayers,
    };
    use crate::entities::flag::flag::{Flag, Flags};
    use crate::entities::laser::laser::Laser;
    use crate::entities::pickup::pickup::Pickup;
    use crate::entities::projectile::projectile::{self};
    use crate::game_objects::game_objects::GameObjectDefinitions;
    use crate::match_manager::match_manager::MatchManager;
    use crate::match_state::match_state::{MatchState, MatchType};
    use crate::simulation_pipe::simulation_pipe::{GamePendingEvents, GameStagePendingEvents};
    use crate::snapshot::snapshot::{Snapshot, SnapshotFor, SnapshotManager, SnapshotStage};
    use crate::sql::account_created::{self, AccountCreated};
    use crate::sql::account_info::{AccountInfo, StatementResult};
    use crate::sql::save;
    use crate::stage::stage::Stages;
    use crate::types::types::{GameOptions, GameType};
    use crate::weapons::definitions::weapon_def::Weapon;

    use super::super::{
        collision::collision::Collision, entities::character::character::Character,
        simulation_pipe::simulation_pipe::SimulationPipeStage, spawns::GameSpawns,
        stage::stage::GameStage, world::world::WorldPool,
    };

    use thiserror::Error;

    #[derive(Error, Debug)]
    pub enum GameError {
        #[error("Stage ID was not found")]
        InvalidStage,
    }

    pub(crate) const TICKS_PER_SECOND: u64 = 50;

    #[derive(Debug, Clone, Copy)]
    pub enum VanillaRconCommandCheat {
        WeaponsAll,
        Tune,
    }

    #[derive(Debug, Clone, Copy)]
    pub enum VanillaRconCommand {
        Info,
        Cheats(VanillaRconCommandCheat),
        ConfVariable,
    }

    pub struct Game {
        pub(crate) stages: Stages,

        pub players: Players,
        pub spectator_players: SpectatorPlayers,

        pub timeout_players: FxLinkedHashMap<(PlayerUniqueId, u64), (PlayerId, GameTickCooldown)>,

        pub game_pending_events: GamePendingEvents,

        pub voted_player: Option<PlayerId>,
    }

    #[derive(Debug)]
    pub enum GameDbQueries {
        AccountInfo {
            player_id: PlayerId,
            account_info: StatementResult,
        },
        AccountCreated {
            account_id: AccountId,
            cert_fingerprint: Hash,
            affected_rows: account_created::StatementAffected,
            err: Option<anyhow::Error>,
        },
    }

    pub struct GameStatements {
        account_created: AccountCreated,
    }

    pub struct GameDb {
        pub(crate) io_rt: IoRuntime,
        pub(crate) account_info: Option<AccountInfo>,
        pub(crate) statements: Option<GameStatements>,

        pub(crate) cur_queries: Vec<IoRuntimeTask<GameDbQueries>>,
        pub(crate) cur_queries_helper: Vec<IoRuntimeTask<GameDbQueries>>,
    }

    /// A game state is a collection of game related attributes such as the world,
    /// which handles the entities,
    /// the current tick, the starting tick, if the game is paused,
    /// the stages of the game etc.
    pub struct GameState {
        pub(crate) prev_game: Game,
        pub(crate) game: Game,

        pub(crate) id_generator: IdGenerator,
        pub(crate) event_id_generator: EventIdGenerator,

        pub player_events: FxHashMap<PlayerId, GameStagePendingEvents>,

        // only useful for server
        pub stage_0_id: StageId,

        // physics
        pub(crate) collision: Box<Collision>,
        pub(crate) spawns: Rc<GameSpawns>,
        /// empty definitions for previous state
        pub(crate) prev_game_objects_definitions: Rc<GameObjectDefinitions>,
        pub(crate) game_objects_definitions: Rc<GameObjectDefinitions>,

        // game
        pub(crate) game_options: GameOptions,

        pub(crate) chat_commands: ChatCommands,
        pub(crate) rcon_chain: CommandChain<VanillaRconCommand>,
        cache: ParserCache,
        map_name: NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,

        // db
        game_db: GameDb,

        // pooling
        pub(crate) world_pool: WorldPool,
        pub(crate) spectator_player_clone_pool: Pool<FxLinkedHashMap<PlayerId, SpectatorPlayer>>,
        player_clone_pool: Pool<Vec<(PlayerId, Player)>>,
        pub(crate) game_pools: GamePooling,

        // snapshot
        pub(crate) snap_shot_manager: SnapshotManager,
    }

    impl GameState {
        fn get_game_type_from_conf(conf: ConfigGameType) -> GameType {
            match conf {
                ConfigGameType::Ctf => GameType::Sided,
                ConfigGameType::Dm => GameType::Solo,
            }
        }

        fn get_mod_name_from_conf(
            conf: ConfigGameType,
        ) -> NetworkString<MAX_PHYSICS_GAME_TYPE_NAME_LEN> {
            match conf {
                ConfigGameType::Dm => "dm".try_into().unwrap(),
                ConfigGameType::Ctf => "ctf".try_into().unwrap(),
            }
        }

        fn is_sided_from_conf(conf: ConfigGameType) -> bool {
            matches!(conf, ConfigGameType::Ctf)
        }

        /// Returns the unhandled commands
        fn handle_initial_args(
            lines: Vec<String>,
            config: &mut ConfigVanilla,
            rcon_chain: &CommandChain<VanillaRconCommand>,
            cache: &ParserCache,
        ) -> (
            Vec<Result<NetworkString<65536>, NetworkString<65536>>>,
            Vec<parser::Command>,
        ) {
            let mut res: Vec<Result<NetworkString<65536>, NetworkString<65536>>> =
                Default::default();
            let mut res_cmds: Vec<parser::Command> = Default::default();
            for line in lines {
                let cmds = command_parser::parser::parse(&line, &rcon_chain.parser, cache);
                for cmd in cmds {
                    let handle_cmd = || match cmd {
                        CommandType::Full(cmd) => {
                            let Some(chain_cmd) = rcon_chain.by_ident(&cmd.ident) else {
                                return Err(anyhow!("Rcon command {} was not found", cmd.ident));
                            };

                            match chain_cmd.cmd {
                                VanillaRconCommand::ConfVariable => {
                                    let mut van_config = ConfigVanillaWrapper {
                                        vanilla: config.clone(),
                                    };
                                    match handle_config_variable_cmd(&cmd, &mut van_config).map(
                                        |msg| {
                                            format!("Updated value for {}: {}", cmd.cmd_text, msg)
                                        },
                                    ) {
                                        Ok(res) => {
                                            *config = van_config.vanilla;
                                            Ok(res)
                                        }
                                        Err(err) => Err(err),
                                    }
                                }
                                _ => {
                                    res_cmds.push(cmd);
                                    Ok("".into())
                                }
                            }
                        }
                        CommandType::Partial(cmd) => Err(anyhow!(
                            "Initial args expect full command, but was invalid: {cmd}"
                        )),
                    };

                    match handle_cmd() {
                        Ok(msg) => res.push(Ok(NetworkString::new_lossy(msg))),
                        Err(err) => res.push(Err(NetworkString::new_lossy(err.to_string()))),
                    }
                }
            }
            (res, res_cmds)
        }

        fn handle_map_config_variables(
            config: &mut ConfigVanilla,
            config_variables: ConfigVariables,
        ) {
            for (cmd, val) in config_variables {
                if cmd == "vanilla.game_type" {
                    if let Err(err) = config.try_set_from_str(
                        cmd.clone(),
                        None,
                        Some(val.value.clone()),
                        None,
                        config::traits::ConfigFromStrOperation::Set,
                    ) {
                        log::warn!(
                            "Failed to set config variable {cmd} to {}: {err}",
                            val.value
                        );
                    }
                } else {
                    log::warn!(
                        "UNSUPPORTED: config variable {} {} \
                        is not supported by this game mod and ignored",
                        cmd,
                        val.value
                    );
                }
            }
        }

        fn new_impl(
            map: Vec<u8>,
            map_name: NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
            options: GameStateCreateOptions,
            io_rt: IoRuntime,
            db: Arc<dyn DbInterface>,
        ) -> anyhow::Result<(Self, GameStateStaticInfo)>
        where
            Self: Sized,
        {
            let rcon_cmds = vec![
                (
                    "info".try_into().unwrap(),
                    Command {
                        rcon: RconEntry {
                            args: Default::default(),
                            description: "Prints information about this modification"
                                .try_into()
                                .unwrap(),
                            usage: "".try_into().unwrap(),
                        },
                        cmd: VanillaRconCommand::Info,
                    },
                ),
                (
                    "cheats.all_weapons".try_into().unwrap(),
                    Command {
                        rcon: RconEntry {
                            args: Default::default(),
                            description: "Gives the player all weapons (cheat)".try_into().unwrap(),
                            usage: "".try_into().unwrap(),
                        },
                        cmd: VanillaRconCommand::Cheats(VanillaRconCommandCheat::WeaponsAll),
                    },
                ),
                (
                    "cheats.tune".try_into().unwrap(),
                    Command {
                        rcon: RconEntry {
                            description: "Tunes a physics value to a given value"
                                .try_into()
                                .unwrap(),
                            usage: "<name> <val>".try_into().unwrap(),
                            args: vec![
                                CommandArg {
                                    ty: CommandArgType::TextFrom({
                                        let mut names: Vec<NetworkString<65536>> =
                                            Default::default();

                                        parse_conf_values_as_str_list(
                                            "".into(),
                                            &mut |entry, _| {
                                                names.push(entry.name.as_str().try_into().unwrap());
                                            },
                                            Tunings::conf_value(),
                                            "".into(),
                                            Default::default(),
                                        );

                                        names
                                    }),
                                    user_ty: None,
                                },
                                CommandArg {
                                    ty: CommandArgType::Float,
                                    user_ty: None,
                                },
                            ],
                        },
                        cmd: VanillaRconCommand::Cheats(VanillaRconCommandCheat::Tune),
                    },
                ),
            ];

            let mut rcon_vars: Vec<_> = Default::default();
            config::parsing::parse_conf_values_as_str_list(
                "".into(),
                &mut |add, _| {
                    rcon_vars.push((
                        add.name.try_into().unwrap(),
                        Command {
                            rcon: RconEntry {
                                args: add.args,
                                usage: add.usage.as_str().try_into().unwrap(),
                                description: add.description.as_str().try_into().unwrap(),
                            },
                            cmd: VanillaRconCommand::ConfVariable,
                        },
                    ));
                },
                ConfigVanillaWrapper::conf_value(),
                "".into(),
                Default::default(),
            );

            let rcon_chain = CommandChain::new(
                rcon_cmds.into_iter().collect(),
                rcon_vars.into_iter().collect(),
            );

            let cache = Default::default();

            let rcon_commands = RconEntries {
                cmds: rcon_chain
                    .cmd_list()
                    .iter()
                    .map(|(name, cmd)| (name.clone(), cmd.rcon.clone()))
                    .collect(),
                vars: rcon_chain
                    .var_list()
                    .iter()
                    .map(|(name, cmd)| (name.clone(), cmd.rcon.clone()))
                    .collect(),
            };

            let mut config: ConfigVanilla = options
                .config
                .and_then(|config| serde_json::from_slice(&config).ok())
                .unwrap_or_default();
            let write_config = config.clone();

            let (mut initial_args_res, remaining_cmds) = Self::handle_initial_args(
                options
                    .initial_rcon_input
                    .into_iter()
                    .map(|r| r.raw.into())
                    .collect(),
                &mut config,
                &rcon_chain,
                &cache,
            );

            let db_task = io_rt.spawn(async move {
                if !db.kinds().is_empty() {
                    if let Err(err) = save::setup(db.clone()).await {
                        log::warn!(
                            target: "sql",
                            "failed to setup databases: {}", err
                        );
                        return Err(err);
                    }

                    let acc_info = AccountInfo::new(db.clone(), options.account_db).await;
                    if let Err(err) = &acc_info {
                        log::warn!(
                        target: "sql", 
                        "failed to prepare account info sql: {}", err);
                    }

                    let account_created = match AccountCreated::new(db, options.account_db).await {
                        Ok(account_created) => Some(account_created),
                        Err(err) => {
                            log::warn!(
                            target: "sql", 
                            "failed to prepare account_created sql: {}", err);
                            None
                        }
                    };

                    let statements =
                        account_created.map(|account_created| GameStatements { account_created });

                    Ok(statements.zip(acc_info.ok()))
                } else {
                    Err(anyhow!("Databases not active."))
                }
            });

            let (physics_group, map_config) = Map::read_physics_group_and_config(&map)?;

            let w = physics_group.attr.width.get() as u32;
            let h = physics_group.attr.height.get() as u32;

            let tiles = physics_group.get_game_layer_tiles();

            let mut collision = Collision::new(&physics_group, true)?;

            // Always handle config variables before commands.
            Self::handle_map_config_variables(&mut config, map_config.config_variables);
            for cmd in map_config.commands {
                if let Some((cmd, val)) =
                    cmd.value
                        .split_once(char::is_whitespace)
                        .and_then(|(s1, s2)| {
                            (s1.trim() == "tune")
                                .then(|| s2.split_once(char::is_whitespace))
                                .flatten()
                        })
                {
                    if let Err(err) = collision.tune_zones[0].try_set_from_str(
                        cmd.to_string(),
                        None,
                        Some(val.to_string()),
                        None,
                        config::traits::ConfigFromStrOperation::Set,
                    ) {
                        log::warn!("Failed to apply global tune: {err}");
                    }
                } else {
                    log::warn!(
                        "UNSUPPORTED: command {} \
                        is not supported by this game mod and ignored",
                        cmd.value
                    );
                }
            }

            let game_objects = GameObjectDefinitions::new(tiles, w, h);

            let mut spawns: Vec<vec2> = Default::default();
            let mut spawns_red: Vec<vec2> = Default::default();
            let mut spawns_blue: Vec<vec2> = Default::default();
            tiles.iter().enumerate().for_each(|(index, tile)| {
                let x = index % w as usize;
                let y = index / w as usize;
                let pos = vec2::new(x as f32 * 32.0 + 16.0, y as f32 * 32.0 + 16.0);
                if tile.index == EntityTiles::Spawn as u8 {
                    spawns.push(pos);
                } else if tile.index == EntityTiles::SpawnRed as u8 {
                    spawns_red.push(pos);
                } else if tile.index == EntityTiles::SpawnBlue as u8 {
                    spawns_blue.push(pos);
                }
            });
            let id_generator = IdGenerator::new();

            let game_type = Self::get_game_type_from_conf(config.game_type);

            let (statements, account_info) = db_task.get_storage().ok().flatten().unzip();

            let has_accounts = account_info.is_some();

            let chat_commands = ChatCommands {
                cmds: vec![("account_info".try_into().unwrap(), vec![])]
                    .into_iter()
                    .collect(),
                prefixes: vec!['/'],
            };

            let mut game = Self {
                game: Game {
                    stages: Default::default(),
                    players: Players::new(),
                    spectator_players: SpectatorPlayers::new(),
                    timeout_players: Default::default(),
                    game_pending_events: GamePendingEvents::default(),
                    voted_player: None,
                },
                prev_game: Game {
                    stages: Default::default(),
                    players: Players::new(),
                    spectator_players: SpectatorPlayers::new(),
                    timeout_players: Default::default(),
                    game_pending_events: GamePendingEvents::default(),
                    voted_player: None,
                },

                player_events: Default::default(),

                // server
                stage_0_id: id_generator.next_id(), // TODO: few lines later the stage_id gets reassigned, but too lazy to improve it rn

                // physics
                collision,
                spawns: Rc::new(GameSpawns {
                    spawns,
                    spawns_red,
                    spawns_blue,
                }),
                game_objects_definitions: Rc::new(game_objects),
                prev_game_objects_definitions: Rc::new(GameObjectDefinitions {
                    pickups: Default::default(),
                }),

                // game
                game_options: GameOptions::new(game_type, config.clone()),
                chat_commands: chat_commands.clone(),
                rcon_chain,
                cache,
                map_name,

                // db
                game_db: GameDb {
                    io_rt,
                    account_info,
                    statements,

                    cur_queries: Default::default(),
                    cur_queries_helper: Default::default(),
                },

                // pool
                world_pool: WorldPool::new(options.hint_max_characters.unwrap_or(64)),
                spectator_player_clone_pool: Pool::with_capacity(2),
                player_clone_pool: Pool::with_capacity(2),
                game_pools: GamePooling::new(options.hint_max_characters),

                id_generator,
                event_id_generator: Default::default(),

                // snapshot
                snap_shot_manager: SnapshotManager::new(&Default::default()),
            };
            game.stage_0_id = game.add_stage(Default::default(), ubvec4::new(0, 0, 0, 0));

            for cmd in remaining_cmds {
                match game.handle_full_command(None, cmd) {
                    Ok(res) => initial_args_res.push(Ok(NetworkString::new_lossy(res))),
                    Err(err) => {
                        initial_args_res.push(Err(NetworkString::new_lossy(err.to_string())))
                    }
                }
            }

            Ok((
                game,
                GameStateStaticInfo {
                    ticks_in_a_second: NonZero::new(TICKS_PER_SECOND).unwrap(),
                    chat_commands,
                    rcon_commands,

                    config: serde_json::to_vec(&write_config).ok(),
                    initial_rcon_response: initial_args_res,

                    mod_name: Self::get_mod_name_from_conf(config.game_type),
                    version: "pre-alpha".try_into().unwrap(),
                    options: GameStateServerOptions {
                        physics_group_name: "vanilla".try_into().unwrap(),
                        allow_stages: config.allow_stages,
                        use_vanilla_sides: Self::is_sided_from_conf(config.game_type),
                        use_account_name: has_accounts,
                        forced_ingame_camera_zoom: Some(FixedZoomLevel::new_lossy(1.0)),
                        allows_voted_player_miniscreen: config.allow_player_vote_cam,
                        ghosts: false,
                        has_ingame_freecam: false,
                    },
                },
            ))
        }
    }

    impl GameStateCreate for GameState {
        fn new(
            map: Vec<u8>,
            map_name: NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
            options: GameStateCreateOptions,
            io_rt: IoRuntime,
            db: Arc<dyn DbInterface>,
        ) -> Result<(Self, GameStateStaticInfo), NetworkString<1024>>
        where
            Self: Sized,
        {
            Self::new_impl(map, map_name, options, io_rt, db)
                .map_err(|err| NetworkString::new_lossy(err.to_string()))
        }
    }

    impl GameState {
        fn add_stage(
            &mut self,
            name: NetworkString<MAX_TEAM_NAME_LEN>,
            stage_color: ubvec4,
        ) -> StageId {
            let stage_id = self.id_generator.next_id();
            self.game.stages.insert(
                stage_id,
                GameStage::new(
                    name,
                    stage_color,
                    stage_id,
                    &self.world_pool,
                    &self.game_objects_definitions,
                    &self.spawns,
                    NonZeroU16::new(self.collision.get_playfield_width() as u16).unwrap(),
                    NonZeroU16::new(self.collision.get_playfield_height() as u16).unwrap(),
                    Some(&self.id_generator),
                    self.game_options.clone(),
                    self.game.game_pending_events.init_stage(stage_id),
                    true,
                ),
            );
            stage_id
        }

        pub fn add_char_to_stage<'a>(
            stages: &'a mut Stages,
            stage_id: &StageId,
            character_id: &CharacterId,
            player_info: PlayerInfo,
            player_input: CharacterInput,
            players: Players,
            spectator_players: SpectatorPlayers,
            network_stats: PlayerNetworkStats,
            forced_side: Option<MatchSide>,
            initial_score: i64,
            default_eyes: TeeEye,
            default_eyes_reset_in: GameTickCooldown,
            game_pool: &GamePooling,
        ) -> &'a mut Character {
            Self::add_char_to_stage_checked(
                stages,
                stage_id,
                character_id,
                player_info,
                player_input,
                players,
                spectator_players,
                network_stats,
                forced_side,
                initial_score,
                default_eyes,
                default_eyes_reset_in,
                game_pool,
            )
            .unwrap()
        }

        pub(crate) fn add_char_to_stage_checked<'a>(
            stages: &'a mut Stages,
            stage_id: &StageId,
            character_id: &CharacterId,
            player_info: PlayerInfo,
            player_input: CharacterInput,
            players: Players,
            spectator_players: SpectatorPlayers,
            network_stats: PlayerNetworkStats,
            forced_side: Option<MatchSide>,
            initial_score: i64,
            default_eyes: TeeEye,
            default_eyes_reset_in: GameTickCooldown,
            game_pool: &GamePooling,
        ) -> anyhow::Result<&'a mut Character> {
            let stage = stages.get_mut(stage_id).ok_or(GameError::InvalidStage)?;

            let side = match stage.match_manager.game_match.ty {
                MatchType::Solo => None,
                MatchType::Sided { .. } => {
                    forced_side.or_else(|| Some(stage.world.evaluate_character_side()))
                }
            };

            // TODO: remove this log (move it somewhere)
            log::info!(target: "world", "added a character into side {:?}", side);

            let pos = stage.world.get_spawn_pos(side);

            let char = stage.world.add_character(
                *character_id,
                stage_id,
                player_info,
                player_input,
                side,
                CharacterPlayerTy::Player {
                    players,
                    spectator_players,
                    network_stats,
                    stage_id: *stage_id,
                },
                pos,
                game_pool,
            );
            char.score.set(initial_score);
            char.core.eye = default_eyes;
            char.core.default_eye = default_eyes;
            char.core.default_eye_reset_in = default_eyes_reset_in;
            Ok(char)
        }

        pub(crate) fn insert_new_stage(
            stages: &mut Stages,
            stage_id: StageId,
            stage_name: NetworkString<MAX_TEAM_NAME_LEN>,
            stage_color: ubvec4,
            world_pool: &WorldPool,
            game_object_definitions: &Rc<GameObjectDefinitions>,
            spawns: &Rc<GameSpawns>,
            width: NonZeroU16,
            height: NonZeroU16,
            id_gen: Option<&IdGenerator>,
            game_options: GameOptions,
            game_pending_events: &mut GamePendingEvents,
            spawn_default_entities: bool,
        ) {
            stages.insert(
                stage_id,
                GameStage::new(
                    stage_name,
                    stage_color,
                    stage_id,
                    world_pool,
                    game_object_definitions,
                    spawns,
                    width,
                    height,
                    id_gen,
                    game_options,
                    game_pending_events.init_stage(stage_id),
                    spawn_default_entities,
                ),
            );
        }

        fn tick_impl(&mut self, is_prediction: bool) {
            for stage in self.game.stages.values_mut() {
                let stage_id = stage.game_element_id;
                let mut sim_pipe = SimulationPipeStage::new(
                    is_prediction,
                    &self.collision,
                    &stage_id,
                    &self.world_pool,
                );

                stage.tick(&mut sim_pipe);
            }
        }

        pub fn player_tick(&mut self) {
            let mut kick_players = Vec::new();
            self.game.timeout_players.retain(|_, player| {
                if player.1.tick().unwrap_or_default() {
                    kick_players.push(player.0);
                    false
                } else {
                    true
                }
            });
            for kick_player in kick_players {
                self.player_drop(&kick_player, PlayerDropReason::Timeout);
            }
        }

        fn query_tick(&mut self) {
            self.game_db.cur_queries_helper.clear();
            for query in self.game_db.cur_queries.drain(..) {
                if query.is_finished() {
                    match query.get_storage() {
                        Ok(query) => match query {
                            GameDbQueries::AccountInfo {
                                player_id,
                                account_info: info,
                            } => {
                                let events = self.player_events.entry(player_id).or_default();
                                events.push(GameWorldEvent::Notification(
                                    GameWorldNotificationEvent::System(
                                        GameWorldSystemMessage::Custom({
                                            let mut s =
                                                self.game_pools.mt_network_string_common_pool.new();
                                            s.try_set(format!(
                                                "user account information:\n\
                                                id: {}\n\
                                                name: {}\n\
                                                creation: {}",
                                                info.id,
                                                info.name,
                                                <chrono::DateTime<chrono::Utc>>::from_timestamp(
                                                    info.create_time.secs as i64,
                                                    info.create_time.subsec_nanos
                                                )
                                                .unwrap()
                                            ))
                                            .unwrap();
                                            s
                                        }),
                                    ),
                                ));

                                if let Some(character) =
                                    self.game.players.player(&player_id).map(|char_info| {
                                        self.game
                                            .stages
                                            .get_mut(&char_info.stage_id())
                                            .unwrap()
                                            .world
                                            .characters
                                            .get_mut(&player_id)
                                            .unwrap()
                                    })
                                {
                                    if character.player_info.account_name.is_none() {
                                        character.player_info.account_name =
                                            Some(info.name.as_str().try_into().unwrap());
                                    }
                                } else {
                                    let name = info.name.as_str();
                                    self.game.spectator_players.handle_mut(
                                        &player_id,
                                        hi_closure!(
                                            [name: &str],
                                            |player: &mut SpectatorPlayer| -> () {
                                                if player.player_info.account_name.is_none() {
                                                    player.player_info.account_name = Some(name.try_into().unwrap());
                                                }
                                            }
                                        ),
                                    );
                                }
                            }
                            GameDbQueries::AccountCreated {
                                account_id,
                                cert_fingerprint,
                                affected_rows,
                                err,
                            } => {
                                log::info!(
                                    "Rewrote {} save for account {} using hash {}",
                                    affected_rows.rewrite_saves,
                                    account_id,
                                    fmt_hash(&cert_fingerprint),
                                );
                                if let Some(err) = err {
                                    log::error!(
                                        "During the rewriting the following error occurred: {err}"
                                    );
                                }
                            }
                        },
                        Err(err) => {
                            log::warn!("query failed: {err}");
                        }
                    }
                } else {
                    self.game_db.cur_queries_helper.push(query);
                }
            }
            std::mem::swap(
                &mut self.game_db.cur_queries_helper,
                &mut self.game_db.cur_queries,
            );
        }

        fn set_player_inp_impl(
            &mut self,
            player_id: &PlayerId,
            inp: &CharacterInput,
            diff: CharacterInputConsumableDiff,
        ) {
            if let Some(player) = self.game.players.player(player_id) {
                let stages = &mut self.game.stages;
                let character = stages
                    .get_mut(&player.stage_id())
                    .unwrap()
                    .world
                    .characters
                    .get_mut(player_id)
                    .unwrap();
                character.core.input = *inp;
                let stage = stages.get_mut(&player.stage_id()).unwrap();
                if matches!(
                    stage.match_manager.game_match.state,
                    MatchState::Running { .. }
                        | MatchState::Paused { .. }
                        | MatchState::SuddenDeath { .. }
                        | MatchState::PausedSuddenDeath { .. }
                ) {
                    stage
                        .world
                        .handle_character_input_change(&self.collision, player_id, diff);
                }
            } else if self.game.spectator_players.contains_key(player_id) {
                self.game.spectator_players.handle_mut(
                    player_id,
                    hi_closure!(
                        [
                            inp: &CharacterInput,
                        ],
                        |spectator_player: &mut SpectatorPlayer| -> () {
                            spectator_player.player_input = *inp;
                        }
                    ),
                );
            }
        }

        fn snapshot_for_impl(&self, snap_for: SnapshotFor) -> MtPoolCow<'static, [u8]> {
            let snapshot = self.snap_shot_manager.snapshot_for(self, snap_for);
            let mut res = self.game_pools.snapshot_pool.new();
            let writer: &mut Vec<_> = res.to_mut();
            bincode::serde::encode_into_std_write(&snapshot, writer, bincode::config::standard())
                .unwrap();
            res
        }

        fn push_account_info_task(
            game_db: &mut GameDb,
            player_id: &PlayerId,
            unique_identifier: &PlayerUniqueId,
        ) {
            if let (Some(account_info), PlayerUniqueId::Account(account_id)) =
                (&game_db.account_info, unique_identifier)
            {
                let account_info = account_info.clone();
                let account_id = *account_id;
                let player_id = *player_id;
                game_db.cur_queries.push(game_db.io_rt.spawn(async move {
                    Ok(GameDbQueries::AccountInfo {
                        player_id,
                        account_info: account_info.fetch(account_id).await?,
                    })
                }));
            }
        }

        fn cmd_account_info(game_db: &mut GameDb, player_id: &PlayerId, character: &Character) {
            Self::push_account_info_task(
                game_db,
                player_id,
                &character.player_info.unique_identifier,
            )
        }

        fn handle_chat_commands(&mut self, player_id: &PlayerId, cmds: Vec<CommandType>) {
            let Some(server_player) = self.game.players.player(player_id) else {
                return;
            };
            let Some(character) = self
                .game
                .stages
                .get(&server_player.stage_id())
                .and_then(|stage| stage.world.characters.get(player_id))
            else {
                return;
            };
            for cmd in cmds {
                match cmd {
                    CommandType::Full(cmd) => {
                        match cmd.ident.as_str() {
                            "account_info" => {
                                Self::cmd_account_info(&mut self.game_db, player_id, character);
                            }
                            _ => {
                                // TODO: send command not found text
                            }
                        }
                    }
                    CommandType::Partial(_) => {
                        // TODO: ignore for now
                        // send back feedback to user
                    }
                }
            }
        }

        fn handle_full_command(
            &mut self,
            player_id: Option<&PlayerId>,
            mut cmd: parser::Command,
        ) -> anyhow::Result<String> {
            let Some(chain_cmd) = self.rcon_chain.by_ident(&cmd.ident) else {
                return Err(anyhow!("Rcon command {} was not found", cmd.ident));
            };

            match chain_cmd.cmd {
                VanillaRconCommand::Info => {
                    self.game
                        .stages
                        .get(&self.stage_0_id)
                        .unwrap()
                        .game_pending_events
                        .push(GameWorldEvent::Notification(
                            GameWorldNotificationEvent::System(GameWorldSystemMessage::Custom({
                                let mut s = self.game_pools.mt_network_string_common_pool.new();
                                s.try_set("You are playing vanilla.").unwrap();
                                s
                            })),
                        ));
                    anyhow::Ok("You are playing vanilla.".to_string())
                }
                VanillaRconCommand::Cheats(cheat) => match cheat {
                    VanillaRconCommandCheat::WeaponsAll => {
                        let Some(player_id) = player_id else {
                            return Err(anyhow!(
                                "Weapon cheat command must be executed by an actual player"
                            ));
                        };
                        let Some(character_info) = self.game.players.player(player_id) else {
                            return Err(anyhow!("The given player was not found in this game"));
                        };
                        if let Some(character) = self
                            .game
                            .stages
                            .get_mut(&character_info.stage_id())
                            .and_then(|stage| stage.world.characters.get_mut(player_id))
                        {
                            let reusable_core = &mut character.reusable_core;
                            let gun = Weapon {
                                cur_ammo: Some(10),
                                next_ammo_regeneration_tick: 0.into(),
                            };
                            reusable_core.weapons.insert(WeaponType::Gun, gun);
                            reusable_core.weapons.insert(WeaponType::Shotgun, gun);
                            reusable_core.weapons.insert(WeaponType::Grenade, gun);
                            reusable_core.weapons.insert(WeaponType::Laser, gun);

                            Ok("Cheated all weapons!".to_string())
                        } else {
                            Err(anyhow!("The given player was not found in this game"))
                        }
                    }
                    VanillaRconCommandCheat::Tune => {
                        let Some(Syn::Float(val)) = cmd.args.pop().map(|(name, _)| name) else {
                            panic!("Expected a float, this is an implementation bug");
                        };
                        let Some(Syn::Text(path)) = cmd.args.pop().map(|(name, _)| name) else {
                            panic!("Expected a text, this is an implementation bug");
                        };

                        match self.collision.tune_zones[0].try_set_from_str(
                            path,
                            None,
                            Some(val),
                            None,
                            Default::default(),
                        ) {
                            Ok(res) => Ok(res),
                            Err(err) => {
                                log::error!("{err}");
                                Err(err.into())
                            }
                        }
                    }
                },
                VanillaRconCommand::ConfVariable => {
                    let mut config = ConfigVanillaWrapper {
                        vanilla: self.game_options.config_clone(),
                    };
                    match handle_config_variable_cmd(&cmd, &mut config)
                        .map(|msg| format!("Updated value for {}: {}", cmd.cmd_text, msg))
                    {
                        Ok(res) => {
                            self.game_options.replace_conf(config.vanilla);
                            Ok(res)
                        }
                        Err(err) => Err(err),
                    }
                }
            }
        }

        fn handle_rcon_commands(
            &mut self,
            player_id: Option<&PlayerId>,
            _auth: AuthLevel,
            cmds: Vec<CommandType>,
        ) -> Vec<Result<NetworkString<65536>, NetworkString<65536>>> {
            let mut res: Vec<Result<NetworkString<65536>, NetworkString<65536>>> =
                Default::default();
            for cmd in cmds {
                let handle_cmd = || match cmd {
                    CommandType::Full(cmd) => self.handle_full_command(player_id, cmd),
                    CommandType::Partial(cmd) => {
                        let Some(cmd) = cmd.ref_cmd_partial() else {
                            return Err(anyhow!("This command was invalid: {cmd}"));
                        };
                        let Some(chain_cmd) = self.rcon_chain.by_ident(&cmd.ident) else {
                            return Err(anyhow!("Command {} not found", cmd.ident));
                        };

                        if let VanillaRconCommand::ConfVariable = chain_cmd.cmd {
                            let mut config = ConfigVanillaWrapper {
                                vanilla: self.game_options.config_clone(),
                            };
                            match handle_config_variable_cmd(cmd, &mut config)
                                .map(|msg| format!("Current value for {}: {}", cmd.cmd_text, msg))
                            {
                                Ok(res) => {
                                    self.game_options.replace_conf(config.vanilla);
                                    Ok(res)
                                }
                                Err(err) => Err(err),
                            }
                        } else {
                            Err(anyhow!("Failed to handle config variable: {cmd}"))
                        }
                    }
                };

                match handle_cmd() {
                    Ok(msg) => res.push(Ok(NetworkString::new_lossy(msg))),
                    Err(err) => res.push(Err(NetworkString::new_lossy(err.to_string()))),
                }
            }
            res
        }

        fn build_prev_from_stages(
            &mut self,
            snap_stages: PoolFxLinkedHashMap<StageId, SnapshotStage>,
        ) {
            SnapshotManager::convert_to_game_stages(
                snap_stages,
                &mut self.prev_game.stages,
                &self.world_pool,
                &self.prev_game_objects_definitions,
                &self.spawns,
                None,
                &self.game_options,
                &self.prev_game.players,
                &self.prev_game.spectator_players,
                NonZeroU16::new(self.collision.get_playfield_width() as u16).unwrap(),
                NonZeroU16::new(self.collision.get_playfield_height() as u16).unwrap(),
                &mut self.prev_game.game_pending_events,
                &self.game_pools,
            );
            self.prev_game.game_pending_events.clear_events();
        }

        // rendering related
        fn stage_projectiles(
            &self,
            prev_stage: &GameStage,
            stage: Option<&GameStage>,
            ratio: f64,
        ) -> PoolFxLinkedHashMap<ProjectileId, ProjectileRenderInfo> {
            let mut res = self.game_pools.projectile_render_info_pool.new();
            let Some(stage) = stage else {
                return res;
            };
            res.extend(
                prev_stage
                    .world
                    .projectiles
                    .iter()
                    .filter_map(|(&id, prev_proj)| {
                        let proj = stage.world.projectiles.get(&id)?;
                        Some((
                            id,
                            ProjectileRenderInfo {
                                ty: prev_proj.projectile.core.ty,
                                pos: projectile::lerped_pos(
                                    &prev_proj.projectile,
                                    &proj.projectile,
                                    ratio,
                                ) / 32.0,
                                vel: projectile::estimated_fly_direction(
                                    &prev_proj.projectile,
                                    &proj.projectile,
                                    ratio,
                                ) / 32.0,
                                owner_id: Some(proj.character_id),
                                phased: false,
                            },
                        ))
                    }),
            );
            res
        }

        fn stage_ctf_flags(
            &self,
            prev_stage: &GameStage,
            stage: Option<&GameStage>,
            ratio: f64,
        ) -> PoolFxLinkedHashMap<CtfFlagId, FlagRenderInfo> {
            let mut res = self.game_pools.flag_render_info_pool.new();
            let Some(stage) = stage else {
                return res;
            };
            let mut collect_flags = |prev_flags: &Flags, flags: &Flags| {
                res.extend(prev_flags.iter().filter_map(|(&id, prev_flag)| {
                    let flag = flags.get(&id)?;
                    // use current flag if non linear event occurred
                    let (pos, flag) =
                        if flag.core.non_linear_event != prev_flag.core.non_linear_event {
                            // try to use carrier position instead
                            let pos = prev_flag
                                .core
                                .carrier
                                .and_then(|id| {
                                    prev_stage
                                        .world
                                        .characters
                                        .get(&id)
                                        .zip(stage.world.characters.get(&id))
                                })
                                .map(|(prev_char, char)| {
                                    self.stage_character_render_info(
                                        prev_stage, stage, prev_char, char, ratio,
                                    )
                                    .lerped_pos
                                })
                                .unwrap_or(prev_flag.core.pos);
                            (pos, prev_flag)
                        } else {
                            (Flag::lerped_pos(prev_flag, flag, ratio) / 32.0, flag)
                        };
                    Some((
                        id,
                        FlagRenderInfo {
                            pos,
                            ty: prev_flag.core.ty,
                            owner_id: flag.core.carrier,
                            phased: false,
                        },
                    ))
                }));
            };
            collect_flags(&prev_stage.world.red_flags, &stage.world.red_flags);
            collect_flags(&prev_stage.world.blue_flags, &stage.world.blue_flags);
            res
        }

        fn stage_lasers(
            &self,
            prev_stage: &GameStage,
            stage: Option<&GameStage>,
            ratio: f64,
        ) -> PoolFxLinkedHashMap<LaserId, LaserRenderInfo> {
            let mut res = self.game_pools.laser_render_info_pool.new();
            let Some(stage) = stage else {
                return res;
            };
            res.extend(
                prev_stage
                    .world
                    .lasers
                    .iter()
                    .filter_map(|(&id, prev_laser)| {
                        let laser = stage.world.lasers.get(&id)?;
                        if laser.laser.core.next_eval_in.is_none() {
                            return None;
                        }
                        Some((
                            id,
                            LaserRenderInfo {
                                ty: prev_laser.laser.core.ty,
                                pos: Laser::lerped_pos(&prev_laser.laser, &laser.laser, ratio)
                                    / 32.0,
                                from: Laser::lerped_from(&prev_laser.laser, &laser.laser, ratio)
                                    / 32.0,
                                eval_tick_ratio: prev_laser.laser.eval_tick_ratio(),
                                owner_id: Some(prev_laser.character_id),
                                phased: false,
                            },
                        ))
                    }),
            );
            res
        }

        fn stage_pickups(
            &self,
            prev_stage: &GameStage,
            stage: Option<&GameStage>,
            ratio: f64,
        ) -> PoolFxLinkedHashMap<PickupId, PickupRenderInfo> {
            let mut res = self.game_pools.pickup_render_info_pool.new();
            let Some(stage) = stage else {
                return res;
            };
            res.extend(
                prev_stage
                    .world
                    .pickups
                    .iter()
                    .filter_map(|(&id, prev_pickup)| {
                        let pickup = stage.world.pickups.get(&id)?;
                        Some((
                            id,
                            PickupRenderInfo {
                                ty: prev_pickup.core.ty,
                                pos: Pickup::lerped_pos(prev_pickup, pickup, ratio) / 32.0,
                                owner_id: None,
                                phased: false,
                            },
                        ))
                    }),
            );
            res
        }

        fn stage_character_render_info(
            &self,
            prev_stage: &GameStage,
            stage: &GameStage,
            prev_character: &Character,
            character: &Character,
            intra_tick_ratio: f64,
        ) -> CharacterRenderInfo {
            let lerped_pos = character::lerp_core_pos(prev_character, character, intra_tick_ratio);
            CharacterRenderInfo {
                lerped_pos: lerped_pos / 32.0,
                lerped_vel: character::lerp_core_vel(prev_character, character, intra_tick_ratio)
                    / 32.0,
                lerped_hook: {
                    // try special logic for when a character is hooked first.
                    let hooked_char = prev_character.phased.hook().hooked_char();
                    hooked_char
                        .and_then(|hooked_char_id| {
                            let prev_hooked_char = prev_stage.world.characters.get(&hooked_char_id);
                            let hooked_char = stage
                                .world
                                .characters
                                .get(&hooked_char_id)
                                .or(prev_hooked_char);
                            prev_hooked_char
                                .zip(hooked_char)
                                .map(|(prev_character, character)| {
                                    character::lerp_core_pos(
                                        prev_character,
                                        character,
                                        intra_tick_ratio,
                                    )
                                })
                        })
                        // else fall back to the latest known hook pos
                        .or_else(|| {
                            character::lerp_core_hook_pos(
                                prev_character,
                                character,
                                intra_tick_ratio,
                            )
                        })
                        .map(|pos| CharacterHookRenderInfo { pos, hooked_char })
                }
                .map(|mut hook| {
                    hook.pos /= 32.0;
                    hook
                }),
                hook_collision: prev_character
                    .core
                    .input
                    .state
                    .flags
                    .contains(CharacterInputFlags::HOOK_COLLISION_LINE)
                    .then(|| {
                        Core::hook_collision(
                            lerped_pos,
                            prev_character.core.input.cursor.to_vec2(),
                            &self.collision,
                            &prev_character.pos.field,
                            &prev_stage.world.characters,
                            prev_character.base.game_element_id,
                        )
                    }),
                has_air_jump: prev_character.core.core.jumped <= 1,
                lerped_cursor_pos: lerp(
                    &prev_character.core.input.cursor.to_vec2(),
                    &character.core.input.cursor.to_vec2(),
                    intra_tick_ratio,
                ),
                lerped_dyn_cam_offset: lerp(
                    &prev_character.core.input.dyn_cam_offset.to_vec2(),
                    &character.core.input.dyn_cam_offset.to_vec2(),
                    intra_tick_ratio,
                ),
                move_dir: *prev_character.core.input.state.dir,
                cur_weapon: prev_character.core.active_weapon,
                recoil_ticks_passed: prev_character.core.attack_recoil.action_ticks(),
                right_eye: prev_character.core.eye,
                left_eye: prev_character.core.eye,
                buffs: {
                    let mut buffs = self.game_pools.character_buffs.new();
                    buffs.extend(prev_character.reusable_core.buffs.iter().map(|(buff, _)| {
                        match buff {
                            CharacterBuff::Ninja => (
                                CharacterBuff::Ninja,
                                CharacterBuffInfo {
                                    remaining_time: None,
                                },
                            ),
                            CharacterBuff::Ghost => (
                                CharacterBuff::Ghost,
                                CharacterBuffInfo {
                                    remaining_time: None,
                                },
                            ),
                        }
                    }));
                    buffs
                },
                debuffs: {
                    let mut debuffs = self.game_pools.character_debuffs.new();
                    debuffs.extend(prev_character.reusable_core.debuffs.iter().map(
                        |(debuff, _)| match debuff {
                            CharacterDebuff::Freeze => (
                                CharacterDebuff::Freeze,
                                CharacterDebuffInfo {
                                    remaining_time: None,
                                },
                            ),
                        },
                    ));
                    debuffs
                },

                animation_ticks_passed: prev_stage.match_manager.game_match.state.passed_ticks(),
                game_ticks_passed: prev_stage.match_manager.game_match.state.passed_ticks(),

                emoticon: prev_character.core.cur_emoticon.and_then(|emoticon| {
                    prev_character
                        .core
                        .emoticon_tick
                        .action_ticks()
                        .map(|tick| (tick, emoticon))
                }),
                phased: false,
            }
        }

        fn stage_characters_render_info(
            &self,
            prev_stage: &GameStage,
            stage: Option<&GameStage>,
            intra_tick_ratio: f64,
        ) -> PoolFxLinkedHashMap<CharacterId, CharacterRenderInfo> {
            let mut render_infos = self.game_pools.character_render_info_pool.new();

            let stage = stage.unwrap_or(prev_stage);
            render_infos.extend(prev_stage.world.characters.iter().filter_map(
                |(id, prev_character)| {
                    let character = stage.world.characters.get(id).unwrap_or(prev_character);
                    (!matches!(
                        prev_character.phased,
                        character::CharacterPhasedState::Dead { .. }
                    ) && !matches!(
                        character.phased,
                        character::CharacterPhasedState::Dead { .. }
                    ))
                    .then(|| {
                        (
                            *id,
                            self.stage_character_render_info(
                                prev_stage,
                                stage,
                                prev_character,
                                character,
                                intra_tick_ratio,
                            ),
                        )
                    })
                },
            ));
            render_infos
        }

        fn game_event_to_world_event(
            game_event: &GameWorldEvent,
            world_events: &mut FxLinkedHashMap<EventId, GameWorldEvent>,
            event_id_generator: &EventIdGenerator,
        ) {
            world_events.insert(event_id_generator.next_id(), game_event.clone());
        }

        fn check_stage_remove(&mut self, stage_id: StageId) {
            if let Some(stage) = self.game.stages.get(&stage_id) {
                if stage_id != self.stage_0_id
                    && !stage
                        .world
                        .characters
                        .values()
                        .any(|c| c.is_player_character().is_some())
                {
                    self.game.stages.remove(&stage_id);
                }
            }
        }

        fn add_from_spectator(
            &mut self,
            player_id: &PlayerId,
            stage_id: StageId,
            side: Option<MatchSide>,
        ) {
            let mut default_eyes = TeeEye::Normal;
            let default_eyes = &mut default_eyes;
            let mut default_eyes_reset_in = GameTickCooldown::default();
            let default_eyes_reset_in = &mut default_eyes_reset_in;
            if !self.game.spectator_players.handle_mut(
                player_id,
                hi_closure!(
                    [
                        default_eyes: &mut TeeEye,
                        default_eyes_reset_in: &mut GameTickCooldown,
                    ],
                    |player: &mut SpectatorPlayer| -> () {
                        *default_eyes = player.default_eye;
                        *default_eyes_reset_in = player.default_eye_reset_in;
                    }
                ),
            ) {
                return;
            }
            let player = self.game.spectator_players.remove(player_id).unwrap();
            Self::add_char_to_stage(
                &mut self.game.stages,
                &stage_id,
                player_id,
                player.player_info,
                player.player_input,
                self.game.players.clone(),
                self.game.spectator_players.clone(),
                player.network_stats,
                side,
                0,
                *default_eyes,
                *default_eyes_reset_in,
                &self.game_pools,
            );
        }

        fn check_player_info(
            &self,
            mut info: NetworkCharacterInfo,
            player_id: Option<PlayerId>,
        ) -> NetworkCharacterInfo {
            // check if the name is already in use
            let name_exists = |name: &str| {
                self.game.stages.values().any(|s| {
                    s.world
                        .characters
                        .iter()
                        .filter_map(|(id, c)| (Some(*id) != player_id).then_some(c))
                        .any(|c| c.player_info.player_info.name.as_str() == name)
                }) || self.game.spectator_players.any_with_name(player_id, name)
            };
            let mut name = info.name.clone();
            if name_exists(name.as_str()) {
                let original_name = name.clone();
                let mut i = 0u32;
                loop {
                    name = format!("({i}) {}", original_name.as_str())
                        .chars()
                        .take(MAX_CHARACTER_NAME_LEN)
                        .collect::<String>()
                        .as_str()
                        .try_into()
                        .unwrap();

                    if !name_exists(name.as_str()) {
                        break;
                    }
                    i += 1;
                }
            }
            info.name = name;
            info
        }
    }

    impl GameStateInterface for GameState {
        fn collect_characters_info(&self) -> PoolFxLinkedHashMap<CharacterId, CharacterInfo> {
            let mut character_infos = self.game_pools.character_info_pool.new();

            let mut players = self.spectator_player_clone_pool.new();
            self.game.spectator_players.pooled_clone_into(&mut players);
            let spectator_players = players.iter().map(|(_, player)| {
                let (player_info, stage_id) = (
                    CharacterPlayerInfo {
                        cam_mode: if player.spectated_characters.len() == 0 {
                            PlayerCameraMode::Free
                        } else {
                            PlayerCameraMode::LockedOn {
                                character_ids: player.spectated_characters.clone(),
                                locked_ingame: false,
                            }
                        },
                        force_scoreboard_visible: player.spectated_characters.len() > 0
                            && player
                                .spectated_characters
                                .iter()
                                .all(|spectated_player_id| {
                                    self.game
                                        .players
                                        .player(spectated_player_id)
                                        .and_then(|info| self.game.stages.get(&info.stage_id()))
                                        .map(|stage| {
                                            matches!(
                                                &stage.match_manager.game_match.state,
                                                MatchState::GameOver { .. }
                                            )
                                        })
                                        .unwrap_or_default()
                                }),
                        ingame_mode: PlayerIngameMode::Spectator,
                    },
                    None,
                );
                (
                    stage_id,
                    (&player.id, None, &player.player_info),
                    Some(player_info),
                    self.game_pools.network_string_score_pool.new(),
                )
            });
            // of all chars (even server-side ones)
            // + all spectator players
            self.game
                .stages
                .iter()
                .flat_map(|(stage_id, stage)| {
                    stage.world.characters.iter().map(|(id, character)| {
                        (
                            Some(*stage_id),
                            (id, Some(character.core.side), &character.player_info),
                            self.game
                                .players
                                .player(id)
                                .is_some()
                                .then_some(CharacterPlayerInfo {
                                    cam_mode: if matches!(
                                        character.phased,
                                        character::CharacterPhasedState::Dead { .. }
                                    ) {
                                        PlayerCameraMode::LockedTo {
                                            pos: *character.pos.pos() / 32.0,
                                            locked_ingame: true,
                                        }
                                    } else {
                                        PlayerCameraMode::Default
                                    },
                                    force_scoreboard_visible: matches!(
                                        stage.match_manager.game_match.state,
                                        MatchState::GameOver { .. }
                                    ),
                                    ingame_mode: PlayerIngameMode::InGame {
                                        in_custom_stage: *stage_id != self.stage_0_id,
                                    },
                                }),
                            {
                                let mut str = self.game_pools.network_string_score_pool.new();
                                str.try_set(format!("{}", character.score.get())).unwrap();
                                str
                            },
                        )
                    })
                })
                .chain(spectator_players)
                .for_each(
                    |(stage_id, (id, character_game_info, info), is_player, score)| {
                        character_infos.insert(
                            *id,
                            CharacterInfo {
                                info: info.player_info.clone(),
                                skin_info: match character_game_info.and_then(|side| side) {
                                    Some(side) => match side {
                                        MatchSide::Red => NetworkSkinInfo::Custom {
                                            body_color: ubvec4::new(255, 107, 107, 255),
                                            feet_color: ubvec4::new(255, 107, 107, 255),
                                        },
                                        MatchSide::Blue => NetworkSkinInfo::Custom {
                                            body_color: ubvec4::new(107, 159, 255, 255),
                                            feet_color: ubvec4::new(107, 159, 255, 255),
                                        },
                                    },
                                    None => {
                                        if character_game_info.is_some() {
                                            info.player_info.skin_info
                                        } else {
                                            NetworkSkinInfo::Custom {
                                                body_color: ubvec4::new(255, 0, 255, 255),
                                                feet_color: ubvec4::new(255, 0, 255, 255),
                                            }
                                        }
                                    }
                                },
                                laser_info: match character_game_info.and_then(|side| side) {
                                    Some(side) => match side {
                                        MatchSide::Red => NetworkLaserInfo {
                                            inner_color: ubvec4::new(255, 0, 0, 255),
                                            outer_color: ubvec4::new(128, 0, 0, 255),
                                        },
                                        MatchSide::Blue => NetworkLaserInfo {
                                            inner_color: ubvec4::new(0, 0, 255, 255),
                                            outer_color: ubvec4::new(0, 0, 128, 255),
                                        },
                                    },
                                    None => {
                                        if character_game_info.is_some() {
                                            info.player_info.laser_info
                                        } else {
                                            NetworkLaserInfo {
                                                inner_color: ubvec4::new(255, 0, 255, 255),
                                                outer_color: ubvec4::new(128, 0, 128, 255),
                                            }
                                        }
                                    }
                                },
                                stage_id,
                                side: character_game_info.flatten(),
                                player_info: is_player,
                                browser_score: score,
                                browser_eye: TeeEye::Normal,
                                account_name: info.account_name.as_ref().map(|account_name| {
                                    let mut name =
                                        self.game_pools.network_string_account_name_pool.new();

                                    name.try_set(account_name.as_str()).unwrap();

                                    name
                                }),
                            },
                        );
                    },
                );

            character_infos
        }

        fn collect_render_ext(&self) -> PoolVec<u8> {
            PoolVec::new_without_pool()
        }

        fn collect_scoreboard_info(&self) -> Scoreboard {
            let mut spectator_scoreboard_infos =
                self.game_pools.player_spectator_scoreboard_pool.new();

            let mut spectator_players = self.spectator_player_clone_pool.new();
            self.game
                .spectator_players
                .pooled_clone_into(&mut spectator_players);

            let mut red_or_solo_stage_infos = self.game_pools.stage_scoreboard_pool.new();
            let mut blue_stage_infos = self.game_pools.stage_scoreboard_pool.new();
            for (&stage_id, stage) in self.game.stages.iter() {
                let mut red_or_solo_characters = self.game_pools.character_scoreboard_pool.new();
                let mut blue_characters = self.game_pools.character_scoreboard_pool.new();

                for (id, character) in stage.world.characters.iter() {
                    let info = ScoreboardCharacterInfo {
                        id: *id,

                        score: ScoreboardScoreType::Points(character.score.get()),
                        ping: if let Some(stats) = character.is_player_character() {
                            ScoreboardConnectionType::Network(stats)
                        } else {
                            ScoreboardConnectionType::Bot
                        },
                    };

                    match character.core.side {
                        Some(side) => match side {
                            MatchSide::Red => red_or_solo_characters.push(info),
                            MatchSide::Blue => blue_characters.push(info),
                        },
                        None => red_or_solo_characters.push(info),
                    }
                }

                red_or_solo_stage_infos.insert(
                    stage_id,
                    ScoreboardStageInfo {
                        characters: red_or_solo_characters,
                        name: {
                            let mut name = self.game_pools.network_string_team_pool.new();
                            (*name).clone_from(&stage.stage_name);
                            name
                        },
                        max_size: 0,
                        color: stage.stage_color,
                        score: ScoreboardScoreType::Points(
                            match stage.match_manager.game_match.ty {
                                MatchType::Solo => stage
                                    .world
                                    .scores
                                    .top_2_leading_characters()
                                    .first()
                                    .map(|(_, score)| *score)
                                    .unwrap_or_default(),
                                MatchType::Sided { scores } => scores[0],
                            },
                        ),
                    },
                );
                blue_stage_infos.insert(
                    stage_id,
                    ScoreboardStageInfo {
                        characters: blue_characters,
                        name: {
                            let mut name = self.game_pools.network_string_team_pool.new();
                            (*name).clone_from(&stage.stage_name);
                            name
                        },
                        max_size: 0,
                        color: stage.stage_color,

                        score: ScoreboardScoreType::Points(
                            match stage.match_manager.game_match.ty {
                                MatchType::Solo => stage
                                    .world
                                    .scores
                                    .top_2_leading_characters()
                                    .first()
                                    .map(|(_, score)| *score)
                                    .unwrap_or_default(),
                                MatchType::Sided { scores } => scores[1],
                            },
                        ),
                    },
                );
            }

            for (id, p) in spectator_players.iter() {
                // add to spectators instead
                spectator_scoreboard_infos.push(ScoreboardPlayerSpectatorInfo {
                    id: *id,

                    score: ScoreboardScoreType::None,
                    ping: ScoreboardConnectionType::Network(p.network_stats),
                });
            }

            for stage in red_or_solo_stage_infos.values_mut() {
                stage.characters.sort_by_key(|c| std::cmp::Reverse(c.score));
            }
            for stage in blue_stage_infos.values_mut() {
                stage.characters.sort_by_key(|c| std::cmp::Reverse(c.score));
            }

            let ty = self.game_options.ty();
            Scoreboard {
                game: match ty {
                    GameType::Solo => ScoreboardGameType::SoloPlay {
                        stages: red_or_solo_stage_infos,
                        ignore_stage: self.stage_0_id,
                        spectator_players: spectator_scoreboard_infos,
                    },
                    GameType::Sided => ScoreboardGameType::SidedPlay {
                        red_stages: red_or_solo_stage_infos,
                        blue_stages: blue_stage_infos,
                        ignore_stage: self.stage_0_id,
                        spectator_players: spectator_scoreboard_infos,

                        red_side_name: {
                            let mut name = self.game_pools.network_string_team_pool.new();
                            name.try_set("Red Team").unwrap();
                            name
                        },
                        blue_side_name: {
                            let mut name = self.game_pools.network_string_team_pool.new();
                            name.try_set("Red Team").unwrap();
                            name
                        },
                    },
                },
                options: ScoreboardGameOptions {
                    map_name: {
                        let mut name = self.game_pools.network_string_map_pool.new();
                        name.try_set(self.map_name.as_str()).unwrap();
                        name
                    },
                    ty: ScoreboardGameTypeOptions::Match {
                        score_limit: self.game_options.score_limit(),
                        time_limit: self.game_options.time_limit(),
                    },
                },
            }
        }

        fn all_stages(
            &self,
            intra_tick_ratio: f64,
        ) -> PoolFxLinkedHashMap<StageId, StageRenderInfo> {
            let mut stages = self.game_pools.stage_render_info.new();

            for (stage_id, prev_stage) in self.prev_game.stages.iter() {
                let stage = self.game.stages.get(stage_id);

                stages.insert(
                    *stage_id,
                    StageRenderInfo {
                        world: WorldRenderInfo {
                            projectiles: self.stage_projectiles(
                                prev_stage,
                                stage,
                                intra_tick_ratio,
                            ),
                            ctf_flags: self.stage_ctf_flags(prev_stage, stage, intra_tick_ratio),
                            lasers: self.stage_lasers(prev_stage, stage, intra_tick_ratio),
                            pickups: self.stage_pickups(prev_stage, stage, intra_tick_ratio),
                            characters: self.stage_characters_render_info(
                                prev_stage,
                                stage,
                                intra_tick_ratio,
                            ),
                        },
                        game: GameRenderInfo::Match {
                            standings: match prev_stage.match_manager.game_match.ty {
                                MatchType::Solo => MatchStandings::Solo {
                                    leading_characters: {
                                        let mut top2 =
                                            prev_stage.world.scores.top_2_leading_characters();
                                        let mut top2 = top2.drain(..);
                                        [
                                            top2.next().map(|(character_id, score)| {
                                                LeadingCharacter {
                                                    character_id,
                                                    score,
                                                }
                                            }),
                                            top2.next().map(|(character_id, score)| {
                                                LeadingCharacter {
                                                    character_id,
                                                    score,
                                                }
                                            }),
                                        ]
                                    },
                                },
                                MatchType::Sided { scores } => {
                                    let carrier = |flags: &Flags| {
                                        flags.values().find_map(|flag| flag.core.carrier).map(
                                            |character_id| FlagCarrierCharacter {
                                                character_id,
                                                score: prev_stage
                                                    .world
                                                    .characters
                                                    .get(&character_id)
                                                    .map(|c| c.score.get())
                                                    .unwrap_or_default(),
                                            },
                                        )
                                    };
                                    MatchStandings::Sided {
                                        score_red: scores[0],
                                        score_blue: scores[1],
                                        flag_carrier_red: carrier(&prev_stage.world.blue_flags),
                                        flag_carrier_blue: carrier(&prev_stage.world.red_flags),
                                    }
                                }
                            },
                            round_time_type: prev_stage
                                .match_manager
                                .game_match
                                .state
                                .round_ticks_left(&prev_stage.world, &self.game_pools),
                            unbalanced: self.game_options.sided_balance_time().is_some()
                                && MatchManager::needs_sided_balance(&prev_stage.world),
                        },
                        game_ticks_passed: prev_stage.match_manager.game_match.state.passed_ticks(),
                    },
                );
            }

            stages
        }

        fn collect_character_local_render_info(
            &self,
            player_id: &PlayerId,
        ) -> LocalCharacterRenderInfo {
            if let Some(p) = self.game.players.player(player_id) {
                let player_char = self
                    .game
                    .stages
                    .get(&p.stage_id())
                    .unwrap()
                    .world
                    .characters
                    .get(player_id)
                    .unwrap();

                LocalCharacterRenderInfo::Vanilla(LocalCharacterVanilla {
                    health: player_char.core.health,
                    armor: player_char.core.armor,
                    ammo_of_weapon: player_char
                        .reusable_core
                        .weapons
                        .get(&player_char.core.active_weapon)
                        .and_then(|w| w.cur_ammo),
                })
            } else {
                // spectators get nothing
                LocalCharacterRenderInfo::Unavailable
            }
        }

        fn get_client_camera_join_pos(&self) -> vec2 {
            // TODO:
            vec2::default()
        }

        fn player_join(&mut self, client_player_info: &PlayerClientInfo) -> PlayerId {
            if let Some((timeout_player_id, character_info)) = self
                .game
                .timeout_players
                .remove(&(client_player_info.unique_identifier, client_player_info.id))
                .and_then(|(id, _)| self.game.players.player(&id).map(|char| (id, char)))
            {
                let char = self
                    .game
                    .stages
                    .get_mut(&character_info.stage_id())
                    .unwrap()
                    .world
                    .characters
                    .get_mut(&timeout_player_id)
                    .unwrap();
                char.core.is_timeout = false;
                return timeout_player_id;
            }

            let player_id = self.id_generator.next_id();
            let stage_0_id = self.stage_0_id;

            let character_info = self.check_player_info(client_player_info.info.clone(), None);

            self.game
                .stages
                .get(&stage_0_id)
                .unwrap()
                .game_pending_events
                .push(GameWorldEvent::Notification(
                    GameWorldNotificationEvent::System(GameWorldSystemMessage::PlayerJoined {
                        id: player_id,
                        name: {
                            let mut s = self.game_pools.mt_network_string_name_pool.new();
                            s.try_set(character_info.name.as_str()).unwrap();
                            s
                        },
                        skin: {
                            let mut skin = self.game_pools.mt_resource_key_pool.new();
                            (*skin).clone_from(&character_info.skin);
                            skin
                        },
                        skin_info: character_info.skin_info,
                    }),
                ));

            if client_player_info.id == 0 {
                let events = self.player_events.entry(player_id).or_default();

                let mut msg = self.game_pools.mt_network_string_common_pool.new();
                msg.try_set("alpha version vanilla.").unwrap();

                events.push(GameWorldEvent::Notification(
                    GameWorldNotificationEvent::Motd { msg },
                ));
            }

            let player_info = PlayerInfo {
                player_info: PoolRc::from_item_without_pool(character_info),
                version: 0,
                unique_identifier: client_player_info.unique_identifier,
                account_name: None,
                id: client_player_info.id,
            };
            if self
                .game
                .stages
                .get(&self.stage_0_id)
                .unwrap()
                .world
                .characters
                .len()
                < self.game_options.max_ingame_players() as usize
            {
                // spawn and send character info
                let default_eyes = player_info.player_info.default_eyes;
                Self::add_char_to_stage(
                    &mut self.game.stages,
                    &stage_0_id,
                    &player_id,
                    player_info,
                    Default::default(),
                    self.game.players.clone(),
                    self.game.spectator_players.clone(),
                    client_player_info.initial_network_stats,
                    None,
                    0,
                    default_eyes,
                    Default::default(),
                    &self.game_pools,
                );
            } else {
                self.game.spectator_players.insert(
                    player_id,
                    SpectatorPlayer::new(
                        player_info,
                        Default::default(),
                        &player_id,
                        self.game_pools.character_id_hashset_pool.new(),
                        client_player_info.info.default_eyes,
                        Default::default(),
                        client_player_info.initial_network_stats,
                    ),
                );
            }

            Self::push_account_info_task(
                &mut self.game_db,
                &player_id,
                &client_player_info.unique_identifier,
            );

            player_id
        }

        fn player_drop(&mut self, player_id: &PlayerId, reason: PlayerDropReason) {
            let name = if let Some(server_player) = self.game.players.player(player_id) {
                let stage = self.game.stages.get_mut(&server_player.stage_id()).unwrap();

                let character = stage.world.characters.get_mut(player_id).unwrap();

                let mut name = self.game_pools.mt_network_string_name_pool.new();
                (*name).clone_from(&character.player_info.player_info.name);

                let skin = {
                    let mut skin = self.game_pools.mt_resource_key_pool.new();
                    (*skin).clone_from(&character.player_info.player_info.skin);
                    skin
                };
                let skin_info = character.player_info.player_info.skin_info;

                character.despawn_completely_silent();
                stage.world.characters.remove(player_id);

                Some((name, skin, skin_info, server_player.stage_id()))
            } else if let Some(spectator_player) = self.game.spectator_players.remove(player_id) {
                let mut name = self.game_pools.mt_network_string_name_pool.new();
                (*name).clone_from(&spectator_player.player_info.player_info.name);
                let skin = {
                    let mut skin = self.game_pools.mt_resource_key_pool.new();
                    (*skin).clone_from(&spectator_player.player_info.player_info.skin);
                    skin
                };
                let skin_info = spectator_player.player_info.player_info.skin_info;
                Some((name, skin, skin_info, self.stage_0_id))
            } else {
                None
            };

            if let Some((name, skin, skin_info, stage_id)) = name {
                let stage = self.game.stages.get(&stage_id).unwrap();
                stage.game_pending_events.push(GameWorldEvent::Notification(
                    GameWorldNotificationEvent::System(GameWorldSystemMessage::PlayerLeft {
                        id: *player_id,
                        name: {
                            let mut s = self.game_pools.mt_network_string_name_pool.new();
                            s.try_set(name.as_str()).unwrap();
                            s
                        },
                        skin,
                        skin_info,
                        reason,
                    }),
                ));

                self.check_stage_remove(stage_id);
            }
        }

        fn try_overwrite_player_character_info(
            &mut self,
            id: &PlayerId,
            info: &NetworkCharacterInfo,
            version: NonZeroU64,
        ) {
            let old_info = &mut None;
            let new_info = self.check_player_info(info.clone(), Some(*id));
            let mut stage_id = self.stage_0_id;
            if let Some(player) = self.game.players.player(id) {
                stage_id = player.stage_id();
                let stage = self.game.stages.get_mut(&player.stage_id()).unwrap();
                let character = stage.world.characters.get_mut(id).unwrap();
                let player_info = &mut character.player_info;
                if player_info.version < version.get() {
                    let old_player_info = std::mem::replace(
                        &mut player_info.player_info,
                        PoolRc::from_item_without_pool(new_info.clone()),
                    );
                    player_info.version = version.get();

                    *old_info = Some((
                        old_player_info.name.clone(),
                        old_player_info.skin.clone(),
                        old_player_info.skin_info,
                    ));

                    if character.core.default_eye_reset_in.is_none() {
                        character.core.default_eye = player_info.player_info.default_eyes;
                        if character.core.normal_eye_in.is_none() {
                            character.core.eye = player_info.player_info.default_eyes;
                        }
                    }
                }
            } else {
                let new_info = &new_info;
                if !self.game.spectator_players.handle_mut(
                    id,
                    hi_closure!(
                        [
                            version: NonZeroU64,
                            new_info: &NetworkCharacterInfo,
                            old_info: &mut Option<
                                (
                                    NetworkString<MAX_CHARACTER_NAME_LEN>,
                                    NetworkResourceKey<MAX_ASSET_NAME_LEN>,
                                    NetworkSkinInfo
                                )
                            >,
                        ],
                        |spectator_player: &mut SpectatorPlayer| -> () {
                        if spectator_player.player_info.version < version.get() {
                            let old_player_info = std::mem::replace(
                                &mut spectator_player.player_info.player_info,
                                PoolRc::from_item_without_pool(new_info.clone())
                            );
                            spectator_player.player_info.version = version.get();

                            *old_info = Some((
                                old_player_info.name.clone(),
                                old_player_info.skin.clone(),
                                old_player_info.skin_info,
                            ));
                        }
                    }),
                ) {
                    panic!("player did not exist, this should not happen");
                }
            }

            if let Some((old_name, old_skin, old_skin_info)) =
                old_info
                    .take()
                    .and_then(|(old_name, old_skin, old_skin_info)| {
                        (old_name != new_info.name).then_some((old_name, old_skin, old_skin_info))
                    })
            {
                let stage = self.game.stages.get(&stage_id).unwrap();
                stage.game_pending_events.push(GameWorldEvent::Notification(
                    GameWorldNotificationEvent::System(
                        GameWorldSystemMessage::CharacterInfoChanged {
                            id: *id,
                            old_name: {
                                let mut s = self.game_pools.mt_network_string_name_pool.new();
                                s.try_set(old_name.as_str()).unwrap();
                                s
                            },
                            old_skin: {
                                let mut skin = self.game_pools.mt_resource_key_pool.new();
                                (*skin).clone_from(&old_skin);
                                skin
                            },
                            old_skin_info,
                            new_name: {
                                let mut s = self.game_pools.mt_network_string_name_pool.new();
                                s.try_set(new_info.name.as_str()).unwrap();
                                s
                            },
                            new_skin: {
                                let mut skin = self.game_pools.mt_resource_key_pool.new();
                                (*skin).clone_from(&new_info.skin);
                                skin
                            },
                            new_skin_info: new_info.skin_info,
                        },
                    ),
                ));
            }
        }

        fn account_created(&mut self, account_id: AccountId, cert_fingerprint: Hash) {
            if let Some(statements) = &self.game_db.statements {
                let account_created = statements.account_created.clone();

                self.game_db
                    .cur_queries
                    .push(self.game_db.io_rt.spawn(async move {
                        let res = account_created.execute(account_id, cert_fingerprint).await;
                        let (err, affected) = match res {
                            Ok(affected) => (None, affected),
                            Err((err, affected)) => (Some(err), affected),
                        };
                        Ok(GameDbQueries::AccountCreated {
                            account_id,
                            cert_fingerprint,
                            affected_rows: affected,
                            err,
                        })
                    }));
            }
        }

        fn account_renamed(
            &mut self,
            account_id: AccountId,
            new_name: &NetworkReducedAsciiString<MAX_ACCOUNT_NAME_LEN>,
        ) {
            let mut players = self.player_clone_pool.new();
            self.game.players.pooled_clone_into(&mut players);

            for (player_id, char_info) in players.drain(..) {
                let stage = self.game.stages.get_mut(&char_info.stage_id()).unwrap();
                let character = stage.world.characters.get_mut(&player_id).unwrap();

                if character
                    .player_info
                    .unique_identifier
                    .is_account_then(|char_account_id| {
                        (char_account_id == account_id).then_some(true)
                    })
                    .unwrap_or_default()
                {
                    character.player_info.account_name = Some(new_name.clone());
                }
            }

            let mut players = self.spectator_player_clone_pool.new();
            self.game.spectator_players.pooled_clone_into(&mut players);

            for (player_id, mut player) in players.drain() {
                if player
                    .player_info
                    .unique_identifier
                    .is_account_then(|char_account_id| {
                        (char_account_id == account_id).then_some(true)
                    })
                    .unwrap_or_default()
                {
                    player.player_info.account_name = Some(new_name.clone());
                    self.game.spectator_players.insert(player_id, player);
                }
            }
        }

        fn network_stats(&mut self, mut stats: PoolFxLinkedHashMap<PlayerId, PlayerNetworkStats>) {
            let mut players = self.player_clone_pool.new();
            self.game.players.pooled_clone_into(&mut players);

            for (player_id, stats) in stats.drain() {
                if let Some((stage_id, character)) =
                    self.game.players.player(&player_id).map(|char_info| {
                        (
                            char_info.stage_id(),
                            self.game
                                .stages
                                .get_mut(&char_info.stage_id())
                                .unwrap()
                                .world
                                .characters
                                .get_mut(&player_id)
                                .unwrap(),
                        )
                    })
                {
                    character.update_player_ty(
                        &stage_id,
                        CharacterPlayerTy::Player {
                            players: self.game.players.clone(),
                            spectator_players: self.game.spectator_players.clone(),
                            network_stats: stats,
                            stage_id,
                        },
                    );
                } else {
                    self.game.spectator_players.handle_mut(
                        &player_id,
                        hi_closure!(
                            [stats: PlayerNetworkStats],
                            |player: &mut SpectatorPlayer| -> () {
                                player.network_stats = stats;
                            }
                        ),
                    );
                }
            }
        }

        fn settings(&self) -> GameStateSettings {
            GameStateSettings {
                max_ingame_players: self.game_options.max_ingame_players(),
                tournament_mode: self.game_options.tournament_mode(),
            }
        }

        fn client_command(&mut self, player_id: &PlayerId, cmd: ClientCommand) {
            match cmd {
                ClientCommand::Kill => {
                    if let Some(server_player) = self.game.players.player(player_id) {
                        self.game
                            .stages
                            .get_mut(&server_player.stage_id())
                            .unwrap()
                            .world
                            .characters
                            .get_mut(player_id)
                            .unwrap()
                            .despawn_to_respawn(true);
                    }
                }
                ClientCommand::Chat(cmd) => {
                    let cmds = command_parser::parser::parse(
                        &cmd.raw,
                        &self.chat_commands.cmds,
                        &self.cache,
                    );
                    self.handle_chat_commands(player_id, cmds);
                }
                ClientCommand::JoinStage(join_stage) => {
                    if self.game_options.allow_stages()
                        || (!Self::is_sided_from_conf(self.game_options.game_ty())
                            && matches!(join_stage, JoinStage::Default))
                    {
                        let stage_id = match join_stage {
                            JoinStage::Default => self.stage_0_id,
                            JoinStage::Own { mut name, color } => {
                                // check if the name is already in use
                                let name_exists = |name: &str| {
                                    self.game
                                        .stages
                                        .iter()
                                        .any(|(_, s)| s.stage_name.as_str() == name)
                                };
                                if name_exists(name.as_str()) {
                                    let original_name = name.clone();
                                    let mut i = 0u32;
                                    loop {
                                        name = format!("({i}) {}", original_name.as_str())
                                            .chars()
                                            .take(MAX_TEAM_NAME_LEN)
                                            .collect::<String>()
                                            .as_str()
                                            .try_into()
                                            .unwrap();

                                        if !name_exists(name.as_str()) {
                                            break;
                                        }
                                        i += 1;
                                    }
                                }

                                self.add_stage(name, ubvec4::new(color[0], color[1], color[2], 20))
                            }
                            JoinStage::Others(name) => self
                                .game
                                .stages
                                .iter()
                                .find(|(_, s)| s.stage_name == name)
                                .map(|(stage_id, _)| *stage_id)
                                .unwrap_or(self.stage_0_id),
                        };
                        if let Some(player) = self
                            .game
                            .players
                            .player(player_id)
                            .and_then(|p| (p.stage_id() != stage_id).then_some(p))
                        {
                            let stage = &mut self.game.stages.get_mut(&player.stage_id()).unwrap();
                            let mut character = stage.world.characters.remove(player_id).unwrap();
                            let player_info = character.player_info.clone();
                            let player_input = character.core.input;
                            let network_stats = character.is_player_character().unwrap();
                            let default_eye = character.core.default_eye;
                            let default_eye_reset_in = character.core.default_eye_reset_in;
                            character.despawn_completely_silent();
                            drop(character);

                            if stage_id != player.stage_id() {
                                self.check_stage_remove(player.stage_id());
                            }

                            Self::add_char_to_stage(
                                &mut self.game.stages,
                                &stage_id,
                                player_id,
                                player_info,
                                player_input,
                                self.game.players.clone(),
                                self.game.spectator_players.clone(),
                                network_stats,
                                None,
                                0,
                                default_eye,
                                default_eye_reset_in,
                                &self.game_pools,
                            );
                        } else {
                            self.add_from_spectator(player_id, stage_id, None);
                        }
                    }
                }
                ClientCommand::JoinSide(side) => {
                    if Self::is_sided_from_conf(self.game_options.game_ty()) {
                        if let Some(player) = self.game.players.player(player_id) {
                            let stage = self.game.stages.get_mut(&player.stage_id()).unwrap();
                            if let Some(character) = stage.world.characters.get_mut(player_id) {
                                if character.core.side != Some(side) {
                                    character.despawn_to_respawn(true);
                                    character.core.side = Some(side);
                                }
                            }
                        } else {
                            self.add_from_spectator(player_id, self.stage_0_id, Some(side));
                        }
                    }
                }
                ClientCommand::JoinSpectator => {
                    if let Some(player) = self.game.players.player(player_id) {
                        if let Some(mut character) = self
                            .game
                            .stages
                            .get_mut(&player.stage_id())
                            .unwrap()
                            .world
                            .characters
                            .remove(player_id)
                        {
                            character.despawn_to_join_spectators();

                            self.check_stage_remove(player.stage_id());
                        }
                    }
                }
                ClientCommand::SetCameraMode(mut mode) => {
                    match &mut mode {
                        ClientCameraMode::None => {
                            // nothing to do
                        }
                        ClientCameraMode::FreeCam(spectated_players)
                        | ClientCameraMode::PhasedFreeCam(spectated_players) => {
                            // validate that these players exist
                            spectated_players.retain(|id| self.game.players.player(id).is_some());
                        }
                    }
                    self.game.spectator_players.set_camera_mode(
                        player_id,
                        &self.game_pools.character_id_hashset_pool,
                        mode,
                    );
                }
            }
        }

        fn rcon_command(
            &mut self,
            player_id: Option<PlayerId>,
            cmd: ExecRconInput,
        ) -> Vec<Result<NetworkString<65536>, NetworkString<65536>>> {
            if !matches!(cmd.auth_level, AuthLevel::None) {
                let cmds =
                    command_parser::parser::parse(&cmd.raw, &self.rcon_chain.parser, &self.cache);
                self.handle_rcon_commands(player_id.as_ref(), cmd.auth_level, cmds)
            } else {
                vec![Err("Only moderators or admins can execute rcon commands"
                    .try_into()
                    .unwrap())]
            }
        }

        fn vote_command(&mut self, cmd: VoteCommand) -> VoteCommandResult {
            match cmd {
                VoteCommand::JoinSpectator(player_id) => {
                    if let Some(player) = self.game.players.player(&player_id) {
                        if let Some(mut character) = self
                            .game
                            .stages
                            .get_mut(&player.stage_id())
                            .unwrap()
                            .world
                            .characters
                            .remove(&player_id)
                        {
                            character.despawn_to_join_spectators();

                            self.check_stage_remove(player.stage_id());
                        }
                    }
                }
                VoteCommand::Misc(cmd) => {
                    let cmds =
                        command_parser::parser::parse(&cmd, &self.rcon_chain.parser, &self.cache);
                    self.handle_rcon_commands(None, AuthLevel::Admin, cmds);
                }
                VoteCommand::RandomUnfinishedMap(_) => {
                    // not supported
                }
            }
            VoteCommandResult::default()
        }

        fn voted_player(&mut self, player_id: Option<PlayerId>) {
            self.game.voted_player = player_id;
        }

        fn set_player_inputs(
            &mut self,
            mut inps: PoolFxLinkedHashMap<PlayerId, CharacterInputInfo>,
        ) {
            for (player_id, CharacterInputInfo { inp, diff }) in inps.drain() {
                self.set_player_inp_impl(&player_id, &inp, diff)
            }
        }

        fn set_player_emoticon(&mut self, player_id: &PlayerId, emoticon: EmoticonType) {
            if let Some(player) = self.game.players.player(player_id) {
                let stages = &mut self.game.stages;
                let character = stages
                    .get_mut(&player.stage_id())
                    .unwrap()
                    .world
                    .characters
                    .get_mut(player_id)
                    .unwrap();

                if character.reusable_core.queued_emoticon.len() < 3 {
                    character
                        .reusable_core
                        .queued_emoticon
                        .push_back((emoticon, 4.into()));
                }
            }
        }

        fn set_player_eye(&mut self, player_id: &PlayerId, eye: TeeEye, duration: Duration) {
            let normal_in = (duration.as_millis().clamp(0, GameTickType::MAX as u128)
                as GameTickType
                / TICKS_PER_SECOND)
                .max(1);
            if let Some(player) = self.game.players.player(player_id) {
                let stages = &mut self.game.stages;
                let character = stages
                    .get_mut(&player.stage_id())
                    .unwrap()
                    .world
                    .characters
                    .get_mut(player_id)
                    .unwrap();

                character.core.eye = eye;
                character.core.default_eye = eye;
                character.core.default_eye_reset_in = normal_in.into();
            } else {
                self.game
                    .spectator_players
                    .set_default_eye(player_id, eye, normal_in);
            }
        }

        fn tick(&mut self, options: TickOptions) -> TickResult {
            self.tick_impl(options.is_future_tick_prediction);

            if !options.is_future_tick_prediction {
                self.player_tick();
                self.query_tick();
            }

            TickResult {
                events: PoolVec::new_without_pool(),
            }
        }

        fn snapshot_for(&self, client: SnapshotClientInfo) -> MtPoolCow<'static, [u8]> {
            self.snapshot_for_impl(SnapshotFor::Client(client))
        }

        fn build_from_snapshot(
            &mut self,
            snapshot: &MtPoolCow<'static, [u8]>,
        ) -> SnapshotLocalPlayers {
            let (snapshot, _) =
                bincode::serde::decode_from_slice(snapshot, bincode::config::standard()).unwrap();

            SnapshotManager::build_from_snapshot(snapshot, self)
        }

        fn snapshot_for_hotreload(&self) -> Option<MtPoolCow<'static, [u8]>> {
            Some(self.snapshot_for_impl(SnapshotFor::Hotreload))
        }

        fn build_from_snapshot_by_hotreload(&mut self, snapshot: &MtPoolCow<'static, [u8]>) {
            let Ok((snapshot, _)) =
                bincode::serde::decode_from_slice(snapshot, bincode::config::standard())
            else {
                return;
            };

            let _ = SnapshotManager::build_from_snapshot(snapshot, self);

            let mut players = self.player_clone_pool.new();
            self.game.players.pooled_clone_into(&mut players);

            for (id, character_info) in players.iter() {
                if let Some(stage) = self.game.stages.get_mut(&character_info.stage_id()) {
                    if let Some(character) = stage.world.characters.get_mut(id) {
                        character.core.is_timeout = true;
                        let key = (
                            character.player_info.unique_identifier,
                            character.player_info.id,
                        );
                        if !self.game.timeout_players.contains_key(&key) {
                            self.game
                                .timeout_players
                                .insert(key, (*id, (TICKS_PER_SECOND * 120).into()));
                        } else {
                            self.player_drop(id, PlayerDropReason::Disconnect);
                        }
                    }
                }
            }
        }

        fn build_from_snapshot_for_prev(&mut self, snapshot: &MtPoolCow<'static, [u8]>) {
            let (snapshot, _): (Snapshot, usize) =
                bincode::serde::decode_from_slice(snapshot, bincode::config::standard()).unwrap();

            self.build_prev_from_stages(snapshot.stages);
        }

        fn build_ghosts_from_snapshot(&self, _: &MtPoolCow<'static, [u8]>) -> GhostResult {
            GhostResult {
                players: PoolFxHashMap::new_without_pool(),
            }
        }

        fn events_for(&self, client: EventClientInfo) -> GameEvents {
            // handle game events
            let mut worlds_events = self.game_pools.worlds_events_pool.new();
            let worlds_events_ref = &mut worlds_events;

            let game_pools = &self.game_pools;
            let event_id_generator = &self.event_id_generator;

            self.game.game_pending_events.for_each(hi_closure!([
                game_pools: &GamePooling,
                event_id_generator: &EventIdGenerator,
                worlds_events_ref: &mut MtPoolFxLinkedHashMap<StageId, GameWorldEvents>,
            ], |world_id: &StageId, evs: &Vec<GameWorldEvent>|
             -> () {
                let mut world_events = game_pools.world_events_pool.new();
                for game_event in evs.iter() {
                    GameState::game_event_to_world_event(game_event, &mut world_events, event_id_generator);
                }
                if !world_events.is_empty() {
                    worlds_events_ref.insert(
                        *world_id,
                        GameWorldEvents {
                            events: world_events,
                        },
                    );
                }
            }));

            for player_id in client.client_player_ids.iter() {
                let Some(events) = self.player_events.get(player_id) else {
                    continue;
                };
                let stage_id = self
                    .game
                    .players
                    .player(player_id)
                    .map(|i| i.stage_id())
                    .unwrap_or(self.stage_0_id);
                for event in events.take().drain(..) {
                    let evs = worlds_events.entry(stage_id).or_insert_with(|| {
                        let world_events = self.game_pools.world_events_pool.new();
                        GameWorldEvents {
                            events: world_events,
                        }
                    });
                    Self::game_event_to_world_event(
                        &event,
                        &mut evs.events,
                        &self.event_id_generator,
                    );
                }
            }

            GameEvents {
                worlds: worlds_events,
                event_id: self.event_id_generator.peek_next_id(),
            }
        }

        fn clear_events(&mut self) {
            self.game.game_pending_events.clear_events();
            self.player_events.clear();
        }

        fn sync_event_id(&self, event_id: IdGeneratorIdType) {
            self.event_id_generator.reset_id_for_client(event_id);
        }
    }
}
