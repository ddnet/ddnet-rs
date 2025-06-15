use std::sync::{atomic::AtomicUsize, Arc};

use anyhow::anyhow;
use ash::vk;
use base::linked_hash_map_view::FxLinkedHashMap;
use graphics_backend_traits::frame_fetcher_plugin::OffscreenCanvasId;
use hiarc::Hiarc;
use num_derive::FromPrimitive;
use strum::EnumCount;

use crate::{
    backend::CustomPipelines,
    backends::vulkan::{fence::Fence, semaphore::Semaphore},
    window::BackendSwapchain,
};

use super::{
    compiler::compiler::ShaderCompiler,
    frame::FrameCanvasIndex,
    frame_resources::FrameResources,
    logical_device::LogicalDevice,
    pipeline_cache::PipelineCacheInner,
    render_pass::{CanvasSetup, CompileOneByOneTypeRef, CompileThreadpools, CompileThreadpoolsRef},
    render_setup::CanvasSetupCreationType,
    swapchain::Swapchain,
    vulkan_allocator::VulkanAllocator,
    vulkan_device::DescriptorLayouts,
    vulkan_types::DeviceDescriptorPools,
};

#[repr(u32)]
#[derive(
    FromPrimitive, Hiarc, Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, EnumCount,
)]
pub enum StencilOpType {
    #[default]
    None,
    AlwaysPass,
    OnlyWhenPassed,
    OnlyWhenNotPassed,
}

#[derive(
    FromPrimitive, Hiarc, Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, EnumCount,
)]
#[repr(u32)]
pub enum ColorWriteMaskType {
    #[default]
    All,
    ColorOnly,
    AlphaOnly,
    None,
}

#[derive(Debug, Hiarc)]
enum CanvasModeInternal {
    Onscreen,
    Offscreen(OffscreenCanvasId),
}

#[derive(Debug)]
pub enum CanvasMode<'a> {
    Onscreen,
    Offscreen {
        id: OffscreenCanvasId,
        frame_resources: &'a mut FrameResources,
    },
}

#[derive(Debug)]
pub struct OffscreenCanvasCreateProps<'a> {
    pub device: &'a Arc<LogicalDevice>,
    pub layouts: &'a DescriptorLayouts,
    pub custom_pipes: &'a CustomPipelines,
    pub pipeline_cache: &'a Option<Arc<PipelineCacheInner>>,
    pub standard_texture_descr_pool: &'a Arc<parking_lot::Mutex<DeviceDescriptorPools>>,
    pub mem_allocator: &'a Arc<parking_lot::Mutex<VulkanAllocator>>,
    pub runtime_threadpool: CompileThreadpools,
    pub should_queue_full_compile: bool,
}

#[derive(Debug, Hiarc)]
pub struct RenderSetup {
    pub onscreen: Arc<CanvasSetup>,
    pub offscreens: FxLinkedHashMap<u128, Arc<CanvasSetup>>,

    cur_canvas_mode: CanvasModeInternal,

    // swapped by use case
    pub queue_submit_semaphores: Vec<Arc<Semaphore>>,

    pub busy_acquire_image_semaphores: Vec<Arc<Semaphore>>,
    pub acquired_image_semaphore: Arc<Semaphore>,

    pub queue_submit_fences: Vec<Arc<Fence>>,
    pub cur_image_index: u32,

    // required data
    pub shader_compiler: Arc<ShaderCompiler>,
    pub pipeline_compile_in_queue: Arc<AtomicUsize>,
    _device: Arc<LogicalDevice>,
}

impl RenderSetup {
    pub fn new(
        device: &Arc<LogicalDevice>,
        layouts: &DescriptorLayouts,
        custom_pipes: &CustomPipelines,
        pipeline_cache: &Option<Arc<PipelineCacheInner>>,
        standard_texture_descr_pool: &Arc<parking_lot::Mutex<DeviceDescriptorPools>>,
        mem_allocator: &Arc<parking_lot::Mutex<VulkanAllocator>>,
        runtime_threadpool: CompileThreadpoolsRef<'_>,
        swapchain: Swapchain,
        swapchain_backend: &BackendSwapchain,
        shader_compiler: ShaderCompiler,
        compile_one_by_one: bool,
        should_queue_full_compile: bool,
        has_multi_sampling: Option<u32>,
    ) -> anyhow::Result<Self> {
        let pipeline_compile_in_queue: Arc<AtomicUsize> = Default::default();

        let shader_compiler = Arc::new(shader_compiler);
        let onscreen = CanvasSetup::new(
            device,
            layouts,
            custom_pipes,
            pipeline_cache,
            standard_texture_descr_pool,
            mem_allocator,
            runtime_threadpool,
            &shader_compiler,
            CanvasSetupCreationType::Swapchain((swapchain, swapchain_backend)),
            if compile_one_by_one && should_queue_full_compile {
                CompileOneByOneTypeRef::CompileAndQueueFullCompile(&pipeline_compile_in_queue)
            } else if compile_one_by_one {
                CompileOneByOneTypeRef::Compile
            } else {
                CompileOneByOneTypeRef::None
            },
            has_multi_sampling,
        )?;

        let sync_object_count = onscreen.swap_chain_image_count();
        let queue_submit_semaphores: Vec<_> = (0..sync_object_count)
            .map(|_| Semaphore::new(device.clone(), device.is_headless))
            .collect::<anyhow::Result<_>>()?;
        let busy_acquire_image_semaphores: Vec<_> = (0..sync_object_count)
            .map(|_| Semaphore::new(device.clone(), device.is_headless))
            .collect::<anyhow::Result<_>>()?;

        let queue_submit_fences: Vec<_> = (0..sync_object_count)
            .map(|_| Fence::new(device.clone()))
            .collect::<anyhow::Result<_>>()?;

        let res = Self {
            onscreen,
            offscreens: Default::default(),

            cur_canvas_mode: CanvasModeInternal::Onscreen,

            queue_submit_semaphores,
            busy_acquire_image_semaphores,
            acquired_image_semaphore: Semaphore::new(device.clone(), device.is_headless)?,
            queue_submit_fences,
            cur_image_index: 0,

            shader_compiler,
            pipeline_compile_in_queue,
            _device: device.clone(),
        };
        Ok(res)
    }

