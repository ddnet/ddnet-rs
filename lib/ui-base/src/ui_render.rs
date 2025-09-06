use std::{collections::HashMap, rc::Rc};

use anyhow::anyhow;
use egui::{
    epaint::{self, Primitive},
    FullOutput, ImageData, TextureId,
};

use crate::{
    custom_callback::CustomCallbackTrait,
    ui::{UiContainer, UiContext},
};
use graphics::handles::{
    backend::backend::GraphicsBackendHandle,
    stream::stream::{GraphicsStreamHandle, TriangleStreamHandle},
    texture::texture::{GraphicsTextureHandle, TextureContainer},
};
use graphics_types::{
    commands::TexFlags,
    rendering::{BlendType, ColorMaskMode, GlColor, GlVertex, State, StencilMode, WrapType},
    types::GraphicsMemoryAllocationType,
};
use hiarc::hi_closure;
use math::math::vector::vec2;

fn prepare_render(
    ui: &mut UiContainer,
    shapes: Vec<epaint::ClippedShape>,
    pixels_per_point: f32,
    as_stencil: bool,
) -> (
    &mut UiContext,
    &Vec<egui::ClippedPrimitive>,
    HashMap<u64, Rc<dyn CustomCallbackTrait>>,
) {
    let context = if as_stencil {
        &mut ui.stencil_context
    } else {
        &mut ui.context
    };
    let custom_paints = if as_stencil {
        Default::default()
    } else {
        std::mem::take(&mut ui.ui_state.custom_paints)
    };

    if std::mem::take(if as_stencil {
        &mut ui.should_tesselate_stencil
    } else {
        &mut ui.should_tesselate
    }) {
        // creates triangles to paint
        *if as_stencil {
            &mut ui.last_clipped_primitives_stencil
        } else {
            &mut ui.last_clipped_primitives
        } = context.egui_ctx.tessellate(shapes, pixels_per_point);
    };
    let clipped_primitives = if as_stencil {
        &ui.last_clipped_primitives_stencil
    } else {
        &ui.last_clipped_primitives
    };

    (context, clipped_primitives, custom_paints)
}

