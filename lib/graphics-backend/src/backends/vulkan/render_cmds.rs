use std::sync::Arc;

use ash::vk;
use graphics_backend_traits::plugin::{
    BackendRenderInterface, BackendShaderStage, SubRenderPassAttributes,
};
use graphics_types::{
    commands::{
        CommandClear, CommandRender, CommandRenderQuadContainer,
        CommandRenderQuadContainerAsSpriteMultiple, CommandsRender, CommandsRenderMod,
        CommandsRenderQuadContainer, CommandsRenderStream, GRAPHICS_DEFAULT_UNIFORM_SIZE,
        GRAPHICS_MAX_UNIFORM_RENDER_COUNT, PrimType, RenderSpriteInfo,
    },
    rendering::{ColorRgba, RenderModeGlass, State, StateTexture, WrapType},
};
use math::math::vector::{vec2, vec4};

use crate::backends::vulkan::vulkan_uniform::UniformGGlass;

use super::{
    command_pool::AutoCommandBuffer,
    logical_device::LogicalDevice,
    render_fill_manager::RenderCommandExecuteBuffer,
    render_manager::RenderManager,
    render_pass::CanvasSetup,
    vulkan::VulkanCustomPipes,
    vulkan_types::{RenderPassType, SupportedAddressModes},
    vulkan_uniform::{
        SUniformPrimExGVertColor, SUniformSpriteMultiGVertColor, UniformGBlur, UniformGPos,
        UniformPrimExGPos, UniformPrimExGPosRotationless, UniformPrimExGVertColorAlign,
        UniformSpriteMultiGPos, UniformSpriteMultiGVertColorAlign,
    },
};

pub fn get_address_mode_index(state: &State) -> usize {
    if state.wrap_mode == WrapType::Repeat {
        SupportedAddressModes::Repeat as usize
    } else {
        SupportedAddressModes::ClampEdges as usize
    }
}

enum RenderMode {
    Normal,
    Blur {
        blur_radius: f32,
        scale: vec2,
        blur_color: vec4,
    },
    Glass(RenderModeGlass),
}

fn render_blur(
    mut render_manager: RenderManager,
    state: &State,
    texture_index: &StateTexture,
    prim_type: PrimType,
    primitive_count: usize,
    blur_radius: f32,
    scale: vec2,
    blur_color: vec4,
) {
    let mut m: [f32; 4 * 2] = Default::default();
    render_manager.get_state_matrix(state, &mut m);

    render_manager.bind_pipeline(state, texture_index, SubRenderPassAttributes::BlurPipeline);

    render_standard_impl::<false>(
        render_manager,
        prim_type,
        primitive_count,
        &m,
        RenderMode::Blur {
            blur_radius,
            scale,
            blur_color,
        },
    )
}

fn render_glass(
    mut render_manager: RenderManager,
    state: &State,
    texture_index: &StateTexture,
    prim_type: PrimType,
    primitive_count: usize,
    glass: RenderModeGlass,
) {
    let mut m: [f32; 4 * 2] = Default::default();
    render_manager.get_state_matrix(state, &mut m);

    render_manager.bind_pipeline(state, texture_index, SubRenderPassAttributes::GlassPipeline);

    render_standard_impl::<false>(
        render_manager,
        prim_type,
        primitive_count,
        &m,
        RenderMode::Glass(glass),
    )
}

