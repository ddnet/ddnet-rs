use std::sync::{atomic::AtomicBool, Arc};

use anyhow::anyhow;
use base::system::System;
use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::{
        backend::backend::GraphicsBackendHandle,
        buffer_object::buffer_object::GraphicsBufferObjectHandle,
        texture::texture::GraphicsTextureHandle,
    },
};
use math::math::vector::vec2;
use network::network::types::NetworkClientCertCheckMode;
use sound::sound_mt::SoundMultiThreaded;

use crate::{
    action_logic::{do_action, undo_action},
    actions::actions::{EditorAction, EditorActionGroup},
    event::{
        ClientProps, EditorCommand, EditorEvent, EditorEventClientToServer, EditorEventGenerator,
        EditorEventOverwriteMap, EditorEventServerToClient, EditorNetEvent,
    },
    map::EditorMap,
    network::EditorNetwork,
    notifications::{EditorNotification, EditorNotifications},
};

/// the editor client handles events from the server if needed
pub struct EditorClient {
    network: EditorNetwork,

    has_events: Arc<AtomicBool>,
    event_generator: Arc<EditorEventGenerator>,

    notifications: EditorNotifications,
    local_client: bool,

    pub(crate) clients: Vec<ClientProps>,
    pub(crate) server_id: u64,

    mapper_name: String,
    color: [u8; 3],
}

impl EditorClient {
    pub fn new(
        sys: &System,
        server_addr: &str,
        server_info: NetworkClientCertCheckMode,
        notifications: EditorNotifications,
        server_password: String,
        local_client: bool,
        mapper_name: Option<String>,
        color: Option<[u8; 3]>,
    ) -> Self {
        let has_events: Arc<AtomicBool> = Default::default();
        let event_generator = Arc::new(EditorEventGenerator::new(has_events.clone()));

        let res = Self {
            network: EditorNetwork::new_client(
                sys,
                event_generator.clone(),
                server_addr,
                server_info,
            ),
            has_events,
            event_generator,
            notifications,
            local_client,

            clients: Default::default(),
            server_id: Default::default(),

            mapper_name: mapper_name.unwrap_or_else(|| "mapper".to_string()),
            color: color.unwrap_or([255, 255, 255]),
        };

        res.network
            .send(EditorEvent::Client(EditorEventClientToServer::Auth {
                password: server_password,
                is_local_client: local_client,
                mapper_name: res.mapper_name.clone(),
                color: res.color,
            }));

        res
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
    ) -> anyhow::Result<Option<EditorEventOverwriteMap>> {
        let mut res = None;

        if self.has_events.load(std::sync::atomic::Ordering::Relaxed) {
            let events = self.event_generator.take();

            for (id, _, event) in events {
                match event {
                    EditorNetEvent::Editor(EditorEvent::Server(ev)) => match ev {
                        EditorEventServerToClient::DoAction(act) => {
                            if !self.local_client {
                                for act in act.actions {
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
                                        self.notifications.push(EditorNotification::Error(format!("There has been an critical error while processing a action of the server: {err}.\nThis usually indicates a bug in the editor code.\nCan not continue.")));
                                        return Err(anyhow!("critical error during do_action"));
                                    }
                                }
                            }
                        }
                        EditorEventServerToClient::UndoAction(act) => {
                            if !self.local_client {
                                for act in act.actions.into_iter().rev() {
                                    if let Err(err) = undo_action(
                                        tp,
                                        sound_mt,
                                        graphics_mt,
                                        buffer_object_handle,
                                        backend_handle,
                                        texture_handle,
                                        act,
                                        map,
                                    ) {
                                        self.notifications.push(EditorNotification::Error(format!("There has been an critical error while processing a action of the server: {err}.\nThis usually indicates a bug in the editor code.\nCan not continue.")));
                                        return Err(anyhow!("critical error during do_action"));
                                    }
                                }
                            }
                        }
                        EditorEventServerToClient::Error(err) => {
                            self.notifications.push(EditorNotification::Error(err));
                        }
                        EditorEventServerToClient::Map(map) => {
                            res = Some(map);
                        }
                        EditorEventServerToClient::Infos(infos) => {
                            self.clients = infos;
                        }
                        EditorEventServerToClient::Info { server_id } => {
                            self.server_id = server_id;
                        }
                    },

                    EditorNetEvent::Editor(EditorEvent::Client(_)) => {
                        // ignore
                    }
                    EditorNetEvent::NetworkEvent(ev) => self.network.handle_network_ev(id, ev),
                }
            }
        }

        Ok(res)
    }

    pub fn execute(&mut self, action: EditorAction, group_identifier: Option<&str>) {
        self.network
            .send(EditorEvent::Client(EditorEventClientToServer::Action(
                EditorActionGroup {
                    actions: vec![action],
                    identifier: group_identifier.map(|s| s.to_string()),
                },
            )));
    }

    pub fn execute_group(&mut self, action_group: EditorActionGroup) {
        self.network
            .send(EditorEvent::Client(EditorEventClientToServer::Action(
                action_group,
            )));
    }

    pub fn undo(&self) {
        self.network
            .send(EditorEvent::Client(EditorEventClientToServer::Command(
                EditorCommand::Undo,
            )));
    }

    pub fn redo(&self) {
        self.network
            .send(EditorEvent::Client(EditorEventClientToServer::Command(
                EditorCommand::Redo,
            )));
    }

    pub fn update_info(&self, cursor_world_pos: vec2) {
        self.network
            .send(EditorEvent::Client(EditorEventClientToServer::Info(
                ClientProps {
                    mapper_name: self.mapper_name.clone(),
                    color: self.color,
                    cursor_world: cursor_world_pos,
                    server_id: self.server_id,
                },
            )));
    }
}
