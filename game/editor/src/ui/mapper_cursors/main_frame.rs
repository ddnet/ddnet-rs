use camera::CameraInterface;
use egui::Color32;
use graphics::handles::canvas::canvas::GraphicsCanvasHandle;
use graphics_types::rendering::State;

use crate::{map::EditorMapInterface, ui::user_data::EditorTabsRefMut};

pub fn render(
    ui: &mut egui::Ui,
    canvas_handle: &GraphicsCanvasHandle,
    tabs: &mut EditorTabsRefMut<'_>,
) {
    if let Some(tab) = tabs.active_tab() {
        for client in tab
            .client
            .clients
            .iter()
            .filter(|c| c.server_id != tab.client.server_id)
        {
            let mut state = State::new();
            tab.map
                .game_camera()
                .project(canvas_handle, &mut state, None);

            let size = ui.ctx().screen_rect().size();
            let (x0, y0, x1, y1) = state.get_canvas_mapping();

            let w = x1 - x0;
            let h = y1 - y0;

            let width_scale = size.x / w;
            let height_scale = size.y / h;
            let x = (client.cursor_world.x - x0) * width_scale;
            let y = (client.cursor_world.y - y0) * height_scale;

            ui.painter().text(
                egui::pos2(x, y - 16.0),
                egui::Align2::CENTER_BOTTOM,
                &client.mapper_name,
                Default::default(),
                Color32::WHITE,
            );
            ui.painter().circle_filled(
                egui::pos2(x, y),
                4.0,
                Color32::from_rgb(client.color[0], client.color[1], client.color[2]),
            );
        }
    }
}
