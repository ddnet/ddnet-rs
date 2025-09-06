use std::time::Duration;

use client_containers::emoticons::EmoticonsContainer;
use game_base::game_types::intra_tick_time_to_ratio;
use game_interface::types::{
    emoticons::EmoticonType,
    game::{GameTickType, NonZeroGameTickType},
    resource_key::ResourceKey,
};
use graphics::{
    graphics::graphics::Graphics, handles::quad_container::quad_container::QuadContainer,
    quad_container::Quad, streaming::quad_scope_begin,
};
use graphics_types::rendering::State;
use math::math::{
    PI,
    vector::{ubvec4, vec2},
};

pub struct RenderEmoticonPipe<'a> {
    pub emoticon_container: &'a mut EmoticonsContainer,
    pub emoticon_key: Option<&'a ResourceKey>,

    pub pos: vec2,
    pub state: &'a State,
    pub emoticon: EmoticonType,
    pub emoticon_ticks: GameTickType,
    pub ticks_per_second: NonZeroGameTickType,
    pub intra_tick_time: Duration,
    pub phased_alpha: f32,
}

pub struct RenderEmoticon {
    quad_container: QuadContainer,
}

impl RenderEmoticon {
    pub fn new(graphics: &Graphics) -> Self {
        let quads: Vec<Quad> = vec![
            Quad::new()
                .from_size_centered(2.0)
                .with_color(&ubvec4::new(255, 255, 255, 255))
                .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
        ];

        let quad_container = graphics.quad_container_handle.create_quad_container(quads);

        Self { quad_container }
    }

    pub fn render(&self, pipe: &mut RenderEmoticonPipe) {
        let emoticon_duration = 2 * pipe.ticks_per_second.get();
        if pipe.emoticon_ticks < emoticon_duration {
            let emoticon = pipe
                .emoticon_container
                .get_or_default_opt(pipe.emoticon_key);
            let texture = &emoticon.emoticons[pipe.emoticon as usize];

            let mut quad_scope = quad_scope_begin();
            quad_scope.set_state(pipe.state);

            let intra_tick_ratio =
                intra_tick_time_to_ratio(pipe.intra_tick_time, pipe.ticks_per_second) as f32;

            let since_start = pipe.emoticon_ticks;
            let since_start_intra = since_start as f32 + intra_tick_ratio;
            let from_end = emoticon_duration - since_start;
            let from_end_intra = emoticon_duration as f32 - since_start_intra;

            let mut a = 1.0;
            if from_end <= pipe.ticks_per_second.get() / 5 {
                a = from_end_intra / (pipe.ticks_per_second.get() as f32 / 5.0);
            }

            let mut h = 1.0;
            if since_start < pipe.ticks_per_second.get() / 10 {
                h = since_start_intra / (pipe.ticks_per_second.get() as f32 / 10.0);
            }

            let mut wiggle = 0.0;
            if since_start < pipe.ticks_per_second.get() / 5 {
                wiggle = since_start_intra / (pipe.ticks_per_second.get() as f32 / 5.0);
            }

            let wiggle_angle = (5.0 * wiggle).sin();

            quad_scope.set_rotation(PI / 6.0 * wiggle_angle);

            quad_scope.set_colors_from_single(1.0, 1.0, 1.0, a * pipe.phased_alpha);

            self.quad_container.render_quad_container_as_sprite(
                0,
                pipe.pos.x,
                pipe.pos.y - 23.0 / 32.0 - 1.0 * h,
                1.0,
                h,
                quad_scope,
                texture.into(),
            );
        }
    }
}
