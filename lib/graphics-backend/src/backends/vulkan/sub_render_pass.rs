use std::sync::{atomic::AtomicBool, Arc};

use anyhow::anyhow;
use ash::vk;
use base::join_all;
use graphics_backend_traits::plugin::{
    BackendPipelineLayout, BackendResourceDescription, BackendShaderStage, BackendVertexFormat,
    SubRenderPassAttributes,
};
use hiarc::Hiarc;
use num_traits::FromPrimitive;
use strum::EnumCount;

use crate::{backend::CustomPipelines, backends::vulkan::vulkan_types::SupportedBlendModes};

use super::{
    compiler::compiler::ShaderCompiler,
    logical_device::LogicalDevice,
    pipeline_cache::PipelineCacheInner,
    pipeline_manager::{PipelineCreationAttributes, PipelineManager},
    pipelines::Pipelines,
    render_group::{ColorWriteMaskType, StencilOpType},
    vulkan_device::DescriptorLayouts,
    vulkan_types::{
        CanvasClipModes, PipelineContainer, PipelineContainerCreateMode, PipelineContainerItem,
        PipelineCreationAttributesEx, PipelineCreationOneByOne, PipelineCreationProps,
        SupportedSamplerTypes,
    },
    vulkan_uniform::{
        SUniformPrimExGVertColor, SUniformSpriteMultiGVertColor, UniformGBlur, UniformGPos,
        UniformPrimExGPos, UniformPrimExGPosRotationless, UniformPrimExGVertColorAlign,
        UniformSpriteMultiGPos, UniformSpriteMultiGVertColorAlign,
    },
};

#[derive(Debug, Hiarc)]
pub struct SubRenderPass {
    pub standard_pipeline: PipelineContainer,
    pub standard_line_pipeline: PipelineContainer,
    pub standard_blur_pipeline: PipelineContainer,
    pub standard_3d_pipeline: PipelineContainer,
    pub blur_pipeline: PipelineContainer,
    pub prim_ex_pipeline: PipelineContainer,
    pub prim_ex_rotationless_pipeline: PipelineContainer,
    pub sprite_multi_pipeline: PipelineContainer,
    pub additional_pipes: Vec<PipelineContainer>,
}

impl SubRenderPass {
    pub fn get_by_type(&self, ty: SubRenderPassAttributes) -> &PipelineContainer {
        match ty {
            SubRenderPassAttributes::StandardPipeline => &self.standard_pipeline,
            SubRenderPassAttributes::StandardLinePipeline => &self.standard_line_pipeline,
            SubRenderPassAttributes::StandardBlurPipeline => &self.standard_blur_pipeline,
            SubRenderPassAttributes::Standard3dPipeline => &self.standard_3d_pipeline,
            SubRenderPassAttributes::BlurPipeline => &self.blur_pipeline,
            SubRenderPassAttributes::PrimExPipeline => &self.prim_ex_pipeline,
            SubRenderPassAttributes::PrimExRotationlessPipeline => {
                &self.prim_ex_rotationless_pipeline
            }
            SubRenderPassAttributes::SpriteMultiPipeline => &self.sprite_multi_pipeline,
            SubRenderPassAttributes::Additional(name) => &self.additional_pipes[name as usize],
        }
    }
}

impl SubRenderPass {
    fn get_pipeline_and_layout_mut(
        container: &mut PipelineContainer,
        is_textured: bool,
        blend_mode_index: usize,
        dynamic_index: usize,
        stencil_mode_index: usize,
        color_mask_index: usize,
        address_mode_index: usize,
    ) -> &mut PipelineContainerItem {
        &mut container.pipelines[blend_mode_index][dynamic_index][is_textured as usize]
            [stencil_mode_index][color_mask_index][address_mode_index]
    }