fn render_standard_impl<const IS_3D_TEXTURED: bool>(
    render_manager: RenderManager,
    prim_type: PrimType,
    primitive_count: usize,
    m: &[f32],
    mode: RenderMode,
) {
    let mut vert_per_prim: usize = 2;
    let mut is_indexed: bool = false;
    if prim_type == PrimType::Quads {
        vert_per_prim = 4;
        is_indexed = true;
    } else if prim_type == PrimType::Triangles {
        vert_per_prim = 3;
    }

    render_manager.bind_vertex_buffer();

    if is_indexed {
        render_manager.bind_index_buffer(0);
    }
    if render_manager.is_textured() {
        render_manager.bind_texture_descriptor_sets(0, 0);
    }

    render_manager.push_constants(BackendShaderStage::VERTEX, 0, unsafe {
        std::slice::from_raw_parts(
            m.as_ptr() as *const _ as *const u8,
            std::mem::size_of_val(m),
        )
    });
    match mode {
        RenderMode::Normal => {
            // nothing to do
        }
        RenderMode::Blur {
            blur_radius,
            scale,
            blur_color,
        } => {
            let viewport_size = render_manager.viewport_size();
            let blur_push = UniformGBlur {
                texture_size: vec2::new(viewport_size.width as f32, viewport_size.height as f32),
                blur_radius,
                scale,
                color: blur_color,
            };
            render_manager.push_constants(
                BackendShaderStage::FRAGMENT,
                std::mem::size_of::<UniformGPos>() as u32,
                unsafe {
                    std::slice::from_raw_parts(
                        &blur_push as *const _ as *const u8,
                        std::mem::size_of::<UniformGBlur>(),
                    )
                },
            );
        }
        RenderMode::Glass(glass) => {
            let blur_push = UniformGGlass {
                elipse_strength: glass.elipse_strength,
                exponent_offset: glass.exponent_offset,
                decay_scale: glass.decay_scale,
                base_factor: glass.base_factor,
                deca_rate: glass.deca_rate,
                refraction_falloff: glass.refraction_falloff,
                noise: glass.noise,
                glow_weight: glass.glow_weight,
                glow_bias: glass.glow_bias,
                glow_edge0: glass.glow_edge0,
                glow_edge1: glass.glow_edge1,
                center: glass.center,
                size: glass.size,
            };
            render_manager.push_constants(
                BackendShaderStage::FRAGMENT,
                std::mem::size_of::<UniformGPos>() as u32,
                unsafe {
                    std::slice::from_raw_parts(
                        &blur_push as *const _ as *const u8,
                        std::mem::size_of::<UniformGGlass>(),
                    )
                },
            );
        }
    }

    if is_indexed {
        render_manager.draw_indexed((primitive_count * 6) as u32, 1, 0, 0, 0);
    } else {
        render_manager.draw((primitive_count * vert_per_prim) as u32, 1, 0, 0);
    }
}

fn render_standard<const IS_3D_TEXTURED: bool>(
    mut render_manager: RenderManager,
    state: &State,
    texture_index: &StateTexture,
    prim_type: PrimType,
    primitive_count: usize,
) {
    let mut m: [f32; 4 * 2] = Default::default();
    render_manager.get_state_matrix(state, &mut m);

    let is_line_geometry: bool = prim_type == PrimType::Lines;

    render_manager.bind_pipeline(
        state,
        texture_index,
        if IS_3D_TEXTURED {
            SubRenderPassAttributes::Standard3dPipeline
        } else if is_line_geometry {
            SubRenderPassAttributes::StandardLinePipeline
        } else {
            SubRenderPassAttributes::StandardPipeline
        },
    );

    render_standard_impl::<{ IS_3D_TEXTURED }>(
        render_manager,
        prim_type,
        primitive_count,
        &m,
        RenderMode::Normal,
    )
}

fn cmd_render(render_manager: RenderManager, cmd: &CommandRender) {
    render_standard::<false>(
        render_manager,
        &cmd.state,
        &cmd.texture_index,
        cmd.prim_type,
        cmd.prim_count,
    )
}

fn cmd_render_blurred(
    render_manager: RenderManager,
    cmd: &CommandRender,
    blur_radius: f32,
    scale: vec2,
    blur_color: vec4,
) {
    // draw where the stencil buffer triggered
    render_blur(
        render_manager,
        &cmd.state,
        &cmd.texture_index,
        cmd.prim_type,
        cmd.prim_count,
        blur_radius,
        scale,
        blur_color,
    )
}

fn cmd_render_glass(render_manager: RenderManager, cmd: &CommandRender, glass: RenderModeGlass) {
    // draw where the stencil buffer triggered
    render_glass(
        render_manager,
        &cmd.state,
        &cmd.texture_index,
        cmd.prim_type,
        cmd.prim_count,
        glass,
    )
}

