use camera::Camera;
use client_containers::{container::ContainerKey, ninja::NinjaContainer, weapons::WeaponContainer};
use client_render_base::render::{
    canvas_mapping::CanvasMappingIngame, toolkit::get_sprite_scale_impl,
};
use game_interface::types::weapons::WeaponType;
use graphics::{
    graphics::graphics::Graphics, handles::quad_container::quad_container::QuadContainer,
    quad_container::Quad, streaming::quad_scope_begin,
};
use graphics_types::rendering::State;
use math::math::vector::{dvec2, vec2};

pub struct RenderCursorPipe<'a> {
    pub weapon_container: &'a mut WeaponContainer,
    pub weapon_key: Option<&'a ContainerKey>,
    pub ninja_container: &'a mut NinjaContainer,
    pub ninja_key: Option<&'a ContainerKey>,
    pub mouse_cursor: dvec2,
    pub cur_weapon: WeaponType,
    pub is_ninja: bool,
    pub camera: &'a Camera,
}

pub struct RenderCursor {
    cursor_quad_container: QuadContainer,
    canvas_mapping: CanvasMappingIngame,
}

impl RenderCursor {
    pub fn new(graphics: &Graphics) -> Self {
        let (scale_x, scale_y) = get_sprite_scale_impl(2, 2);

        let cursor_quad_container = graphics.quad_container_handle.create_quad_container(
            [Quad::new()
                .from_width_and_height_centered(2.0 * scale_x, 2.0 * scale_y)
                .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0))]
            .into(),
        );

        Self {
            cursor_quad_container,
            canvas_mapping: CanvasMappingIngame::new(graphics),
        }
    }

    pub fn render(&self, pipe: &mut RenderCursorPipe) {
        let mut state = State::default();
        let camera = Camera::new(
            Default::default(),
            1.0,
            pipe.camera.forced_aspect_ratio,
            pipe.camera.parallax_aware_zoom,
        );
        self.canvas_mapping
            .map_canvas_for_ingame_items(&mut state, &camera);

        let mut draw_scope = quad_scope_begin();
        draw_scope.set_state(&state);
        let texture = if pipe.is_ninja {
            &pipe
                .ninja_container
                .get_or_default_opt(pipe.ninja_key)
                .cursor
        } else {
            &pipe
                .weapon_container
                .get_or_default_opt(pipe.weapon_key)
                .by_type(pipe.cur_weapon)
                .cursor
        };
        draw_scope.set_colors_from_single(1.0, 1.0, 1.0, 1.0);

        let c = pipe.mouse_cursor;
        let c = vec2::new(c.x as f32, c.y as f32);

        self.cursor_quad_container.render_quad_container_as_sprite(
            0,
            c.x,
            c.y,
            1.0,
            1.0,
            draw_scope,
            texture.into(),
        );
    }
}
