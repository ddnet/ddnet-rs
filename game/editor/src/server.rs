use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use base::system::System;
use client_notifications::overlay::ClientNotifications;
use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::{
        backend::backend::GraphicsBackendHandle,
        buffer_object::buffer_object::GraphicsBufferObjectHandle,
        texture::texture::GraphicsTextureHandle,
    },
};
use map::map::Map;
use math::math::vector::vec2;
use network::network::{
    connection::NetworkConnectionId,
    event::NetworkEvent,
    types::{NetworkServerCertMode, NetworkServerCertModeResult},
};
use sound::sound_mt::SoundMultiThreaded;

use crate::{
    action_logic::{do_action, merge_actions, redo_action, undo_action},
    actions::actions::{EditorActionGroup, EditorActionInterface},
    event::{
        AdminConfigState, ClientProps, EditorCommand, EditorEvent, EditorEventClientToServer,
        EditorEventGenerator, EditorEventOverwriteMap, EditorEventServerToClient, EditorNetEvent,
    },
    map::EditorMap,
    network::EditorNetwork,
    tools::auto_saver::AutoSaver,
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

    admin_password: Option<String>,

    clients: HashMap<NetworkConnectionId, Client>,

    client_ids: u64,
}

impl EditorServer {
    pub fn new(
        sys: &System,
        cert_mode: Option<NetworkServerCertMode>,
        port: Option<u16>,
        password: String,
        admin_password: Option<String>,
    ) -> anyhow::Result<Self> {
        let has_events: Arc<AtomicBool> = Default::default();
        let event_generator = Arc::new(EditorEventGenerator::new(has_events.clone()));

        let (network, cert, port) =
            EditorNetwork::new_server(sys, event_generator.clone(), cert_mode, port)?;
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

            admin_password,

            client_ids: 0,
        })
    }

    fn broadcast_client_infos(&self) {
        self.network
            .send(EditorEvent::Server(EditorEventServerToClient::Infos(
                self.clients.values().map(|c| c.props.clone()).collect(),
            )));
    }

    fn handle_client_ev(
        &mut self,
        id: NetworkConnectionId,
        ev: EditorEventClientToServer,
        tp: &Arc<rayon::ThreadPool>,
        sound_mt: &SoundMultiThreaded,
        graphics_mt: &GraphicsMultiThreaded,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        backend_handle: &GraphicsBackendHandle,
        texture_handle: &GraphicsTextureHandle,
        map: &mut EditorMap,
        auto_saver: &mut AutoSaver,
        notifications: &mut ClientNotifications,
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

                        let map: Map = map.clone().into();

                        let mut map_bytes = Vec::new();
                        map.write(&mut map_bytes, tp).unwrap();

                        self.network.send_to(
                            &id,
                            EditorEvent::Server(EditorEventServerToClient::Map(
                                EditorEventOverwriteMap {
                                    map: map_bytes,
                                    resources,
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
                        if let Some(cur_action_group) = self.cur_action_group {
                            self.action_groups.truncate(cur_action_group + 1);
                        }

                        let mut valid_act = EditorActionGroup {
                            actions: Vec::new(),
                            identifier: act.identifier.clone(),
                        };
                        for act in act.actions {
                            match do_action(
                                tp,
                                sound_mt,
                                graphics_mt,
                                buffer_object_handle,
                                backend_handle,
                                texture_handle,
                                act,
                                map,
                                true,
                            ) {
                                Ok(act) => {
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

                            if let Err(err) = merge_actions(&mut group.actions) {
                                log::error!("{err}");
                                notifications.add_err(err.to_string(), Duration::from_secs(10));
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

                        self.clients
                            .iter()
                            .filter(|(_, client)| !client.is_local_client)
                            .for_each(|(id, _)| {
                                self.network.send_to(
                                    id,
                                    EditorEvent::Server(EditorEventServerToClient::RedoAction {
                                        action: valid_act.clone(),
                                        undo_label: self.undo_label(),
                                        redo_label: self.redo_label(),
                                    }),
                                );
                            });
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
                                        let action_fn =
                                            if is_undo { undo_action } else { redo_action };
                                        if let Err(err) = action_fn(
                                            tp,
                                            sound_mt,
                                            graphics_mt,
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
                                                {err}",
                                                if is_undo { "undo" } else { "redo" }
                                            );
                                            log::error!("{err}");
                                            notifications.add_err(&err, Duration::from_secs(10));
                                            self.network.send_to(
                                                &id,
                                                EditorEvent::Server(
                                                    EditorEventServerToClient::Error(err),
                                                ),
                                            );
                                        }
                                    }
                                    Some(group.clone())
                                } else {
                                    None
                                };

                                if is_undo {
                                    self.cur_action_group = match self.cur_action_group {
                                        Some(index) => index.checked_sub(1),
                                        None => panic!(
                                            "Undo while the action group was None is a bug!."
                                        ),
                                    };
                                }

                                if let Some(group) = group {
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
                                            self.network
                                                .send_to(id, EditorEvent::Server(act.clone()));
                                        });
                                }
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

                        self.broadcast_client_infos();
                    }
                    EditorEventClientToServer::Chat { msg } => {
                        self.network
                            .send(EditorEvent::Server(EditorEventServerToClient::Chat {
                                from: client.props.mapper_name.clone(),
                                msg,
                            }));
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
                }
            }
        }
    }

    pub fn update(
        &mut self,
        tp: &Arc<rayon::ThreadPool>,
        sound_mt: &SoundMultiThreaded,
        graphics_mt: &GraphicsMultiThreaded,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        backend_handle: &GraphicsBackendHandle,
        texture_handle: &GraphicsTextureHandle,
        map: &mut EditorMap,
        auto_saver: &mut AutoSaver,
        notifications: &mut ClientNotifications,
    ) {
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
                            buffer_object_handle,
                            backend_handle,
                            texture_handle,
                            map,
                            auto_saver,
                            notifications,
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