    pub fn get_of_frame(&self, index: FrameCanvasIndex) -> &Arc<CanvasSetup> {
        match index {
            FrameCanvasIndex::Onscreen => &self.onscreen,
            FrameCanvasIndex::Offscreen(id) => self.offscreens.get(&id).unwrap(),
        }
    }

    pub fn get(&self) -> &Arc<CanvasSetup> {
        self.get_of_frame(self.cur_canvas())
    }

    pub fn switch_canvas(&mut self, mode: CanvasMode) -> anyhow::Result<()> {
        self.cur_canvas_mode = match mode {
            CanvasMode::Onscreen => CanvasModeInternal::Onscreen,
            CanvasMode::Offscreen {
                id,
                frame_resources,
            } => {
                frame_resources
                    .render_setups
                    .push(self.offscreens.get(&id).unwrap().clone());

                CanvasModeInternal::Offscreen(id)
            }
        };

        Ok(())
    }

    pub fn create_offscreen_canvas(
        &mut self,
        id: u128,
        width: u32,
        height: u32,
        has_multi_sampling: Option<u32>,
        props: OffscreenCanvasCreateProps<'_>,
    ) -> anyhow::Result<()> {
        self.offscreens.insert(
            id,
            CanvasSetup::new(
                props.device,
                props.layouts,
                props.custom_pipes,
                props.pipeline_cache,
                props.standard_texture_descr_pool,
                props.mem_allocator,
                CompileThreadpoolsRef {
                    one_by_one: &props.runtime_threadpool.one_by_one,
                    async_full: &props.runtime_threadpool.async_full,
                },
                &self.shader_compiler,
                CanvasSetupCreationType::Offscreen {
                    extent: vk::Extent2D { width, height },
                    img_count: self.onscreen.swap_chain_image_count(),
                    img_format: self.onscreen.surf_format,
                },
                if props.should_queue_full_compile {
                    CompileOneByOneTypeRef::CompileAndQueueFullCompile(
                        &self.pipeline_compile_in_queue,
                    )
                } else {
                    CompileOneByOneTypeRef::Compile
                },
                has_multi_sampling,
            )?,
        );
        Ok(())
    }

    pub fn destroy_offscreen_canvas(&mut self, id: u128) {
        let had_item = self.offscreens.remove(&id).is_some();
        debug_assert!(had_item);
    }

    pub fn cur_canvas(&self) -> FrameCanvasIndex {
        match self.cur_canvas_mode {
            CanvasModeInternal::Onscreen => FrameCanvasIndex::Onscreen,
            CanvasModeInternal::Offscreen(id) => FrameCanvasIndex::Offscreen(id),
        }
    }

    pub fn new_frame(&mut self, frame_resources: &mut FrameResources) -> anyhow::Result<()> {
        self.cur_canvas_mode = CanvasModeInternal::Onscreen;

        self.try_finish_compile(frame_resources)
    }

    pub fn try_finish_compile(
        &mut self,
        frame_resources: &mut FrameResources,
    ) -> anyhow::Result<()> {
        if self
            .pipeline_compile_in_queue
            .load(std::sync::atomic::Ordering::SeqCst)
            > 0
        {
            Arc::get_mut(&mut self.onscreen)
                .ok_or(anyhow!(
                    "could not get onscreen canvas setup as mut form Arc"
                ))?
                .try_finish_compile(frame_resources)?;
            for offscreen in self.offscreens.values_mut() {
                if let Some(offscreen) = Arc::get_mut(offscreen) {
                    offscreen.try_finish_compile(frame_resources)?;
                }
            }
        }

        Ok(())
    }
}
