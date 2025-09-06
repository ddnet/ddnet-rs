mod stream_decoder;

use std::{fmt::Debug, sync::Arc};

use hiarc::Hiarc;
use kira::{
    OutputDestination,
    sound::streaming::{StreamingSoundData, StreamingSoundHandle, StreamingSoundSettings},
    spatial::{
        emitter::{EmitterDistances, EmitterHandle, EmitterSettings},
        scene::SpatialSceneHandle,
    },
    tween::Easing,
};
use mint::Vector3;

use sound::{
    stream::{self},
    types::{StreamPlayBaseProps, StreamPlayProps},
};
use stream_decoder::StreamDecoder;

use super::instance::Instance;

#[derive(Hiarc)]
pub struct StreamActive {
    pub(super) stream_handle: StreamingSoundHandle<anyhow::Error>,
    pub(super) emitter: EmitterHandle,
}

impl Debug for StreamActive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamActive").finish()
    }
}

#[derive(Debug, Hiarc)]
pub(super) enum StreamState {
    Active(StreamActive),
    ForcePaused,
    Paused,
}

#[derive(Debug, Hiarc)]
pub(super) struct Stream {
    state: StreamState,
    pub(super) emitter_settings: EmitterSettings,
    pub(super) props: StreamPlayBaseProps,
    #[hiarc_skip_unsafe]
    decoder: Arc<dyn stream::StreamDecoder>,
}

impl Stream {
    pub fn new(
        scene: Option<&mut SpatialSceneHandle>,
        instance: &Instance,
        decoder: Arc<dyn stream::StreamDecoder>,
        props: StreamPlayProps,
    ) -> anyhow::Result<Self> {
        let emitter_settings = EmitterSettings::new()
            .distances(EmitterDistances {
                min_distance: props.min_distance,
                max_distance: props.max_distance,
            })
            .enable_spatialization(props.spatial)
            .attenuation_function(props.pow_attenuation_value.map(Easing::InPowf))
            .persist_until_sounds_finish(false);

        let state = match scene {
            Some(scene) => {
                match Self::play_impl(
                    scene,
                    instance,
                    props.base,
                    emitter_settings,
                    decoder.clone(),
                ) {
                    Ok((emitter, stream_handle)) => StreamState::Active(StreamActive {
                        stream_handle,
                        emitter,
                    }),
                    Err(_) => StreamState::ForcePaused,
                }
            }
            None => StreamState::ForcePaused,
        };

        Ok(Self {
            state,
            props: props.base,
            emitter_settings,
            decoder,
        })
    }

    fn play_impl(
        scene: &mut SpatialSceneHandle,
        instance: &Instance,
        props: StreamPlayBaseProps,
        emitter_settings: EmitterSettings,
        decoder: Arc<dyn stream::StreamDecoder>,
    ) -> anyhow::Result<(EmitterHandle, StreamingSoundHandle<anyhow::Error>)> {
        let emitter = scene.add_emitter(
            Vector3 {
                x: props.pos.x,
                y: props.pos.y,
                z: 0.0,
            },
            emitter_settings,
        )?;

        let sound_settings = StreamingSoundSettings::new()
            .start_time(instance.clock_time())
            .volume(props.volume)
            .panning(props.panning)
            .output_destination(OutputDestination::Emitter(emitter.id()));

        let sound_data = StreamingSoundData::from_decoder(StreamDecoder { decoder });

        let sound_data = sound_data.with_settings(sound_settings);
        let stream_handle = instance.play_stream(sound_data)?;
        Ok((emitter, stream_handle))
    }

    pub fn pause(&mut self) {
        match &self.state {
            StreamState::Active(_) | StreamState::ForcePaused => {
                self.state = StreamState::Paused;
            }
            StreamState::Paused => {
                // nothing to do
            }
        }
    }

    pub fn force_pause(&mut self) {
        match &self.state {
            StreamState::Active(_) => {
                self.state = StreamState::ForcePaused;
            }
            StreamState::ForcePaused | StreamState::Paused => {
                // nothing to do
            }
        }
    }

    pub fn resume(&mut self, scene: Option<&mut SpatialSceneHandle>, instance: &Instance) {
        match self.state {
            StreamState::Active(_) => {
                // nothing to do
            }
            StreamState::ForcePaused | StreamState::Paused => {
                // resume
                match scene {
                    Some(scene) => {
                        match Self::play_impl(
                            scene,
                            instance,
                            self.props,
                            self.emitter_settings,
                            self.decoder.clone(),
                        ) {
                            Ok((emitter, stream_handle)) => {
                                self.state = StreamState::Active(StreamActive {
                                    stream_handle,
                                    emitter,
                                });
                            }
                            Err(_) => {
                                self.state = StreamState::ForcePaused;
                            }
                        }
                    }
                    None => {
                        self.state = StreamState::ForcePaused;
                    }
                }
            }
        }
    }

    pub fn update(&mut self, props: StreamPlayBaseProps) {
        self.props = props;
        match &mut self.state {
            StreamState::Active(stream_active) => {
                stream_active.emitter.set_position(
                    Vector3 {
                        x: props.pos.x,
                        y: props.pos.y,
                        z: 0.0,
                    },
                    Default::default(),
                );
                stream_active
                    .stream_handle
                    .set_volume(props.volume, Default::default());

                stream_active
                    .stream_handle
                    .set_panning(props.panning, Default::default());
            }
            StreamState::ForcePaused | StreamState::Paused => {
                // nothing to do
            }
        }
    }
}
