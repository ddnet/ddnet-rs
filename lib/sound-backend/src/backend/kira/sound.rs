use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
    time::Duration,
};

use anyhow::anyhow;
use base::linked_hash_map_view::FxLinkedHashMap;
use hiarc::Hiarc;
use kira::{
    OutputDestination,
    clock::ClockTime,
    sound::{
        PlaybackState,
        static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings},
    },
    spatial::{
        emitter::{EmitterDistances, EmitterHandle, EmitterSettings},
        scene::SpatialSceneHandle,
    },
    tween::Easing,
};
use mint::Vector3;

use sound::{
    sound_mt_types::SoundBackendMemory,
    types::{SoundPlayBasePos, SoundPlayBaseProps, SoundPlayProps},
};

use super::{
    instance::Instance,
    mem_allocator::{HashedStaticSound, MemoryAllocator},
};

#[derive(Debug, Hiarc)]
pub struct SoundHandleStopOnDrop(StaticSoundHandle);

impl Deref for SoundHandleStopOnDrop {
    type Target = StaticSoundHandle;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SoundHandleStopOnDrop {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for SoundHandleStopOnDrop {
    fn drop(&mut self) {
        // apparently outputting sound to a track
        // does not stop the sound if the handle is
        // dropped.
        self.0.stop(Default::default());
    }
}

/// Actually playing sound in the scene
#[derive(Debug, Hiarc)]
pub(super) struct SoundPlayActive {
    pub(super) handle: SoundHandleStopOnDrop,
    pub(super) emitter: Option<EmitterHandle>,
    emitter_settings: EmitterSettings,
    props: SoundPlayBaseProps,
    start_time: ClockTime,
    initial_start_delay: Duration,
}

/// Sound in the scene that is paused
#[derive(Debug, Hiarc, Clone, Copy)]
pub(super) struct SoundPlayPaused {
    paused_at: Duration,
    emitter_settings: EmitterSettings,
    props: SoundPlayBaseProps,
    start_time: ClockTime,
    initial_start_delay: Duration,
}

#[derive(Debug, Hiarc)]
pub(super) enum SoundPlay {
    Active(SoundPlayActive),
    /// Paused either bcs scene is inactive
    /// or because too many sounds are played.
    ForcePaused {
        state: SoundPlayPaused,
        clock_time: ClockTime,
    },
    Paused(SoundPlayPaused),
}

#[derive(Hiarc)]
pub(super) struct Sound {
    pub(super) sound_data: HashedStaticSound,

    pub(super) plays: FxLinkedHashMap<u128, SoundPlay>,
}

impl Sound {
    pub fn new(mem_allocator: &MemoryAllocator, mem: SoundBackendMemory) -> anyhow::Result<Self> {
        let sound_data = match mem_allocator.sound_data_from_mem(mem) {
            Ok(sound_data) => sound_data,
            Err(err) => {
                log::error!("failed to create sound: {err}");
                mem_allocator.fake_sound()
            }
        };

        Ok(Self {
            sound_data,
            plays: Default::default(),
        })
    }

    fn play_impl(
        sound_data: &StaticSoundData,
        scene: &mut SpatialSceneHandle,
        instance: &Instance,
        emitter_settings: EmitterSettings,
        props: SoundPlayBaseProps,
        start_time_delay: Duration,
        start_position: f64,
    ) -> anyhow::Result<(Option<EmitterHandle>, StaticSoundHandle)> {
        let emitter = match props.pos {
            SoundPlayBasePos::Pos(pos) => {
                let emitter_pos = Vector3 {
                    x: pos.x,
                    y: pos.y,
                    z: 0.0,
                };

                let emitter = scene.add_emitter(emitter_pos, emitter_settings)?;
                Some(emitter)
            }
            SoundPlayBasePos::Global => None,
        };

        let mut sound_settings = StaticSoundSettings::new()
            .volume(props.volume)
            .panning(props.panning)
            .playback_rate(props.playback_speed);

        if props.looped {
            sound_settings = sound_settings.loop_region(..);
        }
        sound_settings = sound_settings
            .start_time(instance.clock_time() + start_time_delay.as_millis() as u64)
            .start_position(start_position);

        match &emitter {
            Some(emitter) => {
                sound_settings =
                    sound_settings.output_destination(OutputDestination::Emitter(emitter.id()));
            }
            None => {
                sound_settings =
                    sound_settings.output_destination(OutputDestination::Track(instance.track()));
            }
        }

        let sound_data = sound_data.with_settings(sound_settings);

        let play = instance.play(sound_data)?;
        Ok((emitter, play))
    }

    pub fn play(
        &mut self,
        play_id: u128,
        scene: Option<&mut SpatialSceneHandle>,
        instance: &Instance,
        props: SoundPlayProps,
    ) -> anyhow::Result<()> {
        let emitter_settings = EmitterSettings::new()
            .distances(EmitterDistances {
                min_distance: props.min_distance,
                max_distance: props.max_distance,
            })
            .enable_spatialization(props.spatial)
            .attenuation_function(props.pow_attenuation_value.map(Easing::InPowf))
            .persist_until_sounds_finish(false);

        let res = match scene {
            Some(scene) => {
                match Self::play_impl(
                    &self.sound_data.data,
                    scene,
                    instance,
                    emitter_settings,
                    props.base,
                    props.start_time_delay,
                    0.0,
                ) {
                    Ok((emitter, play)) => self.plays.insert(
                        play_id,
                        SoundPlay::Active(SoundPlayActive {
                            handle: SoundHandleStopOnDrop(play),
                            emitter,
                            emitter_settings,
                            props: props.base,
                            start_time: instance.clock_time(),
                            initial_start_delay: props.start_time_delay,
                        }),
                    ),
                    Err(_) => self.plays.insert(
                        play_id,
                        SoundPlay::ForcePaused {
                            state: SoundPlayPaused {
                                paused_at: props.start_time_delay,
                                emitter_settings,
                                props: props.base,
                                start_time: instance.clock_time(),
                                initial_start_delay: props.start_time_delay,
                            },
                            clock_time: instance.clock_time(),
                        },
                    ),
                }
            }
            None => self.plays.insert(
                play_id,
                SoundPlay::ForcePaused {
                    state: SoundPlayPaused {
                        paused_at: props.start_time_delay,
                        emitter_settings,
                        props: props.base,
                        start_time: instance.clock_time(),
                        initial_start_delay: props.start_time_delay,
                    },
                    clock_time: instance.clock_time(),
                },
            ),
        };

        anyhow::ensure!(
            res.is_none(),
            "sound play handle with that id {play_id} already existed."
        );

        Ok(())
    }

    pub fn pause(&mut self, play_id: u128) -> anyhow::Result<()> {
        let play = self
            .plays
            .get_mut(&play_id)
            .ok_or_else(|| anyhow!("sound with id {} did not exist", play_id))?;
        match play {
            SoundPlay::Active(sound_play_active) => {
                let play_time = sound_play_active.handle.position();

                *play = SoundPlay::Paused(SoundPlayPaused {
                    paused_at: Duration::from_secs_f64(play_time),
                    emitter_settings: sound_play_active.emitter_settings,
                    props: sound_play_active.props,
                    start_time: sound_play_active.start_time,
                    initial_start_delay: sound_play_active.initial_start_delay,
                });
            }
            SoundPlay::ForcePaused { state, .. } => {
                *play = SoundPlay::Paused(*state);
            }
            SoundPlay::Paused(_) => {
                // nothing to do
            }
        }
        Ok(())
    }

    pub fn force_pause(&mut self, play_id: u128, instance: &Instance) -> anyhow::Result<()> {
        let play = self
            .plays
            .get_mut(&play_id)
            .ok_or_else(|| anyhow!("sound with id {} did not exist", play_id))?;
        match play {
            SoundPlay::Active(sound_play_active) => {
                let play_time = sound_play_active.handle.position();

                *play = SoundPlay::ForcePaused {
                    state: SoundPlayPaused {
                        paused_at: Duration::from_secs_f64(play_time),
                        emitter_settings: sound_play_active.emitter_settings,
                        props: sound_play_active.props,
                        start_time: sound_play_active.start_time,
                        initial_start_delay: sound_play_active.initial_start_delay,
                    },
                    clock_time: instance.clock_time(),
                };
            }
            SoundPlay::Paused(_) | SoundPlay::ForcePaused { .. } => {
                // nothing to do
            }
        }
        Ok(())
    }

    pub fn resume(
        &mut self,
        play_id: u128,
        scene: Option<&mut SpatialSceneHandle>,
        instance: &Instance,
    ) -> anyhow::Result<()> {
        let play = self
            .plays
            .get_mut(&play_id)
            .ok_or_else(|| anyhow!("sound with id {} did not exist", play_id))?;
        let clock_time = if let SoundPlay::ForcePaused { clock_time, .. } = play {
            Some(*clock_time)
        } else {
            None
        };
        match play {
            SoundPlay::Active(play) => {
                play.handle.resume(Default::default());
            }
            SoundPlay::ForcePaused {
                state: sound_play_paused,
                ..
            }
            | SoundPlay::Paused(sound_play_paused) => match scene {
                Some(scene) => {
                    let clock_time_now = Duration::from_millis(instance.clock_time().ticks);
                    let start_time = Duration::from_millis(sound_play_paused.start_time.ticks);
                    let remaining_start_delay = sound_play_paused
                        .initial_start_delay
                        .saturating_sub(clock_time_now.saturating_sub(start_time));
                    match Self::play_impl(
                        &self.sound_data.data,
                        scene,
                        instance,
                        sound_play_paused.emitter_settings,
                        sound_play_paused.props,
                        remaining_start_delay,
                        sound_play_paused.paused_at.as_secs_f64(),
                    ) {
                        Ok((emitter, handle)) => {
                            *play = SoundPlay::Active(SoundPlayActive {
                                handle: SoundHandleStopOnDrop(handle),
                                emitter,
                                emitter_settings: sound_play_paused.emitter_settings,
                                props: sound_play_paused.props,
                                start_time: sound_play_paused.start_time,
                                initial_start_delay: sound_play_paused.initial_start_delay,
                            });
                        }
                        Err(_) => {
                            *play = SoundPlay::ForcePaused {
                                state: *sound_play_paused,
                                clock_time: instance.clock_time(),
                            }
                        }
                    }
                }
                None => {
                    *play = SoundPlay::ForcePaused {
                        state: *sound_play_paused,
                        clock_time: clock_time.unwrap_or_else(|| instance.clock_time()),
                    }
                }
            },
        }
        Ok(())
    }

    pub fn update(
        &mut self,
        play_id: u128,
        props: SoundPlayBaseProps,
        scene: Option<&mut SpatialSceneHandle>,
        instance: &Instance,
    ) -> anyhow::Result<()> {
        let play = self
            .plays
            .get_mut(&play_id)
            .ok_or_else(|| anyhow!("sound with id {} did not exist", play_id))?;

        let new_props = props;

        match play {
            SoundPlay::Active(SoundPlayActive { props, .. })
            | SoundPlay::ForcePaused {
                state: SoundPlayPaused { props, .. },
                ..
            }
            | SoundPlay::Paused(SoundPlayPaused { props, .. }) => {
                *props = new_props;
            }
        }

        if let SoundPlay::Active(active_play) = play {
            match props.pos {
                SoundPlayBasePos::Pos(pos) => {
                    let new_emitter_pos = Vector3 {
                        x: pos.x,
                        y: pos.y,
                        z: 0.0,
                    };
                    match &mut active_play.emitter {
                        Some(emitter) => {
                            emitter.set_position(new_emitter_pos, Default::default());
                        }
                        None => {
                            // reinitialize sound as global sound
                            *play = SoundPlay::ForcePaused {
                                state: SoundPlayPaused {
                                    paused_at: Duration::from_secs_f64(
                                        active_play.handle.position(),
                                    ),
                                    emitter_settings: active_play.emitter_settings,
                                    props: active_play.props,
                                    start_time: active_play.start_time,
                                    initial_start_delay: active_play.initial_start_delay,
                                },
                                clock_time: instance.clock_time(),
                            };
                            self.resume(play_id, scene, instance)?;
                            return Ok(());
                        }
                    }
                }
                SoundPlayBasePos::Global => {
                    match &active_play.emitter {
                        Some(_) => {
                            // reinitialize sound as non-global sound
                            *play = SoundPlay::ForcePaused {
                                state: SoundPlayPaused {
                                    paused_at: Duration::from_secs_f64(
                                        active_play.handle.position(),
                                    ),
                                    emitter_settings: active_play.emitter_settings,
                                    props: active_play.props,
                                    start_time: active_play.start_time,
                                    initial_start_delay: active_play.initial_start_delay,
                                },
                                clock_time: instance.clock_time(),
                            };
                            self.resume(play_id, scene, instance)?;
                            return Ok(());
                        }
                        None => {
                            // nothing to do
                        }
                    }
                }
            }

            active_play
                .handle
                .set_volume(new_props.volume, Default::default());
            active_play
                .handle
                .set_panning(new_props.panning, Default::default());
            active_play
                .handle
                .set_playback_rate(new_props.playback_speed, Default::default());
            if new_props.looped {
                active_play.handle.set_loop_region(..);
            } else {
                active_play.handle.set_loop_region(None);
            }
        }

        Ok(())
    }

    pub fn state(&self, play_id: u128, instance: &Instance) -> anyhow::Result<PlaybackState> {
        let play = self
            .plays
            .get(&play_id)
            .ok_or_else(|| anyhow!("sound play with id {} did not exist", play_id))?;
        match play {
            SoundPlay::Active(play) => Ok(play.handle.state()),
            SoundPlay::ForcePaused { state, clock_time } => {
                let len = self.sound_data.data.duration();
                let clock_time = Duration::from_millis(clock_time.ticks);
                let click_time =
                    Duration::from_millis(instance.clock_time().ticks).saturating_sub(clock_time);
                if len.saturating_sub(state.paused_at) > click_time {
                    Ok(PlaybackState::Playing)
                } else {
                    Ok(PlaybackState::Stopped)
                }
            }
            SoundPlay::Paused(_) => Ok(PlaybackState::Paused),
        }
    }
}

impl Debug for Sound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sound").finish()
    }
}
