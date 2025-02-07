use graphics::{graphics::graphics::Graphics, handles::canvas::canvas::GraphicsCanvasHandle};
use graphics_types::rendering::State;
use hiarc::Hiarc;

use crate::map::render_tools::RenderTools;

#[derive(Debug, Hiarc)]
pub struct CanvasMappingIngame {
    canvas_handle: GraphicsCanvasHandle,
}

impl CanvasMappingIngame {
    pub fn new(graphics: &Graphics) -> Self {
        Self {
            canvas_handle: graphics.canvas_handle.clone(),
        }
    }

    pub fn from(canvas_handle: &GraphicsCanvasHandle) -> Self {
        Self {
            canvas_handle: canvas_handle.clone(),
        }
    }

    pub fn map_canvas_for_ingame_items(
        &self,
        state: &mut State,
        center_x: f32,
        center_y: f32,
        zoom: f32,
        forced_aspect_ratio: Option<f32>,
    ) {
        let points: [f32; 4] = RenderTools::map_canvas_to_world(
            0.0,
            0.0,
            100.0,
            100.0,
            center_x,
            center_y,
            forced_aspect_ratio.unwrap_or(self.canvas_handle.canvas_aspect()),
            zoom,
            true,
        );
        state.map_canvas(points[0], points[1], points[2], points[3]);
    }
}
