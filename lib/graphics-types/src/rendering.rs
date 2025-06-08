use hiarc::Hiarc;
use math::math::vector::{ubvec4, vec2, vec4, vec4_base};
use serde::{Deserialize, Serialize};
use strum::EnumCount;

#[derive(Debug, Hiarc, Default, Copy, Clone, PartialEq, Eq, EnumCount, Serialize, Deserialize)]
pub enum BlendType {
    None = 0,
    #[default]
    Alpha,
    Additive,
}

#[derive(Debug, Hiarc, Default, Copy, Clone, PartialEq, Eq, EnumCount, Serialize, Deserialize)]
pub enum WrapType {
    #[default]
    Repeat = 0,
    Clamp,
}
pub const WRAP_TYPE_COUNT: usize = 2;

pub type SPoint = vec2;

#[derive(Debug, Hiarc, Default, Copy, Clone, Serialize, Deserialize)]
pub enum StateTexture {
    #[default]
    None,
    Texture(u128),
    ColorAttachmentOfPreviousPass,
    ColorAttachmentOfOffscreen(u128),
}

impl StateTexture {
    pub fn is_textured(&self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Debug, Hiarc, Default, Copy, Clone, Serialize, Deserialize)]
pub enum StateTexture2dArray {
    #[default]
    None,
    Texture(u128),
}

impl StateTexture2dArray {
    pub fn is_textured(&self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Debug, Hiarc, Copy, Clone, Serialize, Deserialize)]
pub struct StateClip {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Hiarc, Copy, Clone, Serialize, Deserialize)]
pub struct State {
    pub blend_mode: BlendType,
    pub wrap_mode: WrapType,
    pub canvas_tl: SPoint,
    pub canvas_br: SPoint,

    // clip
    pub clip: Option<StateClip>,

    // stencil mode
    pub stencil_mode: StencilMode,

    /// color mask
    pub color_mask: ColorMaskMode,
}

impl State {
    pub fn new() -> State {
        State {
            blend_mode: Default::default(),
            wrap_mode: Default::default(),
            canvas_tl: SPoint::default(),
            canvas_br: SPoint::default(),

            // clip
            clip: None,

            // stencil
            stencil_mode: Default::default(),

            color_mask: Default::default(),
        }
    }

    pub fn blend(&mut self, mode: BlendType) {
        self.blend_mode = mode;
    }

    pub fn wrap(&mut self, wrap: WrapType) {
        self.wrap_mode = wrap;
    }

    /// like [`Self::clip`] but clamps all values to the window size
    pub fn clip_clamped(
        &mut self,
        x: i32,
        y: i32,
        mut w: u32,
        mut h: u32,
        window_w: u32,
        window_h: u32,
    ) {
        if x < 0 {
            w = (w as i64 + (x as i64).max(-(w as i64))) as u32;
        }

        if y < 0 {
            h = (h as i64 + (y as i64).max(-(h as i64))) as u32;
        }

        let clip_x = x.clamp(0, window_w as i32);
        let clip_y = y.clamp(0, window_h as i32);
        self.clip = Some(StateClip {
            x: clip_x,
            y: clip_y,
            w: w.clamp(0, window_w - clip_x as u32),
            h: h.clamp(0, window_h - clip_y as u32),
        });
    }

    /// clips the current viewport (which is usually fetched with [`Graphics::window_width`] & height),
    /// where the origin is top left.
    ///
    /// "current viewport" also means that it respects the current dynamic viewport
    /// see [`Graphics::update_window_viewport`]
    pub fn clip(&mut self, x: i32, y: i32, w: u32, h: u32) {
        self.clip = Some(StateClip { x, y, w, h });
    }

    /// automatic rounding to nearest integer that can be used in [`Self::clip`]
    pub fn auto_round_clipping(x: f32, y: f32, w: f32, h: f32) -> (i32, i32, u32, u32) {
        let frac_x = x - x.round();
        let frac_y = y - y.round();

        let clip_w = (w + frac_x).round() as u32;
        let clip_h = (h + frac_y).round() as u32;

        (x.round() as i32, y.round() as i32, clip_w, clip_h)
    }

    /// like [`Self::clip`] but with automatic rounding to nearest integer
    /// see [`Self::clip`] for more information
    pub fn clip_auto_rounding(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let (clip_x, clip_y, clip_w, clip_h) = Self::auto_round_clipping(x, y, w, h);
        self.clip = Some(StateClip {
            x: clip_x,
            y: clip_y,
            w: clip_w,
            h: clip_h,
        });
    }

