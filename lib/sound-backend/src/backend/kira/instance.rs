use std::{
    collections::HashMap,
    fmt::Debug,
    sync::{Arc, RwLock},
};

use anyhow::anyhow;
use config::config::ConfigSoundLimits;
use hiarc::{hiarc_safer_rc_refcell, Hiarc};
use kira::{
    clock::{ClockHandle, ClockSpeed, ClockTime},
    effect::Effect,
    manager::{
        backend::{
            mock::{MockBackend, MockBackendSettings},
            DefaultBackend,
        },
        error::PlaySoundError,
        AudioManager, AudioManagerSettings, Capacities,
    },
    sound::{
        static_sound::{StaticSoundData, StaticSoundHandle},
        streaming::{StreamingSoundData, StreamingSoundHandle},
        SoundData,
    },
    spatial::scene::{SpatialSceneHandle, SpatialSceneSettings},
    track::{TrackBuilder, TrackHandle, TrackId},
    tween::Value,
    ResourceLimitReached,
};
use sound::frame_fetcher_plugin::{BackendAudioFrame, BackendFrameFetcher, FetchSoundManagerIndex};

#[derive(Debug, Hiarc)]
pub struct OnAirData {
    pub fetcher_track: TrackHandle,
}

#[derive(Debug, Hiarc)]
pub struct OffAirData {
    pub data: OnAirData,
    pub last_used_in_tick: u64,
}

#[derive(Hiarc)]
pub enum AudioManagerTy {
    OnAir(Box<AudioManager<DefaultBackend>>),
    OffAir(Box<AudioManager<MockBackend>>),
}

impl AudioManagerTy {
    pub fn add_sub_track(
        &mut self,
        builder: TrackBuilder,
    ) -> Result<TrackHandle, ResourceLimitReached> {
        match self {
            AudioManagerTy::OnAir(ty) => ty.add_sub_track(builder),
            AudioManagerTy::OffAir(ty) => ty.add_sub_track(builder),
        }
    }
    pub fn add_clock(
        &mut self,
        speed: impl Into<Value<ClockSpeed>>,
    ) -> Result<ClockHandle, ResourceLimitReached> {
        match self {
            AudioManagerTy::OnAir(ty) => ty.add_clock(speed),
            AudioManagerTy::OffAir(ty) => ty.add_clock(speed),
        }
    }
    pub fn play<D: SoundData>(
        &mut self,
        sound_data: D,
    ) -> Result<D::Handle, PlaySoundError<D::Error>> {
        match self {
            AudioManagerTy::OnAir(ty) => ty.play(sound_data),
            AudioManagerTy::OffAir(ty) => ty.play(sound_data),
        }
    }
    pub fn add_spatial_scene(
        &mut self,
        settings: SpatialSceneSettings,
    ) -> Result<SpatialSceneHandle, ResourceLimitReached> {
        match self {
            AudioManagerTy::OnAir(ty) => ty.add_spatial_scene(settings),
            AudioManagerTy::OffAir(ty) => ty.add_spatial_scene(settings),
        }
    }
}

#[derive(Hiarc)]
pub struct InstanceOffAirProps {
    pub id: u128,
    pub sample_rate: u32,
}

#[hiarc_safer_rc_refcell]
#[derive(Hiarc)]
pub struct Instance {
    manager: AudioManagerTy,
    clock_handle: ClockHandle,

    config: ConfigSoundLimits,

    main_track: TrackHandle,
}

struct OnAirTrack {
    frame_fetchers: Arc<RwLock<HashMap<String, Arc<dyn BackendFrameFetcher>>>>,
}

impl Effect for OnAirTrack {
    fn process(
        &mut self,
        input: kira::Frame,
        _dt: f64,
        _clock_info_provider: &kira::clock::clock_info::ClockInfoProvider,
        _modulator_value_provider: &kira::modulator::value_provider::ModulatorValueProvider,
    ) -> kira::Frame {
        for frame_fetcher in self.frame_fetchers.read().unwrap().values() {
            if let FetchSoundManagerIndex::Onair = frame_fetcher.current_fetch_index() {
                frame_fetcher.next_frame(BackendAudioFrame {
                    left: input.left,
                    right: input.right,
                });
            }
        }

        input
    }
}