    fn standard_pipeline_layout(
        layouts: &DescriptorLayouts,
        is_textured: bool,
        as_line_geometry: bool,
        address_mode: SupportedSamplerTypes,
    ) -> (
        Vec<vk::VertexInputAttributeDescription>,
        Vec<vk::DescriptorSetLayout>,
        Vec<vk::PushConstantRange>,
        vk::DeviceSize,
        bool,
    ) {
        (
            [
                vk::VertexInputAttributeDescription {
                    location: 0,
                    binding: 0,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: 0,
                },
                vk::VertexInputAttributeDescription {
                    location: 1,
                    binding: 0,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: (std::mem::size_of::<f32>() * 2) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 2,
                    binding: 0,
                    format: vk::Format::R8G8B8A8_UNORM,
                    offset: (std::mem::size_of::<f32>() * (2 + 2)) as u32,
                },
            ]
            .into(),
            if is_textured {
                [
                    layouts.standard_textured_descriptor_set_layout.layout,
                    layouts.samplers_layouts[address_mode as usize].layout,
                ]
                .into()
            } else {
                Vec::new()
            },
            [vk::PushConstantRange {
                stage_flags: vk::ShaderStageFlags::VERTEX,
                offset: 0,
                size: std::mem::size_of::<UniformGPos>() as u32,
            }]
            .into(),
            (std::mem::size_of::<f32>() * (2 + 2) + std::mem::size_of::<u8>() * 4)
                as vk::DeviceSize,
            as_line_geometry,
        )
    }

    fn standard_3d_pipeline_layout(
        layouts: &DescriptorLayouts,
        is_textured: bool,
        address_mode: SupportedSamplerTypes,
    ) -> (
        Vec<vk::VertexInputAttributeDescription>,
        Vec<vk::DescriptorSetLayout>,
        Vec<vk::PushConstantRange>,
        vk::DeviceSize,
        bool,
    ) {
        (
            [
                vk::VertexInputAttributeDescription {
                    location: 0,
                    binding: 0,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: 0,
                },
                vk::VertexInputAttributeDescription {
                    location: 1,
                    binding: 0,
                    format: vk::Format::R8G8B8A8_UNORM,
                    offset: (std::mem::size_of::<f32>() * 2) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 2,
                    binding: 0,
                    format: vk::Format::R32G32B32_SFLOAT,
                    offset: (std::mem::size_of::<f32>() * 2 + std::mem::size_of::<u8>() * 4) as u32,
                },
            ]
            .into(),
            if is_textured {
                [
                    layouts
                        .standard_2d_texture_array_descriptor_set_layout
                        .layout,
                    layouts.samplers_layouts[address_mode as usize].layout,
                ]
                .into()
            } else {
                Vec::new()
            },
            [vk::PushConstantRange {
                stage_flags: vk::ShaderStageFlags::VERTEX,
                offset: 0,
                size: std::mem::size_of::<UniformGPos>() as u32,
            }]
            .into(),
            (std::mem::size_of::<f32>() * 2
                + std::mem::size_of::<u8>() * 4
                + std::mem::size_of::<f32>() * 3) as vk::DeviceSize,
            false,
        )
    }

    fn create_graphics_pipeline_generic_impl(
        pipeline_manager: &PipelineManager,
        attrs: &[PipelineCreationAttributes],
    ) -> anyhow::Result<Pipelines> {
        pipeline_manager.create_graphics_pipeline_ex(attrs)
    }

