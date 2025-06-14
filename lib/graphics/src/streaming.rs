use graphics_types::{
    commands::{SColor, STexCoord},
    rendering::{BlendType, ColorMaskMode, GlVertex, RenderMode, State, StencilMode, WrapType},
};
use hiarc::Hiarc;
use math::math::vector::{vec2, vec4};

#[inline(always)]
pub fn rotate_pos(center: &vec2, rotation: f32, point: vec2) -> vec2 {
    let c = rotation.cos();
    let s = rotation.sin();

    let x = point.x - center.x;
    let y = point.y - center.y;
    vec2 {
        x: x * c - y * s + center.x,
        y: x * s + y * c + center.y,
    }
}

pub fn rotate(center: &vec2, rotation: f32, points: &mut [GlVertex]) {
    for point in points.iter_mut() {
        let pos = point.get_pos();
        point.set_pos(&rotate_pos(center, rotation, vec2::new(pos.x, pos.y)));
    }
}

#[derive(Debug, Hiarc, Clone, Copy)]
pub struct DrawScope<const VERTEX_COUNT: usize> {
    pub state: State,
    pub render_mode: RenderMode,
    pub tile_index: u8,
    pub rotation: f32,
    pub colors: [SColor; VERTEX_COUNT],
    pub texture_coords: [STexCoord; VERTEX_COUNT],
}

impl<const VERTEX_COUNT: usize> Default for DrawScope<VERTEX_COUNT> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const VERTEX_COUNT: usize> DrawScope<VERTEX_COUNT> {
    pub fn new() -> Self {
        Self {
            state: State::new(),
            render_mode: RenderMode::default(),
            colors: [(); { VERTEX_COUNT }].map(|_| SColor::default()),
            texture_coords: [(); { VERTEX_COUNT }].map(|_| STexCoord::default()),
            tile_index: Default::default(),
            rotation: Default::default(),
        }
    }

    pub fn set_colors_from_single(&mut self, r: f32, g: f32, b: f32, a: f32) {
        let red = r.clamp(0.0, 1.0) * 255.0;
        let green = g.clamp(0.0, 1.0) * 255.0;
        let blue = b.clamp(0.0, 1.0) * 255.0;
        let alpha = a.clamp(0.0, 1.0) * 255.0;

        for color in &mut self.colors {
            color.set_r(red as u8);
            color.set_g(green as u8);
            color.set_b(blue as u8);
            color.set_a(alpha as u8);
        }
    }

    pub fn set_colors(&mut self, colors: &[vec4; VERTEX_COUNT]) {
        for (index, color) in self.colors.iter_mut().enumerate() {
            let red = colors[index].r().clamp(0.0, 1.0) * 255.0;
            let green = colors[index].g().clamp(0.0, 1.0) * 255.0;
            let blue = colors[index].b().clamp(0.0, 1.0) * 255.0;
            let alpha = colors[index].a().clamp(0.0, 1.0) * 255.0;

            color.set_r(red as u8);
            color.set_g(green as u8);
            color.set_b(blue as u8);
            color.set_a(alpha as u8);
        }
    }

    pub fn set_state(&mut self, state: &State) {
        self.state = *state;
    }

    pub fn set_render_mode(&mut self, render_mode: RenderMode) {
        self.render_mode = render_mode;
    }

    pub fn set_stencil_mode(&mut self, stencil_mode: StencilMode) {
        self.state.set_stencil_mode(stencil_mode);
    }

    pub fn set_color_mask(&mut self, color_mask: ColorMaskMode) {
        self.state.set_color_mask(color_mask);
    }

    pub fn blend(&mut self, mode: BlendType) {
        self.state.blend(mode);
    }

    /// see [`State::clip`]
    pub fn clip(&mut self, x: i32, y: i32, w: u32, h: u32) {
        self.state.clip(x, y, w, h);
    }

    /// see [`State::clip_auto_rounding`]
    pub fn clip_auto_rounding(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.state.clip_auto_rounding(x, y, w, h);
    }

    pub fn wrap(&mut self, mode: WrapType) {
        self.state.wrap(mode);
    }

    pub fn map_canvas(
        &mut self,
        top_left_x: f32,
        top_left_y: f32,
        bottom_right_x: f32,
        bottom_right_y: f32,
    ) {
        self.state
            .map_canvas(top_left_x, top_left_y, bottom_right_x, bottom_right_y);
    }

    pub fn get_canvas_mapping(&self) -> (f32, f32, f32, f32) {
        self.state.get_canvas_mapping()
    }

    pub fn set_rotation(&mut self, angle: f32) {
        self.rotation = angle;
    }
}

pub fn quad_scope_begin() -> DrawScope<4> {
    DrawScope::new()
}
