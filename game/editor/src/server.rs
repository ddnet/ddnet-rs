use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use anyhow::anyhow;
use base::{
    hash::{Hash, generate_hash_for},
    steady_clock::SteadyClock,
};
use base_io::io::Io;
use client_notifications::overlay::ClientNotifications;
use editor_auto_mapper_wasm::manager::AutoMapperWasmManager;
use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::{
        backend::backend::GraphicsBackendHandle,
        buffer_object::buffer_object::GraphicsBufferObjectHandle,
        shader_storage::shader_storage::GraphicsShaderStorageHandle,
        texture::texture::GraphicsTextureHandle,
    },
};
use map::{file::MapFileReader, map::Map};
use math::math::vector::vec2;
use network::network::{
    connection::NetworkConnectionId,
    event::NetworkEvent,
    types::{NetworkServerCertMode, NetworkServerCertModeResult},
};
use rand::{RngCore, seq::SliceRandom};
use sound::sound_mt::SoundMultiThreaded;

use crate::{
    action_logic::{check_and_copy_tiles, do_action, merge_actions, redo_action, undo_action},
    actions::actions::{
        ActTileLayerReplaceTiles, EditorAction, EditorActionGroup, EditorActionInterface,
    },
    dbg::{invalid::random_invalid_action, valid::random_valid_action},
    event::{
        AdminConfigState, ClientProps, EditorCommand, EditorEvent, EditorEventAutoMap,
        EditorEventClientToServer, EditorEventGenerator, EditorEventLayerIndex,
        EditorEventOverwriteMap, EditorEventRuleTy, EditorEventServerToClient, EditorNetEvent,
    },
    map::{EditorLayer, EditorMap, EditorMapGroupsInterface},
    network::EditorNetwork,
    tools::{
        auto_saver::AutoSaver,
        tile_layer::{
            auto_mapper::{
                EditorAutoMapperInterface, TileLayerAutoMapperRuleType, TileLayerAutoMapperWasm,
            },
            legacy_rules::{LegacyRule, LegacyRulesLoading},
        },
    },
};

#[derive(Debug, Default)]
struct Client {
    is_authed: bool,
    is_admin: bool,
    is_local_client: bool,
    props: ClientProps,
}

/// the editor server is mostly there to
/// store the list of events, and keep events
/// synced to all clients
/// Additionally it makes the event list act like
/// an undo/redo manager
pub struct EditorServer {
    action_groups: Vec<EditorActionGroup>,
    cur_action_group: Option<usize>,

    network: EditorNetwork,

    has_events: Arc<AtomicBool>,
    event_generator: Arc<EditorEventGenerator>,

    pub cert: NetworkServerCertModeResult,
    pub port: u16,

    pub password: String,

    pub action_log: VecDeque<String>,

    admin_password: Option<String>,

    clients: HashMap<NetworkConnectionId, Client>,

    auto_mapper_rules: HashMap<(String, String, Hash), TileLayerAutoMapperRuleType>,

    client_ids: u64,

    time: SteadyClock,
    last_client_infos: Duration,
    needs_client_info_update: bool,

    io: Io,
}

impl EditorServer {
    pub fn new(
        time: &SteadyClock,
        cert_mode: Option<NetworkServerCertMode>,
        port: Option<u16>,
        password: String,
        admin_password: Option<String>,
        io: Io,
    ) -> anyhow::Result<Self> {
        let has_events: Arc<AtomicBool> = Default::default();
        let event_generator = Arc::new(EditorEventGenerator::new(has_events.clone()));

        let (network, cert, port) =
            EditorNetwork::new_server(time, event_generator.clone(), cert_mode, port)?;
        Ok(Self {
            action_groups: Default::default(),
            cur_action_group: None,

            has_events,
            event_generator,
            network,
            cert,
            port,
            password,
            clients: Default::default(),

            action_log: Default::default(),

            admin_password,

            auto_mapper_rules: Default::default(),

            client_ids: 0,

            needs_client_info_update: false,
            last_client_infos: time.now(),
            time: time.clone(),

            io,
        })
    }

    fn broadcast_client_infos(&self) {
        self.network
            .send(EditorEvent::Server(EditorEventServerToClient::Infos(
                self.clients.values().map(|c| c.props.clone()).collect(),
            )));
    }

