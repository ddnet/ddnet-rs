use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet},
    num::NonZeroU32,
    sync::Arc,
    time::Duration,
};

use crate::components::{
    cursor::{RenderCursor, RenderCursorPipe},
    game_objects::{GameObjectsRender, GameObjectsRenderPipe},
    hud::{RenderHud, RenderHudPipe},
    players::{PlayerRenderPipe, Players},
};
use base::{
    hash::Hash, linked_hash_map_view::FxLinkedHashMap, network_string::NetworkReducedAsciiString,
    reduced_ascii_str::ReducedAsciiString,
};
use base_io::io::Io;
use client_containers::utils::{load_containers, RenderGameContainers};
pub use client_render::emote_wheel::render::EmoteWheelInput;
use client_render::{
    actionfeed::render::{ActionfeedRender, ActionfeedRenderPipe},
    chat::render::{ChatRender, ChatRenderOptions, ChatRenderPipe},
    emote_wheel::render::{EmoteWheelRender, EmoteWheelRenderPipe},
    motd::page::{MotdRender, MotdRenderPipe},
    scoreboard::render::{ScoreboardRender, ScoreboardRenderPipe},
    spectator_selection::page::{SpectatorSelectionRender, SpectatorSelectionRenderPipe},
    vote::render::{VoteRender, VoteRenderPipe},
};
use client_render_base::{
    map::{
        map::RenderMap,
        render_map_base::{ClientMapRender, RenderMapLoading},
        render_pipe::{Camera, GameTimeInfo, RenderPipeline},
    },
    render::{
        effects::Effects,
        particle_manager::{ParticleGroup, ParticleManager},
    },
};
use client_types::{
    actionfeed::{Action, ActionInFeed, ActionKill, ActionPlayer},
    chat::{ChatMsg, MsgSystem, ServerMsg, SystemMsgPlayerSkin},
};
use client_ui::{
    chat::user_data::{ChatEvent, ChatMode, MsgInChat},
    emote_wheel::user_data::EmoteWheelEvent,
    spectator_selection::user_data::SpectatorSelectionEvent,
    thumbnail_container::{
        load_thumbnail_container, ThumbnailContainer, DEFAULT_THUMBNAIL_CONTAINER_PATH,
    },
    time_display::TimeDisplay,
    vote::user_data::{VoteRenderData, VoteRenderPlayer, VoteRenderType},
};
use config::config::ConfigDebug;
use egui::{FontDefinitions, Rect};
use game_base::network::{
    messages::{RenderModification, RequiredResources},
    types::chat::NetChatMsg,
};
use game_config::config::{
    ConfigDummyScreenAnchor, ConfigGame, ConfigMap, ConfigRender, ConfigSoundRender,
};
use game_interface::{
    chat_commands::ChatCommands,
    events::{
        GameBuffNinjaEventSound, GameBuffSoundEvent, GameCharacterEffectEvent,
        GameCharacterEventEffect, GameCharacterEventSound, GameCharacterSoundEvent,
        GameDebuffFrozenEventSound, GameDebuffSoundEvent, GameEvents, GameFlagEventSound,
        GameGrenadeEventEffect, GameGrenadeEventSound, GameLaserEventSound,
        GamePickupArmorEventSound, GamePickupHeartEventSound, GamePickupSoundEvent,
        GameShotgunEventSound, GameWorldAction, GameWorldEffectEvent, GameWorldEntityEffectEvent,
        GameWorldEntitySoundEvent, GameWorldEvent, GameWorldNotificationEvent, GameWorldSoundEvent,
        GameWorldSystemMessage,
    },
    interface::MAX_PHYSICS_GROUP_NAME_LEN,
    types::{
        character_info::NetworkCharacterInfo,
        flag::FlagType,
        game::GameTickType,
        id_types::{CharacterId, PlayerId, StageId},
        player_info::{PlayerBanReason, PlayerDropReason, PlayerKickReason},
        render::{
            character::{CharacterBuff, CharacterInfo, LocalCharacterRenderInfo},
            game::game_match::MatchSide,
            scoreboard::Scoreboard,
            stage::StageRenderInfo,
        },
    },
    votes::{VoteState, VoteType, Voted},
};
use graphics::{
    graphics::graphics::Graphics,
    handles::{backend::backend::GraphicsBackendHandle, canvas::canvas::GraphicsCanvasHandle},
};
use graphics_types::rendering::ColorRgba;
use math::math::{vector::vec2, Rng, RngSlice};
use pool::{
    datatypes::{
        PoolBTreeMap, PoolFxHashSet, PoolFxLinkedHashMap, PoolFxLinkedHashSet, PoolVec,
        PoolVecDeque,
    },
    pool::Pool,
    rc::PoolRc,
};
use rayon::ThreadPool;
use serde::{Deserialize, Serialize};
use sound::{
    commands::SoundSceneCreateProps, scene_object::SceneObject, sound::SoundManager,
    sound_listener::SoundListener, types::SoundPlayProps,
};
use ui_base::ui::UiCreator;
use url::Url;