    fn create_graphics_pipeline_generic<N, L>(
        pipe_container: &mut PipelineContainer,
        pipeline_manager: &PipelineManager,
        shader_names: N,
        create_layout: L,
    ) -> anyhow::Result<()>
    where
        N: Fn(bool) -> Option<(String, String)>,
        L: Fn(
            bool,
            SupportedSamplerTypes,
        ) -> (
            Vec<vk::VertexInputAttributeDescription>,
            Vec<vk::DescriptorSetLayout>,
            Vec<vk::PushConstantRange>,
            vk::DeviceSize,
            bool,
        ),
    {
        let cap_size = SupportedSamplerTypes::COUNT
            * 2
            * ColorWriteMaskType::COUNT
            * StencilOpType::COUNT
            * SupportedBlendModes::COUNT
            * CanvasClipModes::COUNT;
        let mut attrs: Vec<PipelineCreationAttributes> = Vec::with_capacity(cap_size);
        let mut attrs_ex: Vec<PipelineCreationAttributesEx> = Vec::with_capacity(cap_size);
        for l in 0..SupportedSamplerTypes::COUNT {
            for t in 0..2 {
                let is_textured = t == 0;
                let (attribute_descriptors, set_layouts, push_constants, stride, is_line_geometry) =
                    create_layout(
                        is_textured,
                        SupportedSamplerTypes::from_u32(l as u32).unwrap(),
                    );
                if let Some((vert_name, frag_name)) = shader_names(is_textured) {
                    for c in 0..ColorWriteMaskType::COUNT {
                        for s in 0..StencilOpType::COUNT {
                            for i in 0..SupportedBlendModes::COUNT {
                                for j in 0..CanvasClipModes::COUNT {
                                    let attr = PipelineCreationAttributes {
                                        vert_name: vert_name.clone(),
                                        frag_name: frag_name.clone(),
                                        stride: stride as u32,
                                        input_attributes: attribute_descriptors.clone(),
                                        set_layouts: set_layouts.clone(),
                                        push_constants: push_constants.clone(),
                                        blend_mode: SupportedBlendModes::from_u32(i as u32)
                                            .unwrap(),
                                        dynamic_mode: CanvasClipModes::from_u32(j as u32).unwrap(),
                                        is_line_prim: is_line_geometry,
                                        stencil_mode: StencilOpType::from_u32(s as u32).unwrap(),
                                        color_mask: ColorWriteMaskType::from_u32(c as u32).unwrap(),
                                    };
                                    let address_mode_index = l;
                                    attrs.push(attr);
                                    attrs_ex.push(PipelineCreationAttributesEx {
                                        address_mode_index,
                                        is_textured,
                                    })
                                }
                            }
                        }
                    }
                }
            }
        }

        if !attrs.is_empty() {
            match &pipe_container.mode {
                PipelineContainerCreateMode::AtOnce => {
                    let pipelines =
                        Self::create_graphics_pipeline_generic_impl(pipeline_manager, &attrs)?;
                    let pipelines = pipelines.split_all();
                    for (index, pipeline) in pipelines.into_iter().enumerate() {
                        let attr = &attrs[index];
                        let attr_ex = &attrs_ex[index];
                        let pipe_item = Self::get_pipeline_and_layout_mut(
                            pipe_container,
                            attr_ex.is_textured,
                            attr.blend_mode as usize,
                            attr.dynamic_mode as usize,
                            attr.stencil_mode as usize,
                            attr.color_mask as usize,
                            attr_ex.address_mode_index,
                        );
                        *pipe_item = PipelineContainerItem::Normal { pipeline }
                    }
                }
                PipelineContainerCreateMode::OneByOne(creation_data) => {
                    let creation_data = creation_data.clone();
                    let mutex: Arc<parking_lot::Mutex<()>> = Default::default();
                    for (attr, attr_ex) in attrs.into_iter().zip(attrs_ex.into_iter()) {
                        let pipe_item = Self::get_pipeline_and_layout_mut(
                            pipe_container,
                            attr_ex.is_textured,
                            attr.blend_mode as usize,
                            attr.dynamic_mode as usize,
                            attr.stencil_mode as usize,
                            attr.color_mask as usize,
                            attr_ex.address_mode_index,
                        );
                        *pipe_item = PipelineContainerItem::MaybeUninit {
                            pipeline_and_layout: Default::default(),
                            pipe_create_mutex: mutex.clone(),
                            creation_props: Box::new(PipelineCreationProps { attr, attr_ex }),
                            creation_data: creation_data.clone(),
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn prim_ex_pipeline_layout(
        layouts: &DescriptorLayouts,
        is_textured: bool,
        rotationless: bool,
        address_mode: SupportedSamplerTypes,
    ) -> (
        Vec<vk::VertexInputAttributeDescription>,
        Vec<vk::DescriptorSetLayout>,
        Vec<vk::PushConstantRange>,
        vk::DeviceSize,
        bool,
    ) {
        (
            [
                vk::VertexInputAttributeDescription {
                    location: 0,
                    binding: 0,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: 0,
                },
                vk::VertexInputAttributeDescription {
                    location: 1,
                    binding: 0,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: (std::mem::size_of::<f32>() * 2) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 2,
                    binding: 0,
                    format: vk::Format::R8G8B8A8_UNORM,
                    offset: (std::mem::size_of::<f32>() * (2 + 2)) as u32,
                },
            ]
            .into(),
            if is_textured {
                [
                    layouts.standard_textured_descriptor_set_layout.layout,
                    layouts.samplers_layouts[address_mode as usize].layout,
                ]
                .into()
            } else {
                Vec::new()
            },
            {
                let mut vert_push_constant_size = std::mem::size_of::<UniformPrimExGPos>();
                if rotationless {
                    vert_push_constant_size = std::mem::size_of::<UniformPrimExGPosRotationless>();
                }

                let frag_push_constant_size = std::mem::size_of::<SUniformPrimExGVertColor>();

                [
                    vk::PushConstantRange {
                        stage_flags: vk::ShaderStageFlags::VERTEX,
                        offset: 0,
                        size: vert_push_constant_size as u32,
                    },
                    vk::PushConstantRange {
                        stage_flags: vk::ShaderStageFlags::FRAGMENT,
                        offset: (std::mem::size_of::<UniformPrimExGPos>()
                            + std::mem::size_of::<UniformPrimExGVertColorAlign>())
                            as u32,
                        size: frag_push_constant_size as u32,
                    },
                ]
                .into()
            },
            (std::mem::size_of::<f32>() * (2 + 2) + std::mem::size_of::<u8>() * 4)
                as vk::DeviceSize,
            false,
        )
    }

    fn sprite_multi_pipeline_layout(
        layouts: &DescriptorLayouts,
        address_mode: SupportedSamplerTypes,
    ) -> (
        Vec<vk::VertexInputAttributeDescription>,
        Vec<vk::DescriptorSetLayout>,
        Vec<vk::PushConstantRange>,
        vk::DeviceSize,
        bool,
    ) {
        (
            [
                vk::VertexInputAttributeDescription {
                    location: 0,
                    binding: 0,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: 0,
                },
                vk::VertexInputAttributeDescription {
                    location: 1,
                    binding: 0,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: (std::mem::size_of::<f32>() * 2) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 2,
                    binding: 0,
                    format: vk::Format::R8G8B8A8_UNORM,
                    offset: (std::mem::size_of::<f32>() * (2 + 2)) as u32,
                },
            ]
            .into(),
            [
                layouts.standard_textured_descriptor_set_layout.layout,
                layouts.samplers_layouts[address_mode as usize].layout,
                layouts.vertex_uniform_descriptor_set_layout.layout,
            ]
            .into(),
            {
                let vert_push_constant_size = std::mem::size_of::<UniformSpriteMultiGPos>() as u32;
                let frag_push_constant_size =
                    std::mem::size_of::<SUniformSpriteMultiGVertColor>() as u32;

                [
                    vk::PushConstantRange {
                        stage_flags: vk::ShaderStageFlags::VERTEX,
                        offset: 0,
                        size: vert_push_constant_size,
                    },
                    vk::PushConstantRange {
                        stage_flags: vk::ShaderStageFlags::FRAGMENT,
                        offset: (std::mem::size_of::<UniformSpriteMultiGPos>()
                            + std::mem::size_of::<UniformSpriteMultiGVertColorAlign>())
                            as u32,
                        size: frag_push_constant_size,
                    },
                ]
                .into()
            },
            (std::mem::size_of::<f32>() * (2 + 2) + std::mem::size_of::<u8>() * 4)
                as vk::DeviceSize,
            false,
        )
    }

    fn backend_vertex_format_to_vk_format(format: BackendVertexFormat) -> vk::Format {
        match format {
            BackendVertexFormat::Vec4 => vk::Format::R32G32B32A32_SFLOAT,
            BackendVertexFormat::Vec3 => vk::Format::R32G32B32_SFLOAT,
            BackendVertexFormat::Vec2 => vk::Format::R32G32_SFLOAT,
            BackendVertexFormat::UbVec4Normalized => vk::Format::R8G8B8A8_UNORM,
            BackendVertexFormat::UbVec2 => vk::Format::R8G8_UINT,
            BackendVertexFormat::UbVec4 => vk::Format::R8G8B8A8_UINT,
            BackendVertexFormat::UsVec2 => vk::Format::R16G16_UINT,
        }
    }

    fn backend_pipe_flags_to_vk_pipe_flags(pipe_flags: BackendShaderStage) -> vk::ShaderStageFlags {
        let mut res: vk::ShaderStageFlags = Default::default();
        if pipe_flags.contains(BackendShaderStage::VERTEX) {
            res |= vk::ShaderStageFlags::VERTEX;
        }
        if pipe_flags.contains(BackendShaderStage::FRAGMENT) {
            res |= vk::ShaderStageFlags::FRAGMENT;
        }
        res
    }

    fn backend_layout_to_vk_layout(
        layouts: &DescriptorLayouts,
        mut layout: BackendPipelineLayout,
        address_mode: SupportedSamplerTypes,
    ) -> (
        Vec<vk::VertexInputAttributeDescription>,
        Vec<vk::DescriptorSetLayout>,
        Vec<vk::PushConstantRange>,
        vk::DeviceSize,
        bool,
    ) {
        let mut input_attr: Vec<vk::VertexInputAttributeDescription> = Default::default();
        let mut set_layouts: Vec<vk::DescriptorSetLayout> = Default::default();
        let mut push_constants: Vec<vk::PushConstantRange> = Default::default();

        for attr in layout.vertex_attributes.drain(..) {
            input_attr.push(vk::VertexInputAttributeDescription {
                location: attr.location,
                binding: attr.binding,
                format: Self::backend_vertex_format_to_vk_format(attr.format),
                offset: attr.offset,
            });
        }

        for set_layout in layout.descriptor_layouts.drain(..) {
            match set_layout {
                BackendResourceDescription::Fragment2DTexture => {
                    set_layouts.push(layouts.standard_textured_descriptor_set_layout.layout);
                    set_layouts.push(layouts.samplers_layouts[address_mode as usize].layout);
                }
                BackendResourceDescription::Fragment2DArrayTexture => {
                    set_layouts.push(
                        layouts
                            .standard_2d_texture_array_descriptor_set_layout
                            .layout,
                    );
                    set_layouts.push(layouts.samplers_layouts[address_mode as usize].layout);
                }
                BackendResourceDescription::VertexUniformBuffer => {
                    set_layouts.push(layouts.vertex_uniform_descriptor_set_layout.layout);
                }
                BackendResourceDescription::VertexFragmentUniformBuffer => {
                    set_layouts.push(layouts.vertex_fragment_uniform_descriptor_set_layout.layout);
                }
                BackendResourceDescription::VertexShaderStorage => {
                    set_layouts.push(layouts.vertex_shader_storage_descriptor_set_layout.layout);
                }
            }
        }

        for push_constant in layout.push_constants.drain(..) {
            push_constants.push(vk::PushConstantRange {
                stage_flags: Self::backend_pipe_flags_to_vk_pipe_flags(push_constant.stage_flags),
                offset: push_constant.offset,
                size: push_constant.size,
            });
        }

        (
            input_attr,
            set_layouts,
            push_constants,
            layout.stride,
            layout.geometry_is_line,
        )
    }

    /// `stop_execution_flag` will be checked after every pipeline created (which might be an expensive operation) to allow earlier cancellation if desired
    pub fn create_pipelines(
        &mut self,
        logical_device: &Arc<LogicalDevice>,
        multi_sampling_count: u32,
        layouts: &DescriptorLayouts,
        custom_pipes: &CustomPipelines,
        pipeline_cache: &Option<Arc<PipelineCacheInner>>,
        runtime_threadpool: &Arc<rayon::ThreadPool>,
        shader_compiler: &Arc<ShaderCompiler>,
        swapchain_extent: vk::Extent2D,
        render_pass: vk::RenderPass,
        stop_execution_flag: Option<&Arc<AtomicBool>>,
    ) -> anyhow::Result<()> {
        let pipeline_manager = PipelineManager::new(
            logical_device,
            multi_sampling_count,
            shader_compiler,
            swapchain_extent,
            render_pass,
            pipeline_cache,
        );

        let (res1, res2, res3, res4, res5, res6, res7, res8) = runtime_threadpool.install(|| {
            join_all!(
                || -> anyhow::Result<()> {
                    (!stop_execution_flag
                        .is_some_and(|v| v.load(std::sync::atomic::Ordering::Relaxed)))
                    .then_some(Some(()))
                    .ok_or(anyhow!("Execution termination was requested."))?;
                    Self::create_graphics_pipeline_generic(
                        &mut self.standard_pipeline,
                        &pipeline_manager,
                        |is_textured| {
                            if is_textured {
                                Some((
                                    "shader/vulkan/prim.vert.spv".into(),
                                    "shader/vulkan/prim_textured.frag.spv".into(),
                                ))
                            } else {
                                Some((
                                    "shader/vulkan/prim.vert.spv".into(),
                                    "shader/vulkan/prim.frag.spv".into(),
                                ))
                            }
                        },
                        |is_textured, address_mode| {
                            Self::standard_pipeline_layout(
                                layouts,
                                is_textured,
                                false,
                                address_mode,
                            )
                        },
                    )
                    .map_err(|err| anyhow!("Create standard graphics pipeline: {err}"))?;
                    Ok(())
                },
                || -> anyhow::Result<()> {
                    (!stop_execution_flag
                        .is_some_and(|v| v.load(std::sync::atomic::Ordering::Relaxed)))
                    .then_some(Some(()))
                    .ok_or(anyhow!("Execution termination was requested."))?;
                    Self::create_graphics_pipeline_generic(
                        &mut self.standard_line_pipeline,
                        &pipeline_manager,
                        |is_textured| {
                            if is_textured {
                                None
                            } else {
                                Some((
                                    "shader/vulkan/prim.vert.spv".into(),
                                    "shader/vulkan/prim.frag.spv".into(),
                                ))
                            }
                        },
                        |is_textured, address_mode| {
                            Self::standard_pipeline_layout(layouts, is_textured, true, address_mode)
                        },
                    )
                    .map_err(|err| anyhow!("Create standard line graphics pipeline: {err}"))?;
                    Ok(())
                },
                || -> anyhow::Result<()> {
                    (!stop_execution_flag
                        .is_some_and(|v| v.load(std::sync::atomic::Ordering::Relaxed)))
                    .then_some(Some(()))
                    .ok_or(anyhow!("Execution termination was requested."))?;
                    Self::create_graphics_pipeline_generic(
                        &mut self.blur_pipeline,
                        &pipeline_manager,
                        |is_textured| {
                            if is_textured {
                                Some((
                                    "shader/vulkan/prim.vert.spv".into(),
                                    "shader/vulkan/blur.frag.spv".into(),
                                ))
                            } else {
                                None
                            }
                        },
                        |is_textured, address_mode| {
                            let (
                                input_attributes,
                                set_layouts,
                                mut push_constants,
                                stride,
                                is_line,
                            ) = Self::standard_pipeline_layout(
                                layouts,
                                is_textured,
                                false,
                                address_mode,
                            );
                            push_constants.push(vk::PushConstantRange {
                                stage_flags: vk::ShaderStageFlags::FRAGMENT,
                                offset: std::mem::size_of::<UniformGPos>() as u32,
                                size: std::mem::size_of::<UniformGBlur>() as u32,
                            });
                            (
                                input_attributes,
                                set_layouts,
                                push_constants,
                                stride,
                                is_line,
                            )
                        },
                    )
                    .map_err(|err| anyhow!("Create blur graphics pipeline: {err}"))?;
                    Ok(())
                },
                || -> anyhow::Result<()> {
                    (!stop_execution_flag
                        .is_some_and(|v| v.load(std::sync::atomic::Ordering::Relaxed)))
                    .then_some(Some(()))
                    .ok_or(anyhow!("Execution termination was requested."))?;
                    Self::create_graphics_pipeline_generic(
                        &mut self.standard_3d_pipeline,
                        &pipeline_manager,
                        |is_textured| {
                            if is_textured {
                                Some((
                                    "shader/vulkan/prim3d_textured.vert.spv".into(),
                                    "shader/vulkan/prim3d_textured.frag.spv".into(),
                                ))
                            } else {
                                Some((
                                    "shader/vulkan/prim3d.vert.spv".into(),
                                    "shader/vulkan/prim3d.frag.spv".into(),
                                ))
                            }
                        },
                        |is_textured, address_mode| {
                            Self::standard_3d_pipeline_layout(layouts, is_textured, address_mode)
                        },
                    )
                    .map_err(|err| anyhow!("Create standard 3d graphics pipeline: {err}"))?;

                    Ok(())
                },
                || -> anyhow::Result<()> {
                    (!stop_execution_flag
                        .is_some_and(|v| v.load(std::sync::atomic::Ordering::Relaxed)))
                    .then_some(Some(()))
                    .ok_or(anyhow!("Execution termination was requested."))?;
                    Self::create_graphics_pipeline_generic(
                        &mut self.prim_ex_rotationless_pipeline,
                        &pipeline_manager,
                        |is_textured| {
                            if is_textured {
                                Some((
                                    "shader/vulkan/primex_tex_rotationless.vert.spv".into(),
                                    "shader/vulkan/primex_tex_rotationless.frag.spv".into(),
                                ))
                            } else {
                                Some((
                                    "shader/vulkan/primex_rotationless.vert.spv".into(),
                                    "shader/vulkan/primex_rotationless.frag.spv".into(),
                                ))
                            }
                        },
                        |is_textured, address_mode| {
                            Self::prim_ex_pipeline_layout(layouts, is_textured, true, address_mode)
                        },
                    )
                    .map_err(|err| {
                        anyhow!("Create prim ex graphics rotationless pipeline: {err}")
                    })?;
                    Ok(())
                },
                || -> anyhow::Result<()> {
                    (!stop_execution_flag
                        .is_some_and(|v| v.load(std::sync::atomic::Ordering::Relaxed)))
                    .then_some(Some(()))
                    .ok_or(anyhow!("Execution termination was requested."))?;
                    Self::create_graphics_pipeline_generic(
                        &mut self.prim_ex_pipeline,
                        &pipeline_manager,
                        |is_textured| {
                            if is_textured {
                                Some((
                                    "shader/vulkan/primex_tex.vert.spv".into(),
                                    "shader/vulkan/primex_tex.frag.spv".into(),
                                ))
                            } else {
                                Some((
                                    "shader/vulkan/primex.vert.spv".into(),
                                    "shader/vulkan/primex.frag.spv".into(),
                                ))
                            }
                        },
                        |is_textured, address_mode| {
                            Self::prim_ex_pipeline_layout(layouts, is_textured, false, address_mode)
                        },
                    )
                    .map_err(|err| anyhow!("Create prim ex graphics pipeline: {err}"))?;
                    Ok(())
                },
                || -> anyhow::Result<()> {
                    (!stop_execution_flag
                        .is_some_and(|v| v.load(std::sync::atomic::Ordering::Relaxed)))
                    .then_some(Some(()))
                    .ok_or(anyhow!("Execution termination was requested."))?;
                    Self::create_graphics_pipeline_generic(
                        &mut self.sprite_multi_pipeline,
                        &pipeline_manager,
                        |is_textured| {
                            if is_textured {
                                Some((
                                    "shader/vulkan/spritemulti.vert.spv".into(),
                                    "shader/vulkan/spritemulti.frag.spv".into(),
                                ))
                            } else {
                                None
                            }
                        },
                        |_, address_mode| Self::sprite_multi_pipeline_layout(layouts, address_mode),
                    )
                    .map_err(|err| anyhow!("Create sprite multi graphics pipeline: {err}"))?;
                    Ok(())
                },
                || -> anyhow::Result<()> {
                    let custom_pipes = custom_pipes.read();
                    let mut name_offset = 0;
                    for custom_pipe in custom_pipes.iter() {
                        let count = custom_pipe.pipeline_count();
                        for _ in 0..count {
                            (!stop_execution_flag
                                .is_some_and(|v| v.load(std::sync::atomic::Ordering::Relaxed)))
                            .then_some(Some(()))
                            .ok_or(anyhow!("Execution termination was requested."))?;
                            Self::create_graphics_pipeline_generic(
                                &mut self.additional_pipes[name_offset as usize],
                                &pipeline_manager,
                                |is_textured| {
                                    custom_pipe.pipe_shader_names(name_offset, is_textured)
                                },
                                |is_textured, address_mode| {
                                    Self::backend_layout_to_vk_layout(
                                        layouts,
                                        custom_pipe.pipe_layout_of(name_offset, is_textured),
                                        address_mode,
                                    )
                                },
                            )
                            .map_err(|err| {
                                anyhow!(
                        "Creating custom graphics pipeline (index: {name_offset}) failed: {err}"
                    )
                            })?;
                            name_offset += 1;
                        }
                    }

                    Ok(())
                }
            )
        });

        res1?;
        res2?;
        res3?;
        res4?;
        res5?;
        res6?;
        res7?;
        res8?;

        Ok(())
    }

    pub fn new(
        logical_device: &Arc<LogicalDevice>,
        multi_sampling_count: u32,
        layouts: &DescriptorLayouts,
        custom_pipes: &CustomPipelines,
        pipeline_cache: &Option<Arc<PipelineCacheInner>>,
        runtime_threadpool: &Arc<rayon::ThreadPool>,
        shader_compiler: &Arc<ShaderCompiler>,
        swapchain_extent: vk::Extent2D,
        render_pass: vk::RenderPass,
        compile_one_by_one: bool,
        stop_execution_flag: Option<&Arc<AtomicBool>>,
    ) -> anyhow::Result<Self> {
        let mut name_offset = 0;
        let mut custom_pipelines = custom_pipes.write();
        for custom_pipe in custom_pipelines.iter_mut() {
            let count = custom_pipe.pipeline_count();

            custom_pipe.pipeline_names(name_offset);

            name_offset += count;
        }
        drop(custom_pipelines);

        let mode = if compile_one_by_one {
            PipelineContainerCreateMode::OneByOne(PipelineCreationOneByOne {
                multi_sampling_count,
                device: logical_device.clone(),
                shader_compiler: shader_compiler.clone(),
                swapchain_extent,
                render_pass,
                pipeline_cache: pipeline_cache.clone(),
            })
        } else {
            PipelineContainerCreateMode::AtOnce
        };

        let mut additional_pipes = Vec::with_capacity(name_offset as usize);
        additional_pipes.resize_with(name_offset as usize, || {
            PipelineContainer::new(mode.clone())
        });

        let mut res = Self {
            standard_pipeline: PipelineContainer::new(mode.clone()),
            standard_line_pipeline: PipelineContainer::new(mode.clone()),
            standard_blur_pipeline: PipelineContainer::new(mode.clone()),
            standard_3d_pipeline: PipelineContainer::new(mode.clone()),
            blur_pipeline: PipelineContainer::new(mode.clone()),
            prim_ex_pipeline: PipelineContainer::new(mode.clone()),
            prim_ex_rotationless_pipeline: PipelineContainer::new(mode.clone()),
            sprite_multi_pipeline: PipelineContainer::new(mode.clone()),
            additional_pipes,
        };

        res.create_pipelines(
            logical_device,
            multi_sampling_count,
            layouts,
            custom_pipes,
            pipeline_cache,
            runtime_threadpool,
            shader_compiler,
            swapchain_extent,
            render_pass,
            stop_execution_flag,
        )?;

        Ok(res)
    }
}
