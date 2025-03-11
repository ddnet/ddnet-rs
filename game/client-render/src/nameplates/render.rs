use std::time::Duration;

use egui::{pos2, text::LayoutJob, Color32, FontId, Rect, TextFormat, UiBuilder};
use graphics::{
    graphics::graphics::Graphics,
    handles::{
        backend::backend::GraphicsBackendHandle, canvas::canvas::GraphicsCanvasHandle,
        stream::stream::GraphicsStreamHandle, texture::texture::GraphicsTextureHandle,
    },
};

use graphics_types::rendering::State;
use math::math::vector::vec2;
use ui_base::{
    types::UiRenderPipe,
    ui::{UiContainer, UiCreator},
    ui_render::render_ui,
};

pub struct NameplatePlayer<'a> {
    pub name: &'a str,
    pub pos: &'a vec2,
    pub phased_alpha: f32,
}

pub struct NameplateRenderPipe<'a> {
    pub cur_time: &'a Duration,
    pub state: &'a State,
    pub camera_zoom: f32,
    pub players: &'a mut dyn Iterator<Item = NameplatePlayer<'a>>,
}

pub struct NameplateRender {
    ui: UiContainer,

    canvas_handle: GraphicsCanvasHandle,
    backend_handle: GraphicsBackendHandle,
    texture_handle: GraphicsTextureHandle,
    stream_handle: GraphicsStreamHandle,
}

impl NameplateRender {
    pub fn new(graphics: &Graphics, creator: &UiCreator) -> Self {
        let mut ui = UiContainer::new(creator);
        ui.set_main_panel_color(&Color32::TRANSPARENT);
        Self {
            ui,
            backend_handle: graphics.backend_handle.clone(),
            canvas_handle: graphics.canvas_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),
            texture_handle: graphics.texture_handle.clone(),
        }
    }

    pub fn render(&mut self, pipe: &mut NameplateRenderPipe) {
        // egui crashes if font glyph is too high detail
        if pipe.camera_zoom < 0.3 {
            return;
        }

        let canvas_width = self.canvas_handle.canvas_width();
        let canvas_height = self.canvas_handle.canvas_height();
        let pixels_per_point = self.canvas_handle.pixels_per_point();

        let mut user_data = ();
        let mut dummy_pipe = UiRenderPipe::new(*pipe.cur_time, &mut user_data);

        let (screen_rect, full_output, zoom_level) = self.ui.render_cached(
            canvas_width,
            canvas_height,
            pixels_per_point,
            |ui, _inner_pipe, _ui_state| {
                for NameplatePlayer {
                    name,
                    pos,
                    phased_alpha,
                } in &mut *pipe.players
                {
                    ui.set_opacity(phased_alpha);
                    let size = ui.ctx().screen_rect().size();
                    let (x0, y0, x1, y1) = pipe.state.get_canvas_mapping();

                    let name_scale = self.canvas_handle.canvas_width() as f32 / size.x;

                    let w = x1 - x0;
                    let h = y1 - y0;

                    let width_scale = size.x / w;
                    let height_scale = size.y / h;
                    let mut job = LayoutJob {
                        round_output_to_gui: false,
                        ..Default::default()
                    };
                    let font_size = (1.0 * name_scale) * height_scale;
                    job.append(
                        name,
                        0.0,
                        TextFormat {
                            color: Color32::WHITE,
                            font_id: FontId::proportional(font_size),
                            ..Default::default()
                        },
                    );
                    let x = (pos.x - x0) * width_scale;
                    let y = (pos.y - y0 - 70.0 / 64.0) * height_scale;
                    ui.allocate_new_ui(
                        UiBuilder::default().max_rect(Rect::from_min_max(
                            pos2(x - size.x / 2.0, y - font_size),
                            egui::pos2(x + size.x / 2.0, y),
                        )),
                        |ui| {
                            ui.centered_and_justified(|ui| {
                                ui.label(job);
                            });
                        },
                    );
                }
            },
            &mut dummy_pipe,
            Default::default(),
            false,
            true,
        );
        render_ui(
            &mut self.ui,
            full_output,
            &screen_rect,
            zoom_level,
            &self.backend_handle,
            &self.texture_handle,
            &self.stream_handle,
            false,
        );
    }
}