    fn auto_map(
        rule: &mut TileLayerAutoMapperRuleType,
        auto_map: EditorEventAutoMap,
        map: &mut EditorMap,
    ) -> anyhow::Result<ActTileLayerReplaceTiles> {
        let groups = if auto_map.is_background {
            &mut map.groups.background
        } else {
            &mut map.groups.foreground
        };
        let group = groups
            .get_mut(auto_map.group_index)
            .ok_or_else(|| anyhow!("Group index out of bounds"))?;
        let layer = group
            .layers
            .get_mut(auto_map.layer_index)
            .ok_or_else(|| anyhow!("Layer index is out of bounds"))?;
        let EditorLayer::Tile(layer) = layer else {
            anyhow::bail!("Layer is not of type tile");
        };

        let action = rule.run_layer(
            auto_map.seed,
            layer.layer.attr,
            layer.layer.tiles.clone(),
            0,
            0,
            layer.layer.attr.width,
            layer.layer.attr.height,
            auto_map.is_background,
            auto_map.group_index,
            auto_map.layer_index,
        )?;

        Ok(action)
    }

    fn live_edit(
        auto_map: EditorEventAutoMap,
        live_edit: bool,
        map: &mut EditorMap,
    ) -> anyhow::Result<EditorEventLayerIndex> {
        let groups = if auto_map.is_background {
            &mut map.groups.background
        } else {
            &mut map.groups.foreground
        };
        let group = groups
            .get_mut(auto_map.group_index)
            .ok_or_else(|| anyhow!("Group index out of bounds"))?;
        let layer = group
            .layers
            .get_mut(auto_map.layer_index)
            .ok_or_else(|| anyhow!("Layer index is out of bounds"))?;
        let EditorLayer::Tile(layer) = layer else {
            anyhow::bail!("Layer is not of type tile");
        };

        layer.user.live_edit = live_edit.then_some((
            auto_map.seed,
            (auto_map.resource_and_hash, auto_map.name, auto_map.hash),
        ));

        Ok(EditorEventLayerIndex {
            is_background: auto_map.is_background,
            group_index: auto_map.group_index,
            layer_index: auto_map.layer_index,
        })
    }

    fn prepare_action(&mut self, map: &mut EditorMap, act: EditorAction) -> EditorAction {
        if let EditorAction::TileLayerReplaceTiles(act) = act {
            let groups = if act.base.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            if let Some(EditorLayer::Tile(layer)) = groups
                .get(act.base.group_index)
                .and_then(|g| g.layers.get(act.base.layer_index))
            {
                // execute the auto mapper is required
                if let Some((seed, key)) = layer.user.live_edit.as_ref() {
                    let rule = self
                        .auto_mapper_rules
                        .get_mut(key)
                        .expect("Auto mapper rule was not found");

                    let mut act = act;
                    let mut tiles = layer.layer.tiles.clone();
                    match check_and_copy_tiles(
                        act.base.layer_index,
                        &mut tiles,
                        &mut act.base.old_tiles,
                        &act.base.new_tiles,
                        layer.layer.attr.width.get() as usize,
                        layer.layer.attr.height.get() as usize,
                        act.base.x as usize,
                        act.base.y as usize,
                        act.base.w.get() as usize,
                        act.base.h.get() as usize,
                        true,
                    ) {
                        Ok(_) => {
                            match rule.run_layer(
                                *seed,
                                layer.layer.attr,
                                tiles,
                                act.base.x,
                                act.base.y,
                                act.base.w,
                                act.base.h,
                                act.base.is_background,
                                act.base.group_index,
                                act.base.layer_index,
                            ) {
                                Ok(act) => EditorAction::TileLayerReplaceTiles(act),
                                Err(err) => {
                                    log::error!("failed to execute auto mapper: {err}");
                                    EditorAction::TileLayerReplaceTiles(act)
                                }
                            }
                        }
                        Err(_) => EditorAction::TileLayerReplaceTiles(act),
                    }
                } else {
                    EditorAction::TileLayerReplaceTiles(act)
                }
            } else {
                EditorAction::TileLayerReplaceTiles(act)
            }
        } else {
            act
        }
    }

