use camera::CameraInterface;
use graphics::{graphics::graphics::Graphics, handles::canvas::canvas::GraphicsCanvasHandle};
use graphics_types::rendering::State;
use hiarc::Hiarc;

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

    pub fn map_canvas_for_ingame_items(&self, state: &mut State, camera: &dyn CameraInterface) {
        camera.project(&self.canvas_handle, state, None);
    }
}
