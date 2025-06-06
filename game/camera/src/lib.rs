use std::fmt::Debug;

use graphics::handles::canvas::canvas::GraphicsCanvasHandle;
use graphics_types::rendering::State;
use hiarc::Hiarc;
use map::map::groups::MapGroupAttr;
use math::math::vector::vec2;

pub enum CanvasType<'a> {
    Handle(&'a GraphicsCanvasHandle),
    Custom { aspect_ratio: f32 },
}

pub trait CameraInterface: Debug {
    fn pos(&self) -> vec2;
    fn zoom(&self) -> f32;
    fn parallax_aware_zoom(&self) -> bool;

    fn project(
        &self,
        canvas_handle: &GraphicsCanvasHandle,
        state: &mut State,
        design_group: Option<&MapGroupAttr>,
    );
}

#[derive(Hiarc)]
pub struct Camera {
    pub pos: vec2,
    pub zoom: f32,
    pub forced_aspect_ratio: Option<f32>,
    pub parallax_aware_zoom: bool,
}

impl Debug for Camera {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Camera")
            .field("pos", &self.pos)
            .field("zoom", &self.zoom)
            .field("forced_aspect_ratio", &self.forced_aspect_ratio)
            .field("parallax_aware_zoom", &self.parallax_aware_zoom)
            .finish()
    }
}

impl Camera {
    pub fn new(
        pos: vec2,
        zoom: f32,
        forced_aspect_ratio: Option<f32>,
        parallax_aware_zoom: bool,
    ) -> Self {
        Self {
            pos,
            zoom,
            forced_aspect_ratio,
            parallax_aware_zoom,
        }
    }
    pub fn calc_canvas_params(aspect: f32, zoom: f32, width: &mut f32, height: &mut f32) {
        const AMOUNT: f32 = 1150.0 / 32.0 * 1000.0 / 32.0;
        const WIDTH_MAX: f32 = 1500.0 / 32.0;
        const HEIGHT_MAX: f32 = 1050.0 / 32.0;

        let f = AMOUNT.sqrt() / aspect.sqrt();
        *width = f * aspect;
        *height = f;

        // limit the view
        if *width > WIDTH_MAX {
            *width = WIDTH_MAX;
            *height = *width / aspect;
        }

        if *height > HEIGHT_MAX {
            *height = HEIGHT_MAX;
            *width = *height * aspect;
        }

        *width *= zoom;
        *height *= zoom;
    }

    pub fn map_pos_to_group_attr(center: vec2, parallax: vec2, offset: vec2) -> vec2 {
        let center = vec2::new(center.x * parallax.x / 100.0, center.y * parallax.y / 100.0);
        vec2::new(offset.x + center.x, offset.y + center.y)
    }

    pub fn map_canvas_to_world(
        center: vec2,
        parallax: vec2,
        offset: vec2,
        aspect: f32,
        zoom: f32,
        parallax_aware_zoom: bool,
    ) -> [f32; 4] {
        let mut width = 0.0;
        let mut height = 0.0;
        Self::calc_canvas_params(aspect, zoom, &mut width, &mut height);

        let parallax_zoom = if parallax_aware_zoom {
            parallax.x.max(parallax.y).clamp(0.0, 100.0)
        } else {
            100.0
        };
        let scale = (parallax_zoom * (zoom - 1.0) + 100.0) / 100.0 / zoom;
        width *= scale;
        height *= scale;

        let center = Self::map_pos_to_group_attr(center, parallax, offset);
        let mut points: [f32; 4] = [0.0; 4];
        points[0] = center.x - width / 2.0;
        points[1] = center.y - height / 2.0;
        points[2] = points[0] + width;
        points[3] = points[1] + height;
        points
    }

    pub fn canvas_points_of_group_attr(
        canvas: CanvasType<'_>,
        center: vec2,
        parallax: vec2,
        offset: vec2,
        zoom: f32,
        parallax_aware_zoom: bool,
    ) -> [f32; 4] {
        Self::map_canvas_to_world(
            center,
            parallax,
            offset,
            match canvas {
                CanvasType::Handle(canvas_handle) => canvas_handle.canvas_aspect(),
                CanvasType::Custom { aspect_ratio } => aspect_ratio,
            },
            zoom,
            parallax_aware_zoom,
        )
    }

    pub fn para_and_offset_of_group(design_group: Option<&MapGroupAttr>) -> (vec2, vec2) {
        if let Some(design_group) = design_group {
            (
                vec2::new(
                    design_group.parallax.x.to_num::<f32>(),
                    design_group.parallax.y.to_num::<f32>(),
                ),
                vec2::new(
                    design_group.offset.x.to_num::<f32>(),
                    design_group.offset.y.to_num::<f32>(),
                ),
            )
        } else {
            (vec2::new(100.0, 100.0), vec2::default())
        }
    }

    pub fn canvas_points_of_group(
        canvas: CanvasType<'_>,
        center: vec2,
        design_group: Option<&MapGroupAttr>,
        zoom: f32,
        parallax_aware_zoom: bool,
    ) -> [f32; 4] {
        let (parallax, offset) = Self::para_and_offset_of_group(design_group);
        Self::canvas_points_of_group_attr(
            canvas,
            center,
            parallax,
            offset,
            zoom,
            parallax_aware_zoom,
        )
    }

    pub fn pos_to_group(inp: vec2, design_group: Option<&MapGroupAttr>) -> vec2 {
        let (parallax, offset) = Self::para_and_offset_of_group(design_group);

        Self::map_pos_to_group_attr(inp, parallax, offset)
    }

    pub fn map_canvas_of_group(
        canvas: CanvasType<'_>,
        state: &mut State,
        center: vec2,
        design_group: Option<&MapGroupAttr>,
        zoom: f32,
        parallax_aware_zoom: bool,
    ) {
        let points =
            Self::canvas_points_of_group(canvas, center, design_group, zoom, parallax_aware_zoom);
        state.map_canvas(points[0], points[1], points[2], points[3]);
    }
}

impl CameraInterface for Camera {
    fn pos(&self) -> vec2 {
        self.pos
    }

    fn zoom(&self) -> f32 {
        self.zoom
    }

    fn parallax_aware_zoom(&self) -> bool {
        self.parallax_aware_zoom
    }

    fn project(
        &self,
        canvas_handle: &GraphicsCanvasHandle,
        state: &mut State,
        design_group: Option<&MapGroupAttr>,
    ) {
        Self::map_canvas_of_group(
            self.forced_aspect_ratio
                .map(|aspect_ratio| CanvasType::Custom { aspect_ratio })
                .unwrap_or(CanvasType::Handle(canvas_handle)),
            state,
            self.pos,
            design_group,
            self.zoom,
            self.parallax_aware_zoom,
        )
    }
}
