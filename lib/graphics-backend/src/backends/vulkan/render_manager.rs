use std::sync::Arc;

use ash::vk;
use graphics_backend_traits::plugin::{
    BackendClearAttachment, BackendClearRect, BackendClearValue, BackendExtent2D,
    BackendImageAspectFlags, BackendOffset2D, BackendRect2D, BackendRenderInterface,
    BackendShaderStage, SubRenderPassAttributes,
};
use graphics_types::rendering::{
    BlendType, ColorMaskMode, State, StateTexture, StateTexture2dArray, StencilMode,
};

use super::{
    command_pool::AutoCommandBuffer,
    logical_device::LogicalDevice,
    pipeline_manager::PipelineManager,
    render_cmds::get_address_mode_index,
    render_fill_manager::RenderCommandExecuteBuffer,
    render_group::{ColorWriteMaskType, StencilOpType},
    render_pass::CanvasSetup,
    vulkan_types::{
        CanvasClipModes, PipelineContainer, PipelineContainerItem, RenderPassType,
        SupportedBlendModes,
    },
};

#[derive(Debug)]
pub struct RenderManagerPipeline {
    _pipeline: vk::Pipeline,
    pipe_layout: vk::PipelineLayout,

    is_textured: bool,
}

#[derive(Debug)]
pub struct RenderManager<'a> {
    device: &'a LogicalDevice,
    command_buffer: &'a AutoCommandBuffer,
    exec_buffer: &'a mut RenderCommandExecuteBuffer,
    render: &'a CanvasSetup,
    render_pass_type: RenderPassType,

    bound_pipe_line: Option<RenderManagerPipeline>,
}

impl<'a> RenderManager<'a> {
    pub fn new(
        device: &'a LogicalDevice,
        command_buffer: &'a AutoCommandBuffer,
        exec_buffer: &'a mut RenderCommandExecuteBuffer,
        render: &'a CanvasSetup,
        render_pass_type: RenderPassType,
    ) -> Self {
        Self {
            device,
            command_buffer,
            exec_buffer,
            render,
            render_pass_type,

            bound_pipe_line: None,
        }
    }

    fn get_blend_mode_index(state: &State) -> usize {
        match state.blend_mode {
            BlendType::None => SupportedBlendModes::None as usize,
            BlendType::Alpha => SupportedBlendModes::Alpha as usize,
            BlendType::Additive => SupportedBlendModes::Additive as usize,
        }
    }

    fn get_dynamic_mode_index_from_exec_buffer(exec_buffer: &RenderCommandExecuteBuffer) -> usize {
        if exec_buffer.has_dynamic_state {
            CanvasClipModes::DynamicScissorAndViewport as usize
        } else {
            CanvasClipModes::None as usize
        }
    }

    fn get_stencil_mode_index(state: &State) -> usize {
        (match state.stencil_mode {
            StencilMode::None => StencilOpType::None,
            StencilMode::FillStencil => StencilOpType::AlwaysPass,
            StencilMode::StencilNotPassed { .. } => StencilOpType::OnlyWhenNotPassed,
            StencilMode::StencilPassed => StencilOpType::OnlyWhenPassed,
        }) as usize
    }

    fn get_color_mask_index(state: &State) -> usize {
        (match state.color_mask {
            ColorMaskMode::WriteAll => ColorWriteMaskType::All,
            ColorMaskMode::WriteColorOnly => ColorWriteMaskType::ColorOnly,
            ColorMaskMode::WriteAlphaOnly => ColorWriteMaskType::AlphaOnly,
            ColorMaskMode::WriteNone => ColorWriteMaskType::None,
        }) as usize
    }

    fn get_state_indices(
        &self,
        exec_buffer: &RenderCommandExecuteBuffer,
        state: &State,
        is_texture_used: bool,
        is_textured: &mut bool,
        blend_mode_index: &mut usize,
        dynamic_index: &mut usize,
        address_mode_index: &mut usize,
        stencil_mode_index: &mut usize,
        color_mask_index: &mut usize,
    ) {
        *is_textured = is_texture_used;
        *address_mode_index = self.get_address_mode_index(state) as usize;
        *blend_mode_index = Self::get_blend_mode_index(state);
        *dynamic_index = Self::get_dynamic_mode_index_from_exec_buffer(exec_buffer);
        *stencil_mode_index = Self::get_stencil_mode_index(state);
        *color_mask_index = Self::get_color_mask_index(state);
    }

