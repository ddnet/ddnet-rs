use graphics_types::rendering::GlVertex;
use hiarc::Hiarc;
use math::math::vector::{ubvec4, vec2};

use crate::streaming::rotate;

#[derive(Debug, Hiarc, Default, Clone, Copy)]
#[repr(C)]
pub struct Quad {
    pub vertices: [GlVertex; 4],
}

impl Quad {
    pub fn append_to_bytes_vec(&self, bytes_vec: &mut Vec<u8>) {
        self.vertices
            .iter()
            .for_each(|vert| vert.append_to_bytes_vec(bytes_vec));
    }

    /// Creates a new quad with white color and texture coordinates to match a normal rect
    pub fn new() -> Self {
        Self::default()
            .with_color(&ubvec4 {
                x: 255,
                y: 255,
                z: 255,
                w: 255,
            })
            .with_tex(&[
                vec2 { x: 0.0, y: 0.0 },
                vec2 { x: 1.0, y: 0.0 },
                vec2 { x: 1.0, y: 1.0 },
                vec2 { x: 0.0, y: 1.0 },
            ])
    }

    pub fn from_rect(mut self, x: f32, y: f32, width: f32, height: f32) -> Self {
        self.vertices[0].pos.x = x;
        self.vertices[0].pos.y = y;

        self.vertices[1].pos.x = x + width;
        self.vertices[1].pos.y = y;

        self.vertices[2].pos.x = x + width;
        self.vertices[2].pos.y = y + height;

        self.vertices[3].pos.x = x;
        self.vertices[3].pos.y = y + height;

        self
    }

    pub fn from_width_and_height_centered(self, width: f32, height: f32) -> Self {
        let x = -width / 2.0;
        let y = -height / 2.0;

        self.from_rect(x, y, width, height)
    }

    pub fn from_size_centered(self, size: f32) -> Self {
        self.from_width_and_height_centered(size, size)
    }

    pub fn with_tex(mut self, tex: &[vec2; 4]) -> Self {
        self.vertices[0].tex = tex[0];
        self.vertices[1].tex = tex[1];
        self.vertices[2].tex = tex[2];
        self.vertices[3].tex = tex[3];

        self
    }

    /// builds UV coordinates from 2 points
    pub fn with_uv_from_points(mut self, uv1: &vec2, uv2: &vec2) -> Self {
        self.vertices[0].tex = *uv1;
        self.vertices[1].tex = vec2::new(uv2.x, uv1.y);
        self.vertices[2].tex = *uv2;
        self.vertices[3].tex = vec2::new(uv1.x, uv2.y);

        self
    }

    pub fn with_colors(mut self, colors: &[ubvec4; 4]) -> Self {
        self.vertices[0].color = colors[0];
        self.vertices[1].color = colors[1];
        self.vertices[2].color = colors[2];
        self.vertices[3].color = colors[3];

        self
    }

    pub fn with_color(self, color: &ubvec4) -> Self {
        self.with_colors(&[*color, *color, *color, *color])
    }

    pub fn with_rotation(mut self, rotation: f32) -> Self {
        let x = self.vertices[0].pos.x;
        let y = self.vertices[0].pos.y;
        let w = self.vertices[2].pos.x - self.vertices[0].pos.x;
        let h = self.vertices[2].pos.y - self.vertices[0].pos.y;

        let center = vec2 {
            x: x + w / 2.0,
            y: y + h / 2.0,
        };

        rotate(&center, rotation, &mut self.vertices);

        self
    }
}
