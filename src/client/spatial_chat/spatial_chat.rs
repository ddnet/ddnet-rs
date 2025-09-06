use std::{
    collections::{BTreeMap, HashMap},
    ops::ControlFlow,
};

use base::hash::fmt_hash;
use client_ui::main_menu::spatial_chat::{
    self, EntitiesEvent, MicrophoneDevices, MicrophoneHosts, SpatialChatEntity,
};
use crossbeam::channel::{Sender, TrySendError, bounded};
use game_config::config::{ConfigGame, ConfigSpatialChatPerPlayerOptions};
use game_interface::types::{
    id_types::{CharacterId, PlayerId},
    player_info::PlayerUniqueId,
    render::character::CharacterInfo,
};
use game_network::messages::{ClientToServerMessage, MsgSvSpatialChatOfEntitity};
use math::math::vector::vec2;
use microphone::{
    AnalyzeStream, MicrophoneManager, SoundStream,
    stream::MicrophoneStream,
    stream_sample::StreamSample,
    traits::Microphone,
    types::{MicrophoneNoiseFilterSettings, SoundStreamsettings},
};
use network::network::quinn_network::QuinnNetwork;
use pool::datatypes::PoolFxLinkedHashMap;
use sound::{
    scene_object::SceneObject, sound_listener::SoundListener, stream_object::StreamObject,
    types::StreamPlayProps,
};

/// Keep alive RAII objects
pub struct StreamEntity {
    obj: StreamObject,
    // stream last
    _stream: SoundStream,
}

pub struct PlayerEntity {
    last_id: Option<u64>,
    sender: Sender<StreamSample>,
    // stream last
    ent: StreamEntity,

    cur_settings: ConfigSpatialChatPerPlayerOptions,
}

/// Keep alive RAII objects
pub struct SettingsStream {
    _listener: SoundListener,

    db_analze_stream: AnalyzeStream,
    // stream last
    _obj: StreamEntity,
}

#[derive(Debug, Default)]
pub struct PendingEntity {
    pub opus_frames: BTreeMap<u64, Vec<Vec<u8>>>,
    pub settings: ConfigSpatialChatPerPlayerOptions,
}

/// Keep alive RAII objects
pub struct SpatialChatGameWorld {
    listener: SoundListener,

    sender: MicrophoneStream,
    /// monotonic increasing value to drop
    /// old packets.
    sender_id: u64,

    pending_entities: HashMap<PlayerId, PendingEntity>,
    entities_positions: HashMap<PlayerId, vec2>,

    // entities last
    entities: HashMap<PlayerId, PlayerEntity>,
}

pub enum SpatialChatGameWorldTy {
    None,
    ClientSideDeactivated,
    World(SpatialChatGameWorld),
}

