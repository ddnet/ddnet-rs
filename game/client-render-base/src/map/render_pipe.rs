use std::time::Duration;

use camera::CameraInterface;
use client_containers::{container::ContainerKey, entities::EntitiesContainer};
use game_config::config::ConfigMap;
use game_interface::types::game::NonZeroGameTickType;
use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

use super::{map_buffered::ClientMapBuffered, map_with_visual::MapVisual};

#[derive(Debug, Hiarc, Serialize, Deserialize)]
pub struct GameTimeInfo {
    pub ticks_per_second: NonZeroGameTickType,
    pub intra_tick_time: Duration,
}

#[derive(Debug)]
pub struct RenderPipelineBase<'a> {
    pub map: &'a MapVisual,
    pub config: &'a ConfigMap,
    pub cur_time: &'a Duration,
    pub cur_anim_time: &'a Duration,
    pub include_last_anim_point: bool,
    pub camera: &'a dyn CameraInterface,

    pub map_sound_volume: f64,
}

pub struct RenderPipeline<'a> {
    pub base: RenderPipelineBase<'a>,
    pub buffered_map: &'a ClientMapBuffered,
}

impl<'a> RenderPipeline<'a> {
    pub fn new(
        map: &'a MapVisual,
        buffered_map: &'a ClientMapBuffered,
        config: &'a ConfigMap,
        cur_time: &'a Duration,
        cur_anim_time: &'a Duration,
        include_last_anim_point: bool,
        camera: &'a dyn CameraInterface,
        map_sound_volume: f64,
    ) -> RenderPipeline<'a> {
        RenderPipeline {
            base: RenderPipelineBase {
                map,
                config,
                cur_time,
                cur_anim_time,
                include_last_anim_point,
                camera,
                map_sound_volume,
            },
            buffered_map,
        }
    }
}

pub struct RenderPipelinePhysics<'a> {
    pub base: &'a RenderPipelineBase<'a>,

    pub entities_container: &'a mut EntitiesContainer,
    pub entities_key: Option<&'a ContainerKey>,
    pub physics_group_name: &'a str,
}

impl<'a> RenderPipelinePhysics<'a> {
    pub fn new(
        base: &'a RenderPipelineBase<'a>,
        entities_container: &'a mut EntitiesContainer,
        entities_key: Option<&'a ContainerKey>,
        physics_group_name: &'a str,
    ) -> RenderPipelinePhysics<'a> {
        RenderPipelinePhysics {
            base,

            entities_container,
            entities_key,
            physics_group_name,
        }
    }
}
