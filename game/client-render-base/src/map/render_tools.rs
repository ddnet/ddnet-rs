use std::{fmt::Debug, ops::IndexMut, time::Duration};

use fixed::traits::{FromFixed, ToFixed};
use graphics::handles::{
    canvas::canvas::GraphicsCanvasHandle,
    stream::stream::{GraphicsStreamHandle, QuadStreamHandle},
    stream_types::StreamedQuad,
    texture::texture::TextureContainer,
};
use hiarc::hi_closure;
use map::map::{
    animations::{AnimPoint, AnimPointCurveType},
    groups::MapGroupAttr,
};

use math::math::{
    vector::{ffixed, lffixed, ubvec4, vec2},
    PI,
};

use graphics_types::rendering::State;

/*enum
{
    SPRITE_FLAG_FLIP_Y = 1,
    SPRITE_FLAG_FLIP_X = 2,
};*/

pub enum LayerRenderFlag {
    Opaque = 1,
    Transparent = 2,
}

pub enum TileRenderFlag {
    Extend = 4,
}

pub enum CanvasType<'a> {
    Handle(&'a GraphicsCanvasHandle),
    Custom { aspect_ratio: f32 },
}

pub struct RenderTools {}

impl RenderTools {
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

    pub fn map_pos_to_group_attr(
        center_x: f32,
        center_y: f32,
        parallax_x: f32,
        parallax_y: f32,
        offset_x: f32,
        offset_y: f32,
    ) -> vec2 {
        let center_x = center_x * parallax_x / 100.0;
        let center_y = center_y * parallax_y / 100.0;
        vec2::new(offset_x + center_x, offset_y + center_y)
    }

    pub fn map_canvas_to_world(
        center_x: f32,
        center_y: f32,
        parallax_x: f32,
        parallax_y: f32,
        offset_x: f32,
        offset_y: f32,
        aspect: f32,
        zoom: f32,
    ) -> [f32; 4] {
        let mut width = 0.0;
        let mut height = 0.0;
        Self::calc_canvas_params(aspect, zoom, &mut width, &mut height);

        let parallax_zoom = parallax_x.max(parallax_y).clamp(0.0, 100.0);
        let scale = (parallax_zoom * (zoom - 1.0) + 100.0) / 100.0 / zoom;
        width *= scale;
        height *= scale;

        let center = Self::map_pos_to_group_attr(
            center_x, center_y, parallax_x, parallax_y, offset_x, offset_y,
        );
        let mut points: [f32; 4] = [0.0; 4];
        points[0] = center.x - width / 2.0;
        points[1] = center.y - height / 2.0;
        points[2] = points[0] + width;
        points[3] = points[1] + height;
        points
    }