    pub fn map_canvas(
        &mut self,
        top_left_x: f32,
        top_left_y: f32,
        bottom_right_x: f32,
        bottom_right_y: f32,
    ) {
        self.canvas_br.x = bottom_right_x;
        self.canvas_br.y = bottom_right_y;
        self.canvas_tl.x = top_left_x;
        self.canvas_tl.y = top_left_y;
    }

    pub fn get_canvas_mapping(&self) -> (f32, f32, f32, f32) {
        (
            self.canvas_tl.x,
            self.canvas_tl.y,
            self.canvas_br.x,
            self.canvas_br.y,
        )
    }

    pub fn get_canvas_width(&self) -> f32 {
        self.canvas_br.x - self.canvas_tl.x
    }

    pub fn get_canvas_height(&self) -> f32 {
        self.canvas_br.y - self.canvas_tl.y
    }

    pub fn set_stencil_mode(&mut self, stencil_mode: StencilMode) {
        self.stencil_mode = stencil_mode;
    }

    pub fn set_color_mask(&mut self, color_mask: ColorMaskMode) {
        self.color_mask = color_mask;
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Hiarc, Default, Clone, Copy)]
pub enum RenderMode {
    #[default]
    Standard,
    // a special mode to implement a transition effect (blur)
    Blur {
        blur_radius: f32,
        scale: vec2,
        /// with blur, alpha component of this color determines
        /// how much of the color is used. it's not related to
        /// transparency
        blur_color: vec4,
    },
}

#[derive(Debug, Hiarc, Default, Clone, Copy, Serialize, Deserialize)]
pub enum StencilMode {
    #[default]
    None,
    /// fill stencil buffer
    FillStencil,
    /// render where stencil buffer did not pass
    StencilNotPassed {
        /// if true, basically render the previous pass as is
        clear_stencil: bool,
    },
    StencilPassed,
}

#[derive(Debug, Hiarc, Default, Clone, Copy, Serialize, Deserialize)]
pub enum ColorMaskMode {
    #[default]
    WriteAll,
    /// Color only
    WriteColorOnly,
    /// Alpha only
    WriteAlphaOnly,
    /// Write nothing to the framebuffer at all
    WriteNone,
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone, Serialize, Deserialize, Hiarc)]
pub struct ColorRgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl ColorRgba {
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

impl From<ubvec4> for ColorRgba {
    fn from(value: ubvec4) -> Self {
        Self::new(
            value.r() as f32 / 255.0,
            value.g() as f32 / 255.0,
            value.b() as f32 / 255.0,
            value.a() as f32 / 255.0,
        )
    }
}

pub type GlColorf = ColorRgba;
//use normalized color values
pub type GlColor = vec4_base<u8>;

pub type GlPoint = vec2;
pub type GlTexCoord = vec2;

#[repr(C)]
#[derive(Debug, Hiarc, Copy, Clone, Default, Serialize, Deserialize)]
pub struct GlVertex {
    pub pos: GlPoint,
    pub tex: GlTexCoord,
    pub color: GlColor,
}

impl GlVertex {
    pub fn append_to_bytes_vec(&self, bytes_vec: &mut Vec<u8>) {
        let bytes = self.pos.x.to_ne_bytes();
        bytes.iter().for_each(|byte| {
            bytes_vec.push(*byte);
        });
        let bytes = self.pos.y.to_ne_bytes();
        bytes.iter().for_each(|byte| {
            bytes_vec.push(*byte);
        });
        let bytes = self.tex.x.to_ne_bytes();
        bytes.iter().for_each(|byte| {
            bytes_vec.push(*byte);
        });
        let bytes = self.tex.y.to_ne_bytes();
        bytes.iter().for_each(|byte| {
            bytes_vec.push(*byte);
        });
        bytes_vec.push(self.color.r());
        bytes_vec.push(self.color.g());
        bytes_vec.push(self.color.b());
        bytes_vec.push(self.color.a());
    }

    pub fn set_pos(&mut self, pos: &GlPoint) {
        self.pos = *pos;
    }
    pub fn get_pos(&self) -> &GlPoint {
        &self.pos
    }
    pub fn set_tex_coords(&mut self, coords: &GlTexCoord) {
        self.tex = *coords;
    }
    pub fn set_color(&mut self, color: &GlColor) {
        self.color = *color;
    }
}
