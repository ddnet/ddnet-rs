use std::{
    borrow::Borrow,
    cell::Cell,
    fmt::Debug,
    ops::{IndexMut, Range},
    time::Duration,
};

use crate::map::{
    map_buffered::{MapRenderLayer, MapRenderTextOverlayType, QuadVisualRangeAnim},
    render_pipe::RenderPipelinePhysics,
};

use super::{
    map_buffered::{
        MapPhysicsRenderInfo, PhysicsTileLayerVisuals, QuadLayerVisuals, TileLayerBufferedVisuals,
        TileLayerVisuals, TileLayerVisualsBase,
    },
    map_pipeline::{EditorTileLayerRenderProps, MapGraphics, QuadRenderInfo, TileLayerDrawInfo},
    map_sound::MapSoundProcess,
    map_with_visual::{MapVisual, MapVisualLayerBase},
    render_pipe::{RenderPipeline, RenderPipelineBase},
    render_tools::RenderTools,
};
use camera::CameraInterface;
use client_containers::{
    container::ContainerKey,
    entities::{Entities, EntitiesContainer},
};
use fixed::traits::{FromFixed, ToFixed};
use game_config::config::ConfigMap;
use game_interface::types::game::{GameTickType, NonZeroGameTickType};
use graphics::handles::{
    backend::backend::GraphicsBackendHandle,
    buffer_object::buffer_object::BufferObject,
    canvas::canvas::GraphicsCanvasHandle,
    shader_storage::shader_storage::ShaderStorage,
    stream::stream::{GraphicsStreamHandle, StreamedUniforms},
    stream_types::StreamedQuad,
    texture::texture::{
        TextureContainer, TextureContainer2dArray, TextureType, TextureType2dArray,
    },
};
use hiarc::HiarcTrait;
use hiarc::{Hiarc, hi_closure};
use map::{
    map::{
        animations::{AnimBase, AnimPoint},
        groups::{
            MapGroupAttr, MapGroupAttrClipping,
            layers::design::{Quad, Sound, SoundShape},
        },
    },
    skeleton::{
        animations::AnimationsSkeleton, groups::layers::physics::MapLayerPhysicsSkeleton,
        resources::MapResourcesSkeleton,
    },
};
use pool::{datatypes::PoolFxHashMap, mixed_pool::Pool as MixedPool, pool::Pool};
use rustc_hash::FxHashMap;
use serde::de::DeserializeOwned;

use math::math::{
    PI, mix,
    vector::{fvec3, nffixed, nfvec4, ubvec4, uffixed, vec2},
};

use graphics_types::rendering::{BlendType, ColorRgba, State};
use sound::sound_object::SoundObject;

#[derive(Debug, Clone, Copy)]
pub enum RenderLayerType {
    Background,
    Foreground,
}

