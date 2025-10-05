/*******************************
 * UNIFORM PUSH CONSTANT LAYOUTS
 ********************************/

use graphics_types::rendering::ColorRgba;
use math::math::vector::{vec2, vec4};

#[derive(Default)]
#[repr(C)]
pub struct UniformGPos {
    pub pos: [f32; 4 * 2],
}

#[derive(Default)]
#[repr(C)]
pub struct UniformGBlur {
    pub texture_size: vec2,
    pub scale: vec2,
    pub color: vec4,
    pub blur_radius: f32,
}

#[derive(Default)]
#[repr(C)]
pub struct UniformGGlass {
    pub center: vec2,
    pub size: vec2,

    pub elipse_strength: f32,
    pub exponent_offset: f32,
    pub decay_scale: f32,
    pub base_factor: f32,
    pub deca_rate: f32,
    pub refraction_falloff: f32,
    pub noise: f32,
    pub glow_weight: f32,
    pub glow_bias: f32,
    pub glow_edge0: f32,
    pub glow_edge1: f32,
}

#[derive(Default)]
#[repr(C)]
pub struct UniformPrimExGPosRotationless {
    pub pos: [f32; 4 * 2],
}

#[derive(Default)]
#[repr(C)]
pub struct UniformPrimExGPos {
    pub base: UniformPrimExGPosRotationless,
    pub center: vec2,
    pub rotation: f32,
}

pub type SUniformPrimExGVertColor = ColorRgba;

#[derive(Default)]
#[repr(C)]
pub struct UniformPrimExGVertColorAlign {
    pub pad: [f32; (48 - 44) / 4],
}

#[derive(Default)]
#[repr(C)]
pub struct UniformSpriteMultiGPos {
    pub pos: [f32; 4 * 2],
    pub center: vec2,
}

pub type SUniformSpriteMultiGVertColor = ColorRgba;

#[derive(Default)]
#[repr(C)]
pub struct UniformSpriteMultiGVertColorAlign {
    pub pad: [f32; (48 - 40) / 4],
}