struct OffAirTrack {
    id: u128,
    frame_fetchers: Arc<RwLock<HashMap<String, Arc<dyn BackendFrameFetcher>>>>,
}

impl Effect for OffAirTrack {
    fn process(
        &mut self,
        input: kira::Frame,
        _dt: f64,
        _clock_info_provider: &kira::clock::clock_info::ClockInfoProvider,
        _modulator_value_provider: &kira::modulator::value_provider::ModulatorValueProvider,
    ) -> kira::Frame {
        for frame_fetcher in self.frame_fetchers.read().unwrap().values() {
            if frame_fetcher.current_fetch_index() == FetchSoundManagerIndex::Offair(self.id) {
                frame_fetcher.next_frame(BackendAudioFrame {
                    left: input.left,
                    right: input.right,
                });
            }
        }

        kira::Frame {
            left: 0.0,
            right: 0.0,
        }
    }
}

#[hiarc_safer_rc_refcell]
impl Instance {
    pub fn new(
        frame_fetchers: Arc<RwLock<HashMap<String, Arc<dyn BackendFrameFetcher>>>>,
        off_air_props: Option<InstanceOffAirProps>,
        config: ConfigSoundLimits,
    ) -> anyhow::Result<Self> {
        let capacities = Capacities {
            sound_capacity: config.max_sounds,
            spatial_scene_capacity: config.max_spatial_scenes,
            ..Default::default()
        };

        let mut manager = if let Some(props) = &off_air_props {
            let settings = AudioManagerSettings::<MockBackend> {
                capacities,
                main_track_builder: Default::default(),
                backend_settings: MockBackendSettings {
                    sample_rate: props.sample_rate,
                },
            };
            AudioManagerTy::OffAir(Box::new(
                AudioManager::new(settings)
                    .map_err(|_| anyhow!("Mock backend failed to be created."))?,
            ))
        } else {
            let settings = AudioManagerSettings::<DefaultBackend> {
                capacities,
                main_track_builder: Default::default(),
                backend_settings: Default::default(),
            };
            AudioManagerTy::OnAir(Box::new(AudioManager::<DefaultBackend>::new(settings)?))
        };

        let mut track_builder = TrackBuilder::new();
        if let Some(props) = &off_air_props {
            track_builder.add_built_effect(Box::new(OffAirTrack {
                frame_fetchers,
                id: props.id,
            }));
        } else {
            track_builder.add_built_effect(Box::new(OnAirTrack { frame_fetchers }));
        }
        let fetcher_track = manager.add_sub_track(track_builder)?;

        let mut clock_handle = manager.add_clock(ClockSpeed::TicksPerMinute(1000.0 * 60.0))?;
        clock_handle.start();

        Ok(Self {
            clock_handle,
            manager,

            config,

            main_track: fetcher_track,
        })
    }

    pub fn clock_time(&self) -> ClockTime {
        self.clock_handle.time()
    }

    pub fn add_spatial_scene(&mut self) -> anyhow::Result<SpatialSceneHandle> {
        Ok(self.manager.add_spatial_scene(SpatialSceneSettings {
            emitter_capacity: self.config.max_sounds,
            listener_capacity: self.config.max_listeners,
        })?)
    }

    pub fn play(&mut self, sound_data: StaticSoundData) -> anyhow::Result<StaticSoundHandle> {
        Ok(self.manager.play(sound_data)?)
    }

    pub fn play_stream(
        &mut self,
        sound_data: StreamingSoundData<anyhow::Error>,
    ) -> anyhow::Result<StreamingSoundHandle<anyhow::Error>> {
        Ok(self.manager.play(sound_data)?)
    }

    pub fn track(&self) -> TrackId {
        self.main_track.id()
    }

    pub fn is_onair(&self) -> bool {
        matches!(self.manager, AudioManagerTy::OnAir(_))
    }

    pub fn process_samples(&mut self, samples: u32) -> anyhow::Result<Vec<kira::Frame>> {
        let AudioManagerTy::OffAir(manager) = &mut self.manager else {
            return Err(anyhow!("Not an off-air scene."));
        };

        manager.backend_mut().on_start_processing();
        Ok((0..samples)
            .map(|_| manager.backend_mut().process())
            .collect())
    }
}

impl Debug for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Instance").finish()
    }
}
