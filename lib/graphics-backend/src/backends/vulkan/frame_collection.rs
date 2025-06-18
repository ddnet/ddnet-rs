use std::sync::Arc;

use anyhow::anyhow;
use ash::vk;

use pool::mt_datatypes::PoolVec as MtPoolVec;

use crate::backends::vulkan::{
    barriers::image_barrier, image::ImageLayout,
    utils::blit_color_attachment_to_color_attachment_auto_transition,
};

use super::{
    frame::{Frame, FrameRenderCanvas},
    frame_resources::FrameResources,
    logical_device::LogicalDevice,
    render_pass::CanvasSetup,
    utils::copy_color_attachment_to_present_src,
    vulkan::{VulkanBackend, VulkanBackendProps},
    vulkan_types::{RenderPassSubType, RenderPassType},
};

pub struct FrameCollector<'a> {
    backend: &'a mut VulkanBackend,
}

impl<'a> FrameCollector<'a> {
    fn command_buffer_start_render_pass(
        device: &Arc<LogicalDevice>,
        render: &CanvasSetup,
        swap_chain_extent_info: &vk::Extent2D,
        clear_color: &[f32; 4],
        cur_image_index: u32,
        render_pass_type: RenderPassType,
        command_buffer: vk::CommandBuffer,
    ) -> anyhow::Result<()> {
        let mut render_pass_info = vk::RenderPassBeginInfo::default();
        render_pass_info = render_pass_info.render_pass(match render_pass_type {
            RenderPassType::Normal(ty) => match ty {
                RenderPassSubType::Single => render.native.render_pass.pass.pass,
                RenderPassSubType::Switching1 => render.switching.passes[0].render_pass.pass.pass,
                RenderPassSubType::Switching2 => render.switching.passes[1].render_pass.pass.pass,
            },
            RenderPassType::MultiSampling => {
                render
                    .multi_sampling
                    .as_ref()
                    .unwrap()
                    .native
                    .render_pass
                    .pass
                    .pass
            }
        });
        render_pass_info = render_pass_info
            .framebuffer(match render_pass_type {
                RenderPassType::Normal(ty) => match ty {
                    RenderPassSubType::Single => {
                        render.native.framebuffer_list[cur_image_index as usize].buffer
                    }
                    RenderPassSubType::Switching1 => {
                        render.switching.passes[0].framebuffer_list[cur_image_index as usize].buffer
                    }
                    RenderPassSubType::Switching2 => {
                        render.switching.passes[1].framebuffer_list[cur_image_index as usize].buffer
                    }
                },
                RenderPassType::MultiSampling => {
                    render
                        .multi_sampling
                        .as_ref()
                        .unwrap()
                        .native
                        .framebuffer_list[cur_image_index as usize]
                        .buffer
                }
            })
            .render_area(vk::Rect2D {
                offset: vk::Offset2D::default(),
                extent: *swap_chain_extent_info,
            });

        let clear_color_val = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [
                        clear_color[0],
                        clear_color[1],
                        clear_color[2],
                        clear_color[3],
                    ],
                },
            },
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [
                        clear_color[0],
                        clear_color[1],
                        clear_color[2],
                        clear_color[3],
                    ],
                },
            },
        ];
        let clear_color_val_switching_pass = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [
                        clear_color[0],
                        clear_color[1],
                        clear_color[2],
                        clear_color[3],
                    ],
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            },
        ];
        render_pass_info = render_pass_info.clear_values(match render_pass_type {
            RenderPassType::Normal(RenderPassSubType::Single) => &clear_color_val[..1],
            RenderPassType::MultiSampling => &clear_color_val[..2],
            RenderPassType::Normal(RenderPassSubType::Switching1)
            | RenderPassType::Normal(RenderPassSubType::Switching2) => {
                &clear_color_val_switching_pass[..2]
            }
        });

        unsafe {
            device.device.cmd_begin_render_pass(
                command_buffer,
                &render_pass_info,
                vk::SubpassContents::SECONDARY_COMMAND_BUFFERS,
            );
        }

        Ok(())
    }

    fn command_buffer_end_render_pass(
        device: &Arc<LogicalDevice>,
        render: &CanvasSetup,
        command_buffer: vk::CommandBuffer,
        render_pass_type: RenderPassType,
        cur_image_index: u32,
    ) -> anyhow::Result<()> {
        unsafe { device.device.cmd_end_render_pass(command_buffer) };

        match render_pass_type {
            RenderPassType::Normal(ty) => {
                match ty {
                    RenderPassSubType::Single => {
                        &render.native.framebuffer_list[cur_image_index as usize]
                    }
                    RenderPassSubType::Switching1 => {
                        &render.switching.passes[0].framebuffer_list[cur_image_index as usize]
                    }
                    RenderPassSubType::Switching2 => {
                        &render.switching.passes[1].framebuffer_list[cur_image_index as usize]
                    }
                }
                .transition_images()?;
            }
            RenderPassType::MultiSampling => {
                render
                    .multi_sampling
                    .as_ref()
                    .unwrap()
                    .native
                    .framebuffer_list[cur_image_index as usize]
                    .transition_images()?;
            }
        }

        Ok(())
    }

    fn advance_to_render_pass_type(
        current_frame_resources: &mut FrameResources,
        render: &CanvasSetup,
        props: &VulkanBackendProps,
        cur_image_index: u32,
        main_command_buffer: vk::CommandBuffer,
        new_render_pass_type: RenderPassType,
        cur_render_pass_type: RenderPassType,
    ) -> anyhow::Result<()> {
        if matches!(
            new_render_pass_type,
            RenderPassType::Normal(RenderPassSubType::Switching1)
                | RenderPassType::Normal(RenderPassSubType::Switching2)
        ) {
            let img = if let RenderPassType::Normal(RenderPassSubType::Switching1) =
                new_render_pass_type
            {
                &render.switching.passes[1].surface.image_list[cur_image_index as usize]
            } else {
                &render.switching.passes[0].surface.image_list[cur_image_index as usize]
            };

            // transition the current frame image to shader_read
            let img_layout = img
                .base
                .image
                .layout
                .load(std::sync::atomic::Ordering::SeqCst);
            assert!(
                img_layout == ImageLayout::Undefined || img_layout == ImageLayout::ColorAttachment,
                "{img_layout:?}"
            );
            image_barrier(
                current_frame_resources,
                &props.ash_vk.vk_device,
                main_command_buffer,
                &img.base.image,
                0,
                1,
                0,
                1,
                if img_layout == ImageLayout::Undefined {
                    vk::ImageLayout::UNDEFINED
                } else {
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
                },
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            )
            .map_err(|err| anyhow!("could not transition image for swapping framebuffer: {err}"))?;

            // if the previous pass type was single, then copy the image data of it
            // to the unused switching color attachment
            if let RenderPassType::Normal(RenderPassSubType::Single)
            | RenderPassType::MultiSampling = cur_render_pass_type
            {
                blit_color_attachment_to_color_attachment_auto_transition(
                    current_frame_resources,
                    &props.ash_vk.vk_device,
                    main_command_buffer,
                    &render.native.swap_chain_images[cur_image_index as usize],
                    &img.base.image,
                    render.native.swap_img_and_viewport_extent.width,
                    render.native.swap_img_and_viewport_extent.height,
                    render.native.swap_img_and_viewport_extent.width,
                    render.native.swap_img_and_viewport_extent.height,
                )?;
            }

            // transition the stencil buffer if needed
            let stencil =
                &render.switching.stencil_list_for_pass_transition[cur_image_index as usize];

            if stencil
                .image
                .layout
                .load(std::sync::atomic::Ordering::SeqCst)
                == ImageLayout::Undefined
            {
                image_barrier(
                    current_frame_resources,
                    &props.ash_vk.vk_device,
                    main_command_buffer,
                    &stencil.image,
                    0,
                    1,
                    0,
                    1,
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                )
                .map_err(|err| {
                    anyhow!("could not transition image for swapping framebuffer: {err}")
                })?;
            }
        }
        Ok(())
    }

    fn render_render_pass_type_ended(
        current_frame_resources: &mut FrameResources,
        render: &CanvasSetup,
        props: &VulkanBackendProps,
        cur_image_index: u32,
        main_command_buffer: vk::CommandBuffer,
        new_render_pass_type: RenderPassType,
    ) -> anyhow::Result<()> {
        if matches!(
            new_render_pass_type,
            RenderPassType::Normal(RenderPassSubType::Switching1)
                | RenderPassType::Normal(RenderPassSubType::Switching2)
        ) {
            let img = if let RenderPassType::Normal(RenderPassSubType::Switching1) =
                new_render_pass_type
            {
                &render.switching.passes[1].surface.image_list[cur_image_index as usize]
            } else {
                &render.switching.passes[0].surface.image_list[cur_image_index as usize]
            };
            // transition the current frame image to shader_read
            image_barrier(
                current_frame_resources,
                &props.ash_vk.vk_device,
                main_command_buffer,
                &img.base.image,
                0,
                1,
                0,
                1,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            )
            .map_err(|err| anyhow!("could not transition image for swapping framebuffer: {err}"))?;
        }
        Ok(())
    }

    fn finish_render_mode_frame_collecting(
        render_pass_type: RenderPassType,
        current_frame_resources: &mut FrameResources,
        render: &CanvasSetup,
        props: &VulkanBackendProps,
        cur_image_index: u32,
        main_command_buffer: vk::CommandBuffer,
    ) -> anyhow::Result<()> {
        // if the frame finished with switching passes, make sure to copy their content
        if let RenderPassType::Normal(RenderPassSubType::Switching1)
        | RenderPassType::Normal(RenderPassSubType::Switching2) = render_pass_type
        {
            // copy to presentation render pass
            let img =
                if let RenderPassType::Normal(RenderPassSubType::Switching1) = render_pass_type {
                    &render.switching.passes[0].surface.image_list[cur_image_index as usize]
                } else {
                    &render.switching.passes[1].surface.image_list[cur_image_index as usize]
                };

            copy_color_attachment_to_present_src(
                current_frame_resources,
                &props.ash_vk.vk_device,
                main_command_buffer,
                &img.base.image,
                &render.native.swap_chain_images[cur_image_index as usize],
                render.native.swap_img_and_viewport_extent.width,
                render.native.swap_img_and_viewport_extent.height,
            )?;
        }

        Ok(())
    }

    fn collect_frame_of_canvas(
        frame: &Frame,
        props: &VulkanBackendProps,
        frame_resources: &mut FrameResources,
        render_setup: &Arc<CanvasSetup>,
        render_canvas: &FrameRenderCanvas,
        main_command_buffer: vk::CommandBuffer,

        cur_image_index: u32,
        clear_color: &[f32; 4],
    ) -> anyhow::Result<()> {
        let mut did_at_least_one_render_pass = false;
        let default_pass = if render_setup.multi_sampling.is_some() {
            RenderPassType::MultiSampling
        } else {
            RenderPassType::default()
        };
        let mut cur_render_pass_type = default_pass;
        for render_pass in render_canvas.passes.iter() {
            Self::advance_to_render_pass_type(
                frame_resources,
                render_setup,
                props,
                cur_image_index,
                main_command_buffer,
                render_pass.render_pass_type,
                cur_render_pass_type,
            )?;

            // start the render pass
            Self::command_buffer_start_render_pass(
                &props.ash_vk.vk_device,
                render_setup,
                &render_setup.native.swap_img_and_viewport_extent,
                clear_color,
                cur_image_index,
                render_pass.render_pass_type,
                main_command_buffer,
            )?;
            did_at_least_one_render_pass = true;

            // collect commands
            for (index, subpass) in render_pass.subpasses.iter().enumerate() {
                if index != 0 {
                    unsafe {
                        props.ash_vk.vk_device.device.cmd_next_subpass(
                            main_command_buffer,
                            vk::SubpassContents::SECONDARY_COMMAND_BUFFERS,
                        )
                    };
                }
                // collect in order
                let mut buffers: MtPoolVec<vk::CommandBuffer> =
                    frame.command_buffer_exec_pool.new();
                buffers.extend(subpass.command_buffers.values().copied());
                unsafe {
                    props
                        .ash_vk
                        .vk_device
                        .device
                        .cmd_execute_commands(main_command_buffer, &buffers);
                }
            }

            // end render pass
            Self::command_buffer_end_render_pass(
                &props.ash_vk.vk_device,
                render_setup,
                main_command_buffer,
                render_pass.render_pass_type,
                cur_image_index,
            )?;

            Self::render_render_pass_type_ended(
                frame_resources,
                render_setup,
                props,
                cur_image_index,
                main_command_buffer,
                render_pass.render_pass_type,
            )?;

            cur_render_pass_type = render_pass.render_pass_type;
        }

        if !did_at_least_one_render_pass {
            // fake (empty) render pass
            Self::command_buffer_start_render_pass(
                &props.ash_vk.vk_device,
                render_setup,
                &render_setup.native.swap_img_and_viewport_extent,
                clear_color,
                cur_image_index,
                default_pass,
                main_command_buffer,
            )?;
            Self::command_buffer_end_render_pass(
                &props.ash_vk.vk_device,
                render_setup,
                main_command_buffer,
                default_pass,
                cur_image_index,
            )?;
        }

        Self::finish_render_mode_frame_collecting(
            cur_render_pass_type,
            frame_resources,
            render_setup,
            props,
            cur_image_index,
            main_command_buffer,
        )?;

        Ok(())
    }

    /// returns if any render pass at all was started
    fn collect_frame(&mut self) -> anyhow::Result<()> {
        let frame = self.backend.frame.lock();
        let main_command_buffer = frame.render.main_command_buffer;

        // going in reverse order. this allows transitive offscreen canvases, so that other offscreen canvases
        // can use also have offscreen canvases.
        for (_, render_canvas) in frame.render.offscreen_canvases.iter().rev() {
            let render_setup = &render_canvas.setup;
            Self::collect_frame_of_canvas(
                &frame,
                &self.backend.props,
                &mut self.backend.current_frame_resources,
                render_setup,
                &render_canvas.canvas,
                main_command_buffer,
                self.backend.render.cur_image_index,
                &self.backend.clear_color,
            )?;
            image_barrier(
                &mut self.backend.current_frame_resources,
                &self.backend.props.ash_vk.vk_device,
                main_command_buffer,
                &render_setup.native.swap_chain_images
                    [self.backend.render.cur_image_index as usize],
                0,
                1,
                0,
                1,
                self.backend.props.ash_vk.vk_device.final_layout(),
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            )
            .map_err(|_| anyhow!("could not transition image for offscreen framebuffer"))?;
        }
        // onscreen canvas always after the offscreen canvases
        Self::collect_frame_of_canvas(
            &frame,
            &self.backend.props,
            &mut self.backend.current_frame_resources,
            &self.backend.render.onscreen,
            &frame.render.onscreen_canvas,
            main_command_buffer,
            self.backend.render.cur_image_index,
            &self.backend.clear_color,
        )?;

        Ok(())
    }

    pub fn collect(backend: &'a mut VulkanBackend) -> anyhow::Result<()> {
        Self { backend }.collect_frame()
    }
}
