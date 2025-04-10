use graphics_types::rendering::{GlColor, GlPoint, GlTexCoord, GlVertex};
use hiarc::Hiarc;
use math::math::vector::{ubvec4, vec2, vec4};

use crate::streaming::rotate;

#[derive(Debug, Hiarc, Copy, Clone)]
pub struct StreamedLine {
    vertices: [GlVertex; 2],
}

impl Default for StreamedLine {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamedLine {
    pub fn new() -> Self {
        Self {
            vertices: Default::default(),
        }
    }

    pub fn from_pos(mut self, pos: [vec2; 2]) -> Self {
        self.vertices[0].set_pos(&GlPoint {
            x: pos[0].x,
            y: pos[0].y,
        });
        self.vertices[1].set_pos(&GlPoint {
            x: pos[1].x,
            y: pos[1].y,
        });

        self
    }

    pub fn with_color(mut self, color: ubvec4) -> Self {
        self.vertices[0].color = color;
        self.vertices[1].color = color;
        self
    }
}

impl From<StreamedLine> for [GlVertex; 2] {
    fn from(val: StreamedLine) -> Self {
        val.vertices
    }
}

#[derive(Debug, Copy, Clone)]
pub struct StreamedTriangle {
    vertices: [GlVertex; 3],
}

impl Default for StreamedTriangle {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamedTriangle {
    pub fn new() -> Self {
        Self {
            vertices: Default::default(),
        }
    }
}

impl From<StreamedTriangle> for [GlVertex; 3] {
    fn from(val: StreamedTriangle) -> Self {
        val.vertices
    }
}

#[derive(Debug, Hiarc, Default, Copy, Clone)]
/// Represents a quad that is rendered in 2 triangles
///
/// Quad's vertices:
/// 1   2
/// |---|
/// | \ |
/// |---|
/// 4   3
///
/// Triangle vertices:
/// 1 + 2 + 3
/// 1 + 3 + 4
pub struct StreamedQuad {
    pub vertices: [GlVertex; 4],
}

impl StreamedQuad {
    pub fn from_center_and_size(mut self, center: vec2, size: vec2) -> Self {
        let wh = size.x / 2.0;
        let hh = size.y / 2.0;
        self.vertices[0].set_pos(&GlPoint {
            x: center.x - wh,
            y: center.y - hh,
        });
        self.vertices[1].set_pos(&GlPoint {
            x: center.x + wh,
            y: center.y - hh,
        });
        self.vertices[2].set_pos(&GlPoint {
            x: center.x + wh,
            y: center.y + hh,
        });
        self.vertices[3].set_pos(&GlPoint {
            x: center.x - wh,
            y: center.y + hh,
        });

        self
    }

    pub fn from_pos_and_size(mut self, pos: vec2, size: vec2) -> Self {
        self.vertices[0].set_pos(&GlPoint { x: pos.x, y: pos.y });
        self.vertices[1].set_pos(&GlPoint {
            x: pos.x + size.x,
            y: pos.y,
        });
        self.vertices[2].set_pos(&GlPoint {
            x: pos.x + size.x,
            y: pos.y + size.y,
        });
        self.vertices[3].set_pos(&GlPoint {
            x: pos.x,
            y: pos.y + size.y,
        });

        self
    }

    pub fn pos_free_form(mut self, tl: vec2, tr: vec2, br: vec2, bl: vec2) -> Self {
        self.vertices[0].set_pos(&GlPoint { x: tl.x, y: tl.y });
        self.vertices[1].set_pos(&GlPoint { x: tr.x, y: tr.y });
        self.vertices[2].set_pos(&GlPoint { x: br.x, y: br.y });
        self.vertices[3].set_pos(&GlPoint { x: bl.x, y: bl.y });

        self
    }

    pub fn tex_free_form(mut self, tl: vec2, tr: vec2, br: vec2, bl: vec2) -> Self {
        self.vertices[0].set_tex_coords(&GlTexCoord { x: tl.x, y: tl.y });
        self.vertices[1].set_tex_coords(&GlTexCoord { x: tr.x, y: tr.y });
        self.vertices[2].set_tex_coords(&GlTexCoord { x: br.x, y: br.y });
        self.vertices[3].set_tex_coords(&GlTexCoord { x: bl.x, y: bl.y });

        self
    }

    pub fn tex_default(self) -> Self {
        self.tex_free_form(
            vec2::new(0.0, 0.0),
            vec2::new(1.0, 0.0),
            vec2::new(1.0, 1.0),
            vec2::new(0.0, 1.0),
        )
    }

    pub fn color(mut self, color: ubvec4) -> Self {
        let color = GlColor {
            x: color.x,
            y: color.y,
            z: color.z,
            w: color.w,
        };
        self.vertices[0].set_color(&color);
        self.vertices[1].set_color(&color);
        self.vertices[2].set_color(&color);
        self.vertices[3].set_color(&color);

        self
    }

    pub fn colorf(mut self, color: vec4) -> Self {
        let color = GlColor {
            x: (color.x * 255.0) as u8,
            y: (color.y * 255.0) as u8,
            z: (color.z * 255.0) as u8,
            w: (color.w * 255.0) as u8,
        };
        self.vertices[0].set_color(&color);
        self.vertices[1].set_color(&color);
        self.vertices[2].set_color(&color);
        self.vertices[3].set_color(&color);

        self
    }

    pub fn color_free_form(mut self, tl: ubvec4, tr: ubvec4, br: ubvec4, bl: ubvec4) -> Self {
        self.vertices[0].set_color(&GlColor {
            x: tl.x,
            y: tl.y,
            z: tl.z,
            w: tl.w,
        });
        self.vertices[1].set_color(&GlColor {
            x: tr.x,
            y: tr.y,
            z: tr.z,
            w: tr.w,
        });
        self.vertices[2].set_color(&GlColor {
            x: br.x,
            y: br.y,
            z: br.z,
            w: br.w,
        });
        self.vertices[3].set_color(&GlColor {
            x: bl.x,
            y: bl.y,
            z: bl.z,
            w: bl.w,
        });

        self
    }

    pub fn rotate_pos(mut self, rot: f32) -> Self {
        let center = vec2::new(
            self.vertices[0].pos.x + (self.vertices[1].pos.x - self.vertices[0].pos.x) / 2.0,
            self.vertices[0].pos.y + (self.vertices[2].pos.y - self.vertices[0].pos.y) / 2.0,
        );

        rotate(&center, rot, &mut self.vertices);
        self
    }
}

impl From<StreamedQuad> for [GlVertex; 4] {
    fn from(val: StreamedQuad) -> Self {
        val.vertices
    }
}
