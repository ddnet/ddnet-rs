use anyhow::anyhow;
use base::linked_hash_map_view::FxLinkedHashMap;
use hiarc::{Hiarc, hiarc_safer_refcell};
use sound::{
    backend_handle::SoundBackendHandle,
    commands::{
        SoundCommand, SoundCommandPlay, SoundCommandSoundListener, SoundCommandSoundObject,
        SoundCommandSoundScene, SoundCommandState,
    },
};

#[derive(Debug, Hiarc)]
struct SoundPlay {}

#[derive(Debug, Hiarc)]
struct Sound {
    plays: FxLinkedHashMap<u128, SoundPlay>,
}

#[derive(Debug, Hiarc)]
struct Listener {}

#[derive(Debug, Hiarc)]
struct Scene {
    sounds: FxLinkedHashMap<u128, Sound>,
    listeners: FxLinkedHashMap<u128, Listener>,
}

#[hiarc_safer_refcell]
#[derive(Debug, Hiarc)]
pub struct SoundCheckerApi {
    id_offset: u128,
    scenes: FxLinkedHashMap<u128, Scene>,
    backend_handle: SoundBackendHandle,
}

#[hiarc_safer_refcell]
impl SoundCheckerApi {
    pub fn new(id_offset: u128, backend_handle: SoundBackendHandle) -> Self {
        Self {
            backend_handle,
            id_offset,
            scenes: Default::default(),
        }
    }