    pub fn canvas_points_of_group_attr(
        canvas: CanvasType<'_>,
        center_x: f32,
        center_y: f32,
        parallax_x: f32,
        parallax_y: f32,
        offset_x: f32,
        offset_y: f32,
        zoom: f32,
    ) -> [f32; 4] {
        Self::map_canvas_to_world(
            center_x,
            center_y,
            parallax_x,
            parallax_y,
            offset_x,
            offset_y,
            match canvas {
                CanvasType::Handle(canvas_handle) => canvas_handle.canvas_aspect(),
                CanvasType::Custom { aspect_ratio } => aspect_ratio,
            },
            zoom,
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
        center_x: f32,
        center_y: f32,
        design_group: Option<&MapGroupAttr>,
        zoom: f32,
    ) -> [f32; 4] {
        let (parallax, offset) = Self::para_and_offset_of_group(design_group);
        Self::canvas_points_of_group_attr(
            canvas, center_x, center_y, parallax.x, parallax.y, offset.x, offset.y, zoom,
        )
    }

    pub fn pos_to_group(inp: vec2, design_group: Option<&MapGroupAttr>) -> vec2 {
        let (parallax, offset) = RenderTools::para_and_offset_of_group(design_group);

        RenderTools::map_pos_to_group_attr(inp.x, inp.y, parallax.x, parallax.y, offset.x, offset.y)
    }

    pub fn map_canvas_of_group(
        canvas: CanvasType<'_>,
        state: &mut State,
        center_x: f32,
        center_y: f32,
        design_group: Option<&MapGroupAttr>,
        zoom: f32,
    ) {
        let points = Self::canvas_points_of_group(canvas, center_x, center_y, design_group, zoom);
        state.map_canvas(points[0], points[1], points[2], points[3]);
    }

    pub fn render_eval_anim<F, T: Debug + Copy + Default + IndexMut<usize, Output = F>>(
        points: &[AnimPoint<T>],
        time_nanos_param: time::Duration,
        channels: usize,
    ) -> T
    where
        F: Copy + FromFixed + ToFixed,
    {
        let mut time_nanos = time_nanos_param;
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
            time_nanos = time::Duration::nanoseconds(
                (time_nanos.whole_nanoseconds().abs() % time_diff.as_nanos() as i128) as i64,
            ) + *min_point_time;
        } else {
            time_nanos = time::Duration::nanoseconds(0);
        }

        let idx = points.partition_point(|p| time_nanos >= p.time);
        let idx_prev = idx.saturating_sub(1);
        let idx = idx.clamp(0, points.len() - 1);
        let point1 = &points[idx_prev];
        let point2 = &points[idx];

        let delta = (point2.time - point1.time).clamp(Duration::from_nanos(100), Duration::MAX);
        let mut a: ffixed = (((lffixed::from_num(time_nanos.whole_nanoseconds()))
            - lffixed::from_num(point1.time.as_nanos()))
            / lffixed::from_num(delta.as_nanos()))
        .to_num();

        match point1.curve_type {
            AnimPointCurveType::Step => {
                a = 0i32.into();
            }
            AnimPointCurveType::Linear => {
                // linear
            }
            AnimPointCurveType::Slow => {
                a = a * a * a;
            }
            AnimPointCurveType::Fast => {
                a = ffixed::from_num(1) - a;
                a = ffixed::from_num(1) - a * a * a;
            }
            AnimPointCurveType::Smooth => {
                // second hermite basis
                a = ffixed::from_num(-2) * a * a * a + ffixed::from_num(3) * a * a;
            }
        }

        let mut res = T::default();
        for c in 0..channels {
            let v0: ffixed = point1.value[c].to_fixed();
            let v1: ffixed = point2.value[c].to_fixed();
            res[c] = F::from_fixed(v0 + (v1 - v0) * a);
        }

        res
    }

    pub fn render_circle(
        stream_handle: &GraphicsStreamHandle,
        pos: &vec2,
        radius: f32,
        color: &ubvec4,
        state: State,
    ) {
        stream_handle.render_quads(
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
            hi_closure!([
                center: &vec2,
                size: &vec2,
                color: &ubvec4,
                texture: Option<&'a TextureContainer>
            ], |mut stream_handle: QuadStreamHandle<'_>| -> () {
                if let Some(texture) = texture {
                    stream_handle.set_texture(texture);
                }
                stream_handle
                    .add_vertices(
                        StreamedQuad::default()
                            .from_pos_and_size(
                                vec2::new(
                                    center.x - size.x / 2.0,
                                    center.y - size.y / 2.0
                                ),
                                *size
                            )
                            .color(
                                *color
                            )
                            .tex_default()
                            .into()
                    );
            }),
            state,
        );
    }

    pub fn render_rect_free(
        stream_handle: &GraphicsStreamHandle,
        quad: StreamedQuad,
        state: State,
        texture: Option<&TextureContainer>,
    ) {
        let quad = &quad;
        stream_handle.render_quads(
            hi_closure!([
                quad: &StreamedQuad,
                texture: Option<&'a TextureContainer>
            ], |mut stream_handle: QuadStreamHandle<'_>| -> () {
                if let Some(texture) = texture {
                    stream_handle.set_texture(texture);
                }
                stream_handle
                    .add_vertices(
                        (*quad).into()
                    );
            }),
            state,
        );
    }
}
