use std::{fmt::Debug, ops::IndexMut};

use fixed::traits::{FromFixed, ToFixed};
use graphics::handles::{
    stream::stream::{GraphicsStreamHandle, QuadStreamHandle},
    stream_types::StreamedQuad,
    texture::texture::TextureContainer,
};
use hiarc::hi_closure;
use map::map::animations::AnimPoint;

use math::math::{
    vector::{ubvec4, vec2},
    PI,
};

use graphics_types::rendering::State;

pub enum LayerRenderFlag {
    Opaque = 1,
    Transparent = 2,
}

pub enum TileRenderFlag {
    Extend = 4,
}

pub struct RenderTools {}

impl RenderTools {
    pub fn render_eval_anim<
        F,
        T: Debug + Copy + Default + IndexMut<usize, Output = F>,
        const CHANNELS: usize,
    >(
        points: &[AnimPoint<T, CHANNELS>],
        mut time_param: time::Duration,
        // include last point in the evaluation
        // usually only good during animating
        include_last_point: bool,
    ) -> T
    where
        F: Copy + FromFixed + ToFixed,
    {
        if points.is_empty() {
            return T::default();
        }

        if points.len() == 1 {
            return points[0].value;
        }

        let max_point_time = &points[points.len() - 1].time;
        let min_point_time = &points[0].time;

        if !max_point_time.is_zero() {
            let time_diff = max_point_time.saturating_sub(*min_point_time);
            if include_last_point {
                let time = time::Duration::nanoseconds(
                    (time_param.whole_nanoseconds().abs() % (time_diff.as_nanos() as i128 + 1))
                        as i64,
                ) + *min_point_time;
                if time == *max_point_time {
                    return points[points.len() - 1].value;
                }
            }
            time_param = time::Duration::nanoseconds(
                (time_param.whole_nanoseconds().abs() % time_diff.as_nanos() as i128) as i64,
            ) + *min_point_time;
        } else {
            time_param = time::Duration::nanoseconds(0);
        }

        let idx = points.partition_point(|p| time_param >= p.time);
        let idx_prev = idx.saturating_sub(1);
        let idx = idx.clamp(0, points.len() - 1);
        let point1 = &points[idx_prev];
        let point2 = &points[idx];

        AnimPoint::eval_curve(point1, point2, time_param)
    }

    pub fn render_circle(
        stream_handle: &GraphicsStreamHandle,
        pos: &vec2,
        radius: f32,
        color: &ubvec4,
        state: State,
    ) {
        stream_handle.stream_quads(
            hi_closure!([
                pos: &vec2,
                radius: f32,
                color: &ubvec4
            ], |mut stream_handle: QuadStreamHandle<'_>| -> () {
                let num_segments = 64;
                let segment_angle = 2.0 * PI / num_segments as f32;
                for i in (0..num_segments).step_by(2) {
                    let a1 = i as f32 * segment_angle;
                    let a2 = (i + 1) as f32 * segment_angle;
                    let a3 = (i + 2) as f32 * segment_angle;
                    stream_handle
                        .add_vertices(
                            StreamedQuad::default().pos_free_form(
                                vec2::new(
                                    pos.x,
                                    pos.y
                                ),
                                vec2::new(
                                    pos.x + a1.cos() * radius,
                                    pos.y + a1.sin() * radius
                                ),
                                vec2::new(
                                    pos.x + a2.cos() * radius,
                                    pos.y + a2.sin() * radius
                                ),
                                vec2::new(
                                    pos.x + a3.cos() * radius,
                                    pos.y + a3.sin() * radius
                                )
                            )
                            .color(
                                *color
                            ).into()
                        );
                }
            }),
            state,
        );
    }

    pub fn render_rect(
        stream_handle: &GraphicsStreamHandle,
        center: &vec2,
        size: &vec2,
        color: &ubvec4,
        state: State,
        texture: Option<&TextureContainer>,
    ) {
        stream_handle.render_quads(
            &[StreamedQuad::default()
                .from_pos_and_size(
                    vec2::new(center.x - size.x / 2.0, center.y - size.y / 2.0),
                    *size,
                )
                .color(*color)
                .tex_default()],
            state,
            texture.into(),
        );
    }

    pub fn render_rect_free(
        stream_handle: &GraphicsStreamHandle,
        quad: StreamedQuad,
        state: State,
        texture: Option<&TextureContainer>,
    ) {
        stream_handle.render_quads(&[quad], state, texture.into());
    }
}
