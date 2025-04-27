use ash::vk;
use graphics_backend_traits::plugin::BackendRenderExecuteInterface;
use graphics_types::{
    commands::{CommandsRender, SColorf},
    rendering::State,
};
use hiarc::Hiarc;

use super::{
    render_cmds::get_address_mode_index,
    render_setup::RenderSetupNativeType,
    vulkan::VulkanBackend,
    vulkan_types::{
        ESupportedSamplerTypes, EVulkanBackendClipModes, RenderPassSubType, RenderPassType,
    },
};

#[derive(Debug, Hiarc, Default)]
pub struct RenderCommandExecuteBuffer {
    pub raw_render_command: Option<CommandsRender>,

    // must be calculated when the buffer gets filled
    pub estimated_render_call_count: usize,

    // useful data
    #[hiarc_skip_unsafe]
    pub buffer: vk::Buffer,
    pub buffer_off: usize,

    // up to two descriptors are supported
    #[hiarc_skip_unsafe]
    pub sampler_descriptors: [Option<vk::DescriptorSet>; 2],
    #[hiarc_skip_unsafe]
    pub texture_descriptors: [Option<vk::DescriptorSet>; 2],
    #[hiarc_skip_unsafe]
    pub uniform_descriptors: [Option<vk::DescriptorSet>; 2],
    #[hiarc_skip_unsafe]
    pub shader_storage_descriptors: Option<vk::DescriptorSet>,

    #[hiarc_skip_unsafe]
    pub index_buffer: vk::Buffer,

    pub clear_color_in_render_thread: bool,

    #[hiarc_skip_unsafe]
    pub viewport_size: vk::Extent2D,

    pub has_dynamic_state: bool,
    #[hiarc_skip_unsafe]
    pub viewport: vk::Viewport,
    #[hiarc_skip_unsafe]
    pub scissor: vk::Rect2D,
}

pub struct RenderCommandExecuteManager<'a> {
    exec_buffer: &'a mut RenderCommandExecuteBuffer,
    pub(crate) backend: &'a mut VulkanBackend,
}

impl<'a> RenderCommandExecuteManager<'a> {
    pub fn new(
        exec_buffer: &'a mut RenderCommandExecuteBuffer,
        backend: &'a mut VulkanBackend,
    ) -> Self {
        Self {
            exec_buffer,
            backend,
        }
    }

    pub fn clear_color_in_render_thread(&mut self, is_forced_clear: bool, color: SColorf) {
        if !is_forced_clear {
            let color_changed: bool = self.backend.clear_color[0] != color.r
                || self.backend.clear_color[1] != color.g
                || self.backend.clear_color[2] != color.b
                || self.backend.clear_color[3] != color.a;
            self.backend.clear_color[0] = color.r;
            self.backend.clear_color[1] = color.g;
            self.backend.clear_color[2] = color.b;
            self.backend.clear_color[3] = color.a;
            if color_changed {
                self.exec_buffer.clear_color_in_render_thread = true;
            }
        } else {
            self.exec_buffer.clear_color_in_render_thread = true;
        }
    }

    fn get_dynamic_mode_index_from_state(&self, state: &State) -> usize {
        if state.clip.is_some() || self.backend.has_dynamic_viewport {
            EVulkanBackendClipModes::DynamicScissorAndViewport as usize
        } else {
            EVulkanBackendClipModes::None as usize
        }
    }
}

