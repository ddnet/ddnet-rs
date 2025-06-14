use std::{borrow::Borrow, time::Duration};

use camera::{Camera, CameraInterface};
use hiarc::Hiarc;
use map::{
    map::groups::{layers::design::SoundShape, MapGroupAttr},
    skeleton::{
        animations::AnimationsSkeleton, groups::layers::design::MapLayerSoundSkeleton,
        resources::MapResourceRefSkeleton,
    },
};
use math::math::{
    length,
    vector::{fvec2, nffixed, vec2},
    PI,
};
use sound::{
    sound_object::SoundObject,
    types::{SoundPlayBasePos, SoundPlayBaseProps, SoundPlayProps},
};

use super::{
    map::RenderMap,
    map_buffered::{ClientMapBuffered, MapSoundProcessInfo, SoundLayerSounds},
    map_with_visual::{MapVisual, MapVisualLayer},
};

#[derive(Debug, Clone, Copy)]
enum SoundLayerType {
    Background,
    Foreground,
}

/// Similar to [`crate::map::map::RenderMap`], but for map sound
#[derive(Debug, Hiarc)]
pub struct MapSoundProcess {}

impl Default for MapSoundProcess {
    fn default() -> Self {
        Self::new()
    }
}

impl MapSoundProcess {
    pub fn new() -> Self {
        Self {}
    }

    /// `None`, if there is no interaction
    pub fn camera_sound_interaction(
        pos: &vec2,
        snd_pos: &fvec2,
        rot: f32,
        snd_shape: &SoundShape,
        falloff: nffixed,
    ) -> Option<(vec2, f32)> {
        let sound_pos = vec2::new(snd_pos.x.to_num(), snd_pos.y.to_num());

        // rotate the pos around the center using the negative rotation value
        // this has the same effect as rotating the sound source
        fn rotate(snd_pos: &vec2, rotation: f32, pos: &vec2) -> vec2 {
            let c = rotation.cos();
            let s = rotation.sin();

            let x = pos.x - snd_pos.x;
            let y = pos.y - snd_pos.y;
            vec2 {
                x: x * c - y * s + snd_pos.x,
                y: x * s + y * c + snd_pos.y,
            }
        }
        let pos = rotate(&sound_pos, rot, pos);

        let diff = pos - sound_pos;
        match snd_shape {
            SoundShape::Rect { size } => {
                let w: f32 = size.x.to_num();
                let h: f32 = size.y.to_num();

                let abs_x = diff.x.abs();
                let abs_y = diff.y.abs();
                if abs_x < w / 2.0 && abs_y < h / 2.0 {
                    // falloff
                    let fx = falloff.to_num::<f32>() * w;
                    let fy = falloff.to_num::<f32>() * h;

                    let falloff_x = if abs_x > fx {
                        (w - abs_x) / (w - fx)
                    } else {
                        1.0
                    };
                    let falloff_y = if abs_y > fy {
                        (h - abs_y) / (h - fy)
                    } else {
                        1.0
                    };

                    Some((vec2::new(falloff_x, falloff_y), (diff.x + (w / 2.0)) / w))
                } else {
                    None
                }
            }
            SoundShape::Circle { radius } => {
                let dist = length(&diff);

                let r = radius.to_num::<f32>();
                if dist < r {
                    let f = falloff.to_num::<f32>() * r;

                    let falloff = if dist > f { (r - dist) / (r - f) } else { 1.0 };

                    Some((vec2::new(falloff, falloff), (diff.x + r) / (r * 2.0)))
                } else {
                    None
                }
            }
        }
    }

