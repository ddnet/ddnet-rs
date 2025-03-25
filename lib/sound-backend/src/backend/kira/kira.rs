use std::{
    collections::HashMap,
    fmt::Debug,
    sync::{Arc, RwLock},
};

use anyhow::anyhow;
use base::linked_hash_map_view::{FxLinkedHashMap, FxLinkedHashSet};
use config::config::ConfigSoundLimits;
use hiarc::Hiarc;
use kira::sound::PlaybackState;

use sound::{
    backend_types::{SoundBackendDriverInterface, SoundBackendMtDriverInterface},
    commands::{
        SceneAirMode, SoundCommand, SoundCommandPlay, SoundCommandSoundListener,
        SoundCommandSoundObject, SoundCommandSoundScene, SoundCommandState, SoundCommandStream,
        SoundCommandStreamObject,
    },
    frame_fetcher_plugin::BackendFrameFetcher,
    sound_mt_types::SoundBackendMemory,
};

use crate::backend::kira::instance::InstanceOffAirProps;

use super::{
    instance::Instance, listener::Listener, mem_allocator::MemoryAllocator, scene::Scene,
    sound::Sound, stream::Stream,
};

#[derive(Hiarc)]
pub struct SoundBackendKira {
    main_instance: Instance,
    #[hiarc_skip_unsafe]
    frame_fetchers: Arc<RwLock<HashMap<String, Arc<dyn BackendFrameFetcher>>>>,
    mem_allocator: MemoryAllocator,

    scenes: FxLinkedHashMap<u128, Scene>,

    detatched_sound_plays: FxLinkedHashSet<(u128, u128, u128)>,

    config: ConfigSoundLimits,

    /// strictly monotonic increasing frame id
    cur_sound_frame: u64,
}

impl Debug for SoundBackendKira {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoundBackendKira").finish()
    }
}

impl SoundBackendKira {
    pub fn new(config: ConfigSoundLimits) -> anyhow::Result<Box<Self>> {
        let frame_fetchers: Arc<RwLock<HashMap<String, Arc<dyn BackendFrameFetcher>>>> =
            Default::default();
        let main_instance = Instance::new(frame_fetchers.clone(), None, config)?;
        Ok(Box::new(Self {
            main_instance,
            frame_fetchers,
            mem_allocator: MemoryAllocator::default(),

            scenes: Default::default(),

            detatched_sound_plays: Default::default(),

            config,

            cur_sound_frame: 0,
        }))
    }
}

impl SoundBackendKira {
    fn stop_detatched_sound_if(
        scenes: &mut FxLinkedHashMap<u128, Scene>,
        scene_id: &u128,
        sound_id: &u128,
        play_id: &u128,
        at_state: impl FnOnce(PlaybackState) -> bool,
    ) -> anyhow::Result<bool> {
        let scene = scenes
            .get_mut(scene_id)
            .ok_or_else(|| anyhow!("scene with id {} does not exist", scene_id))?;
        let sound = scene
            .sounds
            .get_mut(sound_id)
            .ok_or_else(|| anyhow!("sound with id {} does not exist.", sound_id))?;

        let state = sound.state(*play_id, &scene.instance)?;

        if at_state(state) {
            sound.plays.remove(play_id);
            anyhow::Ok(true)
        } else {
            anyhow::Ok(false)
        }
    }

    pub fn update(&mut self) -> anyhow::Result<()> {
        self.detatched_sound_plays
            .retain_with_order(
                |(scene_id, sound_id, play_id)| match Self::stop_detatched_sound_if(
                    &mut self.scenes,
                    scene_id,
                    sound_id,
                    play_id,
                    |state| state == PlaybackState::Stopped,
                ) {
                    Ok(remove) => !remove,
                    Err(_) => false,
                },
            );
        Ok(())
    }

    pub fn get_backend_mt(&self) -> SoundBackendMtKira {
        SoundBackendMtKira {
            mem_allocator: self.mem_allocator.clone(),
        }
    }
}