pub enum SpatialChatGameWorldTyRef<'a, T: 'a> {
    None,
    ClientSideDeactivated(&'a mut SpatialChatGameWorldTy),
    World((&'a mut SpatialChatGameWorldTy, T)),
}

impl SpatialChatGameWorldTy {
    pub fn as_mut(&mut self) -> Option<&mut SpatialChatGameWorld> {
        match self {
            SpatialChatGameWorldTy::None | SpatialChatGameWorldTy::ClientSideDeactivated => None,
            SpatialChatGameWorldTy::World(w) => Some(w),
        }
    }

    pub fn zip_mut<'a, T: 'a>(&'a mut self, f: Option<T>) -> SpatialChatGameWorldTyRef<'a, T> {
        match f {
            Some(f) => match self {
                SpatialChatGameWorldTy::None => SpatialChatGameWorldTyRef::None,
                SpatialChatGameWorldTy::ClientSideDeactivated => {
                    SpatialChatGameWorldTyRef::ClientSideDeactivated(self)
                }
                SpatialChatGameWorldTy::World(_) => SpatialChatGameWorldTyRef::World((self, f)),
            },
            None => SpatialChatGameWorldTyRef::None,
        }
    }
}

pub struct SpatialChat {
    pub spatial_chat: spatial_chat::SpatialChat,
    microphone: MicrophoneManager,

    settings_stream: Option<SettingsStream>,
}

impl SpatialChat {
    pub fn new(spatial_chat: spatial_chat::SpatialChat) -> Self {
        Self {
            spatial_chat,
            microphone: Default::default(),

            settings_stream: None,
        }
    }

    fn ui_settings_to_micro_settings(&self, config: &ConfigGame) -> MicrophoneNoiseFilterSettings {
        let settings = &config.cl.spatial_chat.filter;

        MicrophoneNoiseFilterSettings {
            nf: settings
                .use_nf
                .then_some(microphone::types::NoiseFilterSettings {
                    attenuation: settings.nf.attenuation,
                    processing_threshold: settings.nf.processing_threshold,
                }),
            noise_gate: microphone::types::NoiseGateSettings {
                open_threshold: settings.noise_gate.open_threshold,
                close_threshold: settings.noise_gate.close_threshold,
            },
            boost: settings.boost,
        }
    }

    fn player_settings_to_stream_settings(
        &self,
        settings: &ConfigSpatialChatPerPlayerOptions,
    ) -> SoundStreamsettings {
        SoundStreamsettings {
            nf: settings
                .force_nf
                .then_some(microphone::types::NoiseFilterSettings {
                    attenuation: settings.nf.attenuation,
                    processing_threshold: settings.nf.processing_threshold,
                }),
            noise_gate: settings
                .force_gate
                .then_some(microphone::types::NoiseGateSettings {
                    open_threshold: settings.noise_gate.open_threshold,
                    close_threshold: settings.noise_gate.close_threshold,
                }),
            boost: settings.boost,
        }
    }

    fn settings(&self, config: &ConfigGame) -> (String, String, MicrophoneNoiseFilterSettings) {
        let hosts = self.microphone.hosts();
        let host = if config.cl.spatial_chat.host.is_empty() {
            hosts.default.clone()
        } else if hosts.hosts.contains(&config.cl.spatial_chat.host) {
            config.cl.spatial_chat.host.clone()
        } else {
            hosts.default.clone()
        };
        let devices = self.microphone.devices(&host).ok();
        let device = if config.cl.spatial_chat.device.is_empty() {
            devices
                .and_then(|devices| devices.default.clone())
                .unwrap_or_default()
        } else {
            devices
                .map(|devices| {
                    if devices.devices.contains(&config.cl.spatial_chat.device) {
                        config.cl.spatial_chat.device.clone()
                    } else {
                        devices.default.clone().unwrap_or_default()
                    }
                })
                .unwrap_or_default()
        };
        (host, device, self.ui_settings_to_micro_settings(config))
    }

    fn new_settings_stream(
        &self,
        host: &str,
        device: &str,
        settings: MicrophoneNoiseFilterSettings,
        scene: &SceneObject,
    ) -> Option<SettingsStream> {
        let stream = self.microphone.stream_opus(host, device, settings);
        let analyze_stream = self.microphone.stream_opus(
            host,
            device,
            MicrophoneNoiseFilterSettings {
                nf: settings.nf,
                noise_gate: microphone::types::NoiseGateSettings {
                    open_threshold: -200.0,
                    close_threshold: -200.0,
                },
                boost: settings.boost,
            },
        );

        stream
            .map(|s| SoundStream::new(s, Default::default()))
            .ok()
            .zip(analyze_stream.map(AnalyzeStream::new).ok())
            .map(|(stream, db_analze_stream)| {
                let stream_handler = stream.stream();
                let scene_stream = scene.stream_object_handle.create(
                    stream_handler,
                    StreamPlayProps::with_pos(Default::default()),
                );
                let listener = scene.sound_listener_handle.create(Default::default());

                SettingsStream {
                    _obj: StreamEntity {
                        obj: scene_stream,
                        _stream: stream,
                    },
                    _listener: listener,

                    db_analze_stream,
                }
            })
    }

    pub fn create_world(&self, scene: &SceneObject, config: &ConfigGame) -> SpatialChatGameWorldTy {
        if !config.cl.spatial_chat.activated {
            return SpatialChatGameWorldTy::ClientSideDeactivated;
        }

        let (host, device, settings) = self.settings(config);
        let stream = self.microphone.stream_opus(&host, &device, settings);

        stream
            .ok()
            .map(|stream| {
                let listener = scene.sound_listener_handle.create(Default::default());

                SpatialChatGameWorldTy::World(SpatialChatGameWorld {
                    listener,
                    sender: stream,
                    sender_id: 0,
                    entities: Default::default(),
                    pending_entities: Default::default(),
                    entities_positions: Default::default(),
                })
            })
            .unwrap_or(SpatialChatGameWorldTy::ClientSideDeactivated)
    }

    pub fn update(
        &mut self,
        scene: &SceneObject,
        game_local_player_and_network: SpatialChatGameWorldTyRef<'_, (PlayerId, &QuinnNetwork)>,
        config: &ConfigGame,
    ) {
        let settings_changed = self.spatial_chat.has_changed();
        if self.spatial_chat.is_active() {
            scene.stay_active();

            if self.spatial_chat.should_fill_hosts() {
                let mut hosts: MicrophoneHosts = Default::default();
                let backend_hosts = self.microphone.hosts();

                hosts.default = backend_hosts.default;
                hosts.hosts = backend_hosts
                    .hosts
                    .into_iter()
                    .map(|host| {
                        let name = host;

                        let devices = self.microphone.devices(&name).unwrap_or_default();
                        (
                            name,
                            MicrophoneDevices {
                                default: devices.default,
                                devices: devices.devices,
                            },
                        )
                    })
                    .collect();

                let (host, device, settings) = self.settings(config);
                let stream = self.new_settings_stream(&host, &device, settings, scene);

                self.spatial_chat.fill_hosts(hosts);

                let stream = stream;

                self.settings_stream = stream;
            } else if settings_changed || self.settings_stream.is_none() {
                let (host, device, settings) = self.settings(config);
                self.settings_stream = self.new_settings_stream(&host, &device, settings, scene);
            }

            if let Some(stream) = &self.settings_stream {
                // check noise level of stream
                self.spatial_chat
                    .set_loudest(*stream.db_analze_stream.cur_loudest.read().unwrap());
            }
        } else {
            self.settings_stream = None;
        }

        match game_local_player_and_network {
            SpatialChatGameWorldTyRef::World((world, (local_player, network))) => {
                if !config.cl.spatial_chat.activated {
                    *world = SpatialChatGameWorldTy::ClientSideDeactivated;

                    network.send_unordered_auto_to_server(
                        &ClientToServerMessage::SpatialChatDeactivated,
                    );
                } else if let SpatialChatGameWorldTy::World(game) = world {
                    // reinitialize the mic
                    if settings_changed {
                        let (host, device, settings) = self.settings(config);
                        if let Ok(stream) = self.microphone.stream_opus(&host, &device, settings) {
                            game.sender = stream;
                        }
                    }

                    scene.stay_active();
                    let receiver = &game.sender.opus_receiver;
                    // around 100ms
                    const MAX_PACKETS: usize = 10;
                    let mut count = 0;
                    let mut packets = Vec::new();
                    while count < MAX_PACKETS && !receiver.is_empty() {
                        if let Ok(packet) = receiver.try_recv() {
                            packets.push(packet.data);
                        }

                        count += 1;
                    }

                    if !packets.is_empty() {
                        network.send_unordered_auto_to_server(
                            &ClientToServerMessage::SpatialChat {
                                opus_frames: packets,
                                id: game.sender_id,
                            },
                        );
                        game.sender_id += 1;
                    }

                    // update listener
                    if let Some(pos) = game.entities_positions.get(&local_player) {
                        game.listener.update(*pos);
                    }

                    // update current world
                    for (entity_id, mut entity) in game.pending_entities.drain() {
                        if game
                            .entities
                            .get(&entity_id)
                            .is_some_and(|p| p.cur_settings != entity.settings)
                        {
                            game.entities.remove(&entity_id);
                        }

                        let player = game.entities.entry(entity_id).or_insert_with(|| {
                            let (sender, receiver) = bounded(4096);
                            let stream = SoundStream::from_receiver(
                                receiver,
                                None,
                                self.player_settings_to_stream_settings(&entity.settings),
                            );
                            let stream_handler = stream.stream();
                            let scene_stream = scene.stream_object_handle.create(
                                stream_handler,
                                StreamPlayProps::with_pos(Default::default())
                                    .with_with_spatial(config.cl.spatial_chat.spatial),
                            );
                            PlayerEntity {
                                ent: StreamEntity {
                                    obj: scene_stream,
                                    _stream: stream,
                                },
                                sender,
                                last_id: None,
                                cur_settings: entity.settings,
                            }
                        });

                        if let Some(pos) = game.entities_positions.get(&entity_id) {
                            player.ent.obj.update(StreamPlayProps::with_pos(*pos).base);
                        }

                        while entity
                            .opus_frames
                            .first_key_value()
                            .is_some_and(|(key, _)| {
                                player.last_id.is_some_and(|last_id| last_id >= *key)
                            })
                        {
                            entity.opus_frames.pop_first();
                        }

                        if !entity.opus_frames.is_empty() {
                            player.last_id =
                                entity.opus_frames.last_key_value().map(|(key, _)| *key);
                            if let ControlFlow::Break(_) = entity
                                .opus_frames
                                .into_values()
                                .flat_map(|f| f.into_iter().map(|f| StreamSample { data: f }))
                                .try_for_each(|stream| {
                                    if matches!(
                                        player.sender.try_send(stream),
                                        Err(TrySendError::Disconnected(_))
                                    ) {
                                        ControlFlow::Break(())
                                    } else {
                                        ControlFlow::Continue(())
                                    }
                                })
                            {
                                game.entities.remove(&entity_id);
                            }
                        }
                    }

                    // handle events
                    let events = self.spatial_chat.take_entities_events();
                    for event in events {
                        match event {
                            EntitiesEvent::Mute(id) => {
                                game.entities.remove(&id);
                            }
                            EntitiesEvent::Unmute(_) => {
                                // nothing to do
                            }
                        }
                    }
                }
            }
            SpatialChatGameWorldTyRef::ClientSideDeactivated(game) => {
                if config.cl.spatial_chat.activated {
                    *game = self.create_world(scene, config);
                }
            }
            SpatialChatGameWorldTyRef::None => {
                // nothing to do
            }
        }
    }

    pub fn on_input(
        &self,
        world: Option<(
            &mut SpatialChatGameWorld,
            PoolFxLinkedHashMap<CharacterId, CharacterInfo>,
        )>,
        entities: HashMap<PlayerId, MsgSvSpatialChatOfEntitity>,
        config: &ConfigGame,
    ) {
        if let Some((world, world_entities)) = world {
            // drop all entities that are not part of the packet
            world.entities.retain(|id, _| entities.contains_key(id));

            self.spatial_chat.update_entities(
                entities
                    .iter()
                    .map(|(id, p)| {
                        (
                            *id,
                            SpatialChatEntity {
                                name: world_entities
                                    .get(id)
                                    .map(|e| e.info.name.to_string())
                                    .unwrap_or_else(|| "unknown player".to_string()),
                                unique_id: p.player_unique_id,
                            },
                        )
                    })
                    .collect(),
            );

            for (id, entity, settings) in entities.into_iter().filter_map(|(id, entity)| {
                if let Some(player_settings) = match entity.player_unique_id {
                    PlayerUniqueId::Account(account_id) => config
                        .cl
                        .spatial_chat
                        .account_players
                        .get(&format!("acc_{account_id}")),
                    PlayerUniqueId::CertFingerprint(hash) => {
                        if !config.cl.spatial_chat.from_non_account_users {
                            return None;
                        }
                        config
                            .cl
                            .spatial_chat
                            .account_certs
                            .get(&format!("cert_{}", fmt_hash(&hash)))
                    }
                } {
                    if !player_settings.muted {
                        Some((id, entity, *player_settings))
                    } else {
                        None
                    }
                } else {
                    Some((id, entity, Default::default()))
                }
            }) {
                let entry = world.pending_entities.entry(id).or_default();
                entry.settings = settings;
                entry.opus_frames.extend(entity.latest_opus_frames);
            }
        }
    }

    pub fn on_entity_positions(
        world: Option<&mut SpatialChatGameWorld>,
        entities_positions: HashMap<PlayerId, vec2>,
    ) {
        if let Some(world) = world {
            world.entities_positions = entities_positions;
        }
    }
}
