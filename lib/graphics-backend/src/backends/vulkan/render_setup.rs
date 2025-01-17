use std::sync::Arc;

use anyhow::anyhow;
use ash::vk::{self};
use hiarc::Hiarc;

use crate::{backend::CustomPipelines, window::BackendSwapchain};

use super::{
    compiler::compiler::ShaderCompiler,
    descriptor_set::DescriptorSets,
    frame_resources::FrameResources,
    framebuffer::Framebuffer,
    image::Image,
    image_view::ImageView,
    logical_device::LogicalDevice,
    memory::MemoryImageBlock,
    pipeline_cache::PipelineCacheInner,
    render_pass::{CanvasSetup, CompileOneByOneTypeRef, CompileThreadpoolsRef, RenderPass},
    swapchain::Swapchain,
    vulkan_allocator::VulkanAllocator,
    vulkan_device::{DescriptorLayouts, Device},
    vulkan_types::{DeviceDescriptorPools, SwapChainImageBase, SwapChainImageFull},
};

#[must_use]
fn has_multi_sampling(device: &Arc<LogicalDevice>, multi_sampling_count: u32) -> bool {
    Device::get_sample_count(multi_sampling_count, &device.phy_device.limits)
        != vk::SampleCountFlags::TYPE_1
}

fn create_framebuffers_impl<'a>(
    device: &Arc<LogicalDevice>,
    multi_sampling_count: u32,
    image_views: impl ExactSizeIterator<Item = &'a Arc<ImageView>>,
    multi_sampling_images_views: &[SwapChainImageBase],
    stencil_list_for_pass_transition: Option<&Vec<SwapChainImageBase>>,
    render_pass: &Arc<RenderPass>,
    swapchain_extent: vk::Extent2D,
) -> anyhow::Result<Vec<Framebuffer>> {
    let has_multi_sampling_targets = has_multi_sampling(device, multi_sampling_count);
    let mut framebuffer_list: Vec<Framebuffer> = Vec::with_capacity(image_views.len());

    for (i, image_view) in image_views.enumerate() {
        let mut attachments: Vec<Arc<ImageView>> = Default::default();
        attachments.push(image_view.clone());
        if has_multi_sampling_targets {
            attachments.push(multi_sampling_images_views[i].img_view.clone());
        }
        if let Some(stencil_list_for_pass_transition) = stencil_list_for_pass_transition {
            attachments.push(stencil_list_for_pass_transition[i].img_view.clone());
        }

        framebuffer_list.push(Framebuffer::new(
            device.clone(),
            render_pass.clone(),
            attachments,
            swapchain_extent,
        )?);
    }

    Ok(framebuffer_list)
}

fn create_multi_sampler_image_attachments(
    device: &Arc<LogicalDevice>,
    multi_sampling_count: u32,
    mem_allocator: &Arc<parking_lot::Mutex<VulkanAllocator>>,
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
    swapchain_img_count: usize,
) -> anyhow::Result<Vec<SwapChainImageBase>> {
    let has_multi_sampling = has_multi_sampling(device, multi_sampling_count);
    let mut multi_sampling_images: Vec<SwapChainImageBase> =
        Vec::with_capacity(swapchain_img_count);
    if has_multi_sampling {
        for _ in 0..swapchain_img_count {
            let (img, img_mem) = mem_allocator.lock().create_image_ex(
                swapchain_extent.width,
                swapchain_extent.height,
                1,
                1,
                swapchain_format,
                vk::ImageTiling::OPTIMAL,
                vk::ImageUsageFlags::TRANSIENT_ATTACHMENT | vk::ImageUsageFlags::COLOR_ATTACHMENT,
                Some(multi_sampling_count),
            )?;

            let img_view = Device::create_image_view(
                device,
                &mut FrameResources::new(None),
                &img,
                swapchain_format,
                vk::ImageViewType::TYPE_2D,
                1,
                1,
                vk::ImageAspectFlags::COLOR,
            )?;

            multi_sampling_images.push(SwapChainImageBase {
                image: img,
                img_mem,
                img_view,
            });
        }
    }

    Ok(multi_sampling_images)
}

#[derive(Debug, Hiarc)]
pub enum RenderSetupNativeType {
    Swapchain(Swapchain),
    Offscreen {
        img_mems: Vec<MemoryImageBlock>,
        descrs: Vec<Arc<DescriptorSets>>,
    },
}