pub enum ForcedTexture<'a> {
    TileLayer(&'a TextureContainer2dArray),
    TileLayerTileIndex(&'a TextureContainer2dArray),
    TileLayerTileFlag(&'a TextureContainer2dArray),
    QuadLayer(&'a TextureContainer),
}

enum QuadFlushOrAdd {
    Flush { fully_transparent_color: bool },
    Add { info: QuadRenderInfo },
}

pub struct QuadAnimEvalResult {
    pub pos_anims_values: PoolFxHashMap<(usize, time::Duration), fvec3>,
    pub color_anims_values: PoolFxHashMap<(usize, time::Duration), nfvec4>,
}

#[derive(Debug, Hiarc)]
pub struct RenderMap {
    map_graphics: MapGraphics,

    canvas_handle: GraphicsCanvasHandle,
    stream_handle: GraphicsStreamHandle,

    tile_layer_render_info_pool: MixedPool<Vec<TileLayerDrawInfo>>,
    pos_anims: Pool<FxHashMap<(usize, time::Duration), fvec3>>,
    color_anims: Pool<FxHashMap<(usize, time::Duration), nfvec4>>,

    // sound, handled here because it's such an integral part of the map
    pub sound: MapSoundProcess,
}

impl RenderMap {
    pub fn new(
        backend_handle: &GraphicsBackendHandle,
        canvas_handle: &GraphicsCanvasHandle,
        stream_handle: &GraphicsStreamHandle,
    ) -> RenderMap {
        let (tile_layer_render_info_pool, tile_layer_render_info_sync_point) =
            MixedPool::with_capacity(64);
        backend_handle.add_sync_point(tile_layer_render_info_sync_point);
        RenderMap {
            map_graphics: MapGraphics::new(backend_handle),

            canvas_handle: canvas_handle.clone(),
            stream_handle: stream_handle.clone(),

            tile_layer_render_info_pool,

            pos_anims: Pool::with_capacity(8),
            color_anims: Pool::with_capacity(8),

            sound: MapSoundProcess::new(),
        }
    }

    pub fn calc_anim_time(
        ticks_per_second: NonZeroGameTickType,
        animation_ticks_passed: GameTickType,
        intra_tick_time: &Duration,
    ) -> Duration {
        let tick_to_nanoseconds = (time::Duration::seconds(1).whole_nanoseconds()
            / ticks_per_second.get() as i128) as u64;
        // get the lerp of the current tick and prev
        let min_tick = animation_ticks_passed.saturating_sub(1);
        let cur_tick = animation_ticks_passed;
        Duration::from_nanos(
            (mix::<f64, f64>(
                &0.0,
                &((cur_tick - min_tick) as f64),
                intra_tick_time.as_secs_f64(),
            ) * tick_to_nanoseconds as f64) as u64
                + min_tick * tick_to_nanoseconds,
        )
    }

    pub(crate) fn animation_eval<
        F,
        T: DeserializeOwned + Debug + Copy + Default + IndexMut<usize, Output = F>,
        const CHANNELS: usize,
    >(
        anim: &AnimBase<AnimPoint<T, CHANNELS>>,
        cur_time: &Duration,
        cur_anim_time: &Duration,
        anim_time_offset: &time::Duration,
        include_last_anim_point: bool,
    ) -> T
    where
        F: Copy + FromFixed + ToFixed,
    {
        let total_time = if anim.synchronized {
            time::Duration::try_from(*cur_anim_time).unwrap_or_default()
        } else {
            time::Duration::try_from(*cur_time).unwrap_or_default()
        };
        let anim_time = total_time + *anim_time_offset;

        RenderTools::render_eval_anim(&anim.points, anim_time, include_last_anim_point)
    }

    fn render_tile_layer<AN, AS>(
        &self,
        state: &State,
        texture: TextureType2dArray,
        cur_time: &Duration,
        cur_anim_time: &Duration,
        include_last_anim_point: bool,
        visuals: &TileLayerVisualsBase,
        buffer_object: &Option<BufferObject>,
        shader_storage: &Option<ShaderStorage>,
        color_anim: &Option<usize>,
        color_anim_offset: &time::Duration,
        animations: &AnimationsSkeleton<AN, AS>,
        mut color: ColorRgba,
    ) {
        let (screen_x0, screen_y0, screen_x1, screen_y1) = state.get_canvas_mapping();
        let channels = if let Some(anim) = {
            if let Some(color_anim) = color_anim {
                animations.color.get(*color_anim)
            } else {
                None
            }
        } {
            Self::animation_eval(
                &anim.def,
                cur_time,
                cur_anim_time,
                color_anim_offset,
                include_last_anim_point,
            )
        } else {
            nfvec4::new(
                nffixed::from_num(1),
                nffixed::from_num(1),
                nffixed::from_num(1),
                nffixed::from_num(1),
            )
        };

        let mut draw_border = false;

        let border_y0 = (screen_y0).floor() as i32;
        let border_x0 = (screen_x0).floor() as i32;
        let border_y1 = (screen_y1).ceil() as i32;
        let border_x1 = (screen_x1).ceil() as i32;

        let mut y0 = border_y0;
        let mut x0 = border_x0;
        let mut y1 = border_y1;
        let mut x1 = border_x1;

        let (width, height) = (visuals.width as i32, visuals.height as i32);

        if x0 < 0 {
            x0 = 0;
            draw_border = true;
        }
        if y0 < 0 {
            y0 = 0;
            draw_border = true;
        }
        if x1 > width {
            x1 = width;
            draw_border = true;
        }
        if y1 > height {
            y1 = height;
            draw_border = true;
        }

        let mut draw_layer = true;
        if x1 <= 0 {
            draw_layer = false;
        }
        if y1 <= 0 {
            draw_layer = false;
        }
        if x0 >= width {
            draw_layer = false;
        }
        if y0 >= height {
            draw_layer = false;
        }

        if visuals.ignored_tile_index_and_is_textured_check {
            let shader_storage = shader_storage.as_ref().expect(
                "An empty shader storage for when tile index was ignored \
                is considered a higher level bug in the code.",
            );
            assert!(
                matches!(texture, TextureType2dArray::Texture(_)),
                "When texture check is ignored, it is assumed that \
                a valid texture is always passed down, this is a bug \
                in higher level code."
            );
            self.map_graphics.render_editor_tile_layer(
                state,
                texture,
                shader_storage,
                &color,
                EditorTileLayerRenderProps {
                    x: border_x0 as f32,
                    y: border_y0 as f32,
                    w: border_x1 as f32 - border_x0 as f32,
                    h: border_y1 as f32 - border_y0 as f32,
                    layer_width: visuals.width,
                    layer_height: visuals.height,
                },
            );
        } else {
            if let Some(shader_storage) = shader_storage
                && draw_layer
            {
                // indices buffers we want to draw
                let mut draws = self.tile_layer_render_info_pool.new();

                let reserve: usize = (y1 - y0).unsigned_abs() as usize + 1;
                draws.reserve(reserve);

                for y in y0..y1 {
                    if x0 > x1 {
                        continue;
                    }
                    let xr = x1 - 1;

                    if visuals.tiles_of_layer[(y * width + xr) as usize].quad_offset()
                        < visuals.tiles_of_layer[(y * width + x0) as usize].quad_offset()
                    {
                        panic!("Tile count wrong.");
                    }

                    let num_quads = (visuals.tiles_of_layer[(y * width + xr) as usize]
                        .quad_offset()
                        - visuals.tiles_of_layer[(y * width + x0) as usize].quad_offset())
                        + (if visuals.tiles_of_layer[(y * width + xr) as usize].drawable() {
                            1
                        } else {
                            0
                        });

                    if num_quads > 0 {
                        draws.push(TileLayerDrawInfo {
                            quad_offset: visuals.tiles_of_layer[(y * width + x0) as usize]
                                .quad_offset(),
                            quad_count: num_quads,
                            pos_y: y as f32,
                        });
                    }
                }

                color.r *= channels.r().to_num::<f32>();
                color.g *= channels.g().to_num::<f32>();
                color.b *= channels.b().to_num::<f32>();
                color.a *= channels.a().to_num::<f32>();

                let draw_count = draws.len();
                if draw_count != 0 {
                    self.map_graphics.render_tile_layer(
                        state,
                        texture.clone(),
                        shader_storage,
                        &color,
                        draws,
                    );
                }
            }

            if draw_border {
                self.render_tile_border(
                    state,
                    texture.clone(),
                    visuals,
                    buffer_object,
                    &color,
                    border_x0,
                    border_y0,
                    border_x1,
                    border_y1,
                );
            }
        }
    }

    fn render_tile_border(
        &self,
        state: &State,
        texture: TextureType2dArray,
        visuals: &TileLayerVisualsBase,
        buffer_object_index: &Option<BufferObject>,
        color: &ColorRgba,
        border_x0: i32,
        border_y0: i32,
        border_x1: i32,
        border_y1: i32,
    ) {
        if let Some(buffer_container_index) = &buffer_object_index {
            let mut y0 = border_y0;
            let mut x0 = border_x0;
            let mut y1 = border_y1;
            let mut x1 = border_x1;

            let (width, height) = (visuals.width as i32, visuals.height as i32);

            if x0 < 0 {
                x0 = 0;
            }
            if y0 < 0 {
                y0 = 0;
            }
            if x1 > width {
                x1 = width;
            }
            if y1 > height {
                y1 = height;
            }

            if border_x0 < 0 {
                // Draw corners on left side
                if border_y0 < 0 && visuals.corner_top_left.drawable() {
                    let offset = vec2::new(0.0, 0.0);
                    let scale = vec2::new(border_x0.abs() as f32, border_y0.abs() as f32);

                    self.map_graphics.render_border_tiles(
                        state,
                        texture.clone(),
                        buffer_container_index,
                        color,
                        &offset,
                        &scale,
                        visuals.corner_top_left.quad_offset(),
                        1,
                    );
                }
                if border_y1 > height && visuals.corner_bottom_left.drawable() {
                    let offset = vec2::new(0.0, height as f32);
                    let scale = vec2::new(border_x0.abs() as f32, (border_y1 - height) as f32);

                    self.map_graphics.render_border_tiles(
                        state,
                        texture.clone(),
                        buffer_container_index,
                        color,
                        &offset,
                        &scale,
                        visuals.corner_bottom_left.quad_offset(),
                        1,
                    );
                }
            }
            if border_x1 > width {
                // Draw corners on right side
                if border_y0 < 0 && visuals.corner_top_right.drawable() {
                    let offset = vec2::new(width as f32, 0.0);
                    let scale = vec2::new((border_x1 - width) as f32, border_y0.abs() as f32);

                    self.map_graphics.render_border_tiles(
                        state,
                        texture.clone(),
                        buffer_container_index,
                        color,
                        &offset,
                        &scale,
                        visuals.corner_top_right.quad_offset(),
                        1,
                    );
                }
                if border_y1 > height && visuals.corner_bottom_right.drawable() {
                    let offset = vec2::new(width as f32, height as f32);
                    let scale = vec2::new((border_x1 - width) as f32, (border_y1 - height) as f32);

                    self.map_graphics.render_border_tiles(
                        state,
                        texture.clone(),
                        buffer_container_index,
                        color,
                        &offset,
                        &scale,
                        visuals.corner_bottom_right.quad_offset(),
                        1,
                    );
                }
            }
            if border_x1 > width {
                // Draw right border
                if y0 < height && y1 > 0 {
                    let yb = y1 - 1;
                    let quad_count = (visuals.border_right[yb as usize].quad_offset()
                        - visuals.border_right[y0 as usize].quad_offset())
                        + (if visuals.border_right[yb as usize].drawable() {
                            1
                        } else {
                            0
                        });
                    let quad_offset = visuals.border_right[y0 as usize].quad_offset();
                    let offset = vec2::new(width as f32, 0.0);
                    let scale = vec2::new((border_x1 - width) as f32, 1.0);

                    self.map_graphics.render_border_tiles(
                        state,
                        texture.clone(),
                        buffer_container_index,
                        color,
                        &offset,
                        &scale,
                        quad_offset,
                        quad_count,
                    );
                }
            }

            if border_x0 < 0 {
                // Draw left border
                if y0 < height && y1 > 0 {
                    let yb = y1 - 1;
                    let quad_count = (visuals.border_left[yb as usize].quad_offset()
                        - visuals.border_left[y0 as usize].quad_offset())
                        + (if visuals.border_left[yb as usize].drawable() {
                            1
                        } else {
                            0
                        });
                    let quad_offset = visuals.border_left[y0 as usize].quad_offset();
                    let offset = vec2::new(0.0, 0.0);
                    let scale = vec2::new((border_x0).abs() as f32, 1.0);

                    self.map_graphics.render_border_tiles(
                        state,
                        texture.clone(),
                        buffer_container_index,
                        color,
                        &offset,
                        &scale,
                        quad_offset,
                        quad_count,
                    );
                }
            }
            if border_y0 < 0 {
                // Draw top border
                if x0 < width && x1 > 0 {
                    let xr = x1 - 1;
                    let quad_count = (visuals.border_top[xr as usize].quad_offset()
                        - visuals.border_top[x0 as usize].quad_offset())
                        + (if visuals.border_top[xr as usize].drawable() {
                            1
                        } else {
                            0
                        });
                    let quad_offset = visuals.border_top[x0 as usize].quad_offset();
                    let offset = vec2::new(0.0, 0.0);
                    let scale = vec2::new(1.0, border_y0.abs() as f32);

                    self.map_graphics.render_border_tiles(
                        state,
                        texture.clone(),
                        buffer_container_index,
                        color,
                        &offset,
                        &scale,
                        quad_offset,
                        quad_count,
                    );
                }
            }
            if border_y1 > height {
                // Draw bottom border
                if x0 < width && x1 > 0 {
                    let xr = x1 - 1;
                    let quad_count = (visuals.border_bottom[xr as usize].quad_offset()
                        - visuals.border_bottom[x0 as usize].quad_offset())
                        + (if visuals.border_bottom[xr as usize].drawable() {
                            1
                        } else {
                            0
                        });
                    let quad_offset = visuals.border_bottom[x0 as usize].quad_offset();
                    let offset = vec2::new(0.0, height as f32);
                    let scale = vec2::new(1.0, (border_y1 - height) as f32);

                    self.map_graphics.render_border_tiles(
                        state,
                        texture.clone(),
                        buffer_container_index,
                        color,
                        &offset,
                        &scale,
                        quad_offset,
                        quad_count,
                    );
                }
            }
        }
    }

    fn render_kill_tile_border(
        &self,
        state: &State,
        texture: TextureType2dArray,
        visuals: &TileLayerBufferedVisuals,
        color: &ColorRgba,
    ) {
        if let Some(buffer_container_index) = &visuals.obj.buffer_object {
            let (canvas_x0, canvas_y0, canvas_x1, canvas_y1) = state.get_canvas_mapping();

            let mut draw_border = false;

            let mut border_y0 = (canvas_y0).floor() as i32;
            let mut border_x0 = (canvas_x0).floor() as i32;
            let mut border_y1 = (canvas_y1).ceil() as i32;
            let mut border_x1 = (canvas_x1).ceil() as i32;

            let (width, height) = (visuals.base.width as i32, visuals.base.height as i32);

            if border_x0 < -201 {
                draw_border = true;
            }
            if border_y0 < -201 {
                draw_border = true;
            }
            if border_x1 > width + 201 {
                draw_border = true;
            }
            if border_y1 > height + 201 {
                draw_border = true;
            }

            if !draw_border {
                return;
            }
            if !visuals.base.border_kill_tile.drawable() {
                return;
            }

            if border_x0 < -300 {
                border_x0 = -300;
            }
            if border_y0 < -300 {
                border_y0 = -300;
            }
            if border_x1 >= width + 300 {
                border_x1 = width + 299;
            }
            if border_y1 >= height + 300 {
                border_y1 = height + 299;
            }

            if border_x1 < -300 {
                border_x1 = -300;
            }
            if border_y1 < -300 {
                border_y1 = -300;
            }
            if border_x0 >= width + 300 {
                border_x0 = width + 299;
            }
            if border_y0 >= height + 300 {
                border_y0 = height + 299;
            }

            // Draw left kill tile border
            if border_x0 < -201 {
                let offset = vec2::new(border_x0 as f32, border_y0 as f32);
                let scale = vec2::new((-201 - border_x0) as f32, (border_y1 - border_y0) as f32);
                self.map_graphics.render_border_tiles(
                    state,
                    texture.clone(),
                    buffer_container_index,
                    color,
                    &offset,
                    &scale,
                    visuals.base.border_kill_tile.quad_offset(),
                    1,
                );
            }
            // Draw top kill tile border
            if border_y0 < -201 {
                let offset = vec2::new(border_x0.max(-201) as f32, border_y0 as f32);
                let scale = vec2::new(
                    (border_x1.min(width + 201) - border_x0.max(-201)) as f32,
                    (-201 - border_y0) as f32,
                );
                self.map_graphics.render_border_tiles(
                    state,
                    texture.clone(),
                    buffer_container_index,
                    color,
                    &offset,
                    &scale,
                    visuals.base.border_kill_tile.quad_offset(),
                    1,
                );
            }
            // Draw right kill tile border
            if border_x1 > width + 201 {
                let offset = vec2::new((width + 201) as f32, border_y0 as f32);
                let scale = vec2::new(
                    (border_x1 - (width + 201)) as f32,
                    (border_y1 - border_y0) as f32,
                );
                self.map_graphics.render_border_tiles(
                    state,
                    texture.clone(),
                    buffer_container_index,
                    color,
                    &offset,
                    &scale,
                    visuals.base.border_kill_tile.quad_offset(),
                    1,
                );
            }
            // Draw bottom kill tile border
            if border_y1 > height + 201 {
                let offset = vec2::new(border_x0.max(-201) as f32, (height + 201) as f32);
                let scale = vec2::new(
                    (border_x1.min(width + 201) - border_x0.max(-201)) as f32,
                    (border_y1 - (height + 201)) as f32,
                );
                self.map_graphics.render_border_tiles(
                    state,
                    texture,
                    buffer_container_index,
                    color,
                    &offset,
                    &scale,
                    visuals.base.border_kill_tile.quad_offset(),
                    1,
                );
            }
        }
    }

    fn prepare_quad_rendering_grouped(
        color_anims: &FxHashMap<(usize, time::Duration), nfvec4>,
        pos_anims: &FxHashMap<(usize, time::Duration), fvec3>,
        color_anim: Option<usize>,
        color_anim_offset: &time::Duration,
        pos_anim: Option<usize>,
        pos_anim_offset: &time::Duration,
        mut flush_or_add: impl FnMut(QuadFlushOrAdd),
    ) {
        let color = if let Some(anim) = {
            if let Some(color_anim) = color_anim {
                color_anims.get(&(color_anim, *color_anim_offset)).copied()
            } else {
                None
            }
        } {
            anim
        } else {
            nfvec4::new(
                nffixed::from_num(1),
                nffixed::from_num(1),
                nffixed::from_num(1),
                nffixed::from_num(1),
            )
        };

        let mut offset_x = 0.0;
        let mut offset_y = 0.0;
        let mut rot = 0.0;

        if let Some(pos_channels) = {
            if let Some(pos_anim) = pos_anim {
                pos_anims.get(&(pos_anim, *pos_anim_offset))
            } else {
                None
            }
        } {
            offset_x = pos_channels.x.to_num();
            offset_y = pos_channels.y.to_num();
            rot = pos_channels.z.to_num::<f32>() / 180.0 * PI;
        }

        let is_fully_transparent = color.a() <= 0;
        let needs_flush = is_fully_transparent;

        if needs_flush {
            flush_or_add(QuadFlushOrAdd::Flush {
                fully_transparent_color: is_fully_transparent,
            });
        }

        if !is_fully_transparent {
            flush_or_add(QuadFlushOrAdd::Add {
                info: QuadRenderInfo::new(
                    ColorRgba {
                        r: color.r().to_num(),
                        g: color.g().to_num(),
                        b: color.b().to_num(),
                        a: color.a().to_num(),
                    },
                    vec2::new(offset_x, offset_y),
                    rot,
                ),
            });
        }
    }

    pub fn prepare_quad_rendering(
        mut stream_handle: StreamedUniforms<'_, QuadRenderInfo>,
        color_anims: &FxHashMap<(usize, time::Duration), nfvec4>,
        pos_anims: &FxHashMap<(usize, time::Duration), fvec3>,
        cur_quad_offset: &Cell<usize>,
        quads: &[Quad],
        first_index: usize,
    ) {
        for (i, quad) in quads.iter().enumerate() {
            Self::prepare_quad_rendering_grouped(
                color_anims,
                pos_anims,
                quad.color_anim,
                &quad.color_anim_offset,
                quad.pos_anim,
                &quad.pos_anim_offset,
                |reason| {
                    match reason {
                        QuadFlushOrAdd::Flush {
                            fully_transparent_color,
                        } => {
                            stream_handle.flush();
                            cur_quad_offset.set(i + first_index);
                            if fully_transparent_color {
                                // since this quad is ignored, the offset is the next quad
                                cur_quad_offset.set(cur_quad_offset.get() + 1);
                            }
                        }
                        QuadFlushOrAdd::Add { info } => {
                            stream_handle.add(info);
                        }
                    }
                },
            );
        }
    }

    fn prepare_group_rendering(
        &self,
        color_anims: &FxHashMap<(usize, time::Duration), nfvec4>,
        pos_anims: &FxHashMap<(usize, time::Duration), fvec3>,
        color_anim: Option<usize>,
        color_anim_offset: &time::Duration,
        pos_anim: Option<usize>,
        pos_anim_offset: &time::Duration,
        range: Range<usize>,
        state: &State,
        texture: &TextureType,
        buffer_object: &BufferObject,
    ) {
        Self::prepare_quad_rendering_grouped(
            color_anims,
            pos_anims,
            color_anim,
            color_anim_offset,
            pos_anim,
            pos_anim_offset,
            |reason| {
                if let QuadFlushOrAdd::Add { info } = reason {
                    self.map_graphics.render_quad_layer_grouped(
                        state,
                        texture.clone(),
                        buffer_object,
                        range.end - range.start,
                        range.start,
                        info,
                    );
                }
            },
        );
    }

    fn render_quads_with_anim(
        &self,
        state: &State,
        texture: &TextureType,
        color_anims: &FxHashMap<(usize, time::Duration), nfvec4>,
        pos_anims: &FxHashMap<(usize, time::Duration), fvec3>,
        quads: &[Quad],
        buffer_container: &BufferObject,
        first_index: usize,
    ) {
        let map_graphics = &self.map_graphics;
        let cur_quad_offset_cell = Cell::new(first_index);
        let cur_quad_offset = &cur_quad_offset_cell;
        self.stream_handle.fill_uniform_instance(
            hi_closure!(
                [
                    color_anims: &FxHashMap<(usize, time::Duration), nfvec4>,
                    pos_anims: &FxHashMap<(usize, time::Duration), fvec3>,
                    cur_quad_offset: &Cell<usize>,
                    quads: &[Quad],
                    first_index: usize,
                ],
                |stream_handle: StreamedUniforms<
                    '_,
                    QuadRenderInfo,
                >|
                    -> () {
                    RenderMap::prepare_quad_rendering(
                        stream_handle,
                        color_anims,
                        pos_anims,
                        cur_quad_offset,
                        quads,
                        first_index
                    );
                }
            ),
            hi_closure!([
                map_graphics: &MapGraphics,
                state: &State,
                texture: &TextureType,
                buffer_container: &BufferObject,
                cur_quad_offset: &Cell<usize>
            ],
            |instance: usize, count: usize| -> () {
                map_graphics.render_quad_layer(
                    state,
                    texture.clone(),
                    buffer_container,
                    instance,
                    count,
                    cur_quad_offset.get(),
                );
                cur_quad_offset.set(cur_quad_offset.get() + count);
            }),
        );
    }

    pub fn prepare_quad_anims<AN: HiarcTrait, AS: HiarcTrait>(
        pos_anims: &Pool<FxHashMap<(usize, time::Duration), fvec3>>,
        color_anims: &Pool<FxHashMap<(usize, time::Duration), nfvec4>>,
        cur_time: &Duration,
        cur_anim_time: &Duration,
        include_last_anim_point: bool,
        visuals: &QuadLayerVisuals,
        animations: &AnimationsSkeleton<AN, AS>,
    ) -> QuadAnimEvalResult {
        let mut pos_anims_values = pos_anims.new();
        let mut color_anims_values = color_anims.new();

        for &(pos_anim, pos_anim_offset) in &visuals.pos_anims {
            if let Some(anim) = animations.pos.get(pos_anim) {
                let pos_channels = RenderMap::animation_eval(
                    &anim.def,
                    cur_time,
                    cur_anim_time,
                    &pos_anim_offset,
                    include_last_anim_point,
                );
                pos_anims_values.insert((pos_anim, pos_anim_offset), pos_channels);
            }
        }
        for &(color_anim, color_anim_offset) in &visuals.color_anims {
            if let Some(anim) = animations.color.get(color_anim) {
                let color_channels = RenderMap::animation_eval(
                    &anim.def,
                    cur_time,
                    cur_anim_time,
                    &color_anim_offset,
                    include_last_anim_point,
                );
                color_anims_values.insert((color_anim, color_anim_offset), color_channels);
            }
        }

        QuadAnimEvalResult {
            pos_anims_values,
            color_anims_values,
        }
    }

    fn render_quad_layer<AN: HiarcTrait, AS: HiarcTrait>(
        &self,
        state: &State,
        texture: TextureType,
        cur_time: &Duration,
        cur_anim_time: &Duration,
        include_last_anim_point: bool,
        visuals: &QuadLayerVisuals,
        animations: &AnimationsSkeleton<AN, AS>,
        quads: &[Quad],
    ) {
        let QuadAnimEvalResult {
            pos_anims_values,
            color_anims_values,
        } = Self::prepare_quad_anims(
            &self.pos_anims,
            &self.color_anims,
            cur_time,
            cur_anim_time,
            include_last_anim_point,
            visuals,
            animations,
        );

        if let Some(buffer_container) = &visuals.buffer_object_index {
            let texture = &texture;
            for draw_range in &visuals.draw_ranges {
                match draw_range.anim {
                    QuadVisualRangeAnim::NoAnim => {
                        self.map_graphics.render_quad_layer_grouped(
                            state,
                            texture.clone(),
                            buffer_container,
                            draw_range.range.end - draw_range.range.start,
                            draw_range.range.start,
                            QuadRenderInfo {
                                color: ColorRgba::new(1.0, 1.0, 1.0, 1.0),
                                offsets: Default::default(),
                                rotation: 0.0,
                                padding: 0.0,
                            },
                        );
                    }
                    QuadVisualRangeAnim::ColorAnim { anim, anim_offset } => {
                        self.prepare_group_rendering(
                            &color_anims_values,
                            &pos_anims_values,
                            Some(anim),
                            &anim_offset,
                            None,
                            &Default::default(),
                            draw_range.range.clone(),
                            state,
                            texture,
                            buffer_container,
                        );
                    }
                    QuadVisualRangeAnim::PosAnim { anim, anim_offset } => {
                        self.prepare_group_rendering(
                            &color_anims_values,
                            &pos_anims_values,
                            None,
                            &Default::default(),
                            Some(anim),
                            &anim_offset,
                            draw_range.range.clone(),
                            state,
                            texture,
                            buffer_container,
                        );
                    }
                    QuadVisualRangeAnim::FullAnim {
                        pos,
                        pos_offset,
                        color,
                        color_offset,
                    } => {
                        self.prepare_group_rendering(
                            &color_anims_values,
                            &pos_anims_values,
                            Some(color),
                            &color_offset,
                            Some(pos),
                            &pos_offset,
                            draw_range.range.clone(),
                            state,
                            texture,
                            buffer_container,
                        );
                    }
                    QuadVisualRangeAnim::Chaos => {
                        self.render_quads_with_anim(
                            state,
                            texture,
                            &color_anims_values,
                            &pos_anims_values,
                            &quads[draw_range.range.clone()],
                            buffer_container,
                            draw_range.range.start,
                        );
                    }
                }
            }
        }
    }

    fn get_physics_layer_texture<'a, L>(
        layer: &MapLayerPhysicsSkeleton<L>,
        entities: &'a Entities,
        physics_group_name: &str,
    ) -> &'a TextureContainer2dArray {
        if let MapLayerPhysicsSkeleton::Speedup(_) = layer {
            &entities.speedup
        } else {
            entities.get_or_default(physics_group_name)
        }
    }

    #[must_use]
    pub fn set_group_clipping(
        &self,
        state: &mut State,
        camera: &dyn CameraInterface,
        clipping: &MapGroupAttrClipping,
    ) -> bool {
        let mut fake_state = State::new();
        camera.project(&self.canvas_handle, &mut fake_state, None);
        let (tl_x, tl_y, br_x, br_y) = fake_state.get_canvas_mapping();

        let x0 = (clipping.pos.x.to_num::<f32>() - tl_x) / (br_x - tl_x);
        let y0 = (clipping.pos.y.to_num::<f32>() - tl_y) / (br_y - tl_y);
        let x1 = ((clipping.pos.x.to_num::<f32>() + clipping.size.x.to_num::<f32>()) - tl_x)
            / (br_x - tl_x);
        let y1 = ((clipping.pos.y.to_num::<f32>() + clipping.size.y.to_num::<f32>()) - tl_y)
            / (br_y - tl_y);

        if x1 < 0.0 || x0 > 1.0 || y1 < 0.0 || y0 > 1.0 {
            // group is not visible at all
            return false;
        }

        let (x, y, w, h) = State::auto_round_clipping(
            x0 * self.canvas_handle.canvas_width() as f32,
            y0 * self.canvas_handle.canvas_height() as f32,
            (x1 - x0) * self.canvas_handle.canvas_width() as f32,
            (y1 - y0) * self.canvas_handle.canvas_height() as f32,
        );

        state.clip_clamped(
            x,
            y,
            w,
            h,
            self.canvas_handle.canvas_width(),
            self.canvas_handle.canvas_height(),
        );

        true
    }

    pub fn render_sounds<'a, AN, AS>(
        stream_handle: &GraphicsStreamHandle,
        animations: &AnimationsSkeleton<AN, AS>,
        cur_time: &Duration,
        cur_anim_time: &Duration,
        include_last_anim_point: bool,
        sounds: impl Iterator<Item = &'a Sound>,
        state: State,
    ) {
        for sound in sounds {
            let mut pos = sound.pos;
            let mut rot = 0.0;
            if let Some(anim) = {
                if let Some(pos_anim) = sound.pos_anim {
                    animations.pos.get(pos_anim)
                } else {
                    None
                }
            } {
                let pos_channels = RenderMap::animation_eval(
                    &anim.def,
                    cur_time,
                    cur_anim_time,
                    &sound.pos_anim_offset,
                    include_last_anim_point,
                );
                pos.x += pos_channels.x;
                pos.y += pos_channels.y;
                rot = pos_channels.z.to_num::<f32>() / 180.0 * PI;
            }
            match sound.shape {
                SoundShape::Rect { size } => {
                    let center = vec2::new(pos.x.to_num(), pos.y.to_num());
                    let quad = StreamedQuad::default()
                        .from_center_and_size(center, vec2::new(size.x.to_num(), size.y.to_num()))
                        .color(ubvec4::new(150, 200, 255, 100))
                        .tex_default()
                        .rotate_pos(rot);

                    RenderTools::render_rect_free(stream_handle, quad, state, None);

                    if !sound.falloff.is_zero() {
                        let quad = StreamedQuad::default()
                            .from_center_and_size(
                                center,
                                vec2::new(
                                    (size.x * sound.falloff.to_num::<uffixed>()).to_num(),
                                    (size.y * sound.falloff.to_num::<uffixed>()).to_num(),
                                ),
                            )
                            .color(ubvec4::new(150, 200, 255, 100))
                            .tex_default()
                            .rotate_pos(rot);
                        RenderTools::render_rect_free(stream_handle, quad, state, None);
                    }
                }
                SoundShape::Circle { radius } => {
                    RenderTools::render_circle(
                        stream_handle,
                        &vec2::new(pos.x.to_num(), pos.y.to_num()),
                        radius.to_num(),
                        &ubvec4::new(150, 200, 255, 100),
                        state,
                    );

                    if !sound.falloff.is_zero() {
                        RenderTools::render_circle(
                            stream_handle,
                            &vec2::new(pos.x.to_num(), pos.y.to_num()),
                            (radius * sound.falloff.to_num::<uffixed>()).to_num(),
                            &ubvec4::new(150, 200, 255, 100),
                            state,
                        );
                    }
                }
            }
        }
    }

    pub fn render_layer<T, Q, AN: HiarcTrait, AS: HiarcTrait, S, A>(
        &self,
        animations: &AnimationsSkeleton<AN, AS>,
        resources: &MapResourcesSkeleton<
            (),
            impl Borrow<TextureContainer>,
            impl Borrow<TextureContainer2dArray>,
            impl Borrow<SoundObject>,
        >,
        config: &ConfigMap,
        camera: &dyn CameraInterface,
        cur_time: &Duration,
        cur_anim_time: &Duration,
        include_last_anim_point: bool,
        group_attr: &MapGroupAttr,
        layer: &MapVisualLayerBase<T, Q, S, A>,
        // this can be used to overwrite the layer's texture. only useful for the editor
        forced_texture: Option<ForcedTexture>,
    ) where
        T: Borrow<TileLayerVisuals>,
        Q: Borrow<QuadLayerVisuals>,
    {
        // skip rendering if detail layers if not wanted
        if layer.high_detail() && !config.high_detail {
            return;
        }

        let mut state = State::new();

        // clipping
        if let Some(clipping) = &group_attr.clipping {
            // set clipping
            if !self.set_group_clipping(&mut state, camera, clipping) {
                return;
            }
        }

        camera.project(&self.canvas_handle, &mut state, Some(group_attr));

        match layer {
            MapVisualLayerBase::Tile(layer) => {
                let visual = layer.user.borrow();
                let layer = &layer.layer;
                let texture = if let Some(
                    ForcedTexture::TileLayer(forced_texture)
                    | ForcedTexture::TileLayerTileIndex(forced_texture)
                    | ForcedTexture::TileLayerTileFlag(forced_texture),
                ) = forced_texture
                {
                    Some(forced_texture)
                } else {
                    layer
                        .attr
                        .image_array
                        .map(|image| resources.image_arrays[image].user.borrow())
                };

                let color = ColorRgba {
                    r: layer.attr.color.r().to_num::<f32>(),
                    g: layer.attr.color.g().to_num::<f32>(),
                    b: layer.attr.color.b().to_num::<f32>(),
                    a: layer.attr.color.a().to_num::<f32>()
                        * (100 - config.physics_layer_opacity) as f32
                        / 100.0,
                };

                state.blend(BlendType::Alpha);

                let (buffer_object, shader_storage) =
                    if matches!(forced_texture, Some(ForcedTexture::TileLayerTileIndex(_))) {
                        (
                            &visual.tile_index_obj.buffer_object,
                            &visual.tile_index_obj.shader_storage,
                        )
                    } else if matches!(forced_texture, Some(ForcedTexture::TileLayerTileFlag(_))) {
                        (
                            &visual.tile_flag_obj.buffer_object,
                            &visual.tile_flag_obj.shader_storage,
                        )
                    } else {
                        (
                            &visual.base.obj.buffer_object,
                            &visual.base.obj.shader_storage,
                        )
                    };

                self.render_tile_layer(
                    &state,
                    texture.into(),
                    cur_time,
                    cur_anim_time,
                    include_last_anim_point,
                    &visual.base.base,
                    buffer_object,
                    shader_storage,
                    &layer.attr.color_anim,
                    &layer.attr.color_anim_offset,
                    animations,
                    color,
                );
            }
            MapVisualLayerBase::Quad(layer) => {
                let visual = layer.user.borrow();
                let layer = &layer.layer;
                let texture = if let Some(ForcedTexture::QuadLayer(forced_texture)) = forced_texture
                {
                    Some(forced_texture)
                } else {
                    layer
                        .attr
                        .image
                        .map(|image| resources.images[image].user.borrow())
                };

                if config.show_quads {
                    state.blend(BlendType::Alpha);
                    self.render_quad_layer(
                        &state,
                        texture.into(),
                        cur_time,
                        cur_anim_time,
                        include_last_anim_point,
                        visual,
                        animations,
                        &layer.quads,
                    );
                }
            }
            MapVisualLayerBase::Sound(layer) => {
                // render sound properties
                // note that this should only be called for e.g. the editor
                Self::render_sounds(
                    &self.stream_handle,
                    animations,
                    cur_time,
                    cur_anim_time,
                    include_last_anim_point,
                    layer.layer.sounds.iter(),
                    state,
                );
            }
            _ => {
                panic!("this layer is not interesting for rendering, fix your map & code");
            }
        }
    }

    pub fn render_physics_layer<AN, AS, L>(
        &self,
        animations: &AnimationsSkeleton<AN, AS>,
        entities_container: &mut EntitiesContainer,
        entities_key: Option<&ContainerKey>,
        physics_group_name: &str,
        layer: &MapLayerPhysicsSkeleton<L>,
        camera: &dyn CameraInterface,
        cur_time: &Duration,
        cur_anim_time: &Duration,
        include_last_anim_point: bool,
        physics_layer_opacity: u8,
        // force a texture over the one that will be rendered
        // this is usually only useful for the editor
        forced_texture: Option<ForcedTexture>,
    ) where
        L: Borrow<PhysicsTileLayerVisuals>,
    {
        let entities = entities_container.get_or_default_opt(entities_key);
        let mut state = State::new();

        camera.project(&self.canvas_handle, &mut state, None);

        let is_main_physics_layer = matches!(layer, MapLayerPhysicsSkeleton::Game(_));

        let color = ColorRgba {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: physics_layer_opacity as f32 / 100.0,
        };

        state.blend(BlendType::Alpha);

        let texture = Self::get_physics_layer_texture(layer, entities, physics_group_name);
        // draw kill tiles outside the entity clipping rectangle
        if is_main_physics_layer {
            // slow blinking to hint that it's not a part of the map
            let seconds = cur_time.as_secs_f64();
            let color_hint = ColorRgba {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.3 + 0.7 * (1.0 + (2.0 * PI as f64 * seconds / 3.0).sin() as f32) / 2.0,
            };

            let color_kill = ColorRgba {
                r: color.r * color_hint.r,
                g: color.g * color_hint.g,
                b: color.b * color_hint.b,
                a: color.a * color_hint.a,
            };
            self.render_kill_tile_border(
                &state,
                texture.into(),
                &layer.user().borrow().base.base,
                &color_kill,
            );
        }

        let base = &layer.user().borrow().base;
        let (buffer_object, shader_storage) =
            if matches!(forced_texture, Some(ForcedTexture::TileLayerTileIndex(_))) {
                (
                    &base.tile_index_obj.buffer_object,
                    &base.tile_index_obj.shader_storage,
                )
            } else if matches!(forced_texture, Some(ForcedTexture::TileLayerTileFlag(_))) {
                (
                    &base.tile_flag_obj.buffer_object,
                    &base.tile_flag_obj.shader_storage,
                )
            } else {
                (&base.base.obj.buffer_object, &base.base.obj.shader_storage)
            };
        self.render_tile_layer(
            &state,
            if let Some(
                ForcedTexture::TileLayer(forced_texture)
                | ForcedTexture::TileLayerTileIndex(forced_texture)
                | ForcedTexture::TileLayerTileFlag(forced_texture),
            ) = forced_texture
            {
                Some(forced_texture)
            } else {
                Some(texture)
            }
            .into(),
            cur_time,
            cur_anim_time,
            include_last_anim_point,
            &layer.user().borrow().base.base.base,
            buffer_object,
            shader_storage,
            &None,
            &time::Duration::ZERO,
            animations,
            color,
        );
        for overlay in layer.user().borrow().overlays.iter() {
            let texture = match overlay.ty {
                MapRenderTextOverlayType::Top => &entities.text_overlay_top,
                MapRenderTextOverlayType::Bottom => &entities.text_overlay_bottom,
                MapRenderTextOverlayType::Center => &entities.text_overlay_center,
            };
            self.render_tile_layer(
                &state,
                texture.into(),
                cur_time,
                cur_anim_time,
                include_last_anim_point,
                &overlay.visuals.base,
                &overlay.visuals.obj.buffer_object,
                &overlay.visuals.obj.shader_storage,
                &None,
                &time::Duration::ZERO,
                animations,
                color,
            );
        }
    }

    fn render_design_impl<'a>(
        &self,
        map: &MapVisual,
        pipe: &RenderPipelineBase,
        render_layers: impl Iterator<Item = &'a MapRenderLayer>,
        layer_ty: RenderLayerType,
    ) {
        if pipe.config.physics_layer_opacity == 100 {
            return;
        }

        for render_layer in render_layers.filter(|render_layer| {
            if let MapRenderLayer::Tile(_) = render_layer
                && matches!(layer_ty, RenderLayerType::Background)
                && !pipe.config.background_show_tile_layers
            {
                return false;
            }
            true
        }) {
            let render_info = render_layer.get_render_info();
            let groups = if matches!(layer_ty, RenderLayerType::Background) {
                &map.groups.background
            } else {
                &map.groups.foreground
            };
            let group = &groups[render_info.group_index];

            self.render_layer(
                &map.animations,
                &map.resources,
                pipe.config,
                pipe.camera,
                pipe.cur_time,
                pipe.cur_anim_time,
                pipe.include_last_anim_point,
                &group.attr,
                &group.layers[render_info.layer_index],
                None,
            );
        }
    }

    pub fn render_physics_layers(
        &self,
        pipe: &mut RenderPipelinePhysics,
        render_infos: &[MapPhysicsRenderInfo],
    ) {
        for render_info in render_infos {
            self.render_physics_layer(
                &pipe.base.map.animations,
                pipe.entities_container,
                pipe.entities_key,
                pipe.physics_group_name,
                &pipe.base.map.groups.physics.layers[render_info.layer_index],
                pipe.base.camera,
                pipe.base.cur_time,
                pipe.base.cur_anim_time,
                pipe.base.include_last_anim_point,
                pipe.base.config.physics_layer_opacity,
                None,
            );
        }
    }

    pub fn render_background(&self, pipe: &RenderPipeline) {
        self.render_design_impl(
            pipe.base.map,
            &pipe.base,
            pipe.buffered_map.render.background_render_layers.iter(),
            RenderLayerType::Background,
        );
        self.sound.handle_background(
            pipe.base.cur_time,
            pipe.base.cur_anim_time,
            pipe.base.include_last_anim_point,
            pipe.base.map,
            pipe.buffered_map,
            pipe.base.camera,
            pipe.base.map_sound_volume,
        );
    }

    pub fn render_foreground(&self, pipe: &RenderPipeline) {
        self.render_design_impl(
            pipe.base.map,
            &pipe.base,
            pipe.buffered_map.render.foreground_render_layers.iter(),
            RenderLayerType::Foreground,
        );
        self.sound.handle_foreground(
            pipe.base.cur_time,
            pipe.base.cur_anim_time,
            pipe.base.include_last_anim_point,
            pipe.base.map,
            pipe.buffered_map,
            pipe.base.camera,
            pipe.base.map_sound_volume,
        );
    }

    /// render the whole map but only with design layers at full opacity
    pub fn render_full_design(&self, map: &MapVisual, pipe: &RenderPipeline) {
        self.render_design_impl(
            map,
            &pipe.base,
            pipe.buffered_map.render.background_render_layers.iter(),
            RenderLayerType::Background,
        );
        self.render_design_impl(
            map,
            &pipe.base,
            pipe.buffered_map.render.foreground_render_layers.iter(),
            RenderLayerType::Foreground,
        );
    }
}
