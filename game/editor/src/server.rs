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
use network::network::{
    connection::NetworkConnectionId,
    event::NetworkEvent,
    types::{NetworkServerCertMode, NetworkServerCertModeResult},
};
use sound::sound_mt::SoundMultiThreaded;

use crate::{
    action_logic::{do_action, merge_actions, undo_action},
    actions::actions::EditorActionGroup,
    event::{
        ClientProps, EditorCommand, EditorEvent, EditorEventClientToServer, EditorEventGenerator,
        EditorEventOverwriteMap, EditorEventServerToClient, EditorNetEvent,
    },
    map::EditorMap,
    network::EditorNetwork,
};

#[derive(Debug, Default)]
struct Client {
    is_authed: bool,
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
    cur_action_group: usize,

    network: EditorNetwork,

    has_events: Arc<AtomicBool>,
    event_generator: Arc<EditorEventGenerator>,

    pub cert: NetworkServerCertModeResult,
    pub port: u16,

    pub password: String,

    clients: HashMap<NetworkConnectionId, Client>,

    client_ids: u64,
}

impl EditorServer {
    pub fn new(
        sys: &System,
        cert_mode: Option<NetworkServerCertMode>,
        port: Option<u16>,
        password: String,
    ) -> Self {
        let has_events: Arc<AtomicBool> = Default::default();
        let event_generator = Arc::new(EditorEventGenerator::new(has_events.clone()));

        let (network, cert, port) =
            EditorNetwork::new_server(sys, event_generator.clone(), cert_mode, port);
        Self {
            action_groups: Default::default(),
            cur_action_group: 0,

            has_events,
            event_generator,
            network,
            cert,
            port,
            password,
            clients: Default::default(),

            client_ids: 0,
        }
    }