fn cmd_render_quad_container_ex(
    mut render_manager: RenderManager,
    cmd: &CommandRenderQuadContainer,
) {
    let mut m: [f32; 4 * 2] = Default::default();
    render_manager.get_state_matrix(&cmd.state, &mut m);

    let is_rotationless: bool = cmd.rotation.abs() <= f32::EPSILON;
    render_manager.bind_pipeline(
        &cmd.state,
        &cmd.texture_index,
        if is_rotationless {
            SubRenderPassAttributes::PrimExRotationlessPipeline
        } else {
            SubRenderPassAttributes::PrimExPipeline
        },
    );

    render_manager.bind_vertex_buffer();

    let index_offset = (cmd
        .quad_offset
        .checked_mul(6)
        .and_then(|v| v.checked_mul(std::mem::size_of::<u32>()))
        .unwrap()) as vk::DeviceSize;

    render_manager.bind_index_buffer(index_offset);

    if render_manager.is_textured() {
        render_manager.bind_texture_descriptor_sets(0, 0);
    }

    let mut push_constant_vertex = UniformPrimExGPos::default();
    let mut vertex_push_constant_size: usize = std::mem::size_of::<UniformPrimExGPos>();

    let push_constant_color: SUniformPrimExGVertColor = cmd.vertex_color;
    push_constant_vertex.base.pos = m;

    if !is_rotationless {
        push_constant_vertex.rotation = cmd.rotation;
        push_constant_vertex.center = cmd.center;
    } else {
        vertex_push_constant_size = std::mem::size_of::<UniformPrimExGPosRotationless>();
    }

    render_manager.push_constants(BackendShaderStage::VERTEX, 0, unsafe {
        std::slice::from_raw_parts(
            &push_constant_vertex as *const UniformPrimExGPos as *const u8,
            vertex_push_constant_size,
        )
    });
    render_manager.push_constants(
        BackendShaderStage::FRAGMENT,
        (std::mem::size_of::<UniformPrimExGPos>()
            + std::mem::size_of::<UniformPrimExGVertColorAlign>()) as u32,
        unsafe {
            std::slice::from_raw_parts(
                &push_constant_color as *const ColorRgba as *const u8,
                std::mem::size_of::<SUniformPrimExGVertColor>(),
            )
        },
    );

    let index_count: u32 = (cmd.quad_num.checked_mul(6).unwrap()).try_into().unwrap();
    render_manager.draw_indexed(index_count, 1, 0, 0, 0);
}

fn cmd_render_quad_container_as_sprite_multiple(
    mut render_manager: RenderManager,
    cmd: &CommandRenderQuadContainerAsSpriteMultiple,
) {
    let mut m: [f32; 4 * 2] = Default::default();
    render_manager.get_state_matrix(&cmd.state, &mut m);

    render_manager.bind_pipeline(
        &cmd.state,
        &cmd.texture_index,
        SubRenderPassAttributes::SpriteMultiPipeline,
    );

    render_manager.bind_vertex_buffer();

    let index_offset = cmd
        .quad_offset
        .checked_mul(6)
        .and_then(|v| v.checked_mul(std::mem::size_of::<u32>()))
        .unwrap() as vk::DeviceSize;
    render_manager.bind_index_buffer(index_offset);

    render_manager.bind_texture_descriptor_sets(0, 0);

    let mut push_constant_vertex = UniformSpriteMultiGPos::default();

    let push_constant_color: SUniformSpriteMultiGVertColor = cmd.vertex_color;

    push_constant_vertex.pos = m;
    push_constant_vertex.center = cmd.center;

    render_manager.push_constants(BackendShaderStage::VERTEX, 0, unsafe {
        std::slice::from_raw_parts(
            &push_constant_vertex as *const UniformSpriteMultiGPos as *const u8,
            std::mem::size_of::<UniformSpriteMultiGPos>(),
        )
    });
    render_manager.push_constants(
        BackendShaderStage::FRAGMENT,
        (std::mem::size_of::<UniformSpriteMultiGPos>()
            + std::mem::size_of::<UniformSpriteMultiGVertColorAlign>()) as u32,
        unsafe {
            std::slice::from_raw_parts(
                &push_constant_color as *const SUniformSpriteMultiGVertColor as *const u8,
                std::mem::size_of::<SUniformSpriteMultiGVertColor>(),
            )
        },
    );

    let rsp_count: usize = GRAPHICS_MAX_UNIFORM_RENDER_COUNT
        / (std::mem::size_of::<RenderSpriteInfo>() / GRAPHICS_DEFAULT_UNIFORM_SIZE);
    let mut draw_count = cmd.instance_count;

    while draw_count > 0 {
        let uniform_count = if draw_count > rsp_count {
            rsp_count
        } else {
            draw_count
        };

        render_manager.bind_uniform_descriptor_sets(2, 0);

        let index_count: u32 = cmd.quad_num.checked_mul(6).unwrap().try_into().unwrap();
        render_manager.draw_indexed(index_count, uniform_count as u32, 0, 0, 0);

        draw_count -= uniform_count;
    }
}

