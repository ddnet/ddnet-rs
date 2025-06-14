use std::{ffi::CString, sync::Arc};

use anyhow::anyhow;
use ash::vk;
use hiarc::Hiarc;

use crate::backends::vulkan::{pipeline_layout::PipelineLayout, pipelines::Pipelines};

use super::{
    compiler::compiler::ShaderCompiler,
    logical_device::LogicalDevice,
    pipeline_cache::PipelineCacheInner,
    render_group::{ColorWriteMaskType, StencilOpType},
    vulkan_device::Device,
    vulkan_types::{CanvasClipModes, ShaderModule, SupportedBlendModes},
};

#[derive(Debug, Hiarc, Clone)]
pub struct PipelineCreationAttributes {
    pub vert_name: String,
    pub frag_name: String,
    pub stride: u32,
    #[hiarc_skip_unsafe]
    pub input_attributes: Vec<vk::VertexInputAttributeDescription>,
    #[hiarc_skip_unsafe]
    pub set_layouts: Vec<vk::DescriptorSetLayout>,
    #[hiarc_skip_unsafe]
    pub push_constants: Vec<vk::PushConstantRange>,
    pub blend_mode: SupportedBlendModes,
    pub dynamic_mode: CanvasClipModes,
    pub is_line_prim: bool,

    pub stencil_mode: StencilOpType,
    pub color_mask: ColorWriteMaskType,
}

pub struct PipelineManager<'a> {
    device: &'a Arc<LogicalDevice>,
    multi_sampling_count: u32,
    shader_compiler: &'a Arc<ShaderCompiler>,
    swapchain_extent: vk::Extent2D,
    render_pass: vk::RenderPass,

    pipeline_cache: &'a Option<Arc<PipelineCacheInner>>,
}

impl<'a> PipelineManager<'a> {
    pub fn new(
        device: &'a Arc<LogicalDevice>,
        multi_sampling_count: u32,
        shader_compiler: &'a Arc<ShaderCompiler>,
        swapchain_extent: vk::Extent2D,
        render_pass: vk::RenderPass,
        pipeline_cache: &'a Option<Arc<PipelineCacheInner>>,
    ) -> Self {
        Self {
            device,
            multi_sampling_count,
            shader_compiler,
            swapchain_extent,
            render_pass,
            pipeline_cache,
        }
    }

    pub fn load_shader(
        shader_compiler: &Arc<ShaderCompiler>,
        file_name: &str,
    ) -> anyhow::Result<Vec<u32>> {
        let f = shader_compiler
            .shader_files
            .get(file_name)
            .ok_or(anyhow!("Shader file was not loaded: {file_name}"))?;

        Ok(f.clone())
    }

    pub fn create_shader_module(
        device: &Arc<LogicalDevice>,
        code: &[u32],
    ) -> anyhow::Result<vk::ShaderModule> {
        let create_info = vk::ShaderModuleCreateInfo::default().code(code);

        unsafe { device.device.create_shader_module(&create_info, None) }
            .map_err(|err| anyhow!("Shader module was not created: {err}"))
    }