fn render_ui_prepared(
    textures: &mut HashMap<TextureId, TextureContainer>,
    clipped_primitives: &[egui::ClippedPrimitive],
    mut custom_paints: HashMap<u64, Rc<dyn CustomCallbackTrait>>,
    textures_delta: epaint::textures::TexturesDelta,
    screen_rect: &egui::Rect,
    zoom_level: f32,
    backend_handle: &GraphicsBackendHandle,
    texture_handle: &GraphicsTextureHandle,
    stream_handle: &GraphicsStreamHandle,
    as_stencil: bool,
) {
    textures_delta.set.iter().for_each(|(texture_id, delta)| {
        if delta.pos.is_none() {
            // pos of none basically means delete the current image and recreate it
            textures.remove(texture_id);
        }
        let tex = textures.get(texture_id);
        match tex {
            // update existing texture
            Some(tex_index) => {
                let pos = delta.pos.unwrap_or_default();
                match &delta.image {
                    ImageData::Color(img) => {
                        let mut pixels = Vec::<u8>::new();
                        pixels.resize(img.width() * img.height() * 4, Default::default());
                        pixels.iter_mut().enumerate().for_each(|(index, pixel)| {
                            *pixel = img.pixels[index / 4].to_array()[index % 4];
                        });
                        tex_index
                            .update_texture(
                                pos[0] as isize,
                                pos[1] as isize,
                                img.width(),
                                img.height(),
                                pixels,
                            )
                            .unwrap();
                    } /*ImageData::Font(img_font) => {
                          let mut pixels = Vec::<u8>::new();
                          pixels.resize(img_font.width() * img_font.height() * 4, Default::default());
                          img_font
                              .srgba_pixels(None)
                              .enumerate()
                              .for_each(|(index, img_pixel)| {
                                  let texel = img_pixel.to_array();
                                  pixels.as_mut_slice()[index * 4] = texel[0];
                                  pixels.as_mut_slice()[(index * 4) + 1] = texel[1];
                                  pixels.as_mut_slice()[(index * 4) + 2] = texel[2];
                                  pixels.as_mut_slice()[(index * 4) + 3] = texel[3];
                              });
                          tex_index
                              .update_texture(
                                  pos[0] as isize,
                                  pos[1] as isize,
                                  img_font.width(),
                                  img_font.height(),
                                  pixels,
                              )
                              .unwrap();
                      }*/
                }
            }
            // create new texture
            None => {
                assert!(delta.pos.is_none(), "can this happen?");
                let tex_index;
                match &delta.image {
                    ImageData::Color(img) => {
                        let mut pixels =
                            backend_handle.mem_alloc(GraphicsMemoryAllocationType::TextureRgbaU8 {
                                width: img.width().try_into().unwrap(),
                                height: img.height().try_into().unwrap(),
                                flags: TexFlags::TEXFLAG_NOMIPMAPS,
                            });
                        pixels
                            .as_mut_slice()
                            .iter_mut()
                            .enumerate()
                            .for_each(|(index, pixel)| {
                                *pixel = img.pixels[index / 4].to_array()[index % 4];
                            });
                        tex_index =
                            Some(texture_handle.load_texture_rgba_u8(pixels, "ui").unwrap());
                    } /*ImageData::Font(img_font) => {
                          let mut pixels_mem =
                              backend_handle.mem_alloc(GraphicsMemoryAllocationType::TextureRgbaU8 {
                                  width: img_font.width().try_into().unwrap(),
                                  height: img_font.height().try_into().unwrap(),
                                  flags: TexFlags::TEXFLAG_NOMIPMAPS,
                              });
                          let pixels = pixels_mem.as_mut_slice();
                          img_font
                              .srgba_pixels(None)
                              .enumerate()
                              .for_each(|(index, img_pixel)| {
                                  let texel = img_pixel.to_array();
                                  pixels[index * 4] = texel[0];
                                  pixels[(index * 4) + 1] = texel[1];
                                  pixels[(index * 4) + 2] = texel[2];
                                  pixels[(index * 4) + 3] = texel[3];
                              });
                          tex_index = Some(
                              texture_handle
                                  .load_texture_rgba_u8(pixels_mem, "ui")
                                  .unwrap(),
                          );
                      }*/
                }
                if let Some(tex) = tex_index {
                    textures.insert(*texture_id, tex);
                }
            }
        }
    });

    clipped_primitives.iter().for_each(|v| match &v.primitive {
        Primitive::Mesh(mesh) => {
            let mut state = State::new();
            state.set_stencil_mode(if as_stencil {
                StencilMode::FillStencil
            } else {
                StencilMode::None
            });
            state.set_color_mask(if as_stencil {
                ColorMaskMode::WriteAlphaOnly
            } else {
                ColorMaskMode::WriteAll
            });
            state.map_canvas(
                screen_rect.left_top().x,
                screen_rect.left_top().y,
                screen_rect.width(),
                screen_rect.height(),
            );

            state.clip_auto_rounding(
                v.clip_rect.left_top().x * zoom_level,
                v.clip_rect.left_top().y * zoom_level,
                v.clip_rect.width() * zoom_level,
                v.clip_rect.height() * zoom_level,
            );

            state.blend(BlendType::Additive);
            state.wrap(WrapType::Clamp);
            stream_handle.stream_triangles(hi_closure!(
                [
                    textures: &mut HashMap<TextureId, TextureContainer>,
                    mesh: &egui::Mesh
                ],
                |mut stream_handle: TriangleStreamHandle<'_>| -> () {
                    let tex_index = textures.get(&mesh.texture_id);
                    if let Some(tex_index) = tex_index {
                        stream_handle.set_texture(tex_index);
                    }

                    for vert_index in 0..mesh.indices.len() / 3 {
                        let mut vertices: [GlVertex; 3] = Default::default();
                        for (i, vertex) in vertices.iter_mut().enumerate() {
                            let index = vert_index;
                            let mesh_index = mesh.indices[index * 3  + i];
                            vertex.set_pos(&vec2 {
                                x: mesh.vertices[mesh_index as usize].pos.x,
                                y: mesh.vertices[mesh_index as usize].pos.y,
                            });
                            let vert_color = mesh.vertices[mesh_index as usize].color.to_array();
                            let color = GlColor {
                                x: vert_color[0],
                                y: vert_color[1],
                                z: vert_color[2],
                                w: vert_color[3],
                            };
                            vertex.set_color(&color);

                            let tex = vec2 {
                                x: mesh.vertices[mesh_index as usize].uv.x,
                                y: mesh.vertices[mesh_index as usize].uv.y,
                            };
                            vertex.set_tex_coords(&tex);
                        }
                        stream_handle.add_vertices(vertices);
                    }
                }
            ), state);
        }
        Primitive::Callback(cb) => {
            // TODO: support custom pipes?
            let cb = cb
                .callback
                .downcast_ref::<u64>()
                .ok_or_else(|| anyhow!("Custom callback must be u64 and added over `UiState`"))
                .unwrap();

            if let Some(custom_paint) = custom_paints.remove(cb) {
                custom_paint.render();
            }
        }
    });

    // we delete textures now, so any kind of drawing has to have finished
    textures_delta.free.iter().for_each(|tex_id| {
        let _ = textures.remove(tex_id);
    });
}

pub fn render_ui(
    ui: &mut UiContainer,
    full_output: FullOutput,
    screen_rect: &egui::Rect,
    zoom_level: f32,
    backend_handle: &GraphicsBackendHandle,
    texture_handle: &GraphicsTextureHandle,
    stream_handle: &GraphicsStreamHandle,
    as_stencil: bool,
) -> egui::PlatformOutput {
    let (context, clipped_primitives, custom_paints) = prepare_render(
        ui,
        full_output.shapes,
        full_output.pixels_per_point,
        as_stencil,
    );
    render_ui_prepared(
        &mut context.textures.borrow_mut(),
        clipped_primitives,
        custom_paints,
        full_output.textures_delta,
        screen_rect,
        zoom_level,
        backend_handle,
        texture_handle,
        stream_handle,
        as_stencil,
    );
    full_output.platform_output
}