    pub fn handle_sound_layer<S, AN, AS>(
        &self,
        animations: &AnimationsSkeleton<AN, AS>,
        cur_time: &Duration,
        cur_anim_time: &Duration,
        include_last_anim_point: bool,
        sounds: &[MapResourceRefSkeleton<impl Borrow<SoundObject>>],
        group_attr: &MapGroupAttr,
        layer: &MapLayerSoundSkeleton<S>,
        camera: &dyn CameraInterface,
        map_sound_volume: f64,
    ) where
        S: Borrow<SoundLayerSounds>,
    {
        if let Some(sound_index) = layer.layer.attr.sound {
            let sound_object: &SoundObject = sounds[sound_index].user.borrow();
            for (index, sound) in layer.layer.sounds.iter().enumerate() {
                let mut pos = sound.pos;
                let mut rot = 0.0;
                if let Some(anim) = {
                    if let Some(pos_anim) = sound.pos_anim {
                        animations.pos.get(pos_anim)
                    } else {
                        None
                    }
                } {
                    let pos_channels = RenderMap::animation_eval(
                        &anim.def,
                        cur_time,
                        cur_anim_time,
                        &sound.pos_anim_offset,
                        include_last_anim_point,
                    );
                    pos.x += pos_channels.x;
                    pos.y += pos_channels.y;
                    rot = pos_channels.z.to_num::<f32>() / 180.0 * PI;
                }
                let mut volume = 1.0;
                if let Some(anim) = {
                    if let Some(sound_anim) = sound.sound_anim {
                        animations.sound.get(sound_anim)
                    } else {
                        None
                    }
                } {
                    let sound_volume = RenderMap::animation_eval(
                        &anim.def,
                        cur_time,
                        cur_anim_time,
                        &sound.sound_anim_offset,
                        include_last_anim_point,
                    );
                    volume *= sound_volume.x.to_num::<f64>();
                }

                let interact = Self::camera_sound_interaction(
                    &Camera::pos_to_group(camera.pos(), Some(group_attr)),
                    &pos,
                    rot,
                    &sound.shape,
                    sound.falloff,
                );
                // check if the sound should play, else play or update
                let sounds: &SoundLayerSounds = layer.user.borrow();
                if let Some((falloff, panning)) = interact {
                    let panning = if sound.panning { panning } else { 0.5 };

                    let base_props = SoundPlayBaseProps {
                        pos: SoundPlayBasePos::Global,
                        looped: sound.looped,
                        volume: volume * falloff.x.max(falloff.y) as f64 * map_sound_volume,
                        panning: panning as f64,
                        playback_speed: 1.0,
                    };
                    if !sounds.is_playing(index) {
                        sounds.play(
                            index,
                            sound_object.play(SoundPlayProps {
                                base: base_props,
                                start_time_delay: sound.time_delay,
                                min_distance: 1.0,
                                max_distance: 50.0,
                                pow_attenuation_value: None,
                                spatial: false,
                            }),
                        );
                    } else {
                        // update
                        sounds.resume(index);
                        sounds.update(index, base_props);
                    }
                }
                // check if the sound is playing, but should not
                if interact.is_none() && sounds.is_playing(index) {
                    sounds.pause(index);
                }
            }
        }
    }

    fn handle_impl<'a>(
        &self,
        cur_time: &Duration,
        cur_anim_time: &Duration,
        include_last_anim_point: bool,
        map: &MapVisual,
        sound_layers: impl Iterator<Item = &'a MapSoundProcessInfo>,
        layer_ty: SoundLayerType,
        camera: &dyn CameraInterface,
        map_sound_volume: f64,
    ) {
        let groups = match layer_ty {
            SoundLayerType::Background => &map.groups.background,
            SoundLayerType::Foreground => &map.groups.foreground,
        };
        for sound_layer in sound_layers {
            let group = &groups[sound_layer.group_index];
            let MapVisualLayer::Sound(layer) = &group.layers[sound_layer.layer_index] else {
                panic!("layer was not a sound layer");
            };
            self.handle_sound_layer(
                &map.animations,
                cur_time,
                cur_anim_time,
                include_last_anim_point,
                &map.resources.sounds,
                &group.attr,
                layer,
                camera,
                map_sound_volume,
            );
        }
    }

    pub fn handle_background(
        &self,
        cur_time: &Duration,
        cur_anim_time: &Duration,
        include_last_anim_point: bool,
        map: &MapVisual,
        buffered_map: &ClientMapBuffered,
        camera: &dyn CameraInterface,
        map_sound_volume: f64,
    ) {
        map.user.sound_scene.stay_active();
        self.handle_impl(
            cur_time,
            cur_anim_time,
            include_last_anim_point,
            map,
            buffered_map.sound.background_sound_layers.iter(),
            SoundLayerType::Background,
            camera,
            map_sound_volume,
        )
    }
    pub fn handle_foreground(
        &self,
        cur_time: &Duration,
        cur_anim_time: &Duration,
        include_last_anim_point: bool,
        map: &MapVisual,
        buffered_map: &ClientMapBuffered,
        camera: &dyn CameraInterface,
        map_sound_volume: f64,
    ) {
        map.user.sound_scene.stay_active();
        self.handle_impl(
            cur_time,
            cur_anim_time,
            include_last_anim_point,
            map,
            buffered_map.sound.foreground_sound_layers.iter(),
            SoundLayerType::Foreground,
            camera,
            map_sound_volume,
        )
    }
}
