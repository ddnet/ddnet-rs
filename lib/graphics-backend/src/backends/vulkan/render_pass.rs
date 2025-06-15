use std::ops::Deref;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::Arc;

use anyhow::anyhow;
use arc_swap::{ArcSwap, Guard};
use ash::vk::{self};
use base::rayon_join_handle::RayonJoinHandle;
use hiarc::Hiarc;

use crate::backend::CustomPipelines;

use super::compiler::compiler::ShaderCompiler;
use super::frame_resources::FrameResources;
use super::image::ImageLayout;
use super::pipeline_cache::PipelineCacheInner;
use super::render_setup::{
    CanvasSetupCreationType, CanvasSetupNative, CanvasSetupSwitching, OffscreenSurface,
    RenderSetupCreationType, RenderSetupNativeType, RenderSetupSwitchingCreation,
};
use super::sub_render_pass::SubRenderPass;
use super::swapchain::Swapchain;
use super::vulkan_allocator::VulkanAllocator;
use super::vulkan_device::DescriptorLayouts;
use super::vulkan_types::{DeviceDescriptorPools, RenderPassSubType, RenderPassType};
use super::{logical_device::LogicalDevice, phy_device::PhyDevice, vulkan_device::Device};

#[derive(Debug, Hiarc)]
pub struct RenderPassInner {
    #[hiarc_skip_unsafe]
    pub pass: vk::RenderPass,
    device: Arc<LogicalDevice>,
}

impl RenderPassInner {
    pub fn new(
        device: Arc<LogicalDevice>,
        create_render_pass_info: &vk::RenderPassCreateInfo,
    ) -> anyhow::Result<Arc<Self>> {
        Ok(Arc::new(Self {
            pass: unsafe {
                device
                    .device
                    .create_render_pass(create_render_pass_info, None)
            }?,
            device,
        }))
    }
}