#[derive(Serialize, Deserialize)]
pub enum PlayerFeedbackEvent {
    Chat(ChatEvent),
    EmoteWheel(EmoteWheelEvent),
    SpectatorSelection(SpectatorSelectionEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderModTy {
    /// Load the native built-in render mod
    Native,
    /// Try to load the given mod, falling back to
    /// [`RenderModTy::Native`] on error.
    Try {
        name: ReducedAsciiString,
        /// If the hash is `Some`, the hash must match the WASM module.
        hash: Option<Hash>,
        /// Client local loaded render mod name
        local_name: Option<ReducedAsciiString>,
    },
    /// Load the given mod or abort the whole loading process.
    Required {
        name: ReducedAsciiString,
        /// If the hash is `Some`, the hash must match the WASM module.
        hash: Option<Hash>,
    },
}

impl RenderModTy {
    pub fn render_mod(server_render_mod: &RenderModification, config_game: &ConfigGame) -> Self {
        let (local_render_mod, local_mod_name) =
            match config_game.cl.render_mod.to_lowercase().as_str() {
                "" | "vanilla" | "native" | "default" | "ddnet" => (Self::Native, None),
                _ => {
                    let name: ReducedAsciiString =
                        config_game.cl.render_mod.as_str().try_into().unwrap();
                    (
                        Self::Try {
                            name: name.clone(),
                            hash: None,
                            local_name: None,
                        },
                        Some(name),
                    )
                }
            };
        match server_render_mod {
            RenderModification::Native => local_render_mod,
            RenderModification::TryWasm { name, hash } => Self::Try {
                name: name.clone().into(),
                hash: Some(*hash),
                local_name: local_mod_name,
            },
            RenderModification::RequiresWasm { name, hash } => Self::Required {
                name: name.clone().into(),
                hash: Some(*hash),
            },
        }
    }
}

pub type ClientLocalInfos = Vec<NetworkCharacterInfo>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderGameCreateOptions {
    pub physics_group_name: NetworkReducedAsciiString<MAX_PHYSICS_GROUP_NAME_LEN>,
    pub resource_http_download_url: Option<Url>,
    pub resource_download_server: Option<Url>,
    pub fonts: FontDefinitions,
    pub sound_props: SoundSceneCreateProps,
    pub render_mod: RenderModTy,
    /// The required resources for the server,
    /// it's the server's duty to make sure these resources
    /// can be downloaded from it properly.
    ///
    /// This object is mostly used as a hint for the render
    /// mod to download & prepare the required resources as soon
    /// as possible, but generally is optional.
    pub required_resources: RequiredResources,
    /// The initial client local infos.
    ///
    /// These don't need to come from the server,
    /// instead they can be the infos that come from
    /// the config values.
    ///
    /// The implementation can use this information to speed up
    /// loading of the resources _likely_ to be used.
    pub client_local_infos: ClientLocalInfos,
}

#[derive(Default, Serialize, Deserialize)]
pub struct RenderGameResult {
    /// Events from rendering per player
    pub player_events: FxLinkedHashMap<PlayerId, Vec<PlayerFeedbackEvent>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub enum RenderPlayerCameraMode {
    #[default]
    Default,
    AtPos {
        pos: vec2,
        /// If `true`, then the camera acts like players
        /// from other stages are actually in other stages.
        locked_ingame: bool,
    },
    OnCharacters {
        character_ids: PoolFxHashSet<CharacterId>,
        /// If character is not found
        fallback_pos: vec2,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RenderForPlayer {
    pub chat_info: Option<(ChatMode, String, Option<egui::RawInput>)>,
    pub emote_wheel_input: Option<EmoteWheelInput>,
    pub spectator_selection_input: Option<Option<egui::RawInput>>,
    pub local_player_info: LocalCharacterRenderInfo,
    pub chat_show_all: bool,
    pub scoreboard_active: bool,

    pub zoom: f32,
    pub cam_mode: RenderPlayerCameraMode,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ObservedAnchoredSize {
    pub width: NonZeroU32,
    pub height: NonZeroU32,
}

impl Default for ObservedAnchoredSize {
    fn default() -> Self {
        Self {
            width: 40.try_into().unwrap(),
            height: 40.try_into().unwrap(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ObservedDummyAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl From<ConfigDummyScreenAnchor> for ObservedDummyAnchor {
    fn from(value: ConfigDummyScreenAnchor) -> Self {
        match value {
            ConfigDummyScreenAnchor::TopLeft => Self::TopLeft,
            ConfigDummyScreenAnchor::TopRight => Self::TopRight,
            ConfigDummyScreenAnchor::BottomLeft => Self::BottomLeft,
            ConfigDummyScreenAnchor::BottomRight => Self::BottomRight,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ObservedPlayer {
    /// Player observes a dummy
    Dummy {
        player_id: PlayerId,
        local_player_info: LocalCharacterRenderInfo,
        anchor: ObservedDummyAnchor,
    },
    /// The server allows to obverse a voted player.
    Vote { player_id: PlayerId },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RenderGameForPlayer {
    pub render_for_player: RenderForPlayer,
    /// Players that this player observes.
    /// What that means is:
    /// - A mini screen for the dummy is requested.
    /// - A player is about to be voted (kicked or whatever).
    pub observed_players: PoolVec<ObservedPlayer>,
    /// For all anchored observed players, these are the size properties.
    pub observed_anchored_size_props: ObservedAnchoredSize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AllowPredictionEventType {
    None,
    /// Events only from local characters, this excludes
    /// owned projectiles etc.
    LocalCharacter,
    /// All events from prediction
    All,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RenderGameSettings {
    pub spatial_sound: bool,
    pub sound_playback_speed: f64,
    /// For music from the map
    pub map_sound_volume: f64,
    /// For all the various sounds ingame
    pub ingame_sound_volume: f64,

    pub nameplates: bool,
    pub nameplate_own: bool,
    /// DPI to be used for HUD & UI
    pub pixels_per_point: f32,
    /// Transparency of objects that are phased from the
    /// local character's world (e.g. ddrace teams or solo)
    pub phased_alpha: f32,
    /// Whether hook related sounds should happen where the hook
    /// is instead of where the character that owns the hook
    /// is. In teeworlds it's the latter.
    pub hook_sound_on_hook_pos: bool,
    /// Whether prediction events are allowed or not
    pub allow_prediction_events: AllowPredictionEventType,
    /// The concept of ingame aspect works as followed:
    /// The UI rendering should be seen as something completely
    /// decoupled from the ingame aspect ratio, allowing to use
    /// e.g. 5:4 for ingame rendering while the UI is still normally rendered
    /// natively.
    pub ingame_aspect: Option<f32>,
    /// Whether to enable dynamic camera while spectating another
    /// character.
    pub spec_dyncam: bool,
}

impl RenderGameSettings {
    pub fn new(
        render: &ConfigRender,
        snd: &ConfigSoundRender,
        window_pixels_per_point: f32,
        sound_playback_speed: f64,
        anti_ping: bool,
        global_volume: f64,
    ) -> Self {
        Self {
            spatial_sound: snd.spatial,
            sound_playback_speed,
            nameplates: render.nameplates,
            nameplate_own: render.own_nameplate,
            ingame_sound_volume: snd.ingame_sound_volume * global_volume,
            map_sound_volume: snd.map_sound_volume * global_volume,
            pixels_per_point: window_pixels_per_point
                .max(render.ingame_ui_min_pixels_per_point as f32)
                * render.ingame_ui_scale as f32,
            phased_alpha: render.phased_alpha as f32,
            hook_sound_on_hook_pos: render.hook_sound_on_hook_pos,
            allow_prediction_events: if anti_ping {
                AllowPredictionEventType::All
            } else {
                AllowPredictionEventType::LocalCharacter
            },
            ingame_aspect: render
                .use_ingame_aspect_ratio
                .then_some(render.ingame_aspect_ratio as f32),
            spec_dyncam: render.spec_dyncam,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RenderGameInput {
    pub players: PoolFxLinkedHashMap<PlayerId, RenderGameForPlayer>,
    /// Players currently not controlled by a human player
    /// (and thus not interesting for rendering).
    #[doc(alias = "inactive_players")]
    #[doc(alias = "uncontrolled_players")]
    pub dummies: PoolFxLinkedHashSet<PlayerId>,
    /// The bool indicates if the events were generated on the client (`true`) or
    /// from the server.
    pub events: PoolBTreeMap<(GameTickType, bool), GameEvents>,
    pub chat_msgs: PoolVecDeque<NetChatMsg>,
    /// Vote state
    pub vote: Option<(PoolRc<VoteState>, Option<Voted>, Duration)>,

    pub character_infos: PoolFxLinkedHashMap<CharacterId, CharacterInfo>,
    pub stages: PoolFxLinkedHashMap<StageId, StageRenderInfo>,
    pub scoreboard_info: Option<Scoreboard>,

    pub game_time_info: GameTimeInfo,

    pub settings: RenderGameSettings,

    /// Arbitrary space for any kind of extensions
    pub ext: PoolVec<u8>,
}

type RenderPlayerHelper = (i32, i32, u32, u32, (PlayerId, RenderGameForPlayer));

pub struct RenderGame {
    // containers
    containers: RenderGameContainers,
    map_vote_thumbnails_container: ThumbnailContainer,

    // render components
    players: Players,
    render: GameObjectsRender,
    cursor_render: RenderCursor,
    chat: ChatRender,
    actionfeed: ActionfeedRender,
    scoreboard: ScoreboardRender,
    hud: RenderHud,
    particles: ParticleManager,
    emote_wheel: EmoteWheelRender,
    vote: VoteRender,
    motd: MotdRender,
    spectator_selection: SpectatorSelectionRender,

    // chat commands
    chat_commands: ChatCommands,

    last_event_monotonic_tick: Option<GameTickType>,

    // map
    map: ClientMapRender,
    physics_group_name: NetworkReducedAsciiString<MAX_PHYSICS_GROUP_NAME_LEN>,

    canvas_handle: GraphicsCanvasHandle,
    backend_handle: GraphicsBackendHandle,

    // props
    client_local_infos: ClientLocalInfos,

    // helpers
    helper: Pool<Vec<RenderPlayerHelper>>,

    world_sound_scene: SceneObject,
    world_sound_listeners: HashMap<PlayerId, SoundListener>,
    world_sound_listeners_pool: Pool<HashMap<PlayerId, SoundListener>>,
    rng: Rng,
}

impl RenderGame {
    pub fn new(
        sound: &SoundManager,
        graphics: &Graphics,
        io: &Io,
        thread_pool: &Arc<ThreadPool>,
        cur_time: &Duration,
        map_file: Vec<u8>,
        config: &ConfigDebug,
        props: RenderGameCreateOptions,
    ) -> Result<Self, String> {
        let scene = sound.scene_handle.create(props.sound_props.clone());

        let physics_group_name = props.physics_group_name;
        let map = ClientMapRender::new(RenderMapLoading::new(
            thread_pool.clone(),
            map_file,
            props.resource_download_server.clone(),
            io.clone(),
            sound,
            props.sound_props,
            graphics,
            config,
            Some("downloaded".as_ref()),
        ));

        let mut containers = load_containers(
            io,
            thread_pool,
            props.resource_http_download_url,
            props.resource_download_server.clone(),
            false,
            graphics,
            sound,
            &scene,
        );

        // Use own ui context for nameplates
        let mut nameplats_creator = UiCreator::default();
        nameplats_creator.load_font(&props.fonts);

        let players = Players::new(graphics, &nameplats_creator);
        let render = GameObjectsRender::new(graphics);
        let cursor_render = RenderCursor::new(graphics);
        let particles = ParticleManager::new(graphics, cur_time);

        let mut creator = UiCreator::default();
        creator.load_font(&props.fonts);

        let hud = RenderHud::new(graphics, &creator);
        let chat = ChatRender::new(graphics, &creator);
        let actionfeed = ActionfeedRender::new(graphics, &creator);
        let scoreboard = ScoreboardRender::new(graphics, &creator);
        let emote_wheel = EmoteWheelRender::new(graphics, &creator);
        let vote = VoteRender::new(graphics, &creator);
        let motd = MotdRender::new(graphics, &creator);
        let spectator_selection = SpectatorSelectionRender::new(graphics, &creator);

        let mut map_vote_thumbnails_container = load_thumbnail_container(
            io.clone(),
            thread_pool.clone(),
            DEFAULT_THUMBNAIL_CONTAINER_PATH,
            "map-votes-thumbnail",
            graphics,
            sound,
            scene.clone(),
            props.resource_download_server,
        );

        Self::update_containers_impl(
            &mut containers,
            &mut map_vote_thumbnails_container,
            cur_time,
            props.client_local_infos.iter(),
        );

        Ok(Self {
            // containers
            containers,
            map_vote_thumbnails_container,

            // components
            players,
            render,
            cursor_render,
            chat,
            actionfeed,
            scoreboard,
            hud,
            particles,
            emote_wheel,
            vote,
            motd,
            spectator_selection,

            // chat commands
            chat_commands: Default::default(),

            last_event_monotonic_tick: None,

            map,
            physics_group_name,

            canvas_handle: graphics.canvas_handle.clone(),
            backend_handle: graphics.backend_handle.clone(),

            client_local_infos: props.client_local_infos,

            helper: Pool::with_capacity(1),

            world_sound_scene: scene,
            world_sound_listeners: Default::default(),
            world_sound_listeners_pool: Pool::with_capacity(2),
            rng: Rng::new(0),
        })
    }

    fn render_ingame(
        &mut self,

        config_map: &ConfigMap,
        cur_time: &Duration,

        render_info: &RenderGameInput,
        player_info: Option<(&PlayerId, &RenderForPlayer)>,
    ) {
        let map = self.map.try_get().unwrap();

        let mut cam = Camera {
            pos: Default::default(),
            zoom: 1.0,
            forced_aspect_ratio: render_info.settings.ingame_aspect,
            parallax_aware_zoom: true,
        };

        let camera_player_id = player_info.and_then(|(player_id, p)| match &p.cam_mode {
            RenderPlayerCameraMode::Default | RenderPlayerCameraMode::AtPos { .. } => {
                Some(player_id)
            }
            RenderPlayerCameraMode::OnCharacters { character_ids, .. } => {
                character_ids.iter().next()
            }
        });

        // Spectators for example don't need any phased state
        let forced_non_phased_rendering = player_info
            .map(|(_, p)| match &p.cam_mode {
                RenderPlayerCameraMode::Default => false,
                RenderPlayerCameraMode::AtPos { locked_ingame, .. } => !*locked_ingame,
                RenderPlayerCameraMode::OnCharacters { .. } => false,
            })
            .unwrap_or_default();

        let camera_character_info =
            camera_player_id.and_then(|player_id| render_info.character_infos.get(player_id));

        let camera_character_render_info = camera_character_info
            .zip(camera_player_id)
            .and_then(|(c, player_id)| {
                c.stage_id
                    .and_then(|id| render_info.stages.get(&id).map(|p| (p, player_id)))
            })
            .and_then(|(s, player_id)| s.world.characters.get(player_id));

        let mut cur_anim_time = Duration::ZERO;
        if let (Some((_, local_render_info)), Some(character)) =
            (player_info, camera_character_render_info)
        {
            cam.pos = character.lerped_pos;
            cam.zoom = local_render_info.zoom;
            cur_anim_time = RenderMap::calc_anim_time(
                render_info.game_time_info.ticks_per_second,
                character.animation_ticks_passed,
                &render_info.game_time_info.intra_tick_time,
            );
        } else if let Some(character_stage_info) = camera_character_info.and_then(|c| {
            c.stage_id
                .and_then(|stage_id| render_info.stages.get(&stage_id))
        }) {
            // fallback to the stage info to calculate the anim time.
            cur_anim_time = RenderMap::calc_anim_time(
                render_info.game_time_info.ticks_per_second,
                character_stage_info.game_ticks_passed,
                &render_info.game_time_info.intra_tick_time,
            );
        }
        if let Some((_, p)) = player_info {
            cam.pos = match p.cam_mode {
                RenderPlayerCameraMode::Default => {
                    // add dyn cam offset if it existing
                    cam.pos
                        + camera_character_render_info
                            .map(|c| {
                                vec2::new(
                                    c.lerped_dyn_cam_offset.x as f32,
                                    c.lerped_dyn_cam_offset.y as f32,
                                )
                            })
                            .unwrap_or_default()
                }
                RenderPlayerCameraMode::AtPos { pos, .. } => {
                    // also update zoom
                    cam.zoom = p.zoom;
                    pos
                }
                RenderPlayerCameraMode::OnCharacters { fallback_pos, .. } => {
                    camera_character_render_info
                        .map(|character| {
                            character.lerped_pos
                                + if render_info.settings.spec_dyncam {
                                    vec2::new(
                                        character.lerped_dyn_cam_offset.x as f32,
                                        character.lerped_dyn_cam_offset.y as f32,
                                    )
                                } else {
                                    vec2::default()
                                }
                        })
                        .unwrap_or(fallback_pos)
                }
            };
        }

        let render_map = map;

        // map + ingame objects
        let mut render_pipe = RenderPipeline::new(
            &render_map.data.buffered_map.map_visual,
            &render_map.data.buffered_map,
            config_map,
            cur_time,
            &cur_anim_time,
            &cam,
            &mut self.containers.entities_container,
            camera_character_info.map(|c| c.info.entities.borrow()),
            self.physics_group_name.as_str(),
            render_info.settings.map_sound_volume,
        );
        render_map.render.render_background(&mut render_pipe);
        self.particles.render_group(
            ParticleGroup::ProjectileTrail,
            &mut self.containers.particles_container,
            &render_info.character_infos,
            &cam,
        );
        for ((_, stage), local_characters_stage) in render_info
            .stages
            .iter()
            .filter(|(&stage_id, _)| {
                camera_character_info.and_then(|c| c.stage_id) != Some(stage_id)
            })
            .map(|s| (s, false))
            .chain(
                camera_character_info
                    .and_then(|c| c.stage_id)
                    .and_then(|stage_id| render_info.stages.get_key_value(&stage_id))
                    .into_iter()
                    .map(|s| (s, true)),
            )
        {
            self.render.render(&mut GameObjectsRenderPipe {
                particle_manager: &mut self.particles,
                cur_time,
                game_time_info: &render_info.game_time_info,

                projectiles: &stage.world.projectiles,
                flags: &stage.world.ctf_flags,
                pickups: &stage.world.pickups,
                lasers: &stage.world.lasers,
                character_infos: &render_info.character_infos,

                ctf_container: &mut self.containers.ctf_container,
                game_container: &mut self.containers.game_container,
                ninja_container: &mut self.containers.ninja_container,
                weapon_container: &mut self.containers.weapon_container,

                camera: &cam,

                local_character_id: camera_player_id,

                phased_alpha: render_info.settings.phased_alpha,
                phased: !local_characters_stage && !forced_non_phased_rendering,
            });
            self.players.render(&mut PlayerRenderPipe {
                cur_time,
                game_time_info: &render_info.game_time_info,
                render_infos: &stage.world.characters,
                character_infos: &render_info.character_infos,

                particle_manager: &mut self.particles,

                skins: &mut self.containers.skin_container,
                ninjas: &mut self.containers.ninja_container,
                freezes: &mut self.containers.freeze_container,
                hooks: &mut self.containers.hook_container,
                weapons: &mut self.containers.weapon_container,
                emoticons: &mut self.containers.emoticons_container,

                collision: &render_map.data.collision,
                camera: &cam,

                own_character: camera_player_id,

                spatial_sound: render_info.settings.spatial_sound,
                sound_playback_speed: render_info.settings.sound_playback_speed,
                ingame_sound_volume: render_info.settings.ingame_sound_volume,

                phased_alpha: render_info.settings.phased_alpha,
                phased: !local_characters_stage && !forced_non_phased_rendering,
            });
        }
        let mut render_pipe = RenderPipeline::new(
            &render_map.data.buffered_map.map_visual,
            &render_map.data.buffered_map,
            config_map,
            cur_time,
            &cur_anim_time,
            &cam,
            &mut self.containers.entities_container,
            camera_character_info.map(|c| c.info.entities.borrow()),
            self.physics_group_name.as_str(),
            render_info.settings.map_sound_volume,
        );
        render_map.render.render_physics_layers(
            &mut render_pipe.base,
            &render_map.data.buffered_map.render.physics_render_layers,
        );
        render_map.render.render_foreground(&mut render_pipe);

        for (stage_id, stage) in render_info.stages.iter() {
            let local_characters_stage = camera_character_info
                .map(|c| c.stage_id == Some(*stage_id))
                .unwrap_or_default();
            self.players.render_nameplates(
                cur_time,
                &cam,
                &stage.world.characters,
                &render_info.character_infos,
                render_info.settings.nameplates,
                render_info.settings.nameplate_own,
                player_info.map(|(player_id, _)| player_id),
                !local_characters_stage && !forced_non_phased_rendering,
                render_info.settings.phased_alpha,
            );
        }

        self.particles.render_groups(
            ParticleGroup::Explosions,
            &mut self.containers.particles_container,
            &render_info.character_infos,
            &cam,
        );
        // cursor
        if let Some(player) = camera_character_render_info {
            self.cursor_render.render(&mut RenderCursorPipe {
                mouse_cursor: player.lerped_cursor_pos,
                weapon_container: &mut self.containers.weapon_container,
                weapon_key: camera_character_info.map(|c| c.info.weapon.borrow()),
                cur_weapon: player.cur_weapon,
                is_ninja: player.buffs.contains_key(&CharacterBuff::Ninja),
                ninja_container: &mut self.containers.ninja_container,
                ninja_key: camera_character_info.map(|c| c.info.ninja.borrow()),
                camera: &cam,
            });
        }
    }

    /// render hud + uis: chat, scoreboard etc.
    #[must_use]
    fn render_uis(
        &mut self,

        cur_time: &Duration,

        render_info: &RenderGameInput,
        mut player_info: Option<(&PlayerId, &mut RenderGameForPlayer)>,
        local_player_ids: &Option<HashSet<PlayerId>>,
        player_vote_rect: &mut Option<Rect>,
        expects_player_vote_miniscreen: bool,
    ) -> Vec<PlayerFeedbackEvent> {
        let mut res: Vec<PlayerFeedbackEvent> = Default::default();
        // chat & emote wheel
        if let Some((player_id, player_render_info)) = player_info
            .as_mut()
            .map(|(id, p)| (id, &mut p.render_for_player))
        {
            let mut dummy_str: String = Default::default();
            let mut dummy_str_ref = &mut dummy_str;
            let mut dummy_state = &mut None;
            let mut dummy_mode = ChatMode::Global;

            let chat_active = if let Some((chat_mode, chat_msg, chat_state)) =
                &mut player_render_info.chat_info
            {
                dummy_str_ref = chat_msg;
                dummy_state = chat_state;
                dummy_mode = *chat_mode;
                true
            } else {
                false
            };

            let dummy_local_player_ids = HashSet::default();
            let local_player_ids = local_player_ids.as_ref().unwrap_or(&dummy_local_player_ids);

            res.extend(
                self.chat
                    .render(&mut ChatRenderPipe {
                        cur_time,
                        msg: dummy_str_ref,
                        options: ChatRenderOptions {
                            is_chat_input_active: chat_active,
                            show_chat_history: player_render_info.chat_show_all,
                        },
                        input: dummy_state,
                        mode: dummy_mode,
                        skin_container: &mut self.containers.skin_container,
                        tee_render: &mut self.players.tee_renderer,
                        character_infos: &render_info.character_infos,
                        local_character_ids: local_player_ids,
                    })
                    .into_iter()
                    .map(PlayerFeedbackEvent::Chat),
            );

            let character_info = render_info.character_infos.get(player_id);

            let mut dummy_input = &mut EmoteWheelInput {
                egui: None,
                xrel: 0.0,
                yrel: 0.0,
            };

            let wheel_active = if let Some(emote_input) = &mut player_render_info.emote_wheel_input
            {
                dummy_input = emote_input;
                true
            } else {
                false
            };

            if wheel_active {
                let default_key = self.containers.emoticons_container.default_key.clone();
                let skin_default_key = self.containers.skin_container.default_key.clone();
                res.extend(
                    self.emote_wheel
                        .render(&mut EmoteWheelRenderPipe {
                            cur_time,
                            input: dummy_input,
                            skin_container: &mut self.containers.skin_container,
                            emoticons_container: &mut self.containers.emoticons_container,
                            tee_render: &mut self.players.tee_renderer,
                            emoticons: character_info
                                .map(|c| c.info.emoticons.borrow())
                                .unwrap_or(&*default_key),
                            skin: character_info
                                .map(|c| c.info.skin.borrow())
                                .unwrap_or(&*skin_default_key),
                            skin_info: &character_info.map(|c| c.skin_info),
                        })
                        .into_iter()
                        .map(PlayerFeedbackEvent::EmoteWheel),
                );
            }

            let mut dummy_input = &mut None;

            let spectator_selection_active =
                if let Some(input) = &mut player_render_info.spectator_selection_input {
                    dummy_input = input;
                    true
                } else {
                    false
                };

            // spectator selection list
            if spectator_selection_active {
                let evs = self
                    .spectator_selection
                    .render(&mut SpectatorSelectionRenderPipe {
                        cur_time,
                        input: dummy_input,
                        skin_container: &mut self.containers.skin_container,
                        skin_renderer: &self.players.tee_renderer,
                        character_infos: &render_info.character_infos,
                    });

                res.extend(evs.into_iter().map(PlayerFeedbackEvent::SpectatorSelection));
            }
        }

        // action feed
        self.actionfeed.render(&mut ActionfeedRenderPipe {
            cur_time,
            skin_container: &mut self.containers.skin_container,
            tee_render: &mut self.players.tee_renderer,
            weapon_container: &mut self.containers.weapon_container,
            toolkit_render: &self.players.toolkit_renderer,
            ninja_container: &mut self.containers.ninja_container,
        });

        // hud + scoreboard
        if let Some((player_id, render_for_game)) = player_info {
            let local_render_info = &render_for_game.render_for_player;

            let cam_player_id = match &render_for_game.render_for_player.cam_mode {
                RenderPlayerCameraMode::Default | RenderPlayerCameraMode::AtPos { .. } => player_id,
                RenderPlayerCameraMode::OnCharacters { character_ids, .. } => {
                    if !character_ids.is_empty() {
                        character_ids.iter().next().unwrap()
                    } else {
                        player_id
                    }
                }
            };
            let character_info = render_info.character_infos.get(cam_player_id);

            let stage = render_info
                .character_infos
                .get(cam_player_id)
                .and_then(|c| c.stage_id.and_then(|id| render_info.stages.get(&id)))
                .or_else(|| {
                    // if there is exactly one stage, then we simply use that, so spectators have good experience
                    if render_info.stages.len() == 1 {
                        render_info.stages.front().map(|(_, s)| s)
                    } else {
                        None
                    }
                });
            let p = stage.and_then(|s| s.world.characters.get(cam_player_id));
            self.hud.render(&mut RenderHudPipe {
                hud_container: &mut self.containers.hud_container,
                hud_key: character_info.map(|c| c.info.hud.borrow()),
                weapon_container: &mut self.containers.weapon_container,
                weapon_key: character_info.map(|c| c.info.weapon.borrow()),
                local_player_render_info: &local_render_info.local_player_info,
                cur_weapon: p.map(|c| c.cur_weapon).unwrap_or_default(),
                race_timer_counter: &p
                    .map(|p| p.game_ticks_passed)
                    .or_else(|| stage.map(|s| s.game_ticks_passed))
                    .or_else(|| render_info.stages.front().map(|(_, s)| s.game_ticks_passed))
                    .unwrap_or_default(),
                ticks_per_second: &render_info.game_time_info.ticks_per_second,
                cur_time,
                game: stage.map(|s| &s.game),
                skin_container: &mut self.containers.skin_container,
                skin_renderer: &self.players.tee_renderer,
                ctf_container: &mut self.containers.ctf_container,
                character_infos: &render_info.character_infos,
            });
            if let Some(scoreboard_info) = local_render_info
                .scoreboard_active
                .then_some(())
                .and(render_info.scoreboard_info.as_ref())
            {
                // scoreboard after hud
                self.scoreboard.render(&mut ScoreboardRenderPipe {
                    cur_time,
                    scoreboard: scoreboard_info,
                    character_infos: &render_info.character_infos,
                    skin_container: &mut self.containers.skin_container,
                    tee_render: &mut self.players.tee_renderer,
                    flags_container: &mut self.containers.flags_container,

                    // for scoreboard this should remain the "real" player's id
                    own_character_id: player_id,
                });
            }
        }

        // message of the day
        self.motd.render(&mut MotdRenderPipe { cur_time });

        // current vote
        if let Some((vote, voted, remaining_time)) = &render_info.vote {
            let ty = match &vote.vote {
                VoteType::Map { key, map } => VoteRenderType::Map { key, map },
                VoteType::RandomUnfinishedMap { key } => {
                    VoteRenderType::RandomUnfinishedMap { key }
                }
                VoteType::VoteKickPlayer {
                    key,
                    name,
                    skin,
                    skin_info,
                }
                | VoteType::VoteSpecPlayer {
                    key,
                    name,
                    skin,
                    skin_info,
                } => render_info
                    .character_infos
                    .get(&key.voted_player_id)
                    .map(|player| {
                        let vote_player = VoteRenderPlayer {
                            name: &player.info.name,
                            skin: player.info.skin.borrow(),
                            skin_info: &player.skin_info,
                            reason: &key.reason,
                        };
                        if matches!(vote.vote, VoteType::VoteKickPlayer { .. }) {
                            VoteRenderType::PlayerVoteKick(vote_player)
                        } else {
                            VoteRenderType::PlayerVoteSpec(vote_player)
                        }
                    })
                    .unwrap_or_else(|| {
                        let vote_player = VoteRenderPlayer {
                            name: name.as_str(),
                            skin: (*skin).borrow(),
                            skin_info,
                            reason: &key.reason,
                        };
                        if matches!(vote.vote, VoteType::VoteKickPlayer { .. }) {
                            VoteRenderType::PlayerVoteKick(vote_player)
                        } else {
                            VoteRenderType::PlayerVoteSpec(vote_player)
                        }
                    }),
                VoteType::Misc { key, vote } => VoteRenderType::Misc { key, vote },
            };
            {
                *player_vote_rect = self.vote.render(&mut VoteRenderPipe {
                    cur_time,
                    skin_container: &mut self.containers.skin_container,
                    map_vote_thumbnail_container: &mut self.map_vote_thumbnails_container,
                    tee_render: &mut self.players.tee_renderer,
                    vote_data: VoteRenderData {
                        ty,
                        data: vote,
                        remaining_time,
                        voted: *voted,
                    },
                    expects_player_vote_miniscreen,
                });
            }
        }

        res
    }
}

pub trait RenderGameInterface {
    fn render(
        &mut self,
        config_map: &ConfigMap,
        cur_time: &Duration,
        input: RenderGameInput,
    ) -> RenderGameResult;
    fn continue_loading(&mut self) -> Result<bool, String>;
    fn set_chat_commands(&mut self, chat_commands: ChatCommands);
    /// Clear all rendering state (like particles, sounds etc.)
    fn clear_render_state(&mut self);
    /// Render sound for an off-air scene.
    /// If the game scene is not off-air,
    /// it will throw errors in the sound backend.
    fn render_offair_sound(&mut self, samples: u32);
}

impl RenderGame {
    fn convert_system_ev(ev: &GameWorldSystemMessage) -> String {
        match ev {
            GameWorldSystemMessage::PlayerJoined { name, .. } => {
                format!("\"{}\" joined the game.", name.as_str())
            }
            GameWorldSystemMessage::PlayerLeft { name, reason, .. } => match reason {
                PlayerDropReason::Disconnect => format!("\"{}\" left the game.", name.as_str()),
                PlayerDropReason::Timeout => {
                    format!("\"{}\" has timed out and left the game.", name.as_str())
                }
                PlayerDropReason::Kicked(reason) => match reason {
                    PlayerKickReason::Rcon => {
                        format!(
                            "\"{}\" has been kicked by a moderator \
                            and left the game.",
                            name.as_str()
                        )
                    }
                    PlayerKickReason::Custom(text) => {
                        format!(
                            "\"{}\" has been kicked and left the game: {text}",
                            name.as_str()
                        )
                    }
                },
                PlayerDropReason::Banned { reason, until } => match reason {
                    PlayerBanReason::Vote => format!(
                        "\"{}\" has been banned by vote{}.",
                        name.as_str(),
                        until
                            .map(|until| format!(" until {}", until.to_local_time_string(false)))
                            .unwrap_or_default()
                    ),
                    PlayerBanReason::Rcon => format!(
                        "\"{}\" has been banned by a moderator{}.",
                        name.as_str(),
                        until
                            .map(|until| format!(" until {}", until.to_local_time_string(false)))
                            .unwrap_or_default()
                    ),
                    PlayerBanReason::Custom(text) => format!(
                        "\"{}\" has been banned{}: {text}",
                        name.as_str(),
                        until
                            .map(|until| format!(" until {}", until.to_local_time_string(false)))
                            .unwrap_or_default()
                    ),
                },
            },
            GameWorldSystemMessage::CharacterInfoChanged {
                old_name, new_name, ..
            } => {
                format!(
                    "\"{}\" \u{f061} \"{}\"",
                    old_name.as_str(),
                    new_name.as_str()
                )
            }
            GameWorldSystemMessage::Custom(msg) => msg.to_string(),
        }
    }

    fn handle_action_feed(
        &mut self,
        cur_time: &Duration,
        character_infos: &PoolFxLinkedHashMap<CharacterId, CharacterInfo>,
        ev: GameWorldAction,
    ) {
        match ev {
            GameWorldAction::Kill {
                killer,
                assists,
                victims,
                weapon,
                flags,
            } => {
                self.actionfeed.msgs.push_front(ActionInFeed {
                    action: Action::Kill(ActionKill {
                        killer: killer.and_then(|killer| {
                            character_infos.get(&killer).map(|char| ActionPlayer {
                                name: char.info.name.to_string(),
                                skin: char.info.skin.clone().into(),
                                skin_info: char.skin_info,
                                weapon: char.info.weapon.clone().into(),
                            })
                        }),
                        assists: assists
                            .iter()
                            .filter_map(|id| {
                                character_infos.get(id).map(|char| ActionPlayer {
                                    name: char.info.name.to_string(),
                                    skin: char.info.skin.clone().into(),
                                    skin_info: char.skin_info,
                                    weapon: char.info.weapon.clone().into(),
                                })
                            })
                            .collect(),
                        victims: victims
                            .iter()
                            .filter_map(|id| {
                                character_infos.get(id).map(|char| ActionPlayer {
                                    name: char.info.name.to_string(),
                                    skin: char.info.skin.clone().into(),
                                    skin_info: char.skin_info,
                                    weapon: char.info.weapon.clone().into(),
                                })
                            })
                            .collect(),
                        weapon,
                        flags,
                    }),
                    add_time: *cur_time,
                });
            }
            GameWorldAction::RaceFinish {
                character,
                finish_time,
            } => {
                if let Some(c) = character_infos.get(&character) {
                    self.actionfeed.msgs.push_front(ActionInFeed {
                        action: Action::RaceFinish {
                            player: ActionPlayer {
                                name: c.info.name.to_string(),
                                skin: c.info.skin.clone().into(),
                                skin_info: c.skin_info,
                                weapon: c.info.weapon.clone().into(),
                            },
                            finish_time,
                        },
                        add_time: *cur_time,
                    });
                }
            }
            GameWorldAction::RaceTeamFinish {
                characters,
                team_name,
                finish_time,
            } => {
                self.actionfeed.msgs.push_front(ActionInFeed {
                    action: Action::RaceTeamFinish {
                        players: characters
                            .iter()
                            .filter_map(|c| {
                                character_infos.get(c).map(|c| ActionPlayer {
                                    name: c.info.name.to_string(),
                                    skin: c.info.skin.clone().into(),
                                    skin_info: c.skin_info,
                                    weapon: c.info.weapon.clone().into(),
                                })
                            })
                            .collect(),
                        team_name: team_name.to_string(),
                        finish_time,
                    },
                    add_time: *cur_time,
                });
            }
            GameWorldAction::Custom(_) => todo!(),
        }
    }

    fn handle_character_sound_event(
        &mut self,
        character_infos: &PoolFxLinkedHashMap<CharacterId, CharacterInfo>,
        settings: &RenderGameSettings,
        pos: Option<vec2>,
        ev: GameCharacterSoundEvent,
        id: Option<CharacterId>,
    ) {
        let info = id.and_then(|id| character_infos.get(&id).map(|c| &c.info));
        match ev {
            GameCharacterSoundEvent::Sound(sound) => match sound {
                GameCharacterEventSound::WeaponSwitch { new_weapon } => {
                    self.containers
                        .weapon_container
                        .get_or_default_opt(info.map(|i| &i.weapon))
                        .by_type(new_weapon)
                        .switch
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GameCharacterEventSound::NoAmmo { weapon } => {
                    self.containers
                        .weapon_container
                        .get_or_default_opt(info.map(|i| &i.weapon))
                        .by_type(weapon)
                        .noammo
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GameCharacterEventSound::HammerFire => {
                    self.containers
                        .weapon_container
                        .get_or_default_opt(info.map(|i| &i.weapon))
                        .hammer
                        .weapon
                        .fire
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GameCharacterEventSound::GunFire => {
                    self.containers
                        .weapon_container
                        .get_or_default_opt(info.map(|i| &i.weapon))
                        .gun
                        .weapon
                        .fire
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GameCharacterEventSound::GrenadeFire => {
                    self.containers
                        .weapon_container
                        .get_or_default_opt(info.map(|i| &i.weapon))
                        .grenade
                        .weapon
                        .fire
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GameCharacterEventSound::LaserFire => {
                    self.containers
                        .weapon_container
                        .get_or_default_opt(info.map(|i| &i.weapon))
                        .laser
                        .weapon
                        .fire
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GameCharacterEventSound::ShotgunFire => {
                    self.containers
                        .weapon_container
                        .get_or_default_opt(info.map(|i| &i.weapon))
                        .shotgun
                        .weapon
                        .fire
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GameCharacterEventSound::GroundJump => {
                    self.containers
                        .skin_container
                        .get_or_default_opt(info.map(|i| &i.skin))
                        .sounds
                        .ground_jump
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GameCharacterEventSound::AirJump => {
                    self.containers
                        .skin_container
                        .get_or_default_opt(info.map(|i| &i.skin))
                        .sounds
                        .air_jump
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GameCharacterEventSound::Spawn => {
                    self.containers
                        .skin_container
                        .get_or_default_opt(info.map(|i| &i.skin))
                        .sounds
                        .spawn
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GameCharacterEventSound::Death => {
                    self.containers
                        .skin_container
                        .get_or_default_opt(info.map(|i| &i.skin))
                        .sounds
                        .death
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GameCharacterEventSound::HookHitPlayer { hook_pos } => {
                    let pos = if settings.hook_sound_on_hook_pos {
                        hook_pos
                    } else {
                        pos
                    };
                    self.containers
                        .hook_container
                        .get_or_default_opt(info.map(|i| &i.hook))
                        .hit_player
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GameCharacterEventSound::HookHitHookable { hook_pos } => {
                    let pos = if settings.hook_sound_on_hook_pos {
                        hook_pos
                    } else {
                        pos
                    };
                    self.containers
                        .hook_container
                        .get_or_default_opt(info.map(|i| &i.hook))
                        .hit_hookable
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GameCharacterEventSound::HookHitUnhookable { hook_pos } => {
                    let pos = if settings.hook_sound_on_hook_pos {
                        hook_pos
                    } else {
                        pos
                    };
                    self.containers
                        .hook_container
                        .get_or_default_opt(info.map(|i| &i.hook))
                        .hit_unhookable
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GameCharacterEventSound::Pain { long } => {
                    let sounds = &self
                        .containers
                        .skin_container
                        .get_or_default_opt(info.map(|i| &i.skin))
                        .sounds;
                    let sounds = if long {
                        sounds.pain_long.as_slice()
                    } else {
                        sounds.pain_short.as_slice()
                    };
                    sounds
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GameCharacterEventSound::Hit { strong } => {
                    let sounds = &self
                        .containers
                        .skin_container
                        .get_or_default_opt(info.map(|i| &i.skin))
                        .sounds;
                    let hits = if strong {
                        sounds.hit_strong.as_slice()
                    } else {
                        sounds.hit_weak.as_slice()
                    };
                    hits.random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GameCharacterEventSound::HammerHit => {
                    self.containers
                        .weapon_container
                        .get_or_default_opt(info.map(|i| &i.weapon))
                        .hammer
                        .hits
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
            },
            GameCharacterSoundEvent::Buff(ev) => match ev {
                GameBuffSoundEvent::Ninja(ev) => match ev {
                    GameBuffNinjaEventSound::Spawn => {
                        self.containers
                            .ninja_container
                            .get_or_default_opt(info.map(|i| &i.ninja))
                            .spawn
                            .random_entry(&mut self.rng)
                            .play(
                                SoundPlayProps::new_with_pos_opt(pos)
                                    .with_with_spatial(settings.spatial_sound)
                                    .with_playback_speed(settings.sound_playback_speed)
                                    .with_volume(settings.ingame_sound_volume),
                            )
                            .detatch();
                    }
                    GameBuffNinjaEventSound::Collect => {
                        self.containers
                            .ninja_container
                            .get_or_default_opt(info.map(|i| &i.ninja))
                            .collect
                            .random_entry(&mut self.rng)
                            .play(
                                SoundPlayProps::new_with_pos_opt(pos)
                                    .with_with_spatial(settings.spatial_sound)
                                    .with_playback_speed(settings.sound_playback_speed)
                                    .with_volume(settings.ingame_sound_volume),
                            )
                            .detatch();
                    }
                    GameBuffNinjaEventSound::Attack => {
                        self.containers
                            .ninja_container
                            .get_or_default_opt(info.map(|i| &i.ninja))
                            .attacks
                            .random_entry(&mut self.rng)
                            .play(
                                SoundPlayProps::new_with_pos_opt(pos)
                                    .with_with_spatial(settings.spatial_sound)
                                    .with_playback_speed(settings.sound_playback_speed)
                                    .with_volume(settings.ingame_sound_volume),
                            )
                            .detatch();
                    }
                    GameBuffNinjaEventSound::Hit => {
                        self.containers
                            .ninja_container
                            .get_or_default_opt(info.map(|i| &i.ninja))
                            .hits
                            .random_entry(&mut self.rng)
                            .play(
                                SoundPlayProps::new_with_pos_opt(pos)
                                    .with_with_spatial(settings.spatial_sound)
                                    .with_playback_speed(settings.sound_playback_speed)
                                    .with_volume(settings.ingame_sound_volume),
                            )
                            .detatch();
                    }
                },
            },
            GameCharacterSoundEvent::Debuff(ev) => match ev {
                GameDebuffSoundEvent::Frozen(ev) => match ev {
                    GameDebuffFrozenEventSound::Attack => {
                        self.containers
                            .freeze_container
                            .get_or_default_opt(info.map(|i| &i.freeze))
                            .attacks
                            .random_entry(&mut self.rng)
                            .play(
                                SoundPlayProps::new_with_pos_opt(pos)
                                    .with_with_spatial(settings.spatial_sound)
                                    .with_playback_speed(settings.sound_playback_speed)
                                    .with_volume(settings.ingame_sound_volume),
                            )
                            .detatch();
                    }
                },
            },
        }
    }
    fn handle_character_effect_event(
        &mut self,
        cur_time: &Duration,
        pos: vec2,
        ev: GameCharacterEffectEvent,
        id: Option<CharacterId>,
    ) {
        match ev {
            GameCharacterEffectEvent::Effect(eff) => match eff {
                GameCharacterEventEffect::Spawn => {
                    Effects::new(&mut self.particles, *cur_time).player_spawn(&pos, id);
                }
                GameCharacterEventEffect::Death => {
                    Effects::new(&mut self.particles, *cur_time).player_death(
                        &pos,
                        ColorRgba::new(1.0, 1.0, 1.0, 1.0),
                        id,
                    );
                }
                GameCharacterEventEffect::AirJump => {
                    Effects::new(&mut self.particles, *cur_time).air_jump(&pos, id);
                }
                GameCharacterEventEffect::DamageIndicator { vel } => {
                    Effects::new(&mut self.particles, *cur_time).damage_ind(&pos, &vel, id);
                }
                GameCharacterEventEffect::HammerHit => {
                    Effects::new(&mut self.particles, *cur_time).hammer_hit(&pos, id);
                }
            },
        }
    }

    fn handle_grenade_sound_event(
        &mut self,
        character_infos: &PoolFxLinkedHashMap<CharacterId, CharacterInfo>,
        settings: &RenderGameSettings,
        pos: Option<vec2>,
        ev: GameGrenadeEventSound,
        id: Option<CharacterId>,
    ) {
        let info = id.and_then(|id| character_infos.get(&id).map(|c| &c.info));
        match ev {
            GameGrenadeEventSound::Spawn => {
                self.containers
                    .weapon_container
                    .get_or_default_opt(info.map(|i| &i.weapon))
                    .grenade
                    .spawn
                    .random_entry(&mut self.rng)
                    .play(
                        SoundPlayProps::new_with_pos_opt(pos)
                            .with_with_spatial(settings.spatial_sound)
                            .with_playback_speed(settings.sound_playback_speed)
                            .with_volume(settings.ingame_sound_volume),
                    )
                    .detatch();
            }
            GameGrenadeEventSound::Collect => {
                self.containers
                    .weapon_container
                    .get_or_default_opt(info.map(|i| &i.weapon))
                    .grenade
                    .collect
                    .random_entry(&mut self.rng)
                    .play(
                        SoundPlayProps::new_with_pos_opt(pos)
                            .with_with_spatial(settings.spatial_sound)
                            .with_playback_speed(settings.sound_playback_speed)
                            .with_volume(settings.ingame_sound_volume),
                    )
                    .detatch();
            }
            GameGrenadeEventSound::Explosion => {
                self.containers
                    .weapon_container
                    .get_or_default_opt(info.map(|i| &i.weapon))
                    .grenade
                    .explosions
                    .random_entry(&mut self.rng)
                    .play(
                        SoundPlayProps::new_with_pos_opt(pos)
                            .with_with_spatial(settings.spatial_sound)
                            .with_playback_speed(settings.sound_playback_speed)
                            .with_volume(settings.ingame_sound_volume),
                    )
                    .detatch();
            }
        }
    }

    fn handle_grenade_effect_event(
        &mut self,
        cur_time: &Duration,
        pos: vec2,
        ev: GameGrenadeEventEffect,
        id: Option<CharacterId>,
    ) {
        match ev {
            GameGrenadeEventEffect::Explosion => {
                Effects::new(&mut self.particles, *cur_time).explosion(&pos, id);
            }
        }
    }

    fn handle_laser_sound_event(
        &mut self,
        character_infos: &PoolFxLinkedHashMap<CharacterId, CharacterInfo>,
        settings: &RenderGameSettings,
        pos: Option<vec2>,
        ev: GameLaserEventSound,
        id: Option<CharacterId>,
    ) {
        let info = id.and_then(|id| character_infos.get(&id).map(|c| &c.info));
        match ev {
            GameLaserEventSound::Spawn => {
                self.containers
                    .weapon_container
                    .get_or_default_opt(info.map(|i| &i.weapon))
                    .laser
                    .spawn
                    .random_entry(&mut self.rng)
                    .play(
                        SoundPlayProps::new_with_pos_opt(pos)
                            .with_with_spatial(settings.spatial_sound)
                            .with_playback_speed(settings.sound_playback_speed)
                            .with_volume(settings.ingame_sound_volume),
                    )
                    .detatch();
            }
            GameLaserEventSound::Collect => {
                self.containers
                    .weapon_container
                    .get_or_default_opt(info.map(|i| &i.weapon))
                    .laser
                    .collect
                    .random_entry(&mut self.rng)
                    .play(
                        SoundPlayProps::new_with_pos_opt(pos)
                            .with_with_spatial(settings.spatial_sound)
                            .with_playback_speed(settings.sound_playback_speed)
                            .with_volume(settings.ingame_sound_volume),
                    )
                    .detatch();
            }
            GameLaserEventSound::Bounce => {
                self.containers
                    .weapon_container
                    .get_or_default_opt(info.map(|i| &i.weapon))
                    .laser
                    .bounces
                    .random_entry(&mut self.rng)
                    .play(
                        SoundPlayProps::new_with_pos_opt(pos)
                            .with_with_spatial(settings.spatial_sound)
                            .with_playback_speed(settings.sound_playback_speed)
                            .with_volume(settings.ingame_sound_volume),
                    )
                    .detatch();
            }
        }
    }

    fn handle_shotgun_sound_event(
        &mut self,
        character_infos: &PoolFxLinkedHashMap<CharacterId, CharacterInfo>,
        settings: &RenderGameSettings,
        pos: Option<vec2>,
        ev: GameShotgunEventSound,
        id: Option<CharacterId>,
    ) {
        let info = id.and_then(|id| character_infos.get(&id).map(|c| &c.info));
        match ev {
            GameShotgunEventSound::Spawn => {
                self.containers
                    .weapon_container
                    .get_or_default_opt(info.map(|i| &i.weapon))
                    .shotgun
                    .spawn
                    .random_entry(&mut self.rng)
                    .play(
                        SoundPlayProps::new_with_pos_opt(pos)
                            .with_with_spatial(settings.spatial_sound)
                            .with_playback_speed(settings.sound_playback_speed)
                            .with_volume(settings.ingame_sound_volume),
                    )
                    .detatch();
            }
            GameShotgunEventSound::Collect => {
                self.containers
                    .weapon_container
                    .get_or_default_opt(info.map(|i| &i.weapon))
                    .shotgun
                    .collect
                    .random_entry(&mut self.rng)
                    .play(
                        SoundPlayProps::new_with_pos_opt(pos)
                            .with_with_spatial(settings.spatial_sound)
                            .with_playback_speed(settings.sound_playback_speed)
                            .with_volume(settings.ingame_sound_volume),
                    )
                    .detatch();
            }
        }
    }

    fn handle_flag_sound_event(
        &mut self,
        character_infos: &PoolFxLinkedHashMap<CharacterId, CharacterInfo>,
        main_listener_character_info: Option<&CharacterInfo>,
        settings: &RenderGameSettings,
        pos: Option<vec2>,
        ev: GameFlagEventSound,
        id: Option<CharacterId>,
    ) {
        let info = id.and_then(|id| character_infos.get(&id).map(|c| &c.info));
        match ev {
            GameFlagEventSound::Capture => {
                self.containers
                    .ctf_container
                    .get_or_default_opt(info.map(|i| &i.ctf))
                    .capture
                    .play(
                        SoundPlayProps::new_with_pos_opt(pos)
                            .with_with_spatial(settings.spatial_sound)
                            .with_playback_speed(settings.sound_playback_speed)
                            .with_volume(settings.ingame_sound_volume),
                    )
                    .detatch();
            }
            GameFlagEventSound::Collect(ty) => {
                let ctf = self
                    .containers
                    .ctf_container
                    .get_or_default_opt(info.map(|i| &i.ctf));
                // Find fitting sound
                let snd = match main_listener_character_info {
                    Some(CharacterInfo {
                        side: Some(side), ..
                    }) => {
                        if (matches!(side, MatchSide::Red) && matches!(ty, FlagType::Blue))
                            || (matches!(side, MatchSide::Blue) && matches!(ty, FlagType::Red))
                        {
                            &ctf.collect_friendly
                        } else {
                            &ctf.collect_opponents
                        }
                    }
                    _ => {
                        // Pure spectators (or unspecified) always get the collect friendly sound
                        &ctf.collect_friendly
                    }
                };
                snd.play(
                    SoundPlayProps::new_with_pos_opt(pos)
                        .with_with_spatial(settings.spatial_sound)
                        .with_playback_speed(settings.sound_playback_speed)
                        .with_volume(settings.ingame_sound_volume),
                )
                .detatch();
            }
            GameFlagEventSound::Drop => {
                self.containers
                    .ctf_container
                    .get_or_default_opt(info.map(|i| &i.ctf))
                    .drop
                    .play(
                        SoundPlayProps::new_with_pos_opt(pos)
                            .with_with_spatial(settings.spatial_sound)
                            .with_playback_speed(settings.sound_playback_speed)
                            .with_volume(settings.ingame_sound_volume),
                    )
                    .detatch();
            }
            GameFlagEventSound::Return => {
                self.containers
                    .ctf_container
                    .get_or_default_opt(info.map(|i| &i.ctf))
                    .return_sound
                    .play(
                        SoundPlayProps::new_with_pos_opt(pos)
                            .with_with_spatial(settings.spatial_sound)
                            .with_playback_speed(settings.sound_playback_speed)
                            .with_volume(settings.ingame_sound_volume),
                    )
                    .detatch();
            }
        }
    }

    fn handle_pickup_sound_event(
        &mut self,
        character_infos: &PoolFxLinkedHashMap<CharacterId, CharacterInfo>,
        settings: &RenderGameSettings,
        pos: Option<vec2>,
        ev: GamePickupSoundEvent,
        id: Option<CharacterId>,
    ) {
        let info = id.and_then(|id| character_infos.get(&id).map(|c| &c.info));
        match ev {
            GamePickupSoundEvent::Heart(ev) => match ev {
                GamePickupHeartEventSound::Spawn => {
                    self.containers
                        .game_container
                        .get_or_default_opt(info.map(|i| &i.game))
                        .heart
                        .spawns
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GamePickupHeartEventSound::Collect => {
                    self.containers
                        .game_container
                        .get_or_default_opt(info.map(|i| &i.game))
                        .heart
                        .collects
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
            },
            GamePickupSoundEvent::Armor(ev) => match ev {
                GamePickupArmorEventSound::Spawn => {
                    self.containers
                        .game_container
                        .get_or_default_opt(info.map(|i| &i.game))
                        .shield
                        .spawns
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
                GamePickupArmorEventSound::Collect => {
                    self.containers
                        .game_container
                        .get_or_default_opt(info.map(|i| &i.game))
                        .shield
                        .collects
                        .random_entry(&mut self.rng)
                        .play(
                            SoundPlayProps::new_with_pos_opt(pos)
                                .with_with_spatial(settings.spatial_sound)
                                .with_playback_speed(settings.sound_playback_speed)
                                .with_volume(settings.ingame_sound_volume),
                        )
                        .detatch();
                }
            },
        }
    }

    fn sound_or_effect_event_precond(
        &self,
        is_prediction: bool,
        event_tick_unknown: bool,
        local_players: &PoolFxLinkedHashMap<PlayerId, RenderGameForPlayer>,
        local_dummies: &PoolFxLinkedHashSet<PlayerId>,

        owner_id: Option<CharacterId>,
        settings: &RenderGameSettings,
        is_char_ev: bool,
    ) -> bool {
        match settings.allow_prediction_events {
            AllowPredictionEventType::None => !is_prediction,
            AllowPredictionEventType::LocalCharacter => {
                (is_prediction
                    && is_char_ev
                    && owner_id.is_some_and(|id| {
                        local_players.contains_key(&id) || local_dummies.contains(&id)
                    }))
                    || (!is_prediction
                        && (event_tick_unknown
                            || !is_char_ev
                            || owner_id.is_none_or(|id| {
                                !local_players.contains_key(&id) && !local_dummies.contains(&id)
                            })))
            }
            AllowPredictionEventType::All => {
                (is_prediction
                    && owner_id.is_some_and(|id| {
                        local_players.contains_key(&id) || local_dummies.contains(&id)
                    }))
                    || (!is_prediction
                        && (event_tick_unknown
                            || owner_id.is_none_or(|id| {
                                !local_players.contains_key(&id) && !local_dummies.contains(&id)
                            })))
            }
        }
    }

    fn handle_sound_event(
        &mut self,
        is_prediction: bool,
        event_tick_unknown: bool,
        character_infos: &PoolFxLinkedHashMap<CharacterId, CharacterInfo>,
        local_players: &PoolFxLinkedHashMap<PlayerId, RenderGameForPlayer>,
        local_dummies: &PoolFxLinkedHashSet<PlayerId>,
        settings: &RenderGameSettings,
        GameWorldSoundEvent { owner_id, ev, pos }: GameWorldSoundEvent,
    ) {
        if !self.sound_or_effect_event_precond(
            is_prediction,
            event_tick_unknown,
            local_players,
            local_dummies,
            owner_id,
            settings,
            matches!(ev, GameWorldEntitySoundEvent::Character(_)),
        ) {
            return;
        }
        match ev {
            GameWorldEntitySoundEvent::Character(ev) => {
                self.handle_character_sound_event(character_infos, settings, pos, ev, owner_id);
            }
            GameWorldEntitySoundEvent::Grenade(ev) => {
                self.handle_grenade_sound_event(character_infos, settings, pos, ev, owner_id);
            }
            GameWorldEntitySoundEvent::Laser(ev) => {
                self.handle_laser_sound_event(character_infos, settings, pos, ev, owner_id);
            }
            GameWorldEntitySoundEvent::Shotgun(ev) => {
                self.handle_shotgun_sound_event(character_infos, settings, pos, ev, owner_id);
            }
            GameWorldEntitySoundEvent::Flag(ev) => {
                let main_listener_character_info = match local_players.front() {
                    Some((player_id, p)) => {
                        let char_id = match &p.render_for_player.cam_mode {
                            RenderPlayerCameraMode::Default => Some(player_id),
                            RenderPlayerCameraMode::AtPos { .. } => None,
                            RenderPlayerCameraMode::OnCharacters { character_ids, .. } => {
                                character_ids.iter().next()
                            }
                        };
                        char_id.and_then(|char_id| character_infos.get(char_id))
                    }
                    None => None,
                };
                self.handle_flag_sound_event(
                    character_infos,
                    main_listener_character_info,
                    settings,
                    pos,
                    ev,
                    owner_id,
                );
            }
            GameWorldEntitySoundEvent::Pickup(ev) => {
                self.handle_pickup_sound_event(character_infos, settings, pos, ev, owner_id);
            }
        }
    }

    fn handle_effect_event(
        &mut self,
        is_prediction: bool,
        event_tick_unknown: bool,
        cur_time: &Duration,
        local_players: &PoolFxLinkedHashMap<PlayerId, RenderGameForPlayer>,
        local_dummies: &PoolFxLinkedHashSet<PlayerId>,
        settings: &RenderGameSettings,
        GameWorldEffectEvent { owner_id, ev, pos }: GameWorldEffectEvent,
    ) {
        if !self.sound_or_effect_event_precond(
            is_prediction,
            event_tick_unknown,
            local_players,
            local_dummies,
            owner_id,
            settings,
            matches!(ev, GameWorldEntityEffectEvent::Character(_)),
        ) {
            return;
        }
        match ev {
            GameWorldEntityEffectEvent::Character(ev) => {
                self.handle_character_effect_event(cur_time, pos, ev, owner_id);
            }
            GameWorldEntityEffectEvent::Grenade(ev) => {
                self.handle_grenade_effect_event(cur_time, pos, ev, owner_id);
            }
        }
    }

    fn handle_events(&mut self, cur_time: &Duration, input: &mut RenderGameInput) {
        // handle events
        for ((monotonic_tick, by_prediction), events) in input.events.iter_mut() {
            let event_tick_unknown = self
                .last_event_monotonic_tick
                .is_none_or(|tick| tick < *monotonic_tick);
            for (stage_id, mut world) in events.worlds.drain() {
                if !input.stages.contains_key(&stage_id) {
                    continue;
                }
                for (_, ev) in world.events.drain() {
                    match ev {
                        GameWorldEvent::Sound(ev) => self.handle_sound_event(
                            *by_prediction,
                            event_tick_unknown,
                            &input.character_infos,
                            &input.players,
                            &input.dummies,
                            &input.settings,
                            ev,
                        ),
                        GameWorldEvent::Effect(ev) => self.handle_effect_event(
                            *by_prediction,
                            event_tick_unknown,
                            cur_time,
                            &input.players,
                            &input.dummies,
                            &input.settings,
                            ev,
                        ),
                        GameWorldEvent::Notification(ev) => {
                            // don't rely on prediction for global events.
                            if !*by_prediction {
                                match ev {
                                    GameWorldNotificationEvent::System(ev) => {
                                        let msg = Self::convert_system_ev(&ev);
                                        let (front_skin, end_skin) = match ev {
                                            GameWorldSystemMessage::PlayerJoined {
                                                skin,
                                                skin_info,
                                                ..
                                            }
                                            | GameWorldSystemMessage::PlayerLeft {
                                                skin,
                                                skin_info,
                                                ..
                                            } => (
                                                Some(SystemMsgPlayerSkin {
                                                    skin_name: (*skin).clone().into(),
                                                    skin_info,
                                                }),
                                                None,
                                            ),
                                            GameWorldSystemMessage::CharacterInfoChanged {
                                                old_skin,
                                                old_skin_info,
                                                new_skin,
                                                new_skin_info,
                                                ..
                                            } => (
                                                Some(SystemMsgPlayerSkin {
                                                    skin_name: (*old_skin).clone().into(),
                                                    skin_info: old_skin_info,
                                                }),
                                                (*old_skin != *new_skin
                                                    || old_skin_info != new_skin_info)
                                                    .then(|| SystemMsgPlayerSkin {
                                                        skin_name: (*new_skin).clone().into(),
                                                        skin_info: new_skin_info,
                                                    }),
                                            ),
                                            GameWorldSystemMessage::Custom(_) => (None, None),
                                        };
                                        self.chat.msgs.push_front(MsgInChat {
                                            msg: ServerMsg::System(MsgSystem {
                                                msg,
                                                front_skin,
                                                end_skin,
                                            }),
                                            add_time: *cur_time,
                                        })
                                    }
                                    GameWorldNotificationEvent::Action(ev) => {
                                        self.handle_action_feed(
                                            cur_time,
                                            &input.character_infos,
                                            ev,
                                        );
                                    }
                                    GameWorldNotificationEvent::Motd { msg } => {
                                        self.motd.msg = msg.to_string();
                                        self.motd.started_at = Some(*cur_time);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            self.last_event_monotonic_tick = self
                .last_event_monotonic_tick
                .map(|tick| tick.max(*monotonic_tick))
                .or(Some(*monotonic_tick));
        }
        input.events.clear();
    }

    fn from_net_msg(
        character_infos: &PoolFxLinkedHashMap<CharacterId, CharacterInfo>,
        msg: NetChatMsg,
    ) -> ChatMsg {
        let sender_extra = character_infos.get(&msg.sender.id);
        ChatMsg {
            player: msg.sender.name.into(),
            clan: sender_extra
                .map(|s| s.info.clan.to_string())
                .unwrap_or_default(),
            skin_name: msg.sender.skin.into(),
            skin_info: msg.sender.skin_info,
            msg: msg.msg,
            channel: msg.channel,
        }
    }

    fn handle_chat_msgs(&mut self, cur_time: &Duration, game: &mut RenderGameInput) {
        let it = game.chat_msgs.drain(..).map(|msg| MsgInChat {
            msg: ServerMsg::Chat(Self::from_net_msg(&game.character_infos, msg)),
            add_time: *cur_time,
        });
        for msg in it {
            // push_front is intentionally used over extend or similar, so msgs are
            // only mutable accessed if a new msg is actually added
            self.chat.msgs.push_front(msg);
        }
    }

    fn calc_players_per_row(player_count: usize) -> usize {
        (player_count as f64).sqrt().ceil() as usize
    }

    fn player_render_area(
        index: usize,
        width: u32,
        height: u32,
        players_per_row: usize,
        player_count: usize,
    ) -> (i32, i32, u32, u32) {
        let x = index % players_per_row;
        let y = index / players_per_row;
        let w_splitted = width as usize / players_per_row;
        let mut h_splitted = height as usize / players_per_row;

        if player_count <= (players_per_row * players_per_row) - players_per_row {
            h_splitted = height as usize / (players_per_row - 1);
        }

        let (x, y, w, h) = (
            (x * w_splitted) as i32,
            (y * h_splitted) as i32,
            w_splitted as u32,
            h_splitted as u32,
        );

        (x, y, w.max(1), h.max(1))
    }

    fn render_observers(
        &mut self,
        observed_players: &mut Vec<ObservedPlayer>,
        anchored_size: &ObservedAnchoredSize,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        config_map: &ConfigMap,
        cur_time: &Duration,
        input: &mut RenderGameInput,
        player_vote_rect: Option<Rect>,
    ) {
        let (top_left, top_right, bottom_left, bottom_right) = {
            let mut top_left = 0;
            let mut top_right = 0;
            let mut bottom_left = 0;
            let mut bottom_right = 0;
            observed_players.iter().for_each(|d| {
                if let ObservedPlayer::Dummy { anchor, .. } = d {
                    match anchor {
                        ObservedDummyAnchor::TopLeft => {
                            top_left += 1;
                        }
                        ObservedDummyAnchor::TopRight => {
                            top_right += 1;
                        }
                        ObservedDummyAnchor::BottomLeft => {
                            bottom_left += 1;
                        }
                        ObservedDummyAnchor::BottomRight => {
                            bottom_right += 1;
                        }
                    }
                }
            });
            (top_left, top_right, bottom_left, bottom_right)
        };
        for (index, observed_player) in observed_players.drain(..).enumerate() {
            match observed_player {
                ObservedPlayer::Dummy {
                    player_id,
                    local_player_info,
                    anchor,
                } => {
                    let player_count = match anchor {
                        ObservedDummyAnchor::TopLeft => top_left,
                        ObservedDummyAnchor::TopRight => top_right,
                        ObservedDummyAnchor::BottomLeft => bottom_left,
                        ObservedDummyAnchor::BottomRight => bottom_right,
                    };
                    let players_per_row = Self::calc_players_per_row(player_count);
                    let (px, py, pw, ph) = Self::player_render_area(
                        index,
                        ((w / 2) * anchored_size.width.get()) / 100,
                        ((h / 2) * anchored_size.height.get()) / 100,
                        players_per_row,
                        player_count,
                    );
                    let (off_x, off_y) = match anchor {
                        ObservedDummyAnchor::TopLeft => (px, py),
                        ObservedDummyAnchor::TopRight => (w as i32 - (pw as i32 + px), py),
                        ObservedDummyAnchor::BottomLeft => (px, h as i32 - (ph as i32 + py)),
                        ObservedDummyAnchor::BottomRight => {
                            (w as i32 - (pw as i32 + px), h as i32 - (ph as i32 + py))
                        }
                    };
                    self.canvas_handle
                        .update_window_viewport(x + off_x, y + off_y, pw, ph);
                    self.render_ingame(
                        config_map,
                        cur_time,
                        input,
                        Some((
                            &player_id,
                            &RenderForPlayer {
                                chat_info: None,
                                emote_wheel_input: None,
                                spectator_selection_input: None,
                                local_player_info,
                                chat_show_all: false,
                                scoreboard_active: false,

                                zoom: 1.0,
                                cam_mode: RenderPlayerCameraMode::Default,
                            },
                        )),
                    );
                }
                ObservedPlayer::Vote { player_id } => {
                    if let Some((_, player_vote_rect)) = input
                        .character_infos
                        .get(&player_id)
                        .and_then(|c| {
                            c.stage_id.and_then(|stage_id| {
                                input
                                    .stages
                                    .get(&stage_id)
                                    .and_then(|stage| stage.world.characters.get(&player_id))
                            })
                        })
                        .zip(player_vote_rect)
                    {
                        let off_x = player_vote_rect.min.x.round() as i32;
                        let off_y = player_vote_rect.min.y.round() as i32;
                        let pw = player_vote_rect.width().round() as u32;
                        let ph = player_vote_rect.height().round() as u32;
                        self.canvas_handle
                            .update_window_viewport(x + off_x, y + off_y, pw, ph);

                        let nameplates = input.settings.nameplates;
                        let nameplate_own = input.settings.nameplate_own;
                        input.settings.nameplates = true;
                        input.settings.nameplate_own = true;
                        self.render_ingame(
                            config_map,
                            cur_time,
                            input,
                            Some((
                                &player_id,
                                &RenderForPlayer {
                                    chat_info: None,
                                    emote_wheel_input: None,
                                    spectator_selection_input: None,
                                    local_player_info: LocalCharacterRenderInfo::Unavailable,
                                    chat_show_all: false,
                                    scoreboard_active: false,

                                    zoom: 0.5,
                                    cam_mode: RenderPlayerCameraMode::Default,
                                },
                            )),
                        );
                        input.settings.nameplates = nameplates;
                        input.settings.nameplate_own = nameplate_own;
                    }
                }
            }
        }
    }

    fn check_required_containers_loaded(&mut self) -> bool {
        let loaded = self.client_local_infos.iter().all(|i| {
            self.containers.skin_container.is_loaded_or_failed(&i.skin)
                && self
                    .containers
                    .particles_container
                    .is_loaded_or_failed(&i.particles)
        });
        // since we don't need the property anymore after they are loaded
        // clear it to save memory & performance.
        if loaded {
            self.client_local_infos = Default::default();
        }
        loaded
    }

    fn update_containers_impl<'a, F>(
        containers: &mut RenderGameContainers,
        map_vote_thumbnails_container: &mut ThumbnailContainer,
        cur_time: &Duration,
        character_infos: F,
    ) where
        F: Iterator<Item = &'a NetworkCharacterInfo> + Clone,
    {
        containers.skin_container.update(
            cur_time,
            &Duration::from_secs(5),
            &Duration::from_secs(1),
            character_infos.clone().map(|info| info.skin.borrow()),
            None,
        );
        containers.weapon_container.update(
            cur_time,
            &Duration::from_secs(5),
            &Duration::from_secs(1),
            character_infos.clone().map(|info| info.weapon.borrow()),
            None,
        );
        containers.hook_container.update(
            cur_time,
            &Duration::from_secs(5),
            &Duration::from_secs(1),
            character_infos.clone().map(|info| info.hook.borrow()),
            None,
        );
        containers.ctf_container.update(
            cur_time,
            &Duration::from_secs(5),
            &Duration::from_secs(1),
            character_infos.clone().map(|info| info.ctf.borrow()),
            None,
        );
        containers.ninja_container.update(
            cur_time,
            &Duration::from_secs(5),
            &Duration::from_secs(1),
            character_infos.clone().map(|info| info.ninja.borrow()),
            None,
        );
        containers.freeze_container.update(
            cur_time,
            &Duration::from_secs(5),
            &Duration::from_secs(1),
            character_infos.clone().map(|info| info.freeze.borrow()),
            None,
        );
        containers.entities_container.update(
            cur_time,
            &Duration::from_secs(5),
            &Duration::from_secs(1),
            character_infos.clone().map(|info| info.entities.borrow()),
            None,
        );
        containers.hud_container.update(
            cur_time,
            &Duration::from_secs(5),
            &Duration::from_secs(1),
            character_infos.clone().map(|info| info.hud.borrow()),
            None,
        );
        containers.emoticons_container.update(
            cur_time,
            &Duration::from_secs(5),
            &Duration::from_secs(1),
            character_infos.clone().map(|info| info.emoticons.borrow()),
            None,
        );
        containers.particles_container.update(
            cur_time,
            &Duration::from_secs(5),
            &Duration::from_secs(1),
            character_infos.clone().map(|info| info.particles.borrow()),
            None,
        );
        containers.game_container.update(
            cur_time,
            &Duration::from_secs(5),
            &Duration::from_secs(1),
            character_infos.map(|info| info.game.borrow()),
            None,
        );
        map_vote_thumbnails_container.update(
            cur_time,
            &Duration::from_secs(5),
            &Duration::from_secs(1),
            [].into_iter(),
            None,
        );
    }

    fn update_containers(
        &mut self,
        cur_time: &Duration,
        character_infos: &PoolFxLinkedHashMap<CharacterId, CharacterInfo>,
    ) {
        Self::update_containers_impl(
            &mut self.containers,
            &mut self.map_vote_thumbnails_container,
            cur_time,
            character_infos.values().map(|i| &***i.info),
        );
    }
}

impl RenderGameInterface for RenderGame {
    fn render(
        &mut self,
        config_map: &ConfigMap,
        cur_time: &Duration,
        mut input: RenderGameInput,
    ) -> RenderGameResult {
        // as a first step, update all containers
        self.update_containers(cur_time, &input.character_infos);

        // keep scene active
        self.world_sound_scene.stay_active();

        // set the ui zoom
        self.hud
            .ui
            .ui
            .zoom_level
            .set(Some(input.settings.pixels_per_point.clamp(0.1, 5.0)));

        let mut res = RenderGameResult::default();
        let map = self.map.try_get().unwrap();
        self.particles.update(cur_time, &map.data.collision);

        self.handle_chat_msgs(cur_time, &mut input);
        self.handle_events(cur_time, &mut input);

        let mut has_scoreboard = false;
        let mut has_chat_input = false;

        let mut next_sound_listeners = self.world_sound_listeners_pool.new();
        std::mem::swap(&mut *next_sound_listeners, &mut self.world_sound_listeners);
        for (player_id, player) in input.players.iter() {
            let cam_pos = match &player.render_for_player.cam_mode {
                RenderPlayerCameraMode::Default => input
                    .character_infos
                    .get(player_id)
                    .and_then(|c| {
                        c.stage_id
                            .and_then(|id| input.stages.get(&id))
                            .and_then(|s| s.world.characters.get(player_id))
                    })
                    .map(|c| c.lerped_pos),
                RenderPlayerCameraMode::AtPos { pos, .. } => Some(*pos),
                RenderPlayerCameraMode::OnCharacters {
                    character_ids,
                    fallback_pos,
                } => Some(
                    character_ids
                        .iter()
                        .next()
                        .and_then(|character_id| {
                            input
                                .character_infos
                                .get(character_id)
                                .and_then(|c| {
                                    c.stage_id
                                        .and_then(|id| input.stages.get(&id))
                                        .and_then(|s| s.world.characters.get(character_id))
                                })
                                .map(|c| c.lerped_pos)
                        })
                        .unwrap_or(*fallback_pos),
                ),
            };
            if let Some(cam_pos) = cam_pos {
                if let Some(listener) = next_sound_listeners.remove(player_id) {
                    self.world_sound_listeners
                        .entry(*player_id)
                        .or_insert(listener)
                } else {
                    self.world_sound_listeners
                        .entry(*player_id)
                        .or_insert(self.world_sound_scene.sound_listener_handle.create(cam_pos))
                }
                .update(cam_pos);
            }

            has_scoreboard |= player.render_for_player.scoreboard_active;
            has_chat_input |= player.render_for_player.chat_info.is_some();
        }

        // always clear motd if scoreboard is open
        if has_scoreboard {
            self.motd.msg = "".into();
            self.motd.started_at = None;
        }

        // only call this when chat input is active, since it will allocate memory on heap
        let local_player_ids = has_chat_input.then(|| {
            input
                .players
                .keys()
                .copied()
                .chain(input.dummies.iter().copied())
                .collect()
        });

        let player_count = input.players.len();
        if player_count == 0 {
            self.render_ingame(config_map, cur_time, &input, None);
            self.backend_handle.consumble_multi_samples();
            let _ = self.render_uis(cur_time, &input, None, &local_player_ids, &mut None, false);
        } else {
            let players_per_row = Self::calc_players_per_row(player_count);
            let window_props = self.canvas_handle.window_props();

            let mut helper = self.helper.new();
            let has_viewport_updates = if player_count == 1 {
                let (player_id, render_for_player_game) = input.players.drain().next().unwrap();
                let has_observed_players = !render_for_player_game.observed_players.is_empty();
                helper.push((
                    0,
                    0,
                    window_props.canvas_width,
                    window_props.canvas_height,
                    (player_id, render_for_player_game),
                ));
                has_observed_players
            } else {
                helper.extend(input.players.drain().enumerate().map(
                    |(index, (player_id, render_for_player_game))| {
                        let (x, y, w, h) = Self::player_render_area(
                            index,
                            window_props.canvas_width,
                            window_props.canvas_height,
                            players_per_row,
                            player_count,
                        );
                        (x, y, w, h, (player_id, render_for_player_game))
                    },
                ));
                true
            };

            for (x, y, w, h, (player_id, render_for_player_game)) in helper.iter() {
                if has_viewport_updates {
                    self.canvas_handle.update_window_viewport(*x, *y, *w, *h);
                }
                self.render_ingame(
                    config_map,
                    cur_time,
                    &input,
                    Some((player_id, &render_for_player_game.render_for_player)),
                );
            }
            self.backend_handle.consumble_multi_samples();
            for (x, y, w, h, (player_id, render_for_player_game)) in helper.iter_mut() {
                if has_viewport_updates {
                    self.canvas_handle.update_window_viewport(*x, *y, *w, *h);
                }
                let expected_vote_miniscreen =
                    render_for_player_game.observed_players.iter().any(|p| {
                        if let ObservedPlayer::Vote { player_id } = p {
                            input
                                .character_infos
                                .get(player_id)
                                .and_then(|c| c.stage_id)
                                .and_then(|stage_id| {
                                    input
                                        .stages
                                        .get(&stage_id)
                                        .map(|stage| stage.world.characters.contains_key(player_id))
                                })
                                .unwrap_or_default()
                        } else {
                            false
                        }
                    });
                let mut player_vote_rect = None;
                let res_render = self.render_uis(
                    cur_time,
                    &input,
                    Some((player_id, render_for_player_game)),
                    &local_player_ids,
                    &mut player_vote_rect,
                    expected_vote_miniscreen,
                );
                res.player_events.insert(*player_id, res_render);

                // render observers
                self.render_observers(
                    &mut render_for_player_game.observed_players,
                    &render_for_player_game.observed_anchored_size_props,
                    *x,
                    *y,
                    *w,
                    *h,
                    config_map,
                    cur_time,
                    &mut input,
                    player_vote_rect,
                );
            }
            if has_viewport_updates {
                self.canvas_handle.reset_window_viewport();
            }
        }
        self.particles.update_rates();

        res
    }

    fn continue_loading(&mut self) -> Result<bool, String> {
        let containers_loaded = self.check_required_containers_loaded();
        self.map
            .continue_loading()
            .map(|m| m.is_some() && containers_loaded)
            .map_err(|err| err.to_string())
    }

    fn set_chat_commands(&mut self, chat_commands: ChatCommands) {
        self.chat_commands = chat_commands
    }

    fn clear_render_state(&mut self) {
        self.particles.reset();
        self.world_sound_scene.stop_detatched_sounds();
        self.last_event_monotonic_tick = None;
        self.chat.msgs.clear();
        self.actionfeed.msgs.clear();
    }

    fn render_offair_sound(&mut self, samples: u32) {
        self.world_sound_scene.process_off_air(samples);
    }
}
