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
use network::network::types::NetworkClientCertCheckMode;
use sound::sound_mt::SoundMultiThreaded;

use crate::{
    action_logic::do_action,
    actions::actions::{EditorAction, EditorActionGroup},
    event::{EditorEvent, EditorEventGenerator, EditorEventOverwriteMap, EditorNetEvent},
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
}

impl EditorClient {
    pub fn new(
        sys: &System,
        server_addr: &str,
        server_info: NetworkClientCertCheckMode,
        notifications: EditorNotifications,
        server_password: String,
        local_client: bool,
    ) -> Self {
        let has_events: Arc<AtomicBool> = Default::default();
        let event_generator = Arc::new(EditorEventGenerator::new(has_events.clone()));

        let mut res = Self {
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
        };

        res.network.send(EditorEvent::Auth {
            password: server_password,
            is_local_client: local_client,
        });

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
                    EditorNetEvent::Editor(ev) => match ev {
                        EditorEvent::Action(act) => {
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
                        EditorEvent::Command(_) => {
                            // ignore
                        }
                        EditorEvent::Error(err) => {
                            self.notifications.push(EditorNotification::Error(err));
                        }
                        EditorEvent::Auth { .. } => {
                            // ignore
                        }
                        EditorEvent::Map(map) => {
                            res = Some(map);
                        }
                    },
                    EditorNetEvent::NetworkEvent(ev) => self.network.handle_network_ev(id, ev),
                }
            }
        }

        Ok(res)
    }

    pub fn execute(&mut self, action: EditorAction, group_identifier: Option<&str>) {
        self.network.send(EditorEvent::Action(EditorActionGroup {
            actions: vec![action],
            identifier: group_identifier.map(|s| s.to_string()),
        }));
    }

    pub fn execute_group(&mut self, action_group: EditorActionGroup) {
        self.network.send(EditorEvent::Action(action_group));
    }
}