    fn process_command(&mut self, cmd: &mut SoundCommand) -> anyhow::Result<()> {
        match cmd {
            SoundCommand::State(cmd) => {
                match cmd {
                    SoundCommandState::SoundScene(cmd) => match cmd {
                        SoundCommandSoundScene::Create { id, .. } => {
                            *id += self.id_offset;
                            anyhow::ensure!(
                                self.scenes
                                    .insert(
                                        *id,
                                        Scene {
                                            sounds: Default::default(),
                                            listeners: Default::default(),
                                        },
                                    )
                                    .is_none()
                            );
                        }
                        SoundCommandSoundScene::Destroy { id } => {
                            *id += self.id_offset;
                            anyhow::ensure!(self.scenes.remove(id).is_some());
                        }
                        SoundCommandSoundScene::StayActive { id } => {
                            *id += self.id_offset;
                            anyhow::ensure!(self.scenes.contains_key(id));
                        }
                        SoundCommandSoundScene::StopDetatchedSounds { id } => {
                            *id += self.id_offset;
                            anyhow::ensure!(self.scenes.contains_key(id));
                        }
                        SoundCommandSoundScene::ProcessOffAir { id, samples } => {
                            *id += self.id_offset;
                            anyhow::ensure!(self.scenes.contains_key(id));
                            anyhow::ensure!(*samples <= 1024 * 16);
                        }
                    },
                    SoundCommandState::SoundObject(cmd) => match cmd {
                        SoundCommandSoundObject::Create { id, scene_id, .. } => {
                            *id += self.id_offset;
                            *scene_id += self.id_offset;
                            let scene = self.scenes.get_mut(scene_id).ok_or_else(|| {
                                anyhow!("scene with id {scene_id} did not exist.")
                            })?;
                            anyhow::ensure!(
                                scene
                                    .sounds
                                    .insert(
                                        *id,
                                        Sound {
                                            plays: Default::default()
                                        }
                                    )
                                    .is_none()
                            );
                        }
                        SoundCommandSoundObject::Destroy { id, scene_id } => {
                            *id += self.id_offset;
                            *scene_id += self.id_offset;
                            let scene = self.scenes.get_mut(scene_id).ok_or_else(|| {
                                anyhow!("scene with id {scene_id} did not exist.")
                            })?;
                            anyhow::ensure!(scene.sounds.remove(id).is_some());
                        }
                    },
                    SoundCommandState::SoundListener(cmd) => match cmd {
                        SoundCommandSoundListener::Create { id, scene_id, .. } => {
                            *id += self.id_offset;
                            *scene_id += self.id_offset;
                            let scene = self.scenes.get_mut(scene_id).ok_or_else(|| {
                                anyhow!("scene with id {scene_id} did not exist.")
                            })?;
                            anyhow::ensure!(scene.listeners.insert(*id, Listener {}).is_none());
                        }
                        SoundCommandSoundListener::Update { id, scene_id, .. } => {
                            *id += self.id_offset;
                            *scene_id += self.id_offset;
                        }
                        SoundCommandSoundListener::Destroy { id, scene_id } => {
                            *id += self.id_offset;
                            *scene_id += self.id_offset;
                            let scene = self.scenes.get_mut(scene_id).ok_or_else(|| {
                                anyhow!("scene with id {scene_id} did not exist.")
                            })?;
                            anyhow::ensure!(scene.listeners.remove(id).is_some());
                        }
                    },
                    SoundCommandState::StreamObject(_) => {
                        return Err(anyhow!(
                            "Streamed sounds are not supported by this checker or WASM modules."
                        ));
                    }
                    SoundCommandState::Swap => {
                        // nothing to do
                    }
                }
            }
            SoundCommand::Play(cmd) => match cmd {
                SoundCommandPlay::Play {
                    play_id,
                    sound_id,
                    scene_id,
                    ..
                } => {
                    *play_id += self.id_offset;
                    *sound_id += self.id_offset;
                    *scene_id += self.id_offset;
                    let scene = self
                        .scenes
                        .get_mut(scene_id)
                        .ok_or_else(|| anyhow!("scene with id {scene_id} did not exist."))?;
                    let sound = scene
                        .sounds
                        .get_mut(sound_id)
                        .ok_or_else(|| anyhow!("sound with id {sound_id} did not exist."))?;
                    anyhow::ensure!(sound.plays.insert(*play_id, SoundPlay {}).is_none());
                }
                SoundCommandPlay::Update {
                    play_id,
                    sound_id,
                    scene_id,
                    ..
                }
                | SoundCommandPlay::Pause {
                    play_id,
                    sound_id,
                    scene_id,
                }
                | SoundCommandPlay::Resume {
                    play_id,
                    sound_id,
                    scene_id,
                } => {
                    *play_id += self.id_offset;
                    *sound_id += self.id_offset;
                    *scene_id += self.id_offset;
                }
                SoundCommandPlay::Stop {
                    play_id,
                    sound_id,
                    scene_id,
                }
                | SoundCommandPlay::Detatch {
                    play_id,
                    sound_id,
                    scene_id,
                } => {
                    *play_id += self.id_offset;
                    *sound_id += self.id_offset;
                    *scene_id += self.id_offset;
                    let scene = self
                        .scenes
                        .get_mut(scene_id)
                        .ok_or_else(|| anyhow!("scene with id {scene_id} did not exist."))?;
                    let sound = scene
                        .sounds
                        .get_mut(sound_id)
                        .ok_or_else(|| anyhow!("sound with id {sound_id} did not exist."))?;
                    anyhow::ensure!(sound.plays.remove(play_id).is_some());
                }
            },
            SoundCommand::Stream(_) => {
                return Err(anyhow!(
                    "Stream commands are not supported by this checker or WASM modules."
                ));
            }
        }
        Ok(())
    }

    pub fn process_commands(&mut self, cmds: &mut Vec<SoundCommand>) {
        for cmd in cmds {
            self.process_command(cmd).unwrap();
        }
    }
}

#[hiarc_safer_refcell]
impl Drop for SoundCheckerApi {
    fn drop(&mut self) {
        self.scenes.drain().for_each(|(scene_id, scene)| {
            for (sound_id, sounds) in scene.sounds {
                for (play_id, _) in sounds.plays {
                    self.backend_handle
                        .add_cmd(SoundCommand::Play(SoundCommandPlay::Stop {
                            play_id,
                            sound_id,
                            scene_id,
                        }));
                }
                self.backend_handle
                    .add_cmd(SoundCommand::State(SoundCommandState::SoundObject(
                        SoundCommandSoundObject::Destroy {
                            id: sound_id,
                            scene_id,
                        },
                    )));
            }
            for (listener_id, _) in scene.listeners {
                self.backend_handle
                    .add_cmd(SoundCommand::State(SoundCommandState::SoundListener(
                        SoundCommandSoundListener::Destroy {
                            id: listener_id,
                            scene_id,
                        },
                    )));
            }
            self.backend_handle
                .add_cmd(SoundCommand::State(SoundCommandState::SoundScene(
                    SoundCommandSoundScene::Destroy { id: scene_id },
                )));
        });
    }
}