    fn get_pipeline_and_layout(
        container: &PipelineContainer,
        is_textured: bool,
        blend_mode_index: usize,
        dynamic_index: usize,
        stencil_mode_index: usize,
        color_mask_index: usize,
        address_mode_index: usize,
    ) -> (vk::Pipeline, vk::PipelineLayout) {
        let pipe_item = &container.pipelines[blend_mode_index][dynamic_index][is_textured as usize]
            [stencil_mode_index][color_mask_index][address_mode_index];
        match pipe_item {
            PipelineContainerItem::Normal { pipeline } => pipeline.pipe_and_layout(),
            PipelineContainerItem::MaybeUninit {
                pipeline_and_layout,
                pipe_create_mutex,
                creation_props,
                creation_data,
            } => {
                let pipeline = pipeline_and_layout.load();
                match pipeline.as_ref() {
                    None => {
                        let guard = pipe_create_mutex.lock();
                        let pipeline = pipeline_and_layout.load();

                        if matches!(pipeline.as_ref(), None) {
                            let pipeline_manager = PipelineManager::new(
                                &creation_data.device,
                                creation_data.multi_sampling_count,
                                &creation_data.shader_compiler,
                                creation_data.swapchain_extent,
                                creation_data.render_pass,
                                &creation_data.pipeline_cache,
                            );
                            let pipelines = pipeline_manager
                                .create_graphics_pipeline_ex(std::slice::from_ref(
                                    &creation_props.attr,
                                ))
                                .unwrap();
                            pipeline_and_layout.store(Arc::new(Some(pipelines)));
                        }

                        let res = pipeline_and_layout
                            .load()
                            .as_ref()
                            .as_ref()
                            .unwrap()
                            .pipe_and_layout();
                        drop(guard);
                        res
                    }
                    Some(pipeline) => pipeline.pipe_and_layout(),
                }
            }
            PipelineContainerItem::None => {
                panic!("pipe was never initialized.")
            }
        }
    }

    fn clear_stencil(&mut self) {
        let clear_attachments = [BackendClearAttachment {
            aspect_mask: BackendImageAspectFlags::STENCIL,
            color_attachment: 1, // TODO: this is not 1 if multi sampling is used
            clear_value: BackendClearValue::Stencil(0),
        }];
        let clear_rects = [BackendClearRect {
            rect: BackendRect2D {
                offset: BackendOffset2D { x: 0, y: 0 },
                extent: self.viewport_size(),
            },
            base_array_layer: 0,
            layer_count: 1,
        }];

        self.clear_attachments(&clear_attachments, &clear_rects);
    }

    fn backend_image_aspect_to_vk_image_aspect(
        attachment: BackendImageAspectFlags,
    ) -> vk::ImageAspectFlags {
        let mut res: vk::ImageAspectFlags = Default::default();

        if attachment.contains(BackendImageAspectFlags::COLOR) {
            res |= vk::ImageAspectFlags::COLOR;
        }
        if attachment.contains(BackendImageAspectFlags::STENCIL) {
            res |= vk::ImageAspectFlags::STENCIL;
        }

        res
    }