fn cmd_clear(
    device: &LogicalDevice,
    exec_buffer: &RenderCommandExecuteBuffer,
    command_buffer: &AutoCommandBuffer,
    cmd: &CommandClear,
) {
    if exec_buffer.clear_color_in_render_thread {
        let clear_attachments = [vk::ClearAttachment {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            color_attachment: 0,
            clear_value: vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [cmd.color.r, cmd.color.g, cmd.color.b, cmd.color.a],
                },
            },
        }];
        let clear_rects = [vk::ClearRect {
            rect: vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: exec_buffer.viewport_size,
            },
            base_array_layer: 0,
            layer_count: 1,
        }];

        unsafe {
            device.device.cmd_clear_attachments(
                command_buffer.command_buffer,
                &clear_attachments,
                &clear_rects,
            );
        }
    }
}

pub(crate) fn command_cb_render(
    custom_pipes: &Arc<VulkanCustomPipes>,
    device: &LogicalDevice,
    render: &CanvasSetup,
    render_pass_type: RenderPassType,
    cmd_param: &CommandsRender,
    mut exec_buffer: RenderCommandExecuteBuffer,
    command_buffer: &AutoCommandBuffer,
) -> anyhow::Result<()> {
    let mut render_manager = RenderManager::new(
        device,
        command_buffer,
        &mut exec_buffer,
        render,
        render_pass_type,
    );

    match cmd_param {
        CommandsRender::Clear(cmd) => {
            cmd_clear(device, &exec_buffer, command_buffer, cmd);
            Ok(())
        }
        CommandsRender::Stream(cmd) => match cmd {
            CommandsRenderStream::Render(cmd) => {
                cmd_render(render_manager, cmd);
                Ok(())
            }
            CommandsRenderStream::RenderBlurred {
                cmd,
                blur_radius,
                scale,
                blur_color,
            } => {
                cmd_render_blurred(render_manager, cmd, *blur_radius, *scale, *blur_color);
                Ok(())
            }
            CommandsRenderStream::RenderGlass { cmd, glass } => {
                cmd_render_glass(render_manager, cmd, *glass);
                Ok(())
            }
        },
        CommandsRender::QuadContainer(cmd) => match cmd {
            CommandsRenderQuadContainer::Render(cmd) => {
                cmd_render_quad_container_ex(render_manager, cmd);
                Ok(())
            }
            CommandsRenderQuadContainer::RenderAsSpriteMultiple(cmd) => {
                cmd_render_quad_container_as_sprite_multiple(render_manager, cmd);
                Ok(())
            }
        },
        CommandsRender::Mod(CommandsRenderMod { mod_name, cmd }) => {
            if let Some(pipe_index) = custom_pipes.pipe_indices.get(mod_name.as_str()) {
                custom_pipes.pipes.read()[*pipe_index].render(cmd, &mut render_manager)
            } else {
                Err(anyhow::anyhow!("Missing mod for {}", mod_name.as_str()))
            }
        }
    }
}