impl SoundBackendDriverInterface for SoundBackendKira {
    fn run_cmds(&mut self, cmds: Vec<SoundCommand>) -> anyhow::Result<()> {
        // update every frame
        self.update()?;

        for cmd in cmds {
            match cmd {
                SoundCommand::State(cmd) => match cmd {
                    SoundCommandState::SoundScene(cmd) => match cmd {
                        SoundCommandSoundScene::Create { id, props } => {
                            let res = self
                                .scenes
                                .insert(
                                    id,
                                    Scene::new(
                                        match props.air_mode {
                                            SceneAirMode::OnAir => self.main_instance.clone(),
                                            SceneAirMode::OffAir { id, sample_rate } => {
                                                Instance::new(
                                                    self.frame_fetchers.clone(),
                                                    Some(InstanceOffAirProps { id, sample_rate }),
                                                    self.config,
                                                )?
                                            }
                                        },
                                        self.cur_sound_frame,
                                    )?,
                                )
                                .is_none();
                            anyhow::ensure!(res, "scene with that id {id} already existed");
                        }
                        SoundCommandSoundScene::Destroy { id } => {
                            self.scenes
                                .remove(&id)
                                .ok_or_else(|| anyhow!("scene not found."))?;
                        }
                        SoundCommandSoundScene::StayActive { id } => {
                            // update the sound frame of this id and move scene to end
                            // by moving it to the end later in the [`SoundCommandState::Swap`]
                            // it only needs to check the front scenes
                            let scene = self
                                .scenes
                                .get_mut(&id)
                                .ok_or_else(|| anyhow!("scene not found."))?;
                            scene.last_active_sound_frame = self.cur_sound_frame;

                            scene.resume()?;
                            self.scenes.to_back(&id);
                        }
                        SoundCommandSoundScene::StopDetatchedSounds { id } => {
                            self.detatched_sound_plays.retain_with_order(
                                |(scene_id, sound_id, play_id)| {
                                    if scene_id.ne(&id) {
                                        return true;
                                    }

                                    if let Err(err) = Self::stop_detatched_sound_if(
                                        &mut self.scenes,
                                        scene_id,
                                        sound_id,
                                        play_id,
                                        |_| true,
                                    ) {
                                        log::warn!(target: "kira", "{err}");
                                    }
                                    false
                                },
                            );
                        }
                        SoundCommandSoundScene::ProcessOffAir { id, samples } => {
                            let scene = self
                                .scenes
                                .get_mut(&id)
                                .ok_or_else(|| anyhow!("scene did not exist"))?;

                            scene.instance.process_samples(samples)?;
                        }
                    },
                    SoundCommandState::SoundObject(cmd) => match cmd {
                        SoundCommandSoundObject::Create { id, scene_id, mem } => {
                            let scene = self
                                .scenes
                                .get_mut(&scene_id)
                                .ok_or_else(|| anyhow!("scene did not exist"))?;

                            let res = scene
                                .sounds
                                .insert(
                                    id,
                                    Sound::new(&self.mem_allocator, mem)
                                        .map_err(|err| anyhow!("failed to create sound: {err}"))?,
                                )
                                .is_none();
                            anyhow::ensure!(res, "sound with id {id} already existed.");
                        }
                        SoundCommandSoundObject::Destroy { id, scene_id } => {
                            let scene = self
                                .scenes
                                .get_mut(&scene_id)
                                .ok_or_else(|| anyhow!("scene did not exist"))?;
                            scene
                                .sounds
                                .remove(&id)
                                .ok_or_else(|| anyhow!("sound not found."))?;
                        }
                    },
                    SoundCommandState::SoundListener(cmd) => match cmd {
                        SoundCommandSoundListener::Create { id, scene_id, pos } => {
                            let scene = self
                                .scenes
                                .get_mut(&scene_id)
                                .ok_or_else(|| anyhow!("scene did not exist"))?;

                            let res = scene
                                .listeners
                                .insert(
                                    id,
                                    Listener::new(
                                        &mut scene.instance,
                                        scene.state.scene_handle(),
                                        pos,
                                    ),
                                )
                                .is_none();
                            anyhow::ensure!(res, "listener with id {id} already existed.");
                        }
                        SoundCommandSoundListener::Update { id, scene_id, pos } => {
                            let scene = self
                                .scenes
                                .get_mut(&scene_id)
                                .ok_or_else(|| anyhow!("scene did not exist"))?;

                            scene
                                .listeners
                                .get_mut(&id)
                                .ok_or_else(|| anyhow!("listener with id {id} did not exist"))?
                                .update(pos);
                        }
                        SoundCommandSoundListener::Destroy { id, scene_id } => {
                            let scene = self
                                .scenes
                                .get_mut(&scene_id)
                                .ok_or_else(|| anyhow!("scene did not exist"))?;
                            scene
                                .listeners
                                .remove(&id)
                                .ok_or_else(|| anyhow!("listener not found."))?;
                        }
                    },
                    SoundCommandState::StreamObject(cmd) => match cmd {
                        SoundCommandStreamObject::Create {
                            id,
                            scene_id,
                            stream,
                            props,
                        } => {
                            let scene = self
                                .scenes
                                .get_mut(&scene_id)
                                .ok_or_else(|| anyhow!("scene did not exist"))?;

                            let res = scene
                                .streams
                                .insert(
                                    id,
                                    Stream::new(
                                        scene.state.scene_handle(),
                                        &scene.instance,
                                        stream.0,
                                        props,
                                    )
                                    .map_err(|err| anyhow!("failed to create stream: {err}"))?,
                                )
                                .is_none();
                            anyhow::ensure!(res, "stream with id {id} already existed.");
                        }
                        SoundCommandStreamObject::Destroy { id, scene_id } => {
                            let scene = self
                                .scenes
                                .get_mut(&scene_id)
                                .ok_or_else(|| anyhow!("scene did not exist"))?;
                            scene
                                .streams
                                .remove(&id)
                                .ok_or_else(|| anyhow!("stream not found."))?;
                        }
                    },
                    SoundCommandState::Swap => {
                        // check which scenes were inactive and pause those
                        for scene in self.scenes.values_mut() {
                            if scene.is_onair()
                                && scene.last_active_sound_frame < self.cur_sound_frame
                            {
                                scene.pause()?;
                            } else {
                                break;
                            }
                        }

                        self.cur_sound_frame += 1;
                    }
                },
                SoundCommand::Play(cmd) => match cmd {
                    SoundCommandPlay::Play {
                        play_id,
                        sound_id,
                        scene_id,
                        props,
                    } => {
                        let scene = self
                            .scenes
                            .get_mut(&scene_id)
                            .ok_or_else(|| anyhow!("scene did not exist"))?;
                        let sound = scene
                            .sounds
                            .get_mut(&sound_id)
                            .ok_or_else(|| anyhow!("sound does not exist."))?;
                        sound.play(play_id, scene.state.scene_handle(), &scene.instance, props)?;
                    }
                    SoundCommandPlay::Update {
                        play_id,
                        sound_id,
                        scene_id,
                        props,
                    } => {
                        let scene = self
                            .scenes
                            .get_mut(&scene_id)
                            .ok_or_else(|| anyhow!("scene did not exist"))?;
                        let sound = scene
                            .sounds
                            .get_mut(&sound_id)
                            .ok_or_else(|| anyhow!("sound does not exist."))?;

                        sound.update(
                            play_id,
                            props,
                            scene.state.scene_handle(),
                            &scene.instance,
                        )?;
                    }
                    SoundCommandPlay::Pause {
                        play_id,
                        sound_id,
                        scene_id,
                    } => {
                        let scene = self
                            .scenes
                            .get_mut(&scene_id)
                            .ok_or_else(|| anyhow!("scene did not exist"))?;
                        let sound = scene
                            .sounds
                            .get_mut(&sound_id)
                            .ok_or_else(|| anyhow!("sound does not exist."))?;
                        sound.pause(play_id)?;
                    }
                    SoundCommandPlay::Resume {
                        play_id,
                        sound_id,
                        scene_id,
                    } => {
                        let scene = self
                            .scenes
                            .get_mut(&scene_id)
                            .ok_or_else(|| anyhow!("scene did not exist"))?;
                        let sound = scene
                            .sounds
                            .get_mut(&sound_id)
                            .ok_or_else(|| anyhow!("sound does not exist."))?;

                        sound.resume(play_id, scene.state.scene_handle(), &scene.instance)?;
                    }
                    SoundCommandPlay::Detatch {
                        play_id,
                        sound_id,
                        scene_id,
                    } => {
                        let res = self
                            .detatched_sound_plays
                            .insert((scene_id, sound_id, play_id));
                        anyhow::ensure!(res, "sound play with id {play_id} already existed.");
                    }
                    SoundCommandPlay::Stop {
                        play_id,
                        sound_id,
                        scene_id,
                    } => {
                        let scene = self
                            .scenes
                            .get_mut(&scene_id)
                            .ok_or_else(|| anyhow!("scene did not exist"))?;
                        let sound = scene
                            .sounds
                            .get_mut(&sound_id)
                            .ok_or_else(|| anyhow!("sound does not exist."))?;
                        sound
                            .plays
                            .remove(&play_id)
                            .ok_or_else(|| anyhow!("sound play with id {play_id} did not exist"))?;
                    }
                },
                SoundCommand::Stream(cmd) => match cmd {
                    SoundCommandStream::Update {
                        stream_id,
                        scene_id,
                        props,
                    } => {
                        let scene = self
                            .scenes
                            .get_mut(&scene_id)
                            .ok_or_else(|| anyhow!("scene did not exist"))?;

                        let stream = scene
                            .streams
                            .get_mut(&stream_id)
                            .ok_or_else(|| anyhow!("stream does not exist."))?;
                        stream.update(props);
                    }
                    SoundCommandStream::Pause {
                        stream_id,
                        scene_id,
                    } => {
                        let scene = self
                            .scenes
                            .get_mut(&scene_id)
                            .ok_or_else(|| anyhow!("scene did not exist"))?;

                        let stream = scene
                            .streams
                            .get_mut(&stream_id)
                            .ok_or_else(|| anyhow!("stream does not exist."))?;
                        stream.pause();
                    }
                    SoundCommandStream::Resume {
                        stream_id,
                        scene_id,
                    } => {
                        let scene = self
                            .scenes
                            .get_mut(&scene_id)
                            .ok_or_else(|| anyhow!("scene did not exist"))?;

                        let stream = scene
                            .streams
                            .get_mut(&stream_id)
                            .ok_or_else(|| anyhow!("stream does not exist."))?;
                        stream.resume(scene.state.scene_handle(), &scene.instance);
                    }
                },
            }
        }

        Ok(())
    }

    fn attach_frame_fetcher(&mut self, name: String, fetcher: Arc<dyn BackendFrameFetcher>) {
        self.frame_fetchers.write().unwrap().insert(name, fetcher);
    }

    fn detach_frame_fetcher(&mut self, name: String) {
        self.frame_fetchers.write().unwrap().remove(&name);
    }
}

#[derive(Debug, Hiarc)]
pub struct SoundBackendMtKira {
    mem_allocator: MemoryAllocator,
}

impl SoundBackendMtDriverInterface for SoundBackendMtKira {
    fn mem_alloc(&self, size: usize) -> SoundBackendMemory {
        self.mem_allocator.mem_alloc(size)
    }

    fn try_flush_mem(&self, mem: &mut SoundBackendMemory) -> anyhow::Result<()> {
        self.mem_allocator.try_flush_mem(mem)
    }
}