    fn create_shaders<'b>(
        device: &Arc<LogicalDevice>,
        shader_compiler: &Arc<ShaderCompiler>,
        vert_name: &str,
        frag_name: &str,
        entry_point: &'b CString,
    ) -> anyhow::Result<(ShaderModule, [vk::PipelineShaderStageCreateInfo<'b>; 2])> {
        let vert_data_buff = Self::load_shader(shader_compiler, vert_name)?;
        let frag_data_buff = Self::load_shader(shader_compiler, frag_name)?;

        let vert_shader_module = Self::create_shader_module(device, &vert_data_buff)?;
        let frag_shader_module = Self::create_shader_module(device, &frag_data_buff)?;

        let mut shader_stages = [
            vk::PipelineShaderStageCreateInfo::default(),
            vk::PipelineShaderStageCreateInfo::default(),
        ];

        let vert_shader_stage_info = &mut shader_stages[0];
        *vert_shader_stage_info = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_shader_module)
            .name(entry_point);

        let frag_shader_stage_info = &mut shader_stages[1];
        *frag_shader_stage_info = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_shader_module)
            .name(entry_point);

        Ok((
            ShaderModule::new(vert_shader_module, frag_shader_module, device),
            shader_stages,
        ))
    }

    fn get_standard_pipeline_info<'b>(
        device: &Arc<LogicalDevice>,
        multi_sampling_count: u32,
        swapchain_extent: vk::Extent2D,
        input_assembly: &mut vk::PipelineInputAssemblyStateCreateInfo,
        viewports: &'b mut [vk::Viewport],
        scissors: &'b mut [vk::Rect2D],
        rasterizer: &mut vk::PipelineRasterizationStateCreateInfo,
        multisampling: &mut vk::PipelineMultisampleStateCreateInfo,
        color_blend_attachments: &'b mut [vk::PipelineColorBlendAttachmentState],
        blend_mode: SupportedBlendModes,
        color_mask: ColorWriteMaskType,
    ) -> anyhow::Result<(
        vk::PipelineViewportStateCreateInfo<'b>,
        vk::PipelineColorBlendStateCreateInfo<'b>,
    )> {
        input_assembly.topology = vk::PrimitiveTopology::TRIANGLE_LIST;
        input_assembly.primitive_restart_enable = vk::FALSE;

        let viewport = &mut viewports[0];
        viewport.x = 0.0;
        viewport.y = 0.0;
        viewport.width = swapchain_extent.width as f32;
        viewport.height = swapchain_extent.height as f32;
        viewport.min_depth = 0.0;
        viewport.max_depth = 1.0;

        let scissor = &mut scissors[0];
        scissor.offset = vk::Offset2D { x: 0, y: 0 };
        scissor.extent = swapchain_extent;

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewports(viewports)
            .scissors(scissors);

        rasterizer.depth_clamp_enable = vk::FALSE;
        rasterizer.rasterizer_discard_enable = vk::FALSE;
        rasterizer.polygon_mode = vk::PolygonMode::FILL;
        rasterizer.line_width = 1.0;
        rasterizer.cull_mode = vk::CullModeFlags::NONE;
        rasterizer.front_face = vk::FrontFace::CLOCKWISE;
        rasterizer.depth_bias_enable = vk::FALSE;

        multisampling.sample_shading_enable = vk::FALSE;
        multisampling.rasterization_samples =
            Device::get_sample_count(multi_sampling_count, &device.phy_device.limits);

        let color_blend_attachment = &mut color_blend_attachments[0];
        color_blend_attachment.color_write_mask = match color_mask {
            ColorWriteMaskType::All => {
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A
            }
            ColorWriteMaskType::ColorOnly => {
                vk::ColorComponentFlags::R | vk::ColorComponentFlags::G | vk::ColorComponentFlags::B
            }
            ColorWriteMaskType::AlphaOnly => vk::ColorComponentFlags::A,
            ColorWriteMaskType::None => vk::ColorComponentFlags::empty(),
        };

        color_blend_attachment.blend_enable = if blend_mode == SupportedBlendModes::None {
            vk::FALSE
        } else {
            vk::TRUE
        };

        let src_blend_factor_color = match blend_mode {
            SupportedBlendModes::Additive => vk::BlendFactor::ONE,
            SupportedBlendModes::Alpha => vk::BlendFactor::SRC_ALPHA,
            SupportedBlendModes::None => vk::BlendFactor::SRC_COLOR,
        };

        let dst_blend_factor_color = match blend_mode {
            SupportedBlendModes::Additive => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            SupportedBlendModes::Alpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            SupportedBlendModes::None => vk::BlendFactor::SRC_COLOR,
        };

        let src_blend_factor_alpha = match blend_mode {
            SupportedBlendModes::Additive => vk::BlendFactor::ONE,
            SupportedBlendModes::Alpha => vk::BlendFactor::SRC_ALPHA,
            SupportedBlendModes::None => vk::BlendFactor::SRC_COLOR,
        };

        let dst_blend_factor_alpha = match blend_mode {
            SupportedBlendModes::Additive => vk::BlendFactor::ZERO,
            SupportedBlendModes::Alpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            SupportedBlendModes::None => vk::BlendFactor::SRC_COLOR,
        };

        color_blend_attachment.src_color_blend_factor = src_blend_factor_color;
        color_blend_attachment.dst_color_blend_factor = dst_blend_factor_color;
        color_blend_attachment.color_blend_op = vk::BlendOp::ADD;
        color_blend_attachment.src_alpha_blend_factor = src_blend_factor_alpha;
        color_blend_attachment.dst_alpha_blend_factor = dst_blend_factor_alpha;
        color_blend_attachment.alpha_blend_op = vk::BlendOp::ADD;

        let mut color_blending = vk::PipelineColorBlendStateCreateInfo::default();
        color_blending.logic_op_enable = vk::FALSE;
        color_blending.logic_op = vk::LogicOp::COPY;
        color_blending.attachment_count = 1;
        color_blending = color_blending.attachments(color_blend_attachments);
        color_blending.blend_constants[0] = 0.0;
        color_blending.blend_constants[1] = 0.0;
        color_blending.blend_constants[2] = 0.0;
        color_blending.blend_constants[3] = 0.0;

        Ok((viewport_state, color_blending))
    }

    pub fn create_graphics_pipeline_ex(
        &self,
        attrs: &[PipelineCreationAttributes],
    ) -> anyhow::Result<Pipelines> {
        /// all stuff without lifetime
        #[derive(Debug, Default)]
        struct CreateStackInner {
            binding_descriptions: [vk::VertexInputBindingDescription; 1],

            viewports: [vk::Viewport; 1],
            scissors: [vk::Rect2D; 1],

            color_blend_attachments: [vk::PipelineColorBlendAttachmentState; 1],
            module: Option<ShaderModule>,
            entry_point: CString,
            dynamic_states: [vk::DynamicState; 2],
            push_constants: Vec<vk::PushConstantRange>,
        }
        #[derive(Debug, Default)]
        struct CreateStackInnerForOuter<'c> {
            shader_stages_deref: [vk::PipelineShaderStageCreateInfo<'c>; 2],
            input_assembly: vk::PipelineInputAssemblyStateCreateInfo<'c>,
            rasterizer: vk::PipelineRasterizationStateCreateInfo<'c>,
            multisampling: vk::PipelineMultisampleStateCreateInfo<'c>,
            stencil_state: vk::PipelineDepthStencilStateCreateInfo<'c>,
        }
        struct CreateStack<'b> {
            _shader_stages: [vk::PipelineShaderStageCreateInfo<'b>; 2],
            vertex_input_info: vk::PipelineVertexInputStateCreateInfo<'b>,

            viewport_state: vk::PipelineViewportStateCreateInfo<'b>,
            color_blending: vk::PipelineColorBlendStateCreateInfo<'b>,

            _pipeline_layout_info: vk::PipelineLayoutCreateInfo<'b>,

            dynamic_state_create: vk::PipelineDynamicStateCreateInfo<'b>,
        }
        struct CreateStackOuter<'c> {
            pipeline_info: vk::GraphicsPipelineCreateInfo<'c>,
        }

        let mut create_stack_inner: Vec<CreateStackInner> = Vec::with_capacity(attrs.len());
        let mut create_stack_inner_for_outer: Vec<CreateStackInnerForOuter> =
            Vec::with_capacity(attrs.len());
        let mut create_stack: Vec<CreateStack> = Vec::with_capacity(attrs.len());
        let mut create_stack_outer: Vec<CreateStackOuter> = Vec::with_capacity(attrs.len());
        create_stack_inner.resize_with(attrs.len(), Default::default);
        create_stack_inner_for_outer.resize_with(attrs.len(), Default::default);

        let mut pipe_layouts: Vec<PipelineLayout> = Default::default();

        for (attr, (create_stack_inner, create_stack_inner_for_outer)) in attrs.iter().zip(
            create_stack_inner
                .iter_mut()
                .zip(create_stack_inner_for_outer.iter_mut()),
        ) {
            create_stack_inner.entry_point = CString::new("main").unwrap();
            let (module, shader_stages) = Self::create_shaders(
                self.device,
                self.shader_compiler,
                &attr.vert_name,
                &attr.frag_name,
                &create_stack_inner.entry_point,
            )?;
            create_stack_inner.module = Some(module);

            let shader_stages_deref = [shader_stages[0], shader_stages[1]];

            let binding_descriptions = &mut create_stack_inner.binding_descriptions[0];
            binding_descriptions.binding = 0;
            binding_descriptions.stride = attr.stride;
            binding_descriptions.input_rate = vk::VertexInputRate::VERTEX;

            let binding_descriptions = if attr.input_attributes.is_empty() {
                &[]
            } else {
                create_stack_inner.binding_descriptions.as_slice()
            };

            let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::default()
                .vertex_binding_descriptions(binding_descriptions)
                .vertex_attribute_descriptions(&attr.input_attributes);

            create_stack_inner.dynamic_states =
                [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];

            let (viewport_state, color_blending) = Self::get_standard_pipeline_info(
                self.device,
                self.multi_sampling_count,
                self.swapchain_extent,
                &mut create_stack_inner_for_outer.input_assembly,
                &mut create_stack_inner.viewports,
                &mut create_stack_inner.scissors,
                &mut create_stack_inner_for_outer.rasterizer,
                &mut create_stack_inner_for_outer.multisampling,
                &mut create_stack_inner.color_blend_attachments,
                attr.blend_mode,
                attr.color_mask,
            )?;
            create_stack_inner_for_outer.input_assembly.topology = if attr.is_line_prim {
                vk::PrimitiveTopology::LINE_LIST
            } else {
                vk::PrimitiveTopology::TRIANGLE_LIST
            };

            let mut pipeline_layout_info = vk::PipelineLayoutCreateInfo::default();
            if !attr.set_layouts.is_empty() {
                pipeline_layout_info = pipeline_layout_info.set_layouts(&attr.set_layouts)
            };

            // TODO: workaround since WGSL has no offset attribute
            let push_constants_reranged: Vec<vk::PushConstantRange> = attr
                .push_constants
                .iter()
                .map(|p| {
                    let mut p = *p;
                    p.size += p.offset;
                    p.offset = 0;
                    p.stage_flags = vk::ShaderStageFlags::FRAGMENT | vk::ShaderStageFlags::VERTEX;
                    p
                })
                .collect();
            for p in push_constants_reranged {
                let range_p = p.offset..p.offset + p.size;
                if let Some((range_index, _)) = create_stack_inner
                    .push_constants
                    .iter()
                    .enumerate()
                    .find(|(_, p2)| {
                        let range_p2 = p2.offset..p2.offset + p2.size;

                        range_p.start <= range_p2.end && range_p2.start <= range_p.end
                    })
                {
                    let p2 = &mut create_stack_inner.push_constants[range_index];
                    p2.offset = p2.offset.min(p.offset);
                    p2.size = (p2.offset + p2.size).max(p.offset + p.size) - p2.offset;
                } else {
                    create_stack_inner.push_constants.push(p);
                }
            }
            // workaround end

            if !create_stack_inner.push_constants.is_empty() {
                pipeline_layout_info =
                    pipeline_layout_info.push_constant_ranges(&create_stack_inner.push_constants);
            };

            let pipe_layout = PipelineLayout::new(self.device, &pipeline_layout_info)?;

            create_stack_inner_for_outer.shader_stages_deref = shader_stages_deref;
            match attr.stencil_mode {
                StencilOpType::AlwaysPass => {
                    create_stack_inner_for_outer
                        .stencil_state
                        .stencil_test_enable = vk::TRUE;
                    create_stack_inner_for_outer.stencil_state.front.compare_op =
                        vk::CompareOp::ALWAYS;
                    create_stack_inner_for_outer.stencil_state.front.fail_op =
                        vk::StencilOp::REPLACE;
                    create_stack_inner_for_outer.stencil_state.front.pass_op =
                        vk::StencilOp::REPLACE;
                    create_stack_inner_for_outer
                        .stencil_state
                        .front
                        .depth_fail_op = vk::StencilOp::REPLACE;
                    create_stack_inner_for_outer
                        .stencil_state
                        .front
                        .compare_mask = 0xFF;
                    create_stack_inner_for_outer.stencil_state.front.write_mask = 0xFF;
                    create_stack_inner_for_outer.stencil_state.front.reference = 0x1;
                    create_stack_inner_for_outer.stencil_state.back =
                        create_stack_inner_for_outer.stencil_state.front;
                }
                StencilOpType::OnlyWhenPassed => {
                    create_stack_inner_for_outer
                        .stencil_state
                        .stencil_test_enable = vk::TRUE;
                    create_stack_inner_for_outer.stencil_state.front.compare_op =
                        vk::CompareOp::EQUAL;
                    create_stack_inner_for_outer.stencil_state.front.fail_op = vk::StencilOp::KEEP;
                    create_stack_inner_for_outer.stencil_state.front.pass_op = vk::StencilOp::KEEP;
                    create_stack_inner_for_outer
                        .stencil_state
                        .front
                        .depth_fail_op = vk::StencilOp::KEEP;
                    create_stack_inner_for_outer
                        .stencil_state
                        .front
                        .compare_mask = 0xFF;
                    create_stack_inner_for_outer.stencil_state.front.write_mask = 0xFF;
                    create_stack_inner_for_outer.stencil_state.front.reference = 0x1;
                    create_stack_inner_for_outer.stencil_state.back =
                        create_stack_inner_for_outer.stencil_state.front;
                }
                StencilOpType::OnlyWhenNotPassed => {
                    create_stack_inner_for_outer
                        .stencil_state
                        .stencil_test_enable = vk::TRUE;
                    create_stack_inner_for_outer.stencil_state.front.compare_op =
                        vk::CompareOp::NOT_EQUAL;
                    create_stack_inner_for_outer.stencil_state.front.fail_op = vk::StencilOp::KEEP;
                    create_stack_inner_for_outer.stencil_state.front.pass_op = vk::StencilOp::KEEP;
                    create_stack_inner_for_outer
                        .stencil_state
                        .front
                        .depth_fail_op = vk::StencilOp::KEEP;
                    create_stack_inner_for_outer
                        .stencil_state
                        .front
                        .compare_mask = 0xFF;
                    create_stack_inner_for_outer.stencil_state.front.write_mask = 0xFF;
                    create_stack_inner_for_outer.stencil_state.front.reference = 0x1;
                    create_stack_inner_for_outer.stencil_state.back =
                        create_stack_inner_for_outer.stencil_state.front;
                }
                StencilOpType::None => {
                    // nothing to do
                    create_stack_inner_for_outer
                        .stencil_state
                        .stencil_test_enable = vk::FALSE;
                }
            }

            let dynamic_state_create = vk::PipelineDynamicStateCreateInfo::default()
                .dynamic_states(&create_stack_inner.dynamic_states);
            create_stack.push(CreateStack {
                _shader_stages: shader_stages,
                vertex_input_info,
                viewport_state,
                color_blending,
                _pipeline_layout_info: pipeline_layout_info,
                dynamic_state_create,
            });

            pipe_layouts.push(pipe_layout);
        }
        for ((create_stack, create_stack_inner), attr) in create_stack
            .iter()
            .zip(create_stack_inner_for_outer.iter())
            .zip(attrs.iter())
        {
            let mut pipeline_info = vk::GraphicsPipelineCreateInfo::default()
                .stages(&create_stack_inner.shader_stages_deref)
                .vertex_input_state(&create_stack.vertex_input_info)
                .input_assembly_state(&create_stack_inner.input_assembly)
                .viewport_state(&create_stack.viewport_state)
                .rasterization_state(&create_stack_inner.rasterizer)
                .multisample_state(&create_stack_inner.multisampling)
                .color_blend_state(&create_stack.color_blending);
            pipeline_info = pipeline_info.depth_stencil_state(&create_stack_inner.stencil_state);
            pipeline_info.render_pass = self.render_pass;
            pipeline_info.subpass = 0;
            pipeline_info.base_pipeline_handle = vk::Pipeline::null();

            if attr.dynamic_mode == CanvasClipModes::DynamicScissorAndViewport {
                pipeline_info = pipeline_info.dynamic_state(&create_stack.dynamic_state_create);
            }

            create_stack_outer.push(CreateStackOuter { pipeline_info });
        }

        let pipeline_infos: Vec<vk::GraphicsPipelineCreateInfo<'_>> = create_stack_outer
            .into_iter()
            .map(|create_stack| create_stack.pipeline_info)
            .collect();
        let pipelines = Pipelines::new(
            self.device,
            self.pipeline_cache,
            pipeline_infos
                .into_iter()
                .zip(pipe_layouts.into_iter())
                .collect(),
        )?;

        Ok(pipelines)
    }
}