impl Drop for RenderPassInner {
    fn drop(&mut self) {
        unsafe {
            self.device.device.destroy_render_pass(self.pass, None);
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct RenderPassSubpassCompileAsync {
    pub counter: Arc<AtomicUsize>,
    pub stop_execution_flag: Arc<AtomicBool>,
    pub thread: Option<RayonJoinHandle<anyhow::Result<Vec<SubRenderPass>>>>,
}

impl RenderPassSubpassCompileAsync {
    pub fn new(
        counter: &Arc<AtomicUsize>,
        stop_execution_flag: &Arc<AtomicBool>,
        thread: RayonJoinHandle<anyhow::Result<Vec<SubRenderPass>>>,
    ) -> anyhow::Result<Self> {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Self {
            counter: counter.clone(),
            stop_execution_flag: stop_execution_flag.clone(),
            thread: Some(thread),
        })
    }

    pub fn is_finished(&self) -> bool {
        self.thread.is_none() || self.thread.as_ref().is_some_and(|t| t.is_finished())
    }

    pub fn drop_in_place(mut self) -> anyhow::Result<Vec<SubRenderPass>> {
        if let Some(thread) = self.thread.take() {
            self.stop_execution_flag
                .store(true, std::sync::atomic::Ordering::Relaxed);
            let res = thread
                .join()
                .map_err(|err| anyhow!("Failed to compile subpasses (in thread): {err:?}"))??;
            self.counter
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            Ok(res)
        } else {
            Err(anyhow!("thread was already finished and the result taken."))
        }
    }
}

impl Drop for RenderPassSubpassCompileAsync {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.stop_execution_flag
                .store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = thread.join();
            self.counter
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

#[derive(Debug, Hiarc)]
pub enum CompileOneByOneType {
    None,
    CompileAndQueueFullCompile(Arc<AtomicUsize>),
    Compile,
}

impl CompileOneByOneType {
    fn as_ref(&self) -> CompileOneByOneTypeRef<'_> {
        match self {
            Self::None => CompileOneByOneTypeRef::None,
            Self::CompileAndQueueFullCompile(counter) => {
                CompileOneByOneTypeRef::CompileAndQueueFullCompile(counter)
            }
            Self::Compile => CompileOneByOneTypeRef::Compile,
        }
    }
}

#[derive(Debug, Hiarc, Clone, Copy)]
pub enum CompileOneByOneTypeRef<'a> {
    None,
    CompileAndQueueFullCompile(&'a Arc<AtomicUsize>),
    Compile,
}

impl<'a> From<CompileOneByOneTypeRef<'a>> for CompileOneByOneType {
    fn from(val: CompileOneByOneTypeRef<'a>) -> Self {
        match val {
            CompileOneByOneTypeRef::None => CompileOneByOneType::None,
            CompileOneByOneTypeRef::CompileAndQueueFullCompile(counter) => {
                CompileOneByOneType::CompileAndQueueFullCompile(counter.clone())
            }
            CompileOneByOneTypeRef::Compile => CompileOneByOneType::Compile,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CompileThreadpoolsRef<'a> {
    pub one_by_one: &'a Arc<rayon::ThreadPool>,
    pub async_full: &'a Arc<rayon::ThreadPool>,
}

impl CompileThreadpoolsRef<'_> {
    fn convert(&self) -> CompileThreadpools {
        CompileThreadpools {
            one_by_one: self.one_by_one.clone(),
            async_full: self.async_full.clone(),
        }
    }
}

#[derive(Debug, Hiarc, Clone)]
pub struct CompileThreadpools {
    pub one_by_one: Arc<rayon::ThreadPool>,
    pub async_full: Arc<rayon::ThreadPool>,
}

#[derive(Debug, Hiarc)]
pub struct RenderPass {
    pub pass: Arc<RenderPassInner>,

    pub subpasses: ArcSwap<Vec<SubRenderPass>>,
    pub subpasses_to_compile: parking_lot::RwLock<Option<RenderPassSubpassCompileAsync>>,

    pub attachment_infos: Vec<ImageLayout>,
}

impl RenderPass {
    pub fn new(
        device: &Arc<LogicalDevice>,
        multi_sampling_count: u32,
        layouts: &DescriptorLayouts,
        custom_pipes: &CustomPipelines,
        pipeline_cache: &Option<Arc<PipelineCacheInner>>,
        runtime_threadpool: CompileThreadpoolsRef<'_>,
        shader_compiler: &Arc<ShaderCompiler>,
        swapchain_extent: vk::Extent2D,
        dependencies: &[vk::SubpassDependency],
        subpasses: &[vk::SubpassDescription],
        attachments: &[vk::AttachmentDescription],
        compile_one_by_one: CompileOneByOneTypeRef<'_>,
    ) -> anyhow::Result<Arc<Self>> {
        let pass = RenderPassInner::new(
            device.clone(),
            &vk::RenderPassCreateInfo::default()
                .attachments(attachments)
                .subpasses(subpasses)
                .dependencies(dependencies),
        )?;

        let mut subpasses_to_compile = parking_lot::RwLock::new(None);
        let compile_one_by_one = match compile_one_by_one {
            CompileOneByOneTypeRef::None => false,
            CompileOneByOneTypeRef::CompileAndQueueFullCompile(counter) => {
                let device = device.clone();
                let layouts = layouts.clone();
                let custom_pipes = custom_pipes.clone();
                let pipeline_cache = pipeline_cache.clone();
                let runtime_threadpool = runtime_threadpool.async_full;
                let tp = runtime_threadpool.clone();
                let shader_compiler = shader_compiler.clone();
                let pass = pass.clone();
                let stop_execution_flag: Arc<AtomicBool> = Default::default();
                let stop_execution_flag_clone = stop_execution_flag.clone();
                let join_handle = RayonJoinHandle::run(runtime_threadpool, move || {
                    Ok(SubRenderPass::new(
                        &device,
                        multi_sampling_count,
                        &layouts,
                        &custom_pipes,
                        &pipeline_cache,
                        &tp,
                        &shader_compiler,
                        swapchain_extent,
                        pass.pass,
                        false,
                        Some(&stop_execution_flag_clone),
                    )
                    .map(|v| vec![v]))
                });
                subpasses_to_compile = parking_lot::RwLock::new(Some(
                    RenderPassSubpassCompileAsync::new(counter, &stop_execution_flag, join_handle)?,
                ));
                true
            }
            CompileOneByOneTypeRef::Compile => true,
        };
        let subpasses = vec![SubRenderPass::new(
            device,
            multi_sampling_count,
            layouts,
            custom_pipes,
            pipeline_cache,
            runtime_threadpool.one_by_one,
            shader_compiler,
            swapchain_extent,
            pass.pass,
            compile_one_by_one,
            None,
        )?];

        Ok(Arc::new(Self {
            pass,
            subpasses: ArcSwap::new(Arc::new(subpasses)),
            subpasses_to_compile,

            attachment_infos: attachments.iter().map(|a| a.final_layout.into()).collect(),
        }))
    }

    pub fn try_finish_compile(&self, frame_resources: &mut FrameResources) -> anyhow::Result<()> {
        let task_guard = self.subpasses_to_compile.read();
        if let Some(task) = task_guard.as_ref() {
            if task.is_finished() {
                drop(task_guard);
                let old_subpasses = self.subpasses.swap(Arc::new(
                    self.subpasses_to_compile
                        .write()
                        .take()
                        .unwrap()
                        .drop_in_place()?,
                ));
                frame_resources.sub_render_passes.push(old_subpasses);
            }
        }

        Ok(())
    }
}

pub struct SubRenderPassDeref {
    pub inner: Guard<Arc<Vec<SubRenderPass>>>,
}

impl Deref for SubRenderPassDeref {
    type Target = SubRenderPass;

    fn deref(&self) -> &Self::Target {
        &self.inner[0]
    }
}

#[derive(Debug, Hiarc)]
pub struct CanvasSetupMultiSampling {
    pub native: CanvasSetupNative,
}

#[derive(Debug, Hiarc)]
pub struct CanvasSetupArguments {
    device: Arc<LogicalDevice>,
    layouts: DescriptorLayouts,
    #[hiarc_skip_unsafe]
    custom_pipes: CustomPipelines,
    pipeline_cache: Option<Arc<PipelineCacheInner>>,
    mem_allocator: Arc<parking_lot::Mutex<VulkanAllocator>>,
    runtime_threadpool: CompileThreadpools,
    shader_compiler: Arc<ShaderCompiler>,
    compile_one_by_one: CompileOneByOneType,
}

/// all rendering related stuff
/// such as renderpasses and framebuffers
#[derive(Debug, Hiarc)]
pub struct CanvasSetup {
    #[hiarc_skip_unsafe]
    pub surf_format: vk::SurfaceFormatKHR,
    #[hiarc_skip_unsafe]
    pub stencil_format: vk::Format,

    pub switching: CanvasSetupSwitching,
    pub native: CanvasSetupNative,

    pub multi_sampling: Option<CanvasSetupMultiSampling>,

    setup_props: CanvasSetupArguments,

    pub inner_type: RenderSetupNativeType,
}

impl CanvasSetup {
    pub fn new(
        device: &Arc<LogicalDevice>,
        layouts: &DescriptorLayouts,
        custom_pipes: &CustomPipelines,
        pipeline_cache: &Option<Arc<PipelineCacheInner>>,
        standard_texture_descr_pool: &Arc<parking_lot::Mutex<DeviceDescriptorPools>>,
        mem_allocator: &Arc<parking_lot::Mutex<VulkanAllocator>>,
        runtime_threadpool: CompileThreadpoolsRef<'_>,
        shader_compiler: &Arc<ShaderCompiler>,
        creation_type: CanvasSetupCreationType,
        compile_one_by_one: CompileOneByOneTypeRef<'_>,
        has_multi_sampling: Option<u32>,
    ) -> anyhow::Result<Arc<Self>> {
        let (creation_type, surf_format, ty) = match creation_type {
            CanvasSetupCreationType::Swapchain((swapchain, swapchain_backend)) => {
                let swapchain_images =
                    Swapchain::get_swap_chain_image_handles(swapchain_backend, device)?;
                let swapchain_image_views = CanvasSetupNative::create_image_views(
                    device,
                    swapchain.format.format,
                    &swapchain_images,
                )?;
                let extent = swapchain.extent;
                let swapchain_format = swapchain.format;
                let ty = RenderSetupNativeType::Swapchain(swapchain);
                (
                    RenderSetupCreationType::ExistingImages {
                        images: swapchain_images,
                        image_views: swapchain_image_views,
                        extent,
                        img_format: swapchain_format,
                    },
                    swapchain_format,
                    ty,
                )
            }
            CanvasSetupCreationType::Offscreen {
                img_format,
                img_count,
                extent,
            } => {
                let images_res = CanvasSetupNative::create_offscreen_images(
                    mem_allocator,
                    extent.width,
                    extent.height,
                    img_count,
                    img_format.format,
                )?;
                let (images, img_mems): (Vec<_>, Vec<_>) = images_res.into_iter().unzip();
                let image_views =
                    CanvasSetupNative::create_image_views(device, img_format.format, &images)?;

                let descrs = image_views
                    .iter()
                    .map(|img_view| {
                        Device::create_new_textured_standard_descriptor_sets(
                    device,
                    layouts,
                    standard_texture_descr_pool,
                    img_view,
                )
                .map_err(|err| {
                    anyhow!("Could not create image descriptors for switching pass images: {err}")
                })
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;

                (
                    RenderSetupCreationType::ExistingImages {
                        images,
                        image_views,
                        extent,
                        img_format,
                    },
                    img_format,
                    RenderSetupNativeType::Offscreen { img_mems, descrs },
                )
            }
        };

        let native = CanvasSetupNative::new(
            device,
            0,
            layouts,
            custom_pipes,
            pipeline_cache,
            mem_allocator,
            runtime_threadpool,
            shader_compiler,
            creation_type,
            compile_one_by_one,
            has_multi_sampling.is_none(),
        )?;

        let offscreen_surfaces = [
            OffscreenSurface::new(
                device,
                0,
                layouts,
                standard_texture_descr_pool,
                mem_allocator,
                native.swap_chain_images.len(),
                surf_format.format,
                native.swap_img_and_viewport_extent,
            )?,
            OffscreenSurface::new(
                device,
                0,
                layouts,
                standard_texture_descr_pool,
                mem_allocator,
                native.swap_chain_images.len(),
                surf_format.format,
                native.swap_img_and_viewport_extent,
            )?,
        ];

        let (stencil_images, stencil_format) =
            CanvasSetupSwitching::create_stencil_attachments_for_pass_transition(
                device,
                0,
                mem_allocator,
                native.swap_chain_images.len(),
                native.swap_img_and_viewport_extent,
            )?;

        let (switching, stencil_format) = CanvasSetupSwitching::new(
            device,
            0,
            layouts,
            custom_pipes,
            pipeline_cache,
            runtime_threadpool,
            shader_compiler,
            surf_format.format,
            native.swap_img_and_viewport_extent,
            RenderSetupSwitchingCreation {
                offscreen_surfaces,
                stencil_images,
                stencil_format,
            },
            compile_one_by_one,
        )?;

        let mut res = Self {
            surf_format,

            switching,
            native,

            multi_sampling: Default::default(),

            stencil_format,

            setup_props: CanvasSetupArguments {
                device: device.clone(),
                layouts: layouts.clone(),
                custom_pipes: custom_pipes.clone(),
                pipeline_cache: pipeline_cache.clone(),
                mem_allocator: mem_allocator.clone(),
                runtime_threadpool: runtime_threadpool.convert(),
                shader_compiler: shader_compiler.clone(),
                compile_one_by_one: compile_one_by_one.into(),
            },

            inner_type: ty,
        };

        if let Some(multi_sampling_count) = has_multi_sampling {
            res.init_multi_sampling(multi_sampling_count)?;
        }

        Ok(Arc::new(res))
    }

    pub fn init_multi_sampling(&mut self, multi_sampling_count: u32) -> anyhow::Result<()> {
        let native = CanvasSetupNative::new(
            &self.setup_props.device,
            multi_sampling_count,
            &self.setup_props.layouts,
            &self.setup_props.custom_pipes,
            &self.setup_props.pipeline_cache,
            &self.setup_props.mem_allocator,
            CompileThreadpoolsRef {
                one_by_one: &self.setup_props.runtime_threadpool.one_by_one,
                async_full: &self.setup_props.runtime_threadpool.async_full,
            },
            &self.setup_props.shader_compiler,
            RenderSetupCreationType::ExistingImages {
                images: self.native.swap_chain_images.clone(),
                image_views: self.native.swap_chain_image_view_list.clone(),
                extent: self.native.swap_img_and_viewport_extent,
                img_format: self.surf_format,
            },
            self.setup_props.compile_one_by_one.as_ref(),
            true,
        )?;

        self.multi_sampling = Some(CanvasSetupMultiSampling { native });

        Ok(())
    }

    pub fn sub_render_pass(&self, ty: RenderPassType) -> SubRenderPassDeref {
        match ty {
            RenderPassType::Normal(ty) => match ty {
                RenderPassSubType::Single => SubRenderPassDeref {
                    inner: self.native.render_pass.subpasses.load(),
                },
                RenderPassSubType::Switching1 => SubRenderPassDeref {
                    inner: self.switching.passes[0].render_pass.subpasses.load(),
                },
                RenderPassSubType::Switching2 => SubRenderPassDeref {
                    inner: self.switching.passes[1].render_pass.subpasses.load(),
                },
            },
            RenderPassType::MultiSampling => SubRenderPassDeref {
                inner: self
                    .multi_sampling
                    .as_ref()
                    .unwrap()
                    .native
                    .render_pass
                    .subpasses
                    .load(),
            },
        }
    }

    pub fn swap_chain_image_count(&self) -> usize {
        self.native.swap_chain_images.len()
    }

    fn multisampling_description(
        phy_device: &Arc<PhyDevice>,
        multi_sampling_count: u32,
        format: vk::Format,
    ) -> vk::AttachmentDescription {
        let mut multi_sampling_color_attachment = vk::AttachmentDescription::default();
        multi_sampling_color_attachment.format = format;
        multi_sampling_color_attachment.samples =
            Device::get_sample_count(multi_sampling_count, &phy_device.limits);
        multi_sampling_color_attachment.load_op = vk::AttachmentLoadOp::CLEAR;
        multi_sampling_color_attachment.store_op = vk::AttachmentStoreOp::DONT_CARE;
        multi_sampling_color_attachment.stencil_load_op = vk::AttachmentLoadOp::DONT_CARE;
        multi_sampling_color_attachment.stencil_store_op = vk::AttachmentStoreOp::DONT_CARE;
        multi_sampling_color_attachment.initial_layout = vk::ImageLayout::UNDEFINED;
        multi_sampling_color_attachment.final_layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
        multi_sampling_color_attachment
    }

    fn attachment_description(
        device: &Arc<LogicalDevice>,
        format: vk::Format,
        is_first_render_pass_in_frame: bool,
    ) -> vk::AttachmentDescription {
        let mut color_attachment = vk::AttachmentDescription::default();
        color_attachment.format = format;
        color_attachment.samples = vk::SampleCountFlags::TYPE_1;

        // if this attachment is a result of previous multi sampling => don't clear
        color_attachment.load_op = if is_first_render_pass_in_frame {
            vk::AttachmentLoadOp::CLEAR
        } else {
            vk::AttachmentLoadOp::LOAD
        };
        color_attachment.store_op = vk::AttachmentStoreOp::STORE;
        color_attachment.stencil_load_op = vk::AttachmentLoadOp::DONT_CARE;
        color_attachment.stencil_store_op = vk::AttachmentStoreOp::DONT_CARE;
        color_attachment.initial_layout = if is_first_render_pass_in_frame {
            vk::ImageLayout::UNDEFINED
        } else {
            device.final_layout()
        };
        color_attachment.final_layout = device.final_layout();
        color_attachment
    }

    fn stencil_description_pass(
        stencil_format: vk::Format,
        phy_device: &Arc<PhyDevice>,
        has_multi_sampling: Option<u32>,
    ) -> vk::AttachmentDescription {
        let mut stencil_attachment = vk::AttachmentDescription::default();
        stencil_attachment.format = stencil_format;
        stencil_attachment.samples = if let Some(multi_sampling_count) = has_multi_sampling {
            Device::get_sample_count(multi_sampling_count, &phy_device.limits)
        } else {
            vk::SampleCountFlags::TYPE_1
        };
        stencil_attachment.load_op = vk::AttachmentLoadOp::DONT_CARE;
        stencil_attachment.store_op = vk::AttachmentStoreOp::DONT_CARE;
        stencil_attachment.stencil_load_op = vk::AttachmentLoadOp::LOAD;
        stencil_attachment.stencil_store_op = vk::AttachmentStoreOp::STORE;
        stencil_attachment.initial_layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
        stencil_attachment.final_layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
        stencil_attachment
    }

    pub fn create_render_pass_switching(
        stencil_format: vk::Format,

        logical_device: &Arc<LogicalDevice>,
        layouts: &DescriptorLayouts,
        custom_pipes: &CustomPipelines,
        pipeline_cache: &Option<Arc<PipelineCacheInner>>,
        runtime_threadpool: CompileThreadpoolsRef<'_>,
        shader_compiler: &Arc<ShaderCompiler>,
        swapchain_extent: vk::Extent2D,

        has_multi_sampling: Option<u32>,
        format: vk::Format,
        compile_one_by_one: CompileOneByOneTypeRef<'_>,
    ) -> anyhow::Result<Arc<RenderPass>> {
        let has_multi_sampling_targets = has_multi_sampling.is_some();

        let phy_device = &logical_device.phy_device;

        let multi_sampling_color_attachment = Self::multisampling_description(
            phy_device,
            has_multi_sampling.unwrap_or_default(),
            format,
        );
        let mut color_attachment = Self::attachment_description(logical_device, format, true);
        color_attachment.final_layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;

        let stencil_attachment =
            Self::stencil_description_pass(stencil_format, phy_device, has_multi_sampling);

        let mut color_attachment_ref = vk::AttachmentReference::default();
        color_attachment_ref.layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;

        let mut multi_sampling_color_attachment_ref = vk::AttachmentReference::default();
        multi_sampling_color_attachment_ref.layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;

        let mut stencil_buffer_ref = vk::AttachmentReference::default();
        stencil_buffer_ref.layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;

        let mut attachments: [vk::AttachmentDescription; 6] = Default::default();
        let mut attachment_count = 0;
        attachments[attachment_count] = color_attachment;
        color_attachment_ref.attachment = attachment_count as u32;
        attachment_count += 1;
        if has_multi_sampling_targets {
            attachments[attachment_count] = multi_sampling_color_attachment;
            multi_sampling_color_attachment_ref.attachment = attachment_count as u32;
            attachment_count += 1;
        }
        attachments[attachment_count] = stencil_attachment;
        stencil_buffer_ref.attachment = attachment_count as u32;
        attachment_count += 1;

        let multi_sampling_color_attachment_ref = [multi_sampling_color_attachment_ref];
        let color_attachment_ref = [color_attachment_ref];
        let mut subpasses = vk::SubpassDescription::default();
        subpasses.pipeline_bind_point = vk::PipelineBindPoint::GRAPHICS;
        subpasses.color_attachment_count = 1;
        subpasses = subpasses.color_attachments(if has_multi_sampling_targets {
            &multi_sampling_color_attachment_ref
        } else {
            &color_attachment_ref
        });
        if has_multi_sampling_targets {
            subpasses = subpasses.resolve_attachments(&color_attachment_ref);
        };

        subpasses = subpasses.depth_stencil_attachment(&stencil_buffer_ref);

        let mut dependencies = [vk::SubpassDependency::default()];
        dependencies[0].src_subpass = vk::SUBPASS_EXTERNAL;
        dependencies[0].dst_subpass = 0;
        dependencies[0].src_stage_mask = vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS;
        dependencies[0].src_access_mask = vk::AccessFlags::COLOR_ATTACHMENT_WRITE
            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE;
        dependencies[0].dst_stage_mask = vk::PipelineStageFlags::FRAGMENT_SHADER
            | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS;
        dependencies[0].dst_access_mask = vk::AccessFlags::SHADER_READ
            | vk::AccessFlags::COLOR_ATTACHMENT_WRITE
            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE;
        dependencies[0].dependency_flags = vk::DependencyFlags::BY_REGION;

        /* TODO: remove me
        dependencies[1].src_subpass = 0;
        dependencies[1].dst_subpass = 0;
        dependencies[1].src_stage_mask = vk::PipelineStageFlags::FRAGMENT_SHADER
            | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;
        dependencies[1].src_access_mask = vk::AccessFlags::MEMORY_READ
            | vk::AccessFlags::MEMORY_WRITE
            | vk::AccessFlags::COLOR_ATTACHMENT_WRITE
            | vk::AccessFlags::SHADER_READ;
        dependencies[1].dst_stage_mask = vk::PipelineStageFlags::FRAGMENT_SHADER
            | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;
        dependencies[1].dst_access_mask = vk::AccessFlags::MEMORY_READ
            | vk::AccessFlags::MEMORY_WRITE
            | vk::AccessFlags::COLOR_ATTACHMENT_WRITE
            | vk::AccessFlags::SHADER_READ;
        dependencies[1].dependency_flags = vk::DependencyFlags::BY_REGION;
        */

        RenderPass::new(
            logical_device,
            has_multi_sampling.unwrap_or_default(),
            layouts,
            custom_pipes,
            pipeline_cache,
            runtime_threadpool,
            shader_compiler,
            swapchain_extent,
            &dependencies,
            &[subpasses],
            &attachments[0..attachment_count],
            compile_one_by_one,
        )
        .map_err(|err| {
            anyhow!(format!(
                "Creating the render pass for switching pass failed: {}",
                err
            ))
        })
    }

    pub fn create_render_pass_impl(
        logical_device: &Arc<LogicalDevice>,
        layouts: &DescriptorLayouts,
        custom_pipes: &CustomPipelines,
        pipeline_cache: &Option<Arc<PipelineCacheInner>>,
        runtime_threadpool: CompileThreadpoolsRef<'_>,
        shader_compiler: &Arc<ShaderCompiler>,
        swapchain_extent: vk::Extent2D,
        has_multi_sampling: Option<u32>,
        format: vk::Format,
        compile_one_by_one: CompileOneByOneTypeRef<'_>,
        is_first_render_pass_in_frame: bool,
    ) -> anyhow::Result<Arc<RenderPass>> {
        let has_multi_sampling_targets = has_multi_sampling.is_some();

        let phy_device = &logical_device.phy_device;

        let multi_sampling_color_attachment = Self::multisampling_description(
            phy_device,
            has_multi_sampling.unwrap_or_default(),
            format,
        );

        let color_attachment =
            Self::attachment_description(logical_device, format, is_first_render_pass_in_frame);

        let mut color_attachment_ref = vk::AttachmentReference::default();
        color_attachment_ref.layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;

        let mut multi_sampling_color_attachment_ref = vk::AttachmentReference::default();
        multi_sampling_color_attachment_ref.layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;

        let mut attachments: [vk::AttachmentDescription; 6] = Default::default();
        let mut attachment_count = 0;
        attachments[attachment_count] = color_attachment;
        color_attachment_ref.attachment = attachment_count as u32;
        attachment_count += 1;
        if has_multi_sampling_targets {
            attachments[attachment_count] = multi_sampling_color_attachment;
            multi_sampling_color_attachment_ref.attachment = attachment_count as u32;
            attachment_count += 1;
        }

        let multi_sampling_color_attachment_ref = [multi_sampling_color_attachment_ref];
        let color_attachment_ref = [color_attachment_ref];
        let mut subpasses = vk::SubpassDescription::default();
        subpasses.pipeline_bind_point = vk::PipelineBindPoint::GRAPHICS;
        subpasses.color_attachment_count = 1;
        subpasses = subpasses.color_attachments(if has_multi_sampling_targets {
            &multi_sampling_color_attachment_ref
        } else {
            &color_attachment_ref
        });
        if has_multi_sampling_targets {
            subpasses = subpasses.resolve_attachments(&color_attachment_ref);
        }

        let mut dependencies = [vk::SubpassDependency::default()];
        dependencies[0].src_subpass = vk::SUBPASS_EXTERNAL;
        dependencies[0].dst_subpass = 0;
        dependencies[0].src_stage_mask = vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;
        dependencies[0].src_access_mask = vk::AccessFlags::empty();
        dependencies[0].dst_stage_mask = vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;
        dependencies[0].dst_access_mask = vk::AccessFlags::COLOR_ATTACHMENT_WRITE;
        dependencies[0].dependency_flags = vk::DependencyFlags::BY_REGION;

        // The non multi sampling target loads the previous data, so add COLOR_ATTACHMENT_READ
        if !is_first_render_pass_in_frame {
            dependencies[0].dst_access_mask =
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::COLOR_ATTACHMENT_READ;
        }

        RenderPass::new(
            logical_device,
            has_multi_sampling.unwrap_or_default(),
            layouts,
            custom_pipes,
            pipeline_cache,
            runtime_threadpool,
            shader_compiler,
            swapchain_extent,
            &dependencies,
            &[subpasses],
            &attachments[0..attachment_count],
            compile_one_by_one,
        )
        .map_err(|err| anyhow!(format!("Creating the render pass failed: {}", err)))
    }

    pub fn try_finish_compile(&self, frame_resources: &mut FrameResources) -> anyhow::Result<()> {
        self.native
            .render_pass
            .try_finish_compile(frame_resources)?;
        for switching in self.switching.passes.iter() {
            switching.render_pass.try_finish_compile(frame_resources)?;
        }

        if let Some(mt) = self.multi_sampling.as_ref() {
            mt.native.render_pass.try_finish_compile(frame_resources)?;
        }

        Ok(())
    }
}