pub enum CanvasSetupCreationType<'a> {
    Swapchain((Swapchain, &'a BackendSwapchain)),
    Offscreen {
        extent: vk::Extent2D,
        img_count: usize,
        img_format: vk::SurfaceFormatKHR,
    },
}

pub enum RenderSetupCreationType {
    ExistingImages {
        images: Vec<Arc<Image>>,
        image_views: Vec<Arc<ImageView>>,
        extent: vk::Extent2D,
        img_format: vk::SurfaceFormatKHR,
    },
}

pub struct RenderSetupSwitchingCreation {
    pub offscreen_surfaces: [Arc<OffscreenSurface>; 2],
    pub stencil_images: Arc<Vec<SwapChainImageBase>>,
    pub stencil_format: vk::Format,
}

#[derive(Debug, Hiarc)]
pub struct CanvasSetupNative {
    // swap chain images, that are created by the surface
    pub swap_chain_images: Vec<Arc<Image>>,
    pub swap_chain_image_view_list: Vec<Arc<ImageView>>,
    pub swap_chain_multi_sampling_images: Vec<SwapChainImageBase>,
    pub framebuffer_list: Vec<Framebuffer>,
    #[hiarc_skip_unsafe]
    pub swap_img_and_viewport_extent: vk::Extent2D,

    pub render_pass: Arc<RenderPass>,
}

impl CanvasSetupNative {
    pub fn create_image_views(
        device: &Arc<LogicalDevice>,
        swapchain_format: vk::Format,
        images: &[Arc<Image>],
    ) -> anyhow::Result<Vec<Arc<ImageView>>> {
        let mut image_views: Vec<Arc<ImageView>> = Vec::with_capacity(images.len());

        for image in images.iter() {
            let mut view_create_info = vk::ImageViewCreateInfo::default();
            view_create_info.image = image.img(&mut FrameResources::new(None));
            view_create_info.view_type = vk::ImageViewType::TYPE_2D;
            view_create_info.format = swapchain_format;
            view_create_info.subresource_range.aspect_mask = vk::ImageAspectFlags::COLOR;
            view_create_info.subresource_range.base_mip_level = 0;
            view_create_info.subresource_range.base_array_layer = 0;
            view_create_info.subresource_range.layer_count = 1;
            view_create_info.subresource_range.level_count = 1;

            image_views.push(
                ImageView::new(device.clone(), image.clone(), view_create_info).map_err(|err| {
                    anyhow!("Could not create image views for the swap chain framebuffers: {err}")
                })?,
            );
        }

        Ok(image_views)
    }

    pub fn create_render_pass(
        device: &Arc<LogicalDevice>,
        multi_sampling_count: u32,
        layouts: &DescriptorLayouts,
        custom_pipes: &CustomPipelines,
        pipeline_cache: &Option<Arc<PipelineCacheInner>>,
        runtime_threadpool: CompileThreadpoolsRef<'_>,
        shader_compiler: &Arc<ShaderCompiler>,
        swapchain_extent: vk::Extent2D,
        swapchain_format: vk::Format,
        compile_one_by_one: CompileOneByOneTypeRef<'_>,
        is_first_render_pass_in_frame: bool,
    ) -> anyhow::Result<Arc<RenderPass>> {
        let has_multi_sampling = has_multi_sampling(device, multi_sampling_count);
        CanvasSetup::create_render_pass_impl(
            device,
            layouts,
            custom_pipes,
            pipeline_cache,
            runtime_threadpool,
            shader_compiler,
            swapchain_extent,
            if has_multi_sampling {
                Some(multi_sampling_count)
            } else {
                None
            },
            swapchain_format,
            compile_one_by_one,
            is_first_render_pass_in_frame,
        )
    }

    pub(crate) fn create_offscreen_images(
        mem_allocator: &Arc<parking_lot::Mutex<VulkanAllocator>>,
        width: u32,
        height: u32,
        img_count: usize,
        img_format: vk::Format,
    ) -> anyhow::Result<Vec<(Arc<Image>, MemoryImageBlock)>> {
        let mut images: Vec<(Arc<Image>, MemoryImageBlock)> = Vec::with_capacity(img_count);

        for _ in 0..img_count {
            let (img, img_mem) = mem_allocator.lock().create_image_ex(
                width,
                height,
                1,
                1,
                img_format,
                vk::ImageTiling::OPTIMAL,
                vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::INPUT_ATTACHMENT
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST,
                None,
            )?;

            images.push((img, img_mem));
        }
        Ok(images)
    }

    pub fn new(
        device: &Arc<LogicalDevice>,
        multi_sampling_count: u32,
        layouts: &DescriptorLayouts,
        custom_pipes: &CustomPipelines,
        pipeline_cache: &Option<Arc<PipelineCacheInner>>,
        mem_allocator: &Arc<parking_lot::Mutex<VulkanAllocator>>,
        runtime_threadpool: CompileThreadpoolsRef<'_>,
        shader_compiler: &Arc<ShaderCompiler>,
        create_type: RenderSetupCreationType,
        compile_one_by_one: CompileOneByOneTypeRef<'_>,
        is_first_render_pass_in_frame: bool,
    ) -> anyhow::Result<Self> {
        let (swap_chain_images, img_views, extent, swapchain_format) = match create_type {
            RenderSetupCreationType::ExistingImages {
                images,
                image_views,
                extent,
                img_format,
            } => (images, image_views, extent, img_format.format),
        };

        let multi_sampling_imgs = create_multi_sampler_image_attachments(
            device,
            multi_sampling_count,
            mem_allocator,
            swapchain_format,
            extent,
            swap_chain_images.len(),
        )?;

        let render_pass = Self::create_render_pass(
            device,
            multi_sampling_count,
            layouts,
            custom_pipes,
            pipeline_cache,
            runtime_threadpool,
            shader_compiler,
            extent,
            swapchain_format,
            compile_one_by_one,
            is_first_render_pass_in_frame,
        )?;

        let frame_buffers = create_framebuffers_impl(
            device,
            multi_sampling_count,
            img_views.iter(),
            &multi_sampling_imgs,
            None,
            &render_pass,
            extent,
        )?;

        Ok(Self {
            swap_img_and_viewport_extent: extent,
            swap_chain_images,
            swap_chain_image_view_list: img_views,
            swap_chain_multi_sampling_images: multi_sampling_imgs,
            render_pass,
            framebuffer_list: frame_buffers,
        })
    }
}

/// offscreen in a sense that it is never visible on the screen
/// not like in double buffering
#[derive(Debug, Hiarc, Default)]
pub struct OffscreenSurface {
    pub image_list: Vec<SwapChainImageFull>,
    pub multi_sampling_images: Vec<SwapChainImageBase>,
}

impl OffscreenSurface {
    pub fn create_images_for_switching_passes(
        device: &Arc<LogicalDevice>,
        layouts: &DescriptorLayouts,
        standard_texture_descr_pool: &Arc<parking_lot::Mutex<DeviceDescriptorPools>>,
        mem_allocator: &Arc<parking_lot::Mutex<VulkanAllocator>>,
        swapchain_img_count: usize,
        swapchain_format: vk::Format,
        swapchain_extent: vk::Extent2D,
    ) -> anyhow::Result<Vec<SwapChainImageFull>> {
        let mut image_list: Vec<SwapChainImageFull> = Vec::with_capacity(swapchain_img_count);

        for _ in 0..swapchain_img_count {
            let (img, img_mem) = mem_allocator.lock().create_image_ex(
                swapchain_extent.width,
                swapchain_extent.height,
                1,
                1,
                swapchain_format,
                vk::ImageTiling::OPTIMAL,
                vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::INPUT_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST
                    | vk::ImageUsageFlags::SAMPLED,
                None,
            )?;

            let img_view = Device::create_image_view(
                device,
                &mut FrameResources::new(None),
                &img,
                swapchain_format,
                vk::ImageViewType::TYPE_2D,
                1,
                1,
                vk::ImageAspectFlags::COLOR,
            )?;

            let descr = Device::create_new_textured_standard_descriptor_sets(
                device,
                layouts,
                standard_texture_descr_pool,
                &img_view,
            )
            .map_err(|err| {
                anyhow!("Could not create image descriptors for switching pass images: {err}")
            })?;

            image_list.push(SwapChainImageFull {
                base: SwapChainImageBase {
                    image: img,
                    img_mem,
                    img_view,
                },
                texture_descr_sets: descr,
            });
        }
        Ok(image_list)
    }

    pub fn new(
        device: &Arc<LogicalDevice>,
        multi_sampling_count: u32,
        layouts: &DescriptorLayouts,
        standard_texture_descr_pool: &Arc<parking_lot::Mutex<DeviceDescriptorPools>>,
        mem_allocator: &Arc<parking_lot::Mutex<VulkanAllocator>>,
        swapchain_img_count: usize,
        swapchain_format: vk::Format,
        swapchain_extent: vk::Extent2D,
    ) -> anyhow::Result<Arc<Self>> {
        let images = Self::create_images_for_switching_passes(
            device,
            layouts,
            standard_texture_descr_pool,
            mem_allocator,
            swapchain_img_count,
            swapchain_format,
            swapchain_extent,
        )?;

        let multi_sampling_images = create_multi_sampler_image_attachments(
            device,
            multi_sampling_count,
            mem_allocator,
            swapchain_format,
            swapchain_extent,
            swapchain_img_count,
        )?;

        Ok(Arc::new(Self {
            image_list: images,
            multi_sampling_images,
        }))
    }
}

#[derive(Debug, Hiarc)]
pub struct RenderSetupSwitchingPass {
    pub surface: Arc<OffscreenSurface>,
    // render into an offscreen framebuffer first
    pub render_pass: Arc<RenderPass>,
    pub framebuffer_list: Vec<Framebuffer>,
}

impl RenderSetupSwitchingPass {
    pub fn create_render_pass_switchting(
        device: &Arc<LogicalDevice>,
        multi_sampling_count: u32,
        layouts: &DescriptorLayouts,
        custom_pipes: &CustomPipelines,
        pipeline_cache: &Option<Arc<PipelineCacheInner>>,
        runtime_threadpool: CompileThreadpoolsRef<'_>,
        shader_compiler: &Arc<ShaderCompiler>,
        swapchain_extent: vk::Extent2D,

        stencil_format: vk::Format,
        format: vk::Format,
        compile_one_by_one: CompileOneByOneTypeRef<'_>,
    ) -> anyhow::Result<Arc<RenderPass>> {
        let has_multi_sampling = has_multi_sampling(device, multi_sampling_count);

        let render_pass = CanvasSetup::create_render_pass_switching(
            stencil_format,
            device,
            layouts,
            custom_pipes,
            pipeline_cache,
            runtime_threadpool,
            shader_compiler,
            swapchain_extent,
            if has_multi_sampling {
                Some(multi_sampling_count)
            } else {
                None
            },
            format,
            compile_one_by_one,
        )?;

        Ok(render_pass)
    }

    pub fn new(
        device: &Arc<LogicalDevice>,
        multi_sampling_count: u32,
        layouts: &DescriptorLayouts,
        custom_pipes: &CustomPipelines,
        pipeline_cache: &Option<Arc<PipelineCacheInner>>,
        runtime_threadpool: CompileThreadpoolsRef<'_>,
        shader_compiler: &Arc<ShaderCompiler>,
        stencil_list_for_pass_transition: &Vec<SwapChainImageBase>,
        stencil_format: vk::Format,
        swapchain_format: vk::Format,
        swapchain_extent: vk::Extent2D,
        surface: Arc<OffscreenSurface>,
        compile_one_by_one: CompileOneByOneTypeRef<'_>,
    ) -> anyhow::Result<Self> {
        let render_pass = Self::create_render_pass_switchting(
            device,
            multi_sampling_count,
            layouts,
            custom_pipes,
            pipeline_cache,
            runtime_threadpool,
            shader_compiler,
            swapchain_extent,
            stencil_format,
            swapchain_format,
            compile_one_by_one,
        )?;

        let framebuffers = create_framebuffers_impl(
            device,
            multi_sampling_count,
            surface.image_list.iter().map(|i| &i.base.img_view),
            &surface.multi_sampling_images,
            Some(stencil_list_for_pass_transition),
            &render_pass,
            swapchain_extent,
        )?;

        Ok(Self {
            surface,
            render_pass,
            framebuffer_list: framebuffers,
        })
    }
}

#[derive(Debug, Hiarc)]
pub struct CanvasSetupSwitching {
    pub stencil_list_for_pass_transition: Arc<Vec<SwapChainImageBase>>,
    // switching images generally only write to offscreen buffers
    pub passes: [RenderSetupSwitchingPass; 2],
}

impl CanvasSetupSwitching {
    /// returns stencil images and format
    pub(crate) fn create_stencil_attachments_for_pass_transition(
        device: &Arc<LogicalDevice>,
        multi_sampling_count: u32,
        mem_allocator: &Arc<parking_lot::Mutex<VulkanAllocator>>,
        swapchain_image_count: usize,
        swapchain_extent: vk::Extent2D,
    ) -> anyhow::Result<(Arc<Vec<SwapChainImageBase>>, vk::Format)> {
        let has_multi_sampling = has_multi_sampling(device, multi_sampling_count);
        let multi_sampling_count = if has_multi_sampling {
            Some(multi_sampling_count)
        } else {
            None
        };
        let mut stencil_images: Vec<SwapChainImageBase> = Vec::with_capacity(swapchain_image_count);

        // determine stencil image format
        let stencil_format = [
            vk::Format::S8_UINT,
            vk::Format::D32_SFLOAT_S8_UINT,
            vk::Format::D24_UNORM_S8_UINT,
            vk::Format::D16_UNORM_S8_UINT,
        ]
        .into_iter()
        .find(|format| {
            let props = unsafe {
                device
                    .phy_device
                    .instance
                    .vk_instance
                    .get_physical_device_format_properties(device.phy_device.cur_device, *format)
            };

            let tiling = vk::ImageTiling::OPTIMAL;
            let features = vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT;
            if tiling == vk::ImageTiling::LINEAR && props.linear_tiling_features.contains(features)
            {
                true
            } else {
                tiling == vk::ImageTiling::OPTIMAL
                    && props.optimal_tiling_features.contains(features)
            }
        })
        .unwrap();

        for _ in 0..swapchain_image_count {
            let (img, img_mem) = mem_allocator.lock().create_image_ex(
                swapchain_extent.width,
                swapchain_extent.height,
                1,
                1,
                stencil_format,
                vk::ImageTiling::OPTIMAL,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                multi_sampling_count,
            )?;
            let img_view = Device::create_image_view(
                device,
                &mut FrameResources::new(None),
                &img,
                stencil_format,
                vk::ImageViewType::TYPE_2D,
                1,
                1,
                vk::ImageAspectFlags::STENCIL,
            )?;

            stencil_images.push(SwapChainImageBase {
                image: img,
                img_mem,
                img_view,
            });
        }
        Ok((Arc::new(stencil_images), stencil_format))
    }

    pub fn new(
        device: &Arc<LogicalDevice>,
        multi_sampling_count: u32,
        layouts: &DescriptorLayouts,
        custom_pipes: &CustomPipelines,
        pipeline_cache: &Option<Arc<PipelineCacheInner>>,
        runtime_threadpool: CompileThreadpoolsRef<'_>,
        shader_compiler: &Arc<ShaderCompiler>,
        swapchain_surface_format: vk::Format,
        swapchain_extent: vk::Extent2D,
        creation: RenderSetupSwitchingCreation,
        compile_one_by_one: CompileOneByOneTypeRef<'_>,
    ) -> anyhow::Result<(Self, vk::Format)> {
        let [surface1, surface2] = creation.offscreen_surfaces;

        let passes = [
            RenderSetupSwitchingPass::new(
                device,
                multi_sampling_count,
                layouts,
                custom_pipes,
                pipeline_cache,
                runtime_threadpool,
                shader_compiler,
                &creation.stencil_images,
                creation.stencil_format,
                swapchain_surface_format,
                swapchain_extent,
                surface1,
                compile_one_by_one,
            )?,
            RenderSetupSwitchingPass::new(
                device,
                multi_sampling_count,
                layouts,
                custom_pipes,
                pipeline_cache,
                runtime_threadpool,
                shader_compiler,
                &creation.stencil_images,
                creation.stencil_format,
                swapchain_surface_format,
                swapchain_extent,
                surface2,
                compile_one_by_one,
            )?,
        ];

        Ok((
            Self {
                stencil_list_for_pass_transition: creation.stencil_images,
                passes,
            },
            creation.stencil_format,
        ))
    }
}