    fn broadcast_client_infos(&self) {
        self.network
            .send(EditorEvent::Server(EditorEventServerToClient::Infos(
                self.clients.values().map(|c| c.props.clone()).collect(),
            )));
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
        notifications: &mut ClientNotifications,
    ) {
        if self.has_events.load(std::sync::atomic::Ordering::Relaxed) {
            let events = self.event_generator.take();

            for (id, _, event) in events {
                match event {
                    EditorNetEvent::Editor(EditorEvent::Client(ev)) => {
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

                                        cursor_world: Default::default(),
                                        server_id: {
                                            let id = self.client_ids;
                                            self.client_ids += 1;
                                            id
                                        },
                                    };

                                    if !*is_local_client {
                                        let resources: HashMap<_, _> = map
                                            .resources
                                            .images
                                            .iter()
                                            .flat_map(|r| {
                                                [(
                                                    r.def.meta.blake3_hash,
                                                    r.user.file.as_ref().clone(),
                                                )]
                                                .into_iter()
                                                .chain(
                                                    r.def
                                                        .hq_meta
                                                        .as_ref()
                                                        .zip(r.user.hq.as_ref())
                                                        .map(|(s, (file, _))| {
                                                            (s.blake3_hash, file.as_ref().clone())
                                                        }),
                                                )
                                            })
                                            .chain(map.resources.image_arrays.iter().flat_map(
                                                |r| {
                                                    [(
                                                        r.def.meta.blake3_hash,
                                                        r.user.file.as_ref().clone(),
                                                    )]
                                                    .into_iter()
                                                    .chain(
                                                        r.def
                                                            .hq_meta
                                                            .as_ref()
                                                            .zip(r.user.hq.as_ref())
                                                            .map(|(s, (file, _))| {
                                                                (
                                                                    s.blake3_hash,
                                                                    file.as_ref().clone(),
                                                                )
                                                            }),
                                                    )
                                                },
                                            ))
                                            .chain(map.resources.sounds.iter().flat_map(|r| {
                                                [(
                                                    r.def.meta.blake3_hash,
                                                    r.user.file.as_ref().clone(),
                                                )]
                                                .into_iter()
                                                .chain(
                                                    r.def
                                                        .hq_meta
                                                        .as_ref()
                                                        .zip(r.user.hq.as_ref())
                                                        .map(|(s, (file, _))| {
                                                            (s.blake3_hash, file.as_ref().clone())
                                                        }),
                                                )
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
                                        }),
                                    );
                                    self.broadcast_client_infos();
                                }
                            } else if client.is_authed {
                                match ev {
                                    EditorEventClientToServer::Action(act) => {
                                        self.action_groups.truncate(self.cur_action_group + 1);

                                        if self
                                            .action_groups
                                            .last_mut()
                                            .is_some_and(|group| group.identifier == act.identifier)
                                        {
                                            let group = self.action_groups.last_mut().unwrap();
                                            group.actions.append(&mut act.actions.clone());

                                            if let Err(err) = merge_actions(&mut group.actions) {
                                                notifications.add_err(
                                                    err.to_string(),
                                                    Duration::from_secs(10),
                                                );
                                            }
                                        } else {
                                            let new_index = self.action_groups.len();
                                            self.action_groups.push(act.clone());
                                            self.cur_action_group = new_index;
                                        }
                                        let mut send_act = EditorActionGroup {
                                            actions: Vec::new(),
                                            identifier: act.identifier.clone(),
                                        };
                                        for act in act.actions {
                                            let sent_act = act.clone();
                                            if let Err(err) = do_action(
                                                tp,
                                                sound_mt,
                                                graphics_mt,
                                                buffer_object_handle,
                                                backend_handle,
                                                texture_handle,
                                                act,
                                                map,
                                            ) {
                                                self.network.send_to(
                                                    &id,
                                                    EditorEvent::Server(
                                                        EditorEventServerToClient::Error(format!(
                                                            "Failed to execute your action\n\
                                                        This is usually caused if a \
                                                        previous action invalidates \
                                                        this action, e.g. by a different user.\n\
                                                        If all users are inactive, executing \
                                                        the same action again should work; \
                                                        if not it means it's a bug.\n{err}"
                                                        )),
                                                    ),
                                                );
                                            } else {
                                                send_act.actions.push(sent_act);
                                            }
                                        }
                                        self.clients
                                            .iter()
                                            .filter(|(_, client)| !client.is_local_client)
                                            .for_each(|(id, _)| {
                                                self.network.send_to(
                                                    id,
                                                    EditorEvent::Server(
                                                        EditorEventServerToClient::DoAction(
                                                            send_act.clone(),
                                                        ),
                                                    ),
                                                );
                                            });
                                    }
                                    EditorEventClientToServer::Command(cmd) => match cmd {
                                        EditorCommand::Undo | EditorCommand::Redo => {
                                            let is_undo = matches!(cmd, EditorCommand::Undo);

                                            if !is_undo {
                                                self.cur_action_group = (self.cur_action_group + 1)
                                                    .clamp(
                                                        0,
                                                        self.action_groups.len().saturating_sub(1),
                                                    );
                                            }

                                            if let Some(group) =
                                                self.action_groups.get(self.cur_action_group)
                                            {
                                                let it: Box<dyn Iterator<Item = _>> = if is_undo {
                                                    Box::new(group.actions.iter().rev())
                                                } else {
                                                    Box::new(group.actions.iter())
                                                };
                                                for act in it {
                                                    let action_fn = if is_undo {
                                                        undo_action
                                                    } else {
                                                        do_action
                                                    };
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
                                                        notifications
                                                            .add_err(&err, Duration::from_secs(10));
                                                        self.network.send_to(
                                                            &id,
                                                            EditorEvent::Server(
                                                                EditorEventServerToClient::Error(
                                                                    err,
                                                                ),
                                                            ),
                                                        );
                                                    }
                                                }

                                                let act = if is_undo {
                                                    EditorEventServerToClient::UndoAction(
                                                        group.clone(),
                                                    )
                                                } else {
                                                    EditorEventServerToClient::DoAction(
                                                        group.clone(),
                                                    )
                                                };
                                                self.clients
                                                    .iter()
                                                    .filter(|(_, client)| !client.is_local_client)
                                                    .for_each(|(id, _)| {
                                                        self.network.send_to(
                                                            id,
                                                            EditorEvent::Server(act.clone()),
                                                        );
                                                    });
                                            }

                                            if is_undo {
                                                self.cur_action_group =
                                                    self.cur_action_group.saturating_sub(1);
                                            }
                                        }
                                    },
                                    EditorEventClientToServer::Auth { .. } => {
                                        // ignore here, handled earlier
                                    }
                                    EditorEventClientToServer::Info(mut info) => {
                                        // make sure the id stays unique
                                        info.server_id = client.props.server_id;
                                        client.props = info;

                                        self.broadcast_client_infos();
                                    }
                                }
                            }
                        }
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
                            _ => {
                                // ignore
                            }
                        }
                        self.network.handle_network_ev(id, ev)
                    }
                }
            }
        }
    }
}