    fn handle_client_ev(
        &mut self,
        id: NetworkConnectionId,
        ev: EditorEventClientToServer,
        tp: &Arc<rayon::ThreadPool>,
        sound_mt: &SoundMultiThreaded,
        graphics_mt: &GraphicsMultiThreaded,
        shader_storage_handle: &GraphicsShaderStorageHandle,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        backend_handle: &GraphicsBackendHandle,
        texture_handle: &GraphicsTextureHandle,
        map: &mut EditorMap,
        auto_saver: &mut AutoSaver,
        notifications: &mut ClientNotifications,
        should_save: &mut bool,
    ) {
        // check if client exist and is authed
        if let Some(client) = self.clients.get_mut(&id) {
            if let EditorEventClientToServer::Auth {
                password,
                is_local_client,
                mapper_name,
                color,
            } = &ev
            {
                if self.password.eq(password) {
                    client.is_authed = true;
                    client.is_local_client = *is_local_client;
                    client.props = ClientProps {
                        mapper_name: mapper_name.clone(),
                        color: *color,

                        cursor_world: vec2::new(-10000.0, -10000.0),
                        server_id: {
                            let id = self.client_ids;
                            self.client_ids += 1;
                            id
                        },

                        stats: client.props.stats,
                    };

                    if !*is_local_client {
                        let resources: HashMap<_, _> = map
                            .resources
                            .images
                            .iter()
                            .flat_map(|r| {
                                [(r.def.meta.blake3_hash, r.user.file.as_ref().clone())]
                                    .into_iter()
                                    .chain(r.def.hq_meta.as_ref().zip(r.user.hq.as_ref()).map(
                                        |(s, (file, _))| (s.blake3_hash, file.as_ref().clone()),
                                    ))
                            })
                            .chain(map.resources.image_arrays.iter().flat_map(|r| {
                                [(r.def.meta.blake3_hash, r.user.file.as_ref().clone())]
                                    .into_iter()
                                    .chain(r.def.hq_meta.as_ref().zip(r.user.hq.as_ref()).map(
                                        |(s, (file, _))| (s.blake3_hash, file.as_ref().clone()),
                                    ))
                            }))
                            .chain(map.resources.sounds.iter().flat_map(|r| {
                                [(r.def.meta.blake3_hash, r.user.file.as_ref().clone())]
                                    .into_iter()
                                    .chain(r.def.hq_meta.as_ref().zip(r.user.hq.as_ref()).map(
                                        |(s, (file, _))| (s.blake3_hash, file.as_ref().clone()),
                                    ))
                            }))
                            .collect();

                        let send_map: Map = map.clone().into();

                        let map_bytes = send_map.write(tp).unwrap();

                        self.network.send_to(
                            &id,
                            EditorEvent::Server(EditorEventServerToClient::Map(
                                EditorEventOverwriteMap {
                                    map: map_bytes,
                                    resources,
                                    live_edited_layers: map.groups.live_edited_layers(),
                                },
                            )),
                        );
                    }

                    self.network.send_to(
                        &id,
                        EditorEvent::Server(EditorEventServerToClient::Info {
                            server_id: client.props.server_id,
                            allows_remote_admin: self.admin_password.is_some(),
                        }),
                    );
                    self.broadcast_client_infos();
                } else {
                    self.network.send_to(
                        &id,
                        EditorEvent::Server(EditorEventServerToClient::Error(
                            "wrong password".to_string(),
                        )),
                    );
                }
            } else if client.is_authed {
                match ev {
                    EditorEventClientToServer::Action(act) => {
                        let mut valid_act = EditorActionGroup {
                            actions: Vec::new(),
                            identifier: act.identifier.clone(),
                        };
                        for act in act.actions {
                            match do_action(
                                tp,
                                sound_mt,
                                graphics_mt,
                                shader_storage_handle,
                                buffer_object_handle,
                                backend_handle,
                                texture_handle,
                                self.prepare_action(map, act),
                                map,
                                true,
                            ) {
                                Ok(act) => {
                                    self.action_log
                                        .push_front(format!("[DO] {}", act.redo_info()));
                                    valid_act.actions.push(act);
                                }
                                Err(err) => {
                                    self.network.send_to(
                                        &id,
                                        EditorEvent::Server(EditorEventServerToClient::Error(
                                            format!(
                                                "Failed to execute your action\n\
                                                This is usually caused if a \
                                                previous action invalidates \
                                                this action, e.g. by a different user.\n\
                                                If all users are inactive, executing \
                                                the same action again should work; \
                                                if not it means it's a bug.\n{err}"
                                            ),
                                        )),
                                    );
                                    break;
                                }
                            }
                        }
                        if !valid_act.actions.is_empty() {
                            *should_save = true;
                            if let Some(cur_action_group) = self.cur_action_group {
                                self.action_groups.truncate(cur_action_group + 1);
                            } else {
                                self.action_groups.clear();
                            }

                            if self.action_groups.last_mut().is_some_and(|group| {
                                group
                                    .identifier
                                    .as_ref()
                                    // explicitly check for some here
                                    .is_some_and(|identifier| {
                                        Some(identifier) == valid_act.identifier.as_ref()
                                    })
                            }) {
                                let group = self.action_groups.last_mut().unwrap();
                                group.actions.append(&mut valid_act.actions.clone());

                                match merge_actions(&mut group.actions) {
                                    Ok(had_merge) => {
                                        if had_merge {
                                            let merged_action = group.actions.last().unwrap();
                                            self.action_log.push_front(format!(
                                                "[MERGED] {}",
                                                merged_action.redo_info()
                                            ));
                                        }
                                    }
                                    Err(err) => {
                                        log::error!("{err}{}", err.backtrace());
                                        notifications
                                            .add_err(err.to_string(), Duration::from_secs(10));
                                    }
                                }
                            } else {
                                let new_index = self.action_groups.len();
                                self.action_groups.push(valid_act.clone());
                                self.cur_action_group = Some(new_index);
                            }

                            // Make sure memory doesn't exhaust
                            while self.action_groups.len() > 300 {
                                self.action_groups.remove(0);
                                self.cur_action_group =
                                    self.cur_action_group.map(|index| index.saturating_sub(1));
                            }
                            self.action_log.truncate(4000);

                            self.clients
                                .iter()
                                .filter(|(_, client)| !client.is_local_client)
                                .for_each(|(id, _)| {
                                    self.network.send_to(
                                        id,
                                        EditorEvent::Server(
                                            EditorEventServerToClient::RedoAction {
                                                action: valid_act.clone(),
                                                undo_label: self.undo_label(),
                                                redo_label: self.redo_label(),
                                            },
                                        ),
                                    );
                                });
                        }
                    }
                    EditorEventClientToServer::Command(cmd) => match cmd {
                        EditorCommand::Undo | EditorCommand::Redo => {
                            let is_undo = matches!(cmd, EditorCommand::Undo);

                            if ((is_undo && self.cur_action_group.is_some())
                                || (!is_undo
                                    && self.cur_action_group.is_none_or(|index| {
                                        index < self.action_groups.len().saturating_sub(1)
                                    })))
                                && !self.action_groups.is_empty()
                            {
                                *should_save = true;
                                if !is_undo {
                                    self.cur_action_group =
                                        match self.cur_action_group {
                                            Some(index) => Some((index + 1).clamp(
                                                0,
                                                self.action_groups.len().saturating_sub(1),
                                            )),
                                            None => Some(0),
                                        };
                                }

                                let group = if let Some(group) = self
                                    .action_groups
                                    .get(self.cur_action_group.unwrap_or_default())
                                {
                                    let it: Box<dyn Iterator<Item = _>> = if is_undo {
                                        Box::new(group.actions.iter().rev())
                                    } else {
                                        Box::new(group.actions.iter())
                                    };
                                    for act in it {
                                        let act_label = format!(
                                            "[{}] {}",
                                            if is_undo { "UNDO" } else { "REDO" },
                                            if is_undo {
                                                act.undo_info()
                                            } else {
                                                act.redo_info()
                                            }
                                        );
                                        let action_fn =
                                            if is_undo { undo_action } else { redo_action };
                                        if let Err(act_err) = action_fn(
                                            tp,
                                            sound_mt,
                                            graphics_mt,
                                            shader_storage_handle,
                                            buffer_object_handle,
                                            backend_handle,
                                            texture_handle,
                                            act.clone(),
                                            map,
                                        ) {
                                            let err = format!(
                                                "Failed to execute your action.\n\
                                                Since it was an {} command, this \
                                                probably indicates a bug in the code.\n\
                                                {act_err}",
                                                if is_undo { "undo" } else { "redo" }
                                            );
                                            log::error!("{err}{}", act_err.backtrace());
                                            log::error!("current action: {act_label}");
                                            log::error!(
                                                "latest action log starting with \
                                                the most recent:\n{}",
                                                self.action_log
                                                    .iter()
                                                    .cloned()
                                                    .collect::<Vec<_>>()
                                                    .join("\n")
                                            );
                                            log::error!(
                                                "current actions index: {:?}, \
                                                currently there is a history of size: {}",
                                                self.cur_action_group,
                                                self.action_groups.len()
                                            );
                                            notifications.add_err(&err, Duration::from_secs(10));
                                            self.network.send_to(
                                                &id,
                                                EditorEvent::Server(
                                                    EditorEventServerToClient::Error(err),
                                                ),
                                            );
                                        }

                                        self.action_log.push_front(act_label);
                                    }
                                    group.clone()
                                } else {
                                    panic!("action group did not exists. logic bug")
                                };

                                if is_undo {
                                    self.cur_action_group = match self.cur_action_group {
                                        Some(index) => index.checked_sub(1),
                                        None => panic!(
                                            "Undo while the action group was None is a bug!."
                                        ),
                                    };
                                }

                                let undo_label = self.undo_label();
                                let redo_label = self.redo_label();
                                let act = if is_undo {
                                    EditorEventServerToClient::UndoAction {
                                        action: group,
                                        redo_label,
                                        undo_label,
                                    }
                                } else {
                                    EditorEventServerToClient::RedoAction {
                                        action: group,
                                        redo_label,
                                        undo_label,
                                    }
                                };
                                self.clients
                                    .iter()
                                    .filter(|(_, client)| !client.is_local_client)
                                    .for_each(|(id, _)| {
                                        self.network.send_to(id, EditorEvent::Server(act.clone()));
                                    });

                                self.action_log.truncate(4000);
                            }
                        }
                    },
                    EditorEventClientToServer::Auth { .. } => {
                        // ignore here, handled earlier
                    }
                    EditorEventClientToServer::Info(mut info) => {
                        // make sure the id stays unique
                        info.server_id = client.props.server_id;
                        info.stats = client.props.stats;
                        client.props = info;

                        self.needs_client_info_update = true;
                    }
                    EditorEventClientToServer::Chat { msg } => {
                        if !msg.is_empty() {
                            self.network.send(EditorEvent::Server(
                                EditorEventServerToClient::Chat {
                                    from: client.props.mapper_name.clone(),
                                    msg,
                                },
                            ));
                        }
                    }
                    EditorEventClientToServer::AdminAuth { password } => {
                        if self.admin_password == Some(password) {
                            self.network
                                .send(EditorEvent::Server(EditorEventServerToClient::AdminAuthed));
                            client.is_admin = true;
                            for (id, _) in self.clients.iter().filter(|(_, c)| c.is_admin) {
                                self.network.send_to(
                                    id,
                                    EditorEvent::Server(EditorEventServerToClient::AdminState {
                                        cur_state: AdminConfigState {
                                            auto_save: auto_saver
                                                .active
                                                .then_some(auto_saver.interval)
                                                .flatten(),
                                        },
                                    }),
                                );
                            }
                        }
                    }
                    EditorEventClientToServer::AdminChangeConfig(state) => {
                        if self.admin_password == Some(state.password) {
                            auto_saver.active = state.state.auto_save.is_some();
                            auto_saver.interval = state.state.auto_save;
                            for (id, _) in self.clients.iter().filter(|(_, c)| c.is_admin) {
                                self.network.send_to(
                                    id,
                                    EditorEvent::Server(EditorEventServerToClient::AdminState {
                                        cur_state: state.state.clone(),
                                    }),
                                );
                            }
                        }
                    }
                    EditorEventClientToServer::DbgAction(props) => {
                        if self.admin_password.is_none()
                            && self.clients.values().any(|c| c.is_local_client)
                        {
                            let allows_identifier = !props.no_actions_identifier;
                            let mut run_actions = |map: &mut _, actions: Vec<EditorAction>| {
                                self.handle_client_ev(
                                    id,
                                    EditorEventClientToServer::Action(EditorActionGroup {
                                        actions,
                                        identifier: allows_identifier
                                            .then_some("dbg-action".into()),
                                    }),
                                    tp,
                                    sound_mt,
                                    graphics_mt,
                                    shader_storage_handle,
                                    buffer_object_handle,
                                    backend_handle,
                                    texture_handle,
                                    map,
                                    auto_saver,
                                    notifications,
                                    should_save,
                                );
                            };
                            let gen_actions = |map: &mut _| {
                                let invalid_action = rand::rng().next_u64() % u8::MAX as u64;
                                let is_invalid = invalid_action
                                    < props.invalid_action_probability as u64
                                    || props.invalid_action_probability == u8::MAX;
                                let actions = if is_invalid {
                                    random_invalid_action(map)
                                } else {
                                    random_valid_action(map)
                                };
                                let res = bincode::serde::decode_from_slice::<Vec<EditorAction>, _>(
                                    &bincode::serde::encode_to_vec(
                                        &actions,
                                        bincode::config::standard(),
                                    )
                                    .unwrap(),
                                    bincode::config::standard(),
                                );
                                if is_invalid && res.is_err() {
                                    log::info!(
                                        "would have caught invalid action during deserialization"
                                    );
                                    return Default::default();
                                } else {
                                    assert!(
                                        res.is_ok(),
                                        "failed to de-/serialize the valid actions"
                                    );
                                }
                                actions
                            };

                            let undo_redo = rand::rng().next_u64() % u8::MAX as u64;
                            if undo_redo < props.undo_redo_probability as u64
                                || props.undo_redo_probability == u8::MAX
                            {
                                let is_undo = (rand::rng().next_u64() % 2) == 0;
                                self.handle_client_ev(
                                    id,
                                    EditorEventClientToServer::Command(if is_undo {
                                        EditorCommand::Undo
                                    } else {
                                        EditorCommand::Redo
                                    }),
                                    tp,
                                    sound_mt,
                                    graphics_mt,
                                    shader_storage_handle,
                                    buffer_object_handle,
                                    backend_handle,
                                    texture_handle,
                                    map,
                                    auto_saver,
                                    notifications,
                                    should_save,
                                );
                            } else {
                                let shuffle_action = rand::rng().next_u64() % u8::MAX as u64;
                                if shuffle_action < props.action_shuffle_probability as u64
                                    || props.action_shuffle_probability == u8::MAX
                                {
                                    let mut actions: Vec<_> =
                                        (0..props.num_actions).map(|_| gen_actions(map)).collect();
                                    actions.shuffle(&mut rand::rng());

                                    for actions in actions {
                                        run_actions(map, actions);
                                    }
                                } else {
                                    for _ in 0..props.num_actions {
                                        let actions = gen_actions(map);
                                        if !actions.is_empty() {
                                            run_actions(map, actions);
                                        }
                                    }
                                }
                            }

                            let full_map_validation = rand::rng().next_u64() % u8::MAX as u64;
                            if full_map_validation < props.full_map_validation_probability as u64
                                || props.full_map_validation_probability == u8::MAX
                            {
                                let map: Map = map.clone().into();
                                let map_file: Vec<_> = map.write(tp).unwrap();
                                Map::read(&MapFileReader::new(map_file).unwrap(), tp).unwrap();
                            }
                        }
                    }
                    EditorEventClientToServer::LoadAutoMap {
                        resource_and_hash,
                        name,
                        hash,
                        rule,
                    } => {
                        let rules: Vec<_> = match rule {
                            EditorEventRuleTy::EditorRuleJson(rule) => (generate_hash_for(&rule)
                                == hash)
                                .then(|| {
                                    serde_json::from_slice(&rule)
                                        .ok()
                                        .map(|r| (name, TileLayerAutoMapperRuleType::EditorRule(r)))
                                })
                                .flatten()
                                .into_iter()
                                .collect(),
                            EditorEventRuleTy::Wasm(wasm_file) => (generate_hash_for(&wasm_file)
                                == hash)
                                .then(|| {
                                    let fs = self.io.fs.clone();
                                    let wasm_file_task = wasm_file.clone();
                                    self.io
                                        .rt
                                        .spawn(async move {
                                            AutoMapperWasmManager::load_module(&fs, wasm_file_task)
                                                .await
                                        })
                                        .get()
                                        .ok()
                                        .and_then(|wasm_module| {
                                            TileLayerAutoMapperWasm::new(
                                                wasm_module,
                                                wasm_file,
                                                hash,
                                            )
                                            .ok()
                                        })
                                        .map(|r| {
                                            (name, TileLayerAutoMapperRuleType::Wasm(Box::new(r)))
                                        })
                                })
                                .flatten()
                                .into_iter()
                                .collect(),
                            EditorEventRuleTy::LegacyRules(rules) => {
                                if generate_hash_for(&rules) == hash {
                                    LegacyRulesLoading::new(&rules)
                                        .ok()
                                        .map(|rules| {
                                            let loading_data = Arc::new(rules.file);
                                            rules
                                                .configs
                                                .into_iter()
                                                .map(|(config_name, rule)| {
                                                    let base_name = name
                                                        .split_once('/')
                                                        .map(|(b, _)| b.to_string())
                                                        .unwrap_or_default();
                                                    (
                                                        format!("{base_name}/{config_name}"),
                                                        TileLayerAutoMapperRuleType::LegacyRules {
                                                            rule: LegacyRule { config: rule },
                                                            loading_data: loading_data.clone(),
                                                        },
                                                    )
                                                })
                                                .collect()
                                        })
                                        .unwrap_or_default()
                                } else {
                                    Default::default()
                                }
                            }
                        };
                        if !rules.is_empty() {
                            for (name, rule) in rules {
                                self.auto_mapper_rules
                                    .insert((resource_and_hash.clone(), name, hash), rule);
                            }
                        } else {
                            // else send error
                            self.network.send_to(
                                &id,
                                EditorEvent::Server(EditorEventServerToClient::Error(
                                    "editor rule was invalid".into(),
                                )),
                            );
                        }
                    }
                    EditorEventClientToServer::AutoMap(auto_mapper) => {
                        match self.auto_mapper_rules.get_mut(&(
                            auto_mapper.resource_and_hash.clone(),
                            auto_mapper.name.clone(),
                            auto_mapper.hash,
                        )) {
                            Some(rule) => match Self::auto_map(rule, auto_mapper, map) {
                                Ok(action) => {
                                    self.handle_client_ev(
                                        id,
                                        EditorEventClientToServer::Action(EditorActionGroup {
                                            actions: vec![EditorAction::TileLayerReplaceTiles(
                                                action,
                                            )],
                                            identifier: Some("auto-mapper".to_string()),
                                        }),
                                        tp,
                                        sound_mt,
                                        graphics_mt,
                                        shader_storage_handle,
                                        buffer_object_handle,
                                        backend_handle,
                                        texture_handle,
                                        map,
                                        auto_saver,
                                        notifications,
                                        should_save,
                                    );
                                }
                                Err(err) => {
                                    log::error!("Auto mapper failed: {err}");
                                    self.network.send_to(
                                        &id,
                                        EditorEvent::Server(EditorEventServerToClient::Error(
                                            err.to_string(),
                                        )),
                                    );
                                }
                            },
                            None => {
                                self.network.send_to(
                                    &id,
                                    EditorEvent::Server(
                                        EditorEventServerToClient::AutoMapRuleNotFound(auto_mapper),
                                    ),
                                );
                            }
                        }
                    }
                    EditorEventClientToServer::AutoMapLiveEdit {
                        auto_map,
                        live_edit,
                    } => {
                        match self.auto_mapper_rules.get_mut(&(
                            auto_map.resource_and_hash.clone(),
                            auto_map.name.clone(),
                            auto_map.hash,
                        )) {
                            Some(rule) => {
                                match Self::live_edit(auto_map.clone(), live_edit, map) {
                                    Ok(layer_index) => {
                                        // if rule & layer was found, tell all clients that this layer is now live edited
                                        for (id, _) in
                                            self.clients.iter().filter(|(_, c)| !c.is_local_client)
                                        {
                                            self.network.send_to(
                                                id,
                                                EditorEvent::Server(
                                                    EditorEventServerToClient::AutoMapLiveEdit {
                                                        layer_index,
                                                        live_edit,
                                                    },
                                                ),
                                            );
                                        }
                                        // if live edited was succesful, then try to auto map full layer once
                                        if let Ok(action) = Self::auto_map(rule, auto_map, map) {
                                            self.handle_client_ev(
                                                id,
                                                EditorEventClientToServer::Action(
                                                    EditorActionGroup {
                                                        actions: vec![
                                                            EditorAction::TileLayerReplaceTiles(
                                                                action,
                                                            ),
                                                        ],
                                                        identifier: Some("auto-mapper".to_string()),
                                                    },
                                                ),
                                                tp,
                                                sound_mt,
                                                graphics_mt,
                                                shader_storage_handle,
                                                buffer_object_handle,
                                                backend_handle,
                                                texture_handle,
                                                map,
                                                auto_saver,
                                                notifications,
                                                should_save,
                                            );
                                        }
                                    }
                                    Err(err) => {
                                        self.network.send_to(
                                            &id,
                                            EditorEvent::Server(EditorEventServerToClient::Error(
                                                err.to_string(),
                                            )),
                                        );
                                    }
                                }
                            }
                            None => {
                                self.network.send_to(
                                    &id,
                                    EditorEvent::Server(
                                        EditorEventServerToClient::AutoMapRuleLiveEditNotFound {
                                            auto_mapper: auto_map,
                                            live_edit,
                                        },
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn update(
        &mut self,
        tp: &Arc<rayon::ThreadPool>,
        sound_mt: &SoundMultiThreaded,
        graphics_mt: &GraphicsMultiThreaded,
        shader_storage_handle: &GraphicsShaderStorageHandle,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        backend_handle: &GraphicsBackendHandle,
        texture_handle: &GraphicsTextureHandle,
        map: &mut EditorMap,
        auto_saver: &mut AutoSaver,
        notifications: &mut ClientNotifications,
        should_save: &mut bool,
    ) {
        let now = self.time.now();
        if self.needs_client_info_update
            && now.saturating_sub(self.last_client_infos) > Duration::from_millis(10)
        {
            self.broadcast_client_infos();
            self.needs_client_info_update = false;
            self.last_client_infos = now;
        }

        if self.has_events.load(std::sync::atomic::Ordering::Relaxed) {
            let events = self.event_generator.take();

            for (id, _, event) in events {
                match event {
                    EditorNetEvent::Editor(EditorEvent::Client(ev)) => {
                        self.handle_client_ev(
                            id,
                            ev,
                            tp,
                            sound_mt,
                            graphics_mt,
                            shader_storage_handle,
                            buffer_object_handle,
                            backend_handle,
                            texture_handle,
                            map,
                            auto_saver,
                            notifications,
                            should_save,
                        );
                    }
                    EditorNetEvent::Editor(EditorEvent::Server(_)) => {
                        // ignore
                    }
                    EditorNetEvent::NetworkEvent(ev) => {
                        match &ev {
                            NetworkEvent::Connected { .. } => {
                                self.clients.insert(id, Client::default());

                                self.broadcast_client_infos();
                            }
                            NetworkEvent::Disconnected { .. } => {
                                self.clients.remove(&id);

                                self.broadcast_client_infos();
                            }
                            NetworkEvent::NetworkStats(stats) => {
                                if let Some(client) = self.clients.get_mut(&id) {
                                    client.props.stats = Some(*stats);
                                }
                            }
                            _ => {
                                // ignore
                            }
                        }
                        match self.network.handle_network_ev(id, ev) {
                            Ok(None) => {
                                // ignore
                            }
                            Ok(Some(msg)) => {
                                log::info!("Editor server: {msg}");
                            }
                            Err(err) => {
                                log::error!("{err}");
                                notifications.add_err(err.to_string(), Duration::from_secs(10));
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn undo_label(&self) -> Option<String> {
        self.cur_action_group
            .and_then(|i| self.action_groups.get(i))
            .and_then(|g| g.actions.last().map(|a| (a, g.actions.len())))
            .map(|(a, len)| {
                format!(
                    "{}{}",
                    a.undo_info(),
                    if len > 1 {
                        format!(" + {len} more ")
                    } else {
                        "".to_string()
                    }
                )
            })
    }
    pub fn redo_label(&self) -> Option<String> {
        (!self.action_groups.is_empty()
            && self
                .cur_action_group
                .is_none_or(|i| i < self.action_groups.len().saturating_sub(1)))
        .then(|| {
            self.action_groups.get(match self.cur_action_group {
                Some(val) => val + 1,
                None => 0,
            })
        })
        .flatten()
        .and_then(|g| g.actions.first().map(|a| (a, g.actions.len())))
        .map(|(a, len)| {
            format!(
                "{}{}",
                a.redo_info(),
                if len > 1 {
                    format!(" + {len} more ")
                } else {
                    "".to_string()
                }
            )
        })
    }
}