    fn backend_clear_value_to_vk_clear_value(clear_value: &BackendClearValue) -> vk::ClearValue {
        match clear_value {
            BackendClearValue::Color(clear_color) => vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: *clear_color,
                },
            },
            BackendClearValue::Stencil(clear_stencil_val) => vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: *clear_stencil_val,
                },
            },
        }
    }
    fn bind_pipeline_impl(
        &mut self,
        state: &State,
        is_texture_used: bool,
        pipe_name: SubRenderPassAttributes,
    ) {
        let mut is_textured: bool = Default::default();
        let mut blend_mode_index: usize = Default::default();
        let mut dynamic_index: usize = Default::default();
        let mut address_mode_index: usize = Default::default();
        let mut stencil_mode_index: usize = Default::default();
        let mut color_mask_index: usize = Default::default();
        self.get_state_indices(
            self.exec_buffer,
            state,
            is_texture_used,
            &mut is_textured,
            &mut blend_mode_index,
            &mut dynamic_index,
            &mut address_mode_index,
            &mut stencil_mode_index,
            &mut color_mask_index,
        );

        if let StencilMode::StencilNotPassed { clear_stencil } = state.stencil_mode
            && clear_stencil
        {
            self.clear_stencil();
        }

        let sub_render_pass = self.render.sub_render_pass(self.render_pass_type);
        let (pipeline, pipe_layout) = Self::get_pipeline_and_layout(
            sub_render_pass.get_by_type(pipe_name),
            is_textured,
            blend_mode_index,
            dynamic_index,
            stencil_mode_index,
            color_mask_index,
            address_mode_index,
        );

        self.bound_pipe_line = Some(RenderManagerPipeline {
            _pipeline: pipeline,
            pipe_layout,
            is_textured,
        });

        unsafe {
            self.device.device.cmd_bind_pipeline(
                self.command_buffer.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        let dynamic_state_index: usize =
            Self::get_dynamic_mode_index_from_exec_buffer(self.exec_buffer);
        if dynamic_state_index == CanvasClipModes::DynamicScissorAndViewport as usize {
            unsafe {
                self.device.device.cmd_set_viewport(
                    self.command_buffer.command_buffer,
                    0,
                    &[self.exec_buffer.viewport],
                );
            }
            unsafe {
                self.device.device.cmd_set_scissor(
                    self.command_buffer.command_buffer,
                    0,
                    &[self.exec_buffer.scissor],
                );
            }
        }
    }
}

impl BackendRenderInterface for RenderManager<'_> {
    fn get_state_matrix(&self, state: &State, matrix: &mut [f32; 4 * 2]) {
        *matrix = [
            // column 1
            2.0 / (state.canvas_br.x - state.canvas_tl.x),
            0.0,
            // column 2
            0.0,
            2.0 / (state.canvas_br.y - state.canvas_tl.y),
            // column 3
            0.0,
            0.0,
            // column 4
            -((state.canvas_tl.x + state.canvas_br.x) / (state.canvas_br.x - state.canvas_tl.x)),
            -((state.canvas_tl.y + state.canvas_br.y) / (state.canvas_br.y - state.canvas_tl.y)),
        ];
    }

    fn get_address_mode_index(&self, state: &State) -> u64 {
        get_address_mode_index(state) as u64
    }

    fn bind_pipeline(
        &mut self,
        state: &State,
        texture_index: &StateTexture,
        pipe_name: SubRenderPassAttributes,
    ) {
        self.bind_pipeline_impl(state, texture_index.is_textured(), pipe_name)
    }

    fn bind_pipeline_2d_array_texture(
        &mut self,
        state: &State,
        texture_index: &StateTexture2dArray,
        pipe_name: SubRenderPassAttributes,
    ) {
        self.bind_pipeline_impl(state, texture_index.is_textured(), pipe_name)
    }

    fn bind_vertex_buffer(&self) {
        let vertex_buffers = [self.exec_buffer.buffer];
        let buffer_offsets = [self.exec_buffer.buffer_off as vk::DeviceSize];
        unsafe {
            self.device.device.cmd_bind_vertex_buffers(
                self.command_buffer.command_buffer,
                0,
                &vertex_buffers,
                &buffer_offsets,
            );
        }
    }

    fn bind_index_buffer(&self, index_offset: vk::DeviceSize) {
        unsafe {
            self.device.device.cmd_bind_index_buffer(
                self.command_buffer.command_buffer,
                self.exec_buffer.index_buffer,
                index_offset,
                vk::IndexType::UINT32,
            );
        }
    }

    fn bind_texture_descriptor_sets(&self, first_set: u32, descriptor_index: u64) {
        unsafe {
            self.device.device.cmd_bind_descriptor_sets(
                self.command_buffer.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.bound_pipe_line
                    .as_ref()
                    .unwrap_or_else(|| panic!("no graphics pipeline was bound"))
                    .pipe_layout,
                first_set,
                &[
                    self.exec_buffer.texture_descriptors[descriptor_index as usize].unwrap(),
                    self.exec_buffer.sampler_descriptors[descriptor_index as usize].unwrap(),
                ],
                &[],
            );
        }
    }

    fn bind_uniform_descriptor_sets(&self, first_set: u32, descriptor_index: u64) {
        unsafe {
            self.device.device.cmd_bind_descriptor_sets(
                self.command_buffer.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.bound_pipe_line
                    .as_ref()
                    .unwrap_or_else(|| panic!("no graphics pipeline was bound"))
                    .pipe_layout,
                first_set,
                &[self.exec_buffer.uniform_descriptors[descriptor_index as usize].unwrap()],
                &[],
            );
        }
    }

    fn bind_shader_storage_descriptor_set(&self, first_set: u32) {
        unsafe {
            self.device.device.cmd_bind_descriptor_sets(
                self.command_buffer.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.bound_pipe_line
                    .as_ref()
                    .unwrap_or_else(|| panic!("no graphics pipeline was bound"))
                    .pipe_layout,
                first_set,
                &[self.exec_buffer.shader_storage_descriptors.unwrap()],
                &[],
            );
        }
    }

    fn push_constants(&self, stage_flags: BackendShaderStage, offset: u32, constants: &[u8]) {
        let mut stage_flags_vk: vk::ShaderStageFlags = Default::default();
        if stage_flags.contains(BackendShaderStage::VERTEX) {
            stage_flags_vk |= vk::ShaderStageFlags::VERTEX;
        }
        if stage_flags.contains(BackendShaderStage::FRAGMENT) {
            stage_flags_vk |= vk::ShaderStageFlags::FRAGMENT;
        }
        // TODO workaround for WGSL limitation (no offset)
        stage_flags_vk = vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT;

        unsafe {
            self.device.device.cmd_push_constants(
                self.command_buffer.command_buffer,
                self.bound_pipe_line
                    .as_ref()
                    .unwrap_or_else(|| panic!("no graphics pipeline was bound"))
                    .pipe_layout,
                stage_flags_vk,
                offset,
                constants,
            );
        }
    }

    fn draw_indexed(
        &self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        unsafe {
            self.device.device.cmd_draw_indexed(
                self.command_buffer.command_buffer,
                index_count,
                instance_count,
                first_index,
                vertex_offset,
                first_instance,
            );
        }
    }

    fn draw(&self, vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance: u32) {
        unsafe {
            self.device.device.cmd_draw(
                self.command_buffer.command_buffer,
                vertex_count,
                instance_count,
                first_vertex,
                first_instance,
            );
        }
    }

    fn is_textured(&self) -> bool {
        self.bound_pipe_line
            .as_ref()
            .unwrap_or_else(|| panic!("no graphics pipeline was bound"))
            .is_textured
    }

    fn viewport_size(&self) -> BackendExtent2D {
        BackendExtent2D {
            width: self.exec_buffer.viewport_size.width,
            height: self.exec_buffer.viewport_size.height,
        }
    }

    fn clear_attachments(
        &self,
        attachments: &[BackendClearAttachment],
        rects: &[BackendClearRect],
    ) {
        let attachments: Vec<vk::ClearAttachment> = attachments
            .iter()
            .map(|attachment| vk::ClearAttachment {
                aspect_mask: Self::backend_image_aspect_to_vk_image_aspect(attachment.aspect_mask),
                clear_value: Self::backend_clear_value_to_vk_clear_value(&attachment.clear_value),
                color_attachment: attachment.color_attachment,
            })
            .collect();

        let rects: Vec<vk::ClearRect> = rects
            .iter()
            .map(|rect| vk::ClearRect {
                base_array_layer: rect.base_array_layer,
                layer_count: rect.layer_count,
                rect: vk::Rect2D {
                    extent: vk::Extent2D {
                        width: rect.rect.extent.width,
                        height: rect.rect.extent.height,
                    },
                    offset: vk::Offset2D {
                        x: rect.rect.offset.x,
                        y: rect.rect.offset.y,
                    },
                },
            })
            .collect();

        unsafe {
            self.device.device.cmd_clear_attachments(
                self.command_buffer.command_buffer,
                &attachments,
                &rects,
            );
        }
    }
}