impl BackendRenderExecuteInterface for RenderCommandExecuteManager<'_> {
    fn get_address_mode_index(&self, state: &State) -> u64 {
        get_address_mode_index(state) as u64
    }

    fn estimated_render_calls(&mut self, estimated_render_call_count: u64) {
        self.exec_buffer.estimated_render_call_count = estimated_render_call_count as usize;
    }

    fn set_texture(&mut self, index: u64, texture_index: u128, address_mode_index: u64) {
        self.exec_buffer.texture_descriptors[index as usize] = Some(
            self.backend
                .props
                .device
                .textures
                .get(&texture_index)
                .unwrap()
                .data
                .unwrap_2d_descr()
                .set(&mut self.backend.current_frame_resources),
        );
        self.exec_buffer.sampler_descriptors[index as usize] = Some(
            self.backend.props.device.samplers[address_mode_index as usize]
                .1
                .set(&mut self.backend.current_frame_resources),
        );
    }

    /// the color attachment of the previous render pass
    fn set_color_attachment_as_texture(&mut self, index: u64, address_mode_index: u64) {
        let img = if let Some(RenderPassType::Normal(RenderPassSubType::Switching1)) = self
            .backend
            .current_command_groups
            .get(&self.backend.render.cur_canvas())
            .map(|c| c.render_pass)
        {
            &self.backend.render.get().switching.passes[1]
                .surface
                .image_list[self.backend.cur_image_index as usize]
        } else {
            &self.backend.render.get().switching.passes[0]
                .surface
                .image_list[self.backend.cur_image_index as usize]
        };

        self.exec_buffer.texture_descriptors[index as usize] = Some(
            img.texture_descr_sets
                .set(&mut self.backend.current_frame_resources),
        );
        self.exec_buffer.sampler_descriptors[index as usize] = Some(
            self.backend.props.device.samplers[address_mode_index as usize]
                .1
                .set(&mut self.backend.current_frame_resources),
        );
    }

    fn set_offscreen_attachment_as_texture(
        &mut self,
        offscreen_id: u128,
        index: u64,
        address_mode_index: u64,
    ) {
        let RenderSetupNativeType::Offscreen { descrs, .. } = &self
            .backend
            .render
            .offscreens
            .get(&offscreen_id)
            .expect("no offscreen buffer with that id found")
            .inner_type
        else {
            panic!("implementation bug: offscreen buffer must be of type offscreen");
        };

        self.exec_buffer.texture_descriptors[index as usize] = Some(
            descrs[self.backend.cur_image_index as usize]
                .set(&mut self.backend.current_frame_resources),
        );
        self.exec_buffer.sampler_descriptors[index as usize] = Some(
            self.backend.props.device.samplers[address_mode_index as usize]
                .1
                .set(&mut self.backend.current_frame_resources),
        );
    }

    fn set_texture_3d(&mut self, index: u64, texture_index: u128) {
        self.exec_buffer.texture_descriptors[index as usize] = Some(
            self.backend
                .props
                .device
                .textures
                .get(&texture_index)
                .unwrap()
                .data
                .unwrap_3d_descr()
                .set(&mut self.backend.current_frame_resources),
        );
        self.exec_buffer.sampler_descriptors[index as usize] = Some(
            self.backend.props.device.samplers[ESupportedSamplerTypes::Texture2DArray as usize]
                .1
                .set(&mut self.backend.current_frame_resources),
        );
    }

    fn uses_stream_vertex_buffer(&mut self, offset: u64) {
        let cur_stream_buffer = &self.backend.in_use_data.cur_stream_vertex_buffer.memories[0];
        self.exec_buffer.buffer = cur_stream_buffer
            .buffer
            .get_buffer(&mut self.backend.current_frame_resources);
        self.exec_buffer.buffer_off = cur_stream_buffer.offset + offset as usize;
    }

    fn uses_stream_uniform_buffer(
        &mut self,
        uniform_index: u64,
        stream_instance_index: u64,
        uniform_descriptor_index: u64,
    ) {
        self.exec_buffer.uniform_descriptors[uniform_index as usize] = Some(
            self.backend.in_use_data.cur_stream_uniform_buffers.memories
                [stream_instance_index as usize]
                .user
                .uniform_sets[uniform_descriptor_index as usize]
                .set(&mut self.backend.current_frame_resources),
        );
    }

    fn uses_index_buffer(&mut self) {
        self.exec_buffer.index_buffer = self
            .backend
            .render_index_buffer
            .get_buffer(&mut self.backend.current_frame_resources);
    }

    fn exec_buffer_fill_dynamic_states(&mut self, state: &State) {
        let dynamic_state_index: usize = self.get_dynamic_mode_index_from_state(state);
        if dynamic_state_index == EVulkanBackendClipModes::DynamicScissorAndViewport as usize {
            let mut viewport = vk::Viewport::default();
            if self.backend.has_dynamic_viewport {
                viewport.x = self.backend.dynamic_viewport_offset.x as f32;
                viewport.y = self.backend.dynamic_viewport_offset.y as f32;
                viewport.width = self.backend.dynamic_viewport_size.width as f32;
                viewport.height = self.backend.dynamic_viewport_size.height as f32;
                viewport.min_depth = 0.0;
                viewport.max_depth = 1.0;
            } else {
                viewport.x = 0.0;
                viewport.y = 0.0;
                viewport.width = self
                    .backend
                    .render
                    .get()
                    .native
                    .swap_img_and_viewport_extent
                    .width as f32;
                viewport.height = self
                    .backend
                    .render
                    .get()
                    .native
                    .swap_img_and_viewport_extent
                    .height as f32;
                viewport.min_depth = 0.0;
                viewport.max_depth = 1.0;
            }

            let mut scissor = vk::Rect2D::default();

            let scissor_viewport = self
                .backend
                .render
                .get()
                .native
                .swap_img_and_viewport_extent;
            if let Some(clip) = &state.clip {
                scissor.offset = vk::Offset2D {
                    x: clip.x,
                    y: clip.y,
                };
                scissor.extent = vk::Extent2D {
                    width: clip.w,
                    height: clip.h,
                };
            } else {
                scissor.offset = vk::Offset2D::default();
                scissor.extent = vk::Extent2D {
                    width: scissor_viewport.width,
                    height: scissor_viewport.height,
                };
            }

            // if there is a dynamic viewport shift the clip
            if self.backend.has_dynamic_viewport && state.clip.is_some() {
                scissor.offset.x += self.backend.dynamic_viewport_offset.x;
                scissor.offset.y += self.backend.dynamic_viewport_offset.y;
            }

            viewport.x = viewport.x.clamp(0.0, f32::MAX);
            viewport.y = viewport.y.clamp(0.0, f32::MAX);

            scissor.offset.x = scissor.offset.x.clamp(0, i32::MAX);
            scissor.offset.y = scissor.offset.y.clamp(0, i32::MAX);

            self.exec_buffer.has_dynamic_state = true;
            self.exec_buffer.viewport = viewport;
            self.exec_buffer.scissor = scissor;
        } else {
            self.exec_buffer.has_dynamic_state = false;
        }
    }

    fn set_vertex_buffer(&mut self, buffer_object_index: u128) {
        self.set_vertex_buffer_with_offset(buffer_object_index, 0)
    }

    fn set_vertex_buffer_with_offset(&mut self, buffer_object_index: u128, offset: usize) {
        let buffer_object = self
            .backend
            .props
            .device
            .buffer_objects
            .get(&buffer_object_index)
            .unwrap();

        self.exec_buffer.buffer = buffer_object
            .cur_buffer
            .get_buffer(&mut self.backend.current_frame_resources);
        self.exec_buffer.buffer_off = buffer_object.cur_buffer_offset + offset;
    }

    fn set_shader_storage(&mut self, shader_storage_index: u128) {
        let shader_storage = self
            .backend
            .props
            .device
            .shader_storages
            .get(&shader_storage_index)
            .unwrap();

        self.exec_buffer.shader_storage_descriptors = Some(
            shader_storage
                .descriptor
                .set(&mut self.backend.current_frame_resources),
        );
    }
}
