use std::{
    collections::{HashMap, HashSet},
    ffi::CStr,
    num::NonZeroUsize,
    os::raw::c_void,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize},
    },
};

use base_io::{io::IoFileSys, runtime::IoRuntimeTask};
use crossbeam::channel::{Receiver, bounded, unbounded};
use graphics_backend_traits::{
    frame_fetcher_plugin::{
        BackendFrameFetcher, BackendPresentedImageDataRgba, FetchCanvasError, FetchCanvasIndex,
        OffscreenCanvasId,
    },
    plugin::{BackendCustomPipeline, BackendRenderExecuteInterface},
    traits::{DriverBackendInterface, GraphicsBackendMtInterface},
};
use graphics_base_traits::traits::{
    GraphicsStreamedData, GraphicsStreamedUniformData, GraphicsStreamedUniformDataType,
};

use anyhow::anyhow;
use graphics_types::{
    commands::{
        AllCommands, CommandCanvasResized, CommandClear, CommandCreateBufferObject,
        CommandCreateShaderStorage, CommandDeleteBufferObject, CommandDeleteShaderStorage,
        CommandIndicesForQuadsRequiredNotify, CommandMultiSampling, CommandOffscreenCanvasCreate,
        CommandOffscreenCanvasDestroy, CommandOffscreenCanvasSkipFetchingOnce,
        CommandRecreateBufferObject, CommandRender, CommandRenderQuadContainer,
        CommandRenderQuadContainerAsSpriteMultiple, CommandSwitchCanvasMode,
        CommandSwitchCanvasModeType, CommandTextureCreate, CommandTextureDestroy,
        CommandTextureUpdate, CommandUpdateBufferObject, CommandUpdateBufferRegion,
        CommandUpdateShaderStorage, CommandUpdateViewport, CommandVsync, CommandsMisc,
        CommandsRender, CommandsRenderMod, CommandsRenderQuadContainer, CommandsRenderStream,
        GRAPHICS_DEFAULT_UNIFORM_SIZE, GRAPHICS_MAX_UNIFORM_RENDER_COUNT,
        GRAPHICS_UNIFORM_INSTANCE_COUNT, GlVertexTex3DStream, RenderSpriteInfo, StreamDataMax,
    },
    gpu::Gpus,
    rendering::{GlVertex, State, StateTexture},
    types::{
        GraphicsBackendMemory, GraphicsBackendMemoryAllocation, GraphicsBackendMemoryStatic,
        GraphicsBackendMemoryStaticCleaner, GraphicsMemoryAllocationMode,
        GraphicsMemoryAllocationType,
    },
};

use ash::vk::{self};
use hiarc::Hiarc;
use log::{info, warn};
use pool::{arc::PoolArc, mt_pool::Pool as MtPool, traits::UnclearedVec};
use pool::{datatypes::PoolVec, pool::Pool};

use crate::{
    backend::CustomPipelines,
    backends::{
        null::mem_alloc_lazy, types::BackendWriteFiles, vulkan::pipeline_cache::PipelineCache,
    },
    window::{
        BackendDisplayRequirements, BackendSurface, BackendSurfaceAndHandles, BackendSwapchain,
        BackendWindow,
    },
};

use base::{benchmark::Benchmark, join_thread::JoinThread, linked_hash_map_view::FxLinkedHashMap};
use config::config::{AtomicGfxDebugModes, ConfigDebug, GfxDebugModes};

use super::{
    Options,
    buffer::Buffer,
    command_pool::{AutoCommandBuffer, AutoCommandBufferType, CommandPool},
    compiler::compiler::{ShaderCompiler, ShaderCompilerType},
    dbg_utils_messenger::DebugUtilsMessengerEXT,
    descriptor_set::{DescriptorSet, split_descriptor_sets},
    fence::Fence,
    frame::{Frame, FrameCanvasIndex},
    frame_collection::FrameCollector,
    frame_resources::{
        FrameResources, FrameResourcesPool, RenderThreadFrameResources,
        RenderThreadFrameResourcesPool,
    },
    image::Image,
    instance::Instance,
    logical_device::LogicalDevice,
    mapped_memory::MappedMemory,
    memory::MemoryBlock,
    memory_block::DeviceMemoryBlock,
    phy_device::PhyDevice,
    queue::Queue,
    render_cmds::{command_cb_render, get_address_mode_index},
    render_fill_manager::{RenderCommandExecuteBuffer, RenderCommandExecuteManager},
    render_group::{CanvasMode, OffscreenCanvasCreateProps, RenderSetup},
    render_pass::{CompileThreadpools, CompileThreadpoolsRef},
    render_setup::RenderSetupNativeType,
    stream_memory_pool::{StreamMemoryBlock, StreamMemoryPool},
    swapchain::Swapchain,
    vulkan_allocator::{
        VulkanAllocator, VulkanAllocatorImageCacheEntryData, VulkanDeviceInternalMemory,
    },
    vulkan_dbg::is_verbose,
    vulkan_device::Device,
    vulkan_types::{
        DescriptorPoolType, DeviceDescriptorPools, MemoryBlockType, RenderPassSubType,
        RenderPassType, RenderThread, RenderThreadEvent, StreamedUniformBuffer, TextureData,
        TextureObject, ThreadCommandGroup,
    },
};

#[derive(Debug, Hiarc)]
pub struct VulkanBackendLoadedIo {
    pub shader_compiler: ShaderCompiler,
    pub pipeline_cache: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct VulkanBackendLoadingIo {
    pub shader_compiler: IoRuntimeTask<ShaderCompiler>,
    pub pipeline_cache: IoRuntimeTask<Option<Vec<u8>>>,
}

impl VulkanBackendLoadingIo {
    pub fn new(io: &IoFileSys) -> Self {
        let fs = io.fs.clone();
        let backend_files = io.rt.spawn(async move {
            let mut shader_compiler =
                ShaderCompiler::new(ShaderCompilerType::WgslInSpvOut, fs).await;

            shader_compiler
                .compile("shader/wgsl".as_ref(), "compile.json".as_ref())
                .await?;

            Ok(shader_compiler)
        });

        let pipeline_cache = PipelineCache::load_previous_cache(io);

        Self {
            shader_compiler: backend_files,
            pipeline_cache,
        }
    }
}

#[derive(Hiarc)]
pub struct VulkanBackendAsh {
    pub(crate) vk_device: Arc<LogicalDevice>,
}

impl std::fmt::Debug for VulkanBackendAsh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanBackendAsh").finish()
    }
}

#[derive(Hiarc)]
pub struct VulkanBackendSurfaceAsh {
    vk_swap_chain_ash: BackendSwapchain,
    surface: BackendSurface,
}

impl std::fmt::Debug for VulkanBackendSurfaceAsh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanBackendSurfaceAsh").finish()
    }
}

#[derive(Debug, Hiarc)]
pub struct VulkanFetchFramebuffer {
    get_presented_img_data_helper_mem: Arc<DeviceMemoryBlock>,
    get_presented_img_data_helper_image: Arc<Image>,
    get_presented_img_data_helper_mapped_memory: Arc<MappedMemory>,
    get_presented_img_data_helper_mapped_layout_offset: vk::DeviceSize,
    get_presented_img_data_helper_mapped_layout_pitch: vk::DeviceSize,
    get_presented_img_data_helper_width: u32,
    get_presented_img_data_helper_height: u32,
    get_presented_img_data_helper_fence: Arc<Fence>,
}

#[derive(Debug, Hiarc)]
pub(crate) struct VulkanCustomPipes {
    #[hiarc_skip_unsafe]
    pub(crate) pipes: CustomPipelines,

    pub(crate) pipe_indices: HashMap<String, usize>,
}

impl VulkanCustomPipes {
    pub fn new(pipes: CustomPipelines) -> Arc<Self> {
        let mut pipe_indices: HashMap<String, usize> = Default::default();
        let pipes_guard = pipes.read();
        for (index, pipe) in pipes_guard.iter().enumerate() {
            pipe_indices.insert(pipe.pipe_name(), index);
        }
        drop(pipes_guard);
        Arc::new(Self {
            pipes,
            pipe_indices,
        })
    }
}

#[derive(Debug, Hiarc)]
pub(crate) struct VulkanBackendProps {
    /************************
     * MEMBER VARIABLES
     ************************/
    #[hiarc_skip_unsafe]
    dbg: Arc<AtomicGfxDebugModes>,
    gfx_vsync: bool,

    thread_count: usize,

    pub(crate) graphics_uniform_buffers: MtPool<Vec<GraphicsStreamedUniformData>>,

    pub(crate) ash_vk: VulkanBackendAsh,

    vk_gpu: Arc<PhyDevice>,
    pub(crate) device: Device,
    queue: Arc<Queue>,

    // never read from, but automatic cleanup
    _debug_messenger: Option<Arc<DebugUtilsMessengerEXT>>,

    command_pool: Rc<CommandPool>,

    uniform_buffer_descr_pools: Arc<parking_lot::Mutex<DeviceDescriptorPools>>,

    /************************
     * ERROR MANAGEMENT
     ************************/
    custom_pipes: Arc<VulkanCustomPipes>,
}

fn create_command_pools(
    device: Arc<LogicalDevice>,
    queue_family_index: u32,
    count: usize,
    default_primary_count: usize,
    default_secondary_count: usize,
) -> anyhow::Result<Vec<Rc<CommandPool>>> {
    let mut command_pools = Vec::new();
    for _ in 0..count {
        command_pools.push(CommandPool::new(
            device.clone(),
            queue_family_index,
            default_primary_count,
            default_secondary_count,
        )?);
    }
    Ok(command_pools)
}

#[derive(Debug)]
pub struct VulkanBackendLoading {
    props: VulkanBackendProps,
}

type InitNativeResult = (
    Arc<LogicalDevice>,
    Arc<PhyDevice>,
    Arc<Queue>,
    Device,
    Option<Arc<DebugUtilsMessengerEXT>>,
    Vec<Rc<CommandPool>>,
);

type ArcRwLock<T> = Arc<parking_lot::RwLock<T>>;

type InitialIndexBuffer = ((Arc<Buffer>, Arc<DeviceMemoryBlock>), usize);

impl VulkanBackendLoading {
    unsafe extern "system" fn vk_debug_callback(
        message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
        message_type: vk::DebugUtilsMessageTypeFlagsEXT,
        ptr_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
        _ptr_raw_user: *mut c_void,
    ) -> vk::Bool32 {
        if message_severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
            let msg = unsafe {
                CStr::from_ptr((*ptr_callback_data).p_message)
                    .to_str()
                    .unwrap()
            };
            println!("{msg}");
            panic!("[vulkan debug] error: {msg} {message_severity:?} {message_type:?}");
        } else if message_type.contains(vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE) {
            println!(
                "[vulkan debug] performance: {} {message_severity:?} {message_type:?}",
                unsafe {
                    CStr::from_ptr((*ptr_callback_data).p_message)
                        .to_str()
                        .unwrap()
                }
            );
        } else {
            println!(
                "[vulkan debug]: {} {message_severity:?} {message_type:?}",
                unsafe {
                    CStr::from_ptr((*ptr_callback_data).p_message)
                        .to_str()
                        .unwrap()
                }
            );
        }

        vk::FALSE
    }

    fn setup_debug_callback(
        entry: &ash::Entry,
        instance: &ash::Instance,
    ) -> anyhow::Result<Arc<DebugUtilsMessengerEXT>> {
        let mut create_info = vk::DebugUtilsMessengerCreateInfoEXT::default();
        create_info.message_severity = vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE
            | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
            | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR;
        create_info.message_type = vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
            | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE; // | vk::DebugUtilsMessageTypeFlagsEXT::GENERAL <- too annoying
        create_info.pfn_user_callback = Some(Self::vk_debug_callback);

        let res_dbg = DebugUtilsMessengerEXT::new(entry, instance, &create_info)
            .map_err(|err| anyhow!("Debug extension could not be loaded: {err}"))?;

        warn!("enabled vulkan debug context.");
        Ok(res_dbg)
    }

    fn get_device_queue(
        device: &ash::Device,
        graphics_queue_index: u32,
    ) -> anyhow::Result<(vk::Queue, vk::Queue)> {
        Ok((
            unsafe { device.get_device_queue(graphics_queue_index, 0) },
            unsafe { device.get_device_queue(graphics_queue_index, 0) },
        ))
    }

    fn init_vulkan_with_native(
        display_requirements: &BackendDisplayRequirements,
        dbg_mode: GfxDebugModes,
        dbg: Arc<AtomicGfxDebugModes>,
        texture_memory_usage: Arc<AtomicU64>,
        buffer_memory_usage: Arc<AtomicU64>,
        stream_memory_usage: Arc<AtomicU64>,
        staging_memory_usage: Arc<AtomicU64>,
        options: &Options,
    ) -> anyhow::Result<InitNativeResult> {
        let benchmark = Benchmark::new(options.dbg.bench);
        let instance = Instance::new(display_requirements, dbg_mode)?;
        benchmark.bench("creating vk instance");

        let mut dbg_callback = None;
        if dbg_mode == GfxDebugModes::Minimum || dbg_mode == GfxDebugModes::All {
            let dbg_res = Self::setup_debug_callback(&instance.vk_entry, &instance.vk_instance);
            if let Ok(dbg) = dbg_res {
                dbg_callback = Some(dbg);
            }
        }

        let physical_gpu =
            PhyDevice::new(instance.clone(), options, display_requirements.is_headless)?;
        benchmark.bench("selecting vk physical device");

        let device = LogicalDevice::new(
            physical_gpu.clone(),
            physical_gpu.queue_node_index,
            &instance.vk_instance,
            display_requirements.is_headless,
            dbg.clone(),
            texture_memory_usage.clone(),
            buffer_memory_usage.clone(),
            stream_memory_usage.clone(),
            staging_memory_usage.clone(),
        )?;
        benchmark.bench("creating vk logical device");

        let (graphics_queue, presentation_queue) =
            Self::get_device_queue(&device.device, physical_gpu.queue_node_index)?;

        let queue = Queue::new(graphics_queue, presentation_queue);

        benchmark.bench("creating vk queue");

        let command_pools =
            create_command_pools(device.clone(), physical_gpu.queue_node_index, 1, 5, 0)?;

        let device_instance = Device::new(
            dbg,
            instance.clone(),
            device.clone(),
            physical_gpu.clone(),
            queue.clone(),
            texture_memory_usage,
            buffer_memory_usage,
            stream_memory_usage,
            staging_memory_usage,
            options,
            command_pools[0].clone(),
        )?;
        benchmark.bench("creating vk command pools & layouts etc.");

        Ok((
            device,
            physical_gpu,
            queue,
            device_instance,
            dbg_callback,
            command_pools,
        ))
    }

    pub fn new(
        display_requirements: BackendDisplayRequirements,
        texture_memory_usage: Arc<AtomicU64>,
        buffer_memory_usage: Arc<AtomicU64>,
        stream_memory_usage: Arc<AtomicU64>,
        staging_memory_usage: Arc<AtomicU64>,

        options: &Options,

        custom_pipes: Option<ArcRwLock<Vec<Box<dyn BackendCustomPipeline>>>>,
    ) -> anyhow::Result<Self> {
        let dbg_mode = options.dbg.gfx; // TODO config / options
        let dbg = Arc::new(AtomicGfxDebugModes::new(dbg_mode));

        // thread count
        let auto_thread_count = if options.gl.thread_count == 0 {
            // auto should not alloc more than 3 threads
            // and at most half thread count
            (std::thread::available_parallelism()
                .unwrap_or(NonZeroUsize::new(1).unwrap())
                .get()
                / 2)
            .clamp(1, 3)
        } else {
            options.gl.thread_count as usize
        };
        let thread_count = auto_thread_count.clamp(
            1,
            std::thread::available_parallelism()
                .unwrap_or(NonZeroUsize::new(1).unwrap())
                .get(),
        );

        let (device, phy_gpu, queue, device_instance, dbg_utils_messenger, mut command_pools) =
            Self::init_vulkan_with_native(
                &display_requirements,
                dbg_mode,
                dbg.clone(),
                texture_memory_usage.clone(),
                buffer_memory_usage.clone(),
                stream_memory_usage.clone(),
                staging_memory_usage.clone(),
                options,
            )?;

        let benchmark = Benchmark::new(options.dbg.bench);

        let command_pool = command_pools.remove(0);

        let res = Self {
            props: VulkanBackendProps {
                dbg: dbg.clone(),
                gfx_vsync: options.gl.vsync,
                thread_count,

                graphics_uniform_buffers: MtPool::with_capacity(
                    GRAPHICS_UNIFORM_INSTANCE_COUNT * 2,
                ),

                ash_vk: VulkanBackendAsh {
                    vk_device: device.clone(),
                },

                vk_gpu: phy_gpu.clone(),

                device: device_instance,

                queue,
                _debug_messenger: dbg_utils_messenger,

                command_pool,

                uniform_buffer_descr_pools: DeviceDescriptorPools::new(
                    &device,
                    512,
                    DescriptorPoolType::Uniform,
                )?,

                custom_pipes: VulkanCustomPipes::new(custom_pipes.unwrap_or_default()),
            },
        };
        benchmark.bench("creating initial vk props");

        Ok(res)
    }
}

#[derive(Debug, Hiarc)]
pub struct VulkanMainThreadData {
    instance: Arc<Instance>,
    phy_gpu: Arc<PhyDevice>,
    mem_allocator: Arc<parking_lot::Mutex<VulkanAllocator>>,
}

#[derive(Debug, Hiarc)]
pub struct VulkanMainThreadInit {
    surface: BackendSurface,
}

#[derive(Debug, Hiarc)]
pub struct VulkanInUseStreamData {
    pub(crate) cur_stream_vertex_buffer: PoolArc<StreamMemoryBlock<()>>,
    pub(crate) cur_stream_uniform_buffers: PoolArc<StreamMemoryBlock<StreamedUniformBuffer>>,
}

#[derive(Debug, Hiarc)]
pub struct VulkanBackend {
    pub(crate) props: VulkanBackendProps,
    ash_surf: VulkanBackendSurfaceAsh,
    runtime_threadpool: Arc<rayon::ThreadPool>,
    compile_threadpool: Arc<rayon::ThreadPool>,

    pub(crate) in_use_data: VulkanInUseStreamData,

    streamed_vertex_buffers_pool: StreamMemoryPool<()>,
    streamed_uniform_buffers_pool: StreamMemoryPool<StreamedUniformBuffer>,

    pub(crate) render_index_buffer: Arc<Buffer>,
    render_index_buffer_memory: Arc<DeviceMemoryBlock>,
    cur_render_index_primitive_count: u64,

    last_render_thread_index: usize,
    recreate_swap_chain: bool,
    pub(crate) has_dynamic_viewport: bool,
    #[hiarc_skip_unsafe]
    pub(crate) dynamic_viewport_offset: vk::Offset2D,
    #[hiarc_skip_unsafe]
    pub(crate) dynamic_viewport_size: vk::Extent2D,
    cur_render_cmds_count_in_pipe: usize,

    commands_in_pipe: usize,

    main_render_command_buffer: Option<AutoCommandBuffer>,
    pub(crate) frame: Arc<parking_lot::Mutex<Frame>>,

    order_id_gen: usize,
    cur_frame: u64,
    image_last_frame_check: Vec<u64>,

    fetch_frame_buffer: Option<VulkanFetchFramebuffer>,
    last_presented_swap_chain_image_index: u32,
    #[hiarc_skip_unsafe]
    frame_fetchers: FxLinkedHashMap<String, Arc<dyn BackendFrameFetcher>>,
    frame_data_pool: MtPool<UnclearedVec<u8>>,
    /// Offscreen canvases that asked to be skiped this frame,
    /// e.g. because they couldn't render.
    offscreen_canvases_frame_fetching_skips: HashSet<OffscreenCanvasId>,

    render_threads: Vec<Arc<RenderThread>>,
    pub(crate) render: RenderSetup,
    pub(crate) multi_sampling_count: u32,
    next_multi_sampling_count: u32,

    render_setup_queue_full_pipeline_creation: bool,

    window_width: u32,
    window_height: u32,

    pub(crate) clear_color: [f32; 4],

    pub(crate) current_command_groups: HashMap<FrameCanvasIndex, ThreadCommandGroup>,
    command_groups: Vec<ThreadCommandGroup>,
    pub(crate) current_frame_resources: FrameResources,
    frame_resources: HashMap<u32, FrameResources>,

    frame_resources_pool: FrameResourcesPool,

    pipeline_cache: Option<PipelineCache>,
}

impl VulkanBackend {
    /************************
     * ERROR MANAGEMENT HELPER
     ************************/

    fn skip_frames_until_current_frame_is_used_again(&mut self) -> anyhow::Result<()> {
        // aggressivly try to get more memory
        unsafe {
            let _g = self.props.queue.queues.lock();
            self.props
                .ash_vk
                .vk_device
                .device
                .device_wait_idle()
                .unwrap()
        };
        for _ in 0..self.render.onscreen.swap_chain_image_count() + 1 {
            self.next_frame()?;
        }

        Ok(())
    }

    fn uniform_stream_alloc_func(&mut self, count: usize) -> anyhow::Result<()> {
        let device = &self.props.ash_vk.vk_device;
        let pools = &mut self.props.uniform_buffer_descr_pools;
        let sprite_descr_layout = &self
            .props
            .device
            .layouts
            .vertex_uniform_descriptor_set_layout;
        let quad_descr_layout = &self
            .props
            .device
            .layouts
            .vertex_fragment_uniform_descriptor_set_layout;

        let alloc_func = |buffer: &Arc<Buffer>,
                          mem_offset: vk::DeviceSize,
                          set_count: usize|
         -> anyhow::Result<Vec<StreamedUniformBuffer>> {
            let mut res: Vec<StreamedUniformBuffer> = Vec::with_capacity(set_count);
            let descr1: Vec<Arc<DescriptorSet>> = VulkanAllocator::create_uniform_descriptor_sets(
                device,
                pools,
                sprite_descr_layout,
                set_count,
                buffer,
                GRAPHICS_MAX_UNIFORM_RENDER_COUNT * GRAPHICS_DEFAULT_UNIFORM_SIZE,
                mem_offset,
            )?
            .into_iter()
            .flat_map(|sets| split_descriptor_sets(&sets))
            .collect();
            let descr2: Vec<Arc<DescriptorSet>> = VulkanAllocator::create_uniform_descriptor_sets(
                device,
                pools,
                quad_descr_layout,
                set_count,
                buffer,
                GRAPHICS_MAX_UNIFORM_RENDER_COUNT * GRAPHICS_DEFAULT_UNIFORM_SIZE,
                mem_offset,
            )?
            .into_iter()
            .flat_map(|sets| split_descriptor_sets(&sets))
            .collect();

            for (descr1, descr2) in descr1.into_iter().zip(descr2.into_iter()) {
                res.push(StreamedUniformBuffer {
                    uniform_sets: [descr1, descr2],
                });
            }

            Ok(res)
        };

        self.streamed_uniform_buffers_pool
            .try_alloc(alloc_func, count)?;

        Ok(())
    }

    /************************
     * COMMAND CALLBACKS
     ************************/
    fn command_cb_misc(&mut self, cmd_param: CommandsMisc) -> anyhow::Result<()> {
        match cmd_param {
            CommandsMisc::TextureCreate(cmd) => self.cmd_texture_create(cmd),
            CommandsMisc::TextureDestroy(cmd) => self.cmd_texture_destroy(&cmd),
            CommandsMisc::TextureUpdate(cmd) => self.cmd_texture_update(&cmd),
            CommandsMisc::CreateBufferObject(cmd) => self.cmd_create_buffer_object(cmd),
            CommandsMisc::RecreateBufferObject(cmd) => self.cmd_recreate_buffer_object(cmd),
            CommandsMisc::UpdateBufferObject(cmd) => self.cmd_update_buffer_object(cmd),
            CommandsMisc::DeleteBufferObject(cmd) => self.cmd_delete_buffer_object(&cmd),
            CommandsMisc::CreateShaderStorage(cmd) => self.cmd_create_shader_storage(cmd),
            CommandsMisc::UpdateShaderStorage(cmd) => self.cmd_update_shader_storage(cmd),
            CommandsMisc::DeleteShaderStorage(cmd) => self.cmd_delete_shader_storage(&cmd),
            CommandsMisc::OffscreenCanvasCreate(cmd) => self.cmd_create_offscreen_canvas(&cmd),
            CommandsMisc::OffscreenCanvasDestroy(cmd) => self.cmd_destroy_offscreen_canvas(&cmd),
            CommandsMisc::OffscreenCanvasSkipFetchingOnce(cmd) => {
                self.cmd_skip_fetching_offscreen_canvas(&cmd)
            }
            CommandsMisc::IndicesForQuadsRequiredNotify(cmd) => {
                self.cmd_indices_required_num_notify(&cmd)
            }
            CommandsMisc::Swap => self.cmd_swap(),
            CommandsMisc::NextSwitchPass => self.cmd_switch_to_switching_passes(),
            CommandsMisc::ConsumeMultiSamplingTargets => self.cmd_consume_multi_sampling_targets(),
            CommandsMisc::SwitchCanvas(cmd) => self.cmd_switch_canvas_mode(cmd),
            CommandsMisc::UpdateViewport(cmd) => self.cmd_update_viewport(&cmd),
            CommandsMisc::CanvasResized(cmd) => self.cmd_canvas_resized(&cmd),
            CommandsMisc::Multisampling(cmd) => self.cmd_mutli_sampling(cmd),
            CommandsMisc::VSync(cmd) => self.cmd_vsync(cmd),
        }
    }

    fn fill_execute_buffer(
        &mut self,
        cmd: &CommandsRender,
        exec_buffer: &mut RenderCommandExecuteBuffer,
    ) {
        let mut render_execute_manager = RenderCommandExecuteManager::new(exec_buffer, self);
        match &cmd {
            CommandsRender::Clear(cmd) => {
                Self::cmd_clear_fill_execute_buffer(&mut render_execute_manager, cmd)
            }
            CommandsRender::Stream(cmd) => match cmd {
                CommandsRenderStream::Render(cmd) => {
                    Self::cmd_render_fill_execute_buffer(&mut render_execute_manager, cmd)
                }
                CommandsRenderStream::RenderBlurred { cmd, .. } => {
                    Self::cmd_render_blurred_fill_execute_buffer(&mut render_execute_manager, cmd)
                }
            },
            CommandsRender::QuadContainer(cmd) => match cmd {
                CommandsRenderQuadContainer::Render(cmd) => {
                    Self::cmd_render_quad_container_ex_fill_execute_buffer(
                        &mut render_execute_manager,
                        cmd,
                    )
                }
                CommandsRenderQuadContainer::RenderAsSpriteMultiple(cmd) => {
                    Self::cmd_render_quad_container_as_sprite_multiple_fill_execute_buffer(
                        &mut render_execute_manager,
                        cmd,
                    )
                }
            },
            CommandsRender::Mod(CommandsRenderMod { mod_name, cmd }) => {
                if let Some(mod_index) = render_execute_manager
                    .backend
                    .props
                    .custom_pipes
                    .pipe_indices
                    .get(mod_name.as_str())
                {
                    let pipes = render_execute_manager
                        .backend
                        .props
                        .custom_pipes
                        .pipes
                        .clone();
                    pipes.read()[*mod_index].fill_exec_buffer(cmd, &mut render_execute_manager);
                }
            }
        }
    }

    /*****************************
     * VIDEO AND SCREENSHOT HELPER
     ******************************/
    fn prepare_presented_image_data_image(
        &mut self,
        res_image_data: &mut &mut [u8],
        width: u32,
        height: u32,
    ) -> anyhow::Result<()> {
        let needs_new_img: bool = self.fetch_frame_buffer.is_none()
            || width
                != self
                    .fetch_frame_buffer
                    .as_ref()
                    .unwrap()
                    .get_presented_img_data_helper_width
            || height
                != self
                    .fetch_frame_buffer
                    .as_ref()
                    .unwrap()
                    .get_presented_img_data_helper_height;
        if needs_new_img {
            if self.fetch_frame_buffer.is_some() {
                self.delete_presented_image_data_image();
            }

            let mut image_info = vk::ImageCreateInfo::default();
            image_info.image_type = vk::ImageType::TYPE_2D;
            image_info.extent.width = width;
            image_info.extent.height = height;
            image_info.extent.depth = 1;
            image_info.mip_levels = 1;
            image_info.array_layers = 1;
            image_info.format = vk::Format::R8G8B8A8_UNORM;
            image_info.tiling = vk::ImageTiling::LINEAR;
            image_info.initial_layout = vk::ImageLayout::UNDEFINED;
            image_info.usage = vk::ImageUsageFlags::TRANSFER_DST;
            image_info.samples = vk::SampleCountFlags::TYPE_1;
            image_info.sharing_mode = vk::SharingMode::EXCLUSIVE;

            let presented_img_data_helper_image =
                Image::new(self.props.ash_vk.vk_device.clone(), image_info)?;
            // Create memory to back up the image
            let mem_requirements = unsafe {
                self.props
                    .ash_vk
                    .vk_device
                    .device
                    .get_image_memory_requirements(
                        presented_img_data_helper_image.img(&mut FrameResources::new(None)),
                    )
            };

            let mut mem_alloc_info = vk::MemoryAllocateInfo::default();
            mem_alloc_info.allocation_size = mem_requirements.size;
            mem_alloc_info.memory_type_index = self.props.device.mem.find_memory_type(
                self.props.vk_gpu.cur_device,
                mem_requirements.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_CACHED,
            )?;

            let presented_img_data_helper_mem = DeviceMemoryBlock::new(
                self.props.ash_vk.vk_device.clone(),
                mem_alloc_info,
                MemoryBlockType::Texture,
            )?;
            presented_img_data_helper_image.bind(presented_img_data_helper_mem.clone(), 0)?;

            self.props.device.image_barrier(
                &mut self.current_frame_resources,
                &presented_img_data_helper_image,
                0,
                1,
                0,
                1,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::GENERAL,
            )?;

            let sub_resource = vk::ImageSubresource::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(0)
                .array_layer(0);
            let sub_resource_layout = unsafe {
                self.props
                    .ash_vk
                    .vk_device
                    .device
                    .get_image_subresource_layout(
                        presented_img_data_helper_image.img(&mut FrameResources::new(None)),
                        sub_resource,
                    )
            };

            self.fetch_frame_buffer = Some(VulkanFetchFramebuffer {
                get_presented_img_data_helper_mapped_memory: MappedMemory::new(
                    self.props.ash_vk.vk_device.clone(),
                    presented_img_data_helper_mem.clone(),
                    sub_resource_layout.offset,
                )?,
                get_presented_img_data_helper_mapped_layout_offset: sub_resource_layout.offset,
                get_presented_img_data_helper_mapped_layout_pitch: sub_resource_layout.row_pitch,
                get_presented_img_data_helper_fence: Fence::new(
                    self.props.ash_vk.vk_device.clone(),
                )?,
                get_presented_img_data_helper_width: width,
                get_presented_img_data_helper_height: height,
                get_presented_img_data_helper_image: presented_img_data_helper_image,
                get_presented_img_data_helper_mem: presented_img_data_helper_mem,
            });
        }
        *res_image_data = unsafe {
            std::slice::from_raw_parts_mut(
                self.fetch_frame_buffer
                    .as_ref()
                    .ok_or_else(|| anyhow!("copy image mapped mem was empty"))?
                    .get_presented_img_data_helper_mapped_memory
                    .get_mem(),
                self.fetch_frame_buffer
                    .as_ref()
                    .ok_or_else(|| anyhow!("copy image mem was empty"))?
                    .get_presented_img_data_helper_mem
                    .as_ref()
                    .size() as usize
                    - self
                        .fetch_frame_buffer
                        .as_ref()
                        .ok_or_else(|| anyhow!("copy image offset was empty"))?
                        .get_presented_img_data_helper_mapped_layout_offset
                        as usize,
            )
        };
        Ok(())
    }

    fn delete_presented_image_data_image(&mut self) {
        self.fetch_frame_buffer = None;
    }

    fn get_presented_image_data_impl(
        &mut self,
        fetch_index: FetchCanvasIndex,
    ) -> anyhow::Result<BackendPresentedImageDataRgba, FetchCanvasError> {
        let width: u32;
        let height: u32;
        let mut dest_data_buff = self.frame_data_pool.new();
        let (render, final_layout) = match fetch_index {
            FetchCanvasIndex::Onscreen => (
                &self.render.onscreen,
                self.props.ash_vk.vk_device.final_layout(),
            ),
            FetchCanvasIndex::Offscreen(id) => (
                self.render
                    .offscreens
                    .get(&id)
                    .ok_or(FetchCanvasError::CanvasNotFound)?,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ),
        };
        let mut is_b8_g8_r8_a8: bool = render.surf_format.format == vk::Format::B8G8R8A8_UNORM;
        let uses_rgba_like_format: bool =
            render.surf_format.format == vk::Format::R8G8B8A8_UNORM || is_b8_g8_r8_a8;
        if uses_rgba_like_format && self.last_presented_swap_chain_image_index != u32::MAX {
            let viewport = render.native.swap_img_and_viewport_extent;
            width = viewport.width;
            height = viewport.height;

            let image_total_size: usize = width as usize * height as usize * 4;

            let mut res_image_data: &mut [u8] = &mut [];
            self.prepare_presented_image_data_image(&mut res_image_data, width, height)
                .map_err(|err| anyhow!("Could not prepare presented image data: {err}"))?;

            let render = match fetch_index {
                FetchCanvasIndex::Onscreen => &self.render.onscreen,
                FetchCanvasIndex::Offscreen(id) => self.render.offscreens.get(&id).unwrap(),
            };

            let fetch_frame_buffer = self
                .fetch_frame_buffer
                .as_ref()
                .ok_or_else(|| anyhow!("fetch resources were none"))?;

            let command_buffer = self
                .props
                .device
                .get_memory_command_buffer(&mut FrameResources::new(None))
                .map_err(|err| anyhow!("Could not get memory command buffer: {err}"))?
                .command_buffer;

            let swap_img = &render.native.swap_chain_images
                [self.last_presented_swap_chain_image_index as usize];

            self.props
                .device
                .image_barrier(
                    &mut self.current_frame_resources,
                    &fetch_frame_buffer.get_presented_img_data_helper_image,
                    0,
                    1,
                    0,
                    1,
                    vk::ImageLayout::GENERAL,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                )
                .map_err(|err| anyhow!("Image barrier failed for the helper image: {err}"))?;
            self.props
                .device
                .image_barrier(
                    &mut self.current_frame_resources,
                    swap_img,
                    0,
                    1,
                    0,
                    1,
                    final_layout,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                )
                .map_err(|err| anyhow!("Image barrier failed for the swapchain image: {err}"))?;

            // If source and destination support blit we'll blit as this also does
            // automatic format conversion (e.g. from BGR to RGB)
            if self
                .props
                .ash_vk
                .vk_device
                .phy_device
                .config
                .read()
                .unwrap()
                .optimal_swap_chain_image_blitting
                && self
                    .props
                    .ash_vk
                    .vk_device
                    .phy_device
                    .config
                    .read()
                    .unwrap()
                    .linear_rgba_image_blitting
            {
                let mut blit_size = vk::Offset3D::default();
                blit_size.x = width as i32;
                blit_size.y = height as i32;
                blit_size.z = 1;
                let mut image_blit_region = vk::ImageBlit::default();
                image_blit_region.src_subresource.aspect_mask = vk::ImageAspectFlags::COLOR;
                image_blit_region.src_subresource.layer_count = 1;
                image_blit_region.src_offsets[1] = blit_size;
                image_blit_region.dst_subresource.aspect_mask = vk::ImageAspectFlags::COLOR;
                image_blit_region.dst_subresource.layer_count = 1;
                image_blit_region.dst_offsets[1] = blit_size;

                // Issue the blit command
                unsafe {
                    self.props.ash_vk.vk_device.device.cmd_blit_image(
                        command_buffer,
                        swap_img.img(&mut self.current_frame_resources),
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        fetch_frame_buffer
                            .get_presented_img_data_helper_image
                            .img(&mut FrameResources::new(None)),
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[image_blit_region],
                        vk::Filter::NEAREST,
                    )
                };

                // transformed to RGBA
                is_b8_g8_r8_a8 = false;
            } else {
                // Otherwise use image copy (requires us to manually flip components)
                let mut image_copy_region = vk::ImageCopy::default();
                image_copy_region.src_subresource.aspect_mask = vk::ImageAspectFlags::COLOR;
                image_copy_region.src_subresource.layer_count = 1;
                image_copy_region.dst_subresource.aspect_mask = vk::ImageAspectFlags::COLOR;
                image_copy_region.dst_subresource.layer_count = 1;
                image_copy_region.extent.width = width;
                image_copy_region.extent.height = height;
                image_copy_region.extent.depth = 1;

                // Issue the copy command
                unsafe {
                    self.props.ash_vk.vk_device.device.cmd_copy_image(
                        command_buffer,
                        swap_img.img(&mut self.current_frame_resources),
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        fetch_frame_buffer
                            .get_presented_img_data_helper_image
                            .img(&mut FrameResources::new(None)),
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[image_copy_region],
                    );
                }
            }

            self.props
                .device
                .image_barrier(
                    &mut self.current_frame_resources,
                    &fetch_frame_buffer.get_presented_img_data_helper_image,
                    0,
                    1,
                    0,
                    1,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::GENERAL,
                )
                .map_err(|err| anyhow!("Image barrier failed for the helper image: {err}"))?;
            self.props
                .device
                .image_barrier(
                    &mut self.current_frame_resources,
                    swap_img,
                    0,
                    1,
                    0,
                    1,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    final_layout,
                )
                .map_err(|err| anyhow!("Image barrier failed for the swap chain image: {err}"))?;

            self.props.device.memory_command_buffer = None;

            let command_buffers = [command_buffer];
            let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);

            unsafe {
                self.props
                    .ash_vk
                    .vk_device
                    .device
                    .reset_fences(&[fetch_frame_buffer
                        .get_presented_img_data_helper_fence
                        .fence(&mut self.current_frame_resources)])
            }
            .map_err(|err| anyhow!("Could not reset fences: {err}"))?;
            unsafe {
                let queue = &self.props.queue.queues.lock();
                self.props.ash_vk.vk_device.device.queue_submit(
                    queue.graphics_queue,
                    &[submit_info],
                    fetch_frame_buffer
                        .get_presented_img_data_helper_fence
                        .fence(&mut self.current_frame_resources),
                )
            }
            .map_err(|err| anyhow!("Queue submit failed: {err}"))?;
            unsafe {
                self.props.ash_vk.vk_device.device.wait_for_fences(
                    &[fetch_frame_buffer
                        .get_presented_img_data_helper_fence
                        .fence(&mut self.current_frame_resources)],
                    true,
                    u64::MAX,
                )
            }
            .map_err(|err| anyhow!("Could not wait for fences: {err}"))?;

            let mut mem_range = vk::MappedMemoryRange::default();
            mem_range.memory = fetch_frame_buffer
                .get_presented_img_data_helper_mem
                .mem(&mut FrameResources::new(None));
            mem_range.offset =
                fetch_frame_buffer.get_presented_img_data_helper_mapped_layout_offset;
            mem_range.size = vk::WHOLE_SIZE;
            unsafe {
                self.props
                    .ash_vk
                    .vk_device
                    .device
                    .invalidate_mapped_memory_ranges(&[mem_range])
            }
            .map_err(|err| anyhow!("Could not invalidate mapped memory ranges: {err}"))?;

            let real_full_image_size: usize = image_total_size.max(
                height as usize
                    * fetch_frame_buffer.get_presented_img_data_helper_mapped_layout_pitch as usize,
            );
            if dest_data_buff.len() < real_full_image_size + (width * 4) as usize {
                dest_data_buff.resize(
                    real_full_image_size + (width * 4) as usize,
                    Default::default(),
                ); // extra space for flipping
            }
            let dst_buff = dest_data_buff
                .as_mut_slice()
                .split_at_mut(real_full_image_size)
                .0;
            let src_buff = res_image_data.split_at(real_full_image_size).0;
            dst_buff.copy_from_slice(src_buff);

            // pack image data together without any offset
            // that the driver might require
            if width as u64 * 4
                < fetch_frame_buffer.get_presented_img_data_helper_mapped_layout_pitch
            {
                for y in 0..height as usize {
                    let offset_image_packed: usize = y * width as usize * 4;
                    let offset_image_unpacked: usize = y * fetch_frame_buffer
                        .get_presented_img_data_helper_mapped_layout_pitch
                        as usize;

                    let (img_part, help_part) = dest_data_buff
                        .as_mut_slice()
                        .split_at_mut(real_full_image_size);

                    let unpacked_part = img_part.split_at(offset_image_unpacked).1;
                    help_part[..width as usize * 4]
                        .copy_from_slice(&unpacked_part[..width as usize * 4]);

                    let packed_part = img_part.split_at_mut(offset_image_packed).1;
                    packed_part[..width as usize * 4]
                        .copy_from_slice(&help_part[..width as usize * 4]);
                }
            }

            if is_b8_g8_r8_a8 {
                // swizzle
                for y in 0..height as usize {
                    for x in 0..width as usize {
                        let img_off: usize = (y * width as usize * 4) + (x * 4);
                        if is_b8_g8_r8_a8 {
                            let tmp = dest_data_buff[img_off];
                            dest_data_buff[img_off] = dest_data_buff[img_off + 2];
                            dest_data_buff[img_off + 2] = tmp;
                        }
                        dest_data_buff[img_off + 3] = 255;
                    }
                }
            }

            dest_data_buff.resize(width as usize * height as usize * 4, Default::default());

            Ok(BackendPresentedImageDataRgba {
                width,
                height,
                dest_data_buffer: dest_data_buff,
            })
        } else if !uses_rgba_like_format {
            Err(FetchCanvasError::DriverErr("Swap chain image was not ready to be copied, because it was not in a RGBA like format.".to_string()))
        } else {
            Err(FetchCanvasError::DriverErr(
                "Swap chain image was not ready to be copied.".to_string(),
            ))
        }
    }

    /************************
     * SWAPPING MECHANISM
     ************************/
    fn start_render_thread(&mut self, thread_index: usize) -> anyhow::Result<()> {
        if !self.command_groups.is_empty() {
            let thread = &mut self.render_threads[thread_index];
            for command_group in self.command_groups.drain(..) {
                let render = self.render.get_of_frame(command_group.canvas_index).clone();
                thread
                    .events
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                thread
                    .sender
                    .send(RenderThreadEvent::Render((command_group, render)))?;
            }
        }
        Ok(())
    }

    fn handle_all_command_groups(&mut self) -> anyhow::Result<()> {
        // execute threads
        let mut thread_index = self.last_render_thread_index;
        while !self.command_groups.is_empty() {
            self.start_render_thread(thread_index % self.props.thread_count)?;
            thread_index += 1;
        }
        Ok(())
    }

    fn finish_render_threads(&mut self) -> anyhow::Result<()> {
        self.handle_all_command_groups()?;

        for thread_index in 0..self.props.thread_count {
            let render_thread = &mut self.render_threads[thread_index];
            if render_thread
                .events
                .load(std::sync::atomic::Ordering::SeqCst)
                != 0
            {
                let (sender, receiver) = bounded(1);
                render_thread
                    .events
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                render_thread.sender.send(RenderThreadEvent::Sync(sender))?;
                receiver.recv()?;
            }
        }
        Ok(())
    }

    fn execute_memory_command_buffer(&mut self) {
        if let Some(memory_command_buffer) = self.props.device.memory_command_buffer.take() {
            let command_buffer = memory_command_buffer.command_buffer;
            drop(memory_command_buffer);

            let command_buffers = [command_buffer];
            let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);
            unsafe {
                let queue = &self.props.queue.queues.lock();
                self.props
                    .ash_vk
                    .vk_device
                    .device
                    .queue_submit(queue.graphics_queue, &[submit_info], vk::Fence::null())
                    .unwrap();
            }
            unsafe {
                let queue = &self.props.queue.queues.lock();
                self.props
                    .ash_vk
                    .vk_device
                    .device
                    .queue_wait_idle(queue.graphics_queue)
                    .unwrap();
            }
        }
    }

    fn flush_memory_ranges(&mut self) {
        if !self.props.device.non_flushed_memory_ranges.is_empty() {
            unsafe {
                self.props
                    .ash_vk
                    .vk_device
                    .device
                    .flush_mapped_memory_ranges(
                        self.props.device.non_flushed_memory_ranges.as_slice(),
                    )
                    .unwrap();
            }

            self.props.device.non_flushed_memory_ranges.clear();
        }
    }

    fn upload_non_flushed_buffers(&mut self) {
        self.flush_memory_ranges();
    }

    fn clear_frame_data(&mut self, frame_index: u32) {
        self.flush_memory_ranges();
        self.frame_resources.remove(&frame_index);
    }

    fn clear_frame_memory_usage(&mut self) {
        self.clear_frame_data(self.render.cur_image_index);
    }

    fn start_new_render_pass(&mut self, render_pass_type: RenderPassType) -> anyhow::Result<()> {
        if let Some(current_command_group) =
            self.current_command_groups.get(&self.render.cur_canvas())
        {
            self.new_command_group(
                current_command_group.canvas_index,
                current_command_group.render_pass_index + 1,
                render_pass_type,
            )?;
        }

        Ok(())
    }

    fn cmd_switch_to_switching_passes(&mut self) -> anyhow::Result<()> {
        if let Some(current_command_group) =
            self.current_command_groups.get(&self.render.cur_canvas())
        {
            match current_command_group.render_pass {
                RenderPassType::Normal(ty) => match ty {
                    RenderPassSubType::Single | RenderPassSubType::Switching2 => {
                        self.start_new_render_pass(RenderPassType::Normal(
                            RenderPassSubType::Switching1,
                        ))?;
                    }
                    RenderPassSubType::Switching1 => {
                        self.start_new_render_pass(RenderPassType::Normal(
                            RenderPassSubType::Switching2,
                        ))?;
                    }
                },
                RenderPassType::MultiSampling => {
                    self.start_new_render_pass(RenderPassType::Normal(
                        RenderPassSubType::Switching1,
                    ))?;
                }
            }
        }
        Ok(())
    }

    fn cmd_consume_multi_sampling_targets(&mut self) -> anyhow::Result<()> {
        // if and only if multi sampling is currently active, start a new render pass
        if let Some(RenderPassType::MultiSampling) = self
            .current_command_groups
            .get(&self.render.cur_canvas())
            .map(|c| c.render_pass)
        {
            self.start_new_render_pass(RenderPassType::Normal(RenderPassSubType::Single))?;
        }
        Ok(())
    }

    fn cmd_switch_canvas_mode(&mut self, cmd: CommandSwitchCanvasMode) -> anyhow::Result<()> {
        let (canvas_index, has_multi_sampling) = match &cmd.mode {
            // even if onscreen has multi-sampling. this is not allowed
            CommandSwitchCanvasModeType::Onscreen => (FrameCanvasIndex::Onscreen, false),
            CommandSwitchCanvasModeType::Offscreen { id } => (
                FrameCanvasIndex::Offscreen(*id),
                self.render
                    .offscreens
                    .get(id)
                    .unwrap()
                    .multi_sampling
                    .is_some(),
            ),
        };
        let mut frame_g = self.frame.lock();
        let frame = &mut *frame_g;
        match canvas_index {
            FrameCanvasIndex::Onscreen => {}
            FrameCanvasIndex::Offscreen(index) => {
                frame.new_offscreen(index, self.render.offscreens.get(&index).cloned().unwrap())
            }
        }
        drop(frame_g);
        if !self.current_command_groups.contains_key(&canvas_index) {
            self.new_command_group(
                canvas_index,
                0,
                if has_multi_sampling {
                    RenderPassType::MultiSampling
                } else {
                    RenderPassType::default()
                },
            )?;
        }
        match &cmd.mode {
            CommandSwitchCanvasModeType::Offscreen { id, .. } => {
                self.render.switch_canvas(CanvasMode::Offscreen {
                    id: *id,
                    frame_resources: &mut self.current_frame_resources,
                })?
            }
            CommandSwitchCanvasModeType::Onscreen => {
                self.render.switch_canvas(CanvasMode::Onscreen)?
            }
        }
        Ok(())
    }

    fn new_command_group(
        &mut self,
        canvas_index: FrameCanvasIndex,
        render_pass_index: usize,
        render_pass_type: RenderPassType,
    ) -> anyhow::Result<()> {
        let create_command_group = || {
            let mut command_group = ThreadCommandGroup::default();
            command_group.render_pass = render_pass_type;
            command_group.cur_frame_index = self.render.cur_image_index;
            command_group.canvas_index = canvas_index;
            command_group
        };
        let current_command_group = self
            .current_command_groups
            .entry(canvas_index)
            .or_insert_with(create_command_group);
        if !current_command_group.cmds.is_empty() {
            self.command_groups
                .push(std::mem::take(current_command_group));
        } else {
            // TODO: make this cleaner somehow?
            // This also creates code duplication with the AutoCommandBuffer
            let mut frame_g = self.frame.lock();
            let frame = &mut *frame_g;
            while current_command_group.render_pass_index
                >= frame.render.canvas_mode_mut(canvas_index).passes.len()
            {
                frame.render.canvas_mode_mut(canvas_index).passes.push(
                    super::frame::FrameRenderPass::new(&frame.subpasses_pool, Default::default()),
                );
            }
            frame.render.canvas_mode_mut(canvas_index).passes
                [current_command_group.render_pass_index]
                .render_pass_type = current_command_group.render_pass;
        }

        self.order_id_gen += 1;
        current_command_group.render_pass_index = render_pass_index;
        current_command_group.in_order_id = self.order_id_gen;
        current_command_group.render_pass = render_pass_type;
        current_command_group.cur_frame_index = self.render.cur_image_index;
        current_command_group.canvas_index = canvas_index;

        self.start_render_thread(self.last_render_thread_index)?;
        self.last_render_thread_index =
            (self.last_render_thread_index + 1) % self.props.thread_count;

        Ok(())
    }

    fn add_command_group(
        command_groups: &mut Vec<ThreadCommandGroup>,
        command_group: ThreadCommandGroup,
    ) {
        if !command_group.cmds.is_empty() {
            command_groups.push(command_group);
        }
    }

    fn wait_frame(&mut self) -> anyhow::Result<()> {
        let command_buffer = self
            .main_render_command_buffer
            .as_ref()
            .ok_or(anyhow!("main render command buffer was None"))?
            .command_buffer;

        // make sure even the current unhandled commands get handled
        for (_, current_command_group) in self.current_command_groups.drain() {
            Self::add_command_group(&mut self.command_groups, current_command_group);
        }

        self.finish_render_threads()?;
        self.upload_non_flushed_buffers();

        FrameCollector::collect(self)?;

        // add frame resources
        self.frame_resources.insert(
            self.render.cur_image_index,
            self.current_frame_resources
                .take(Some(&self.frame_resources_pool)),
        );

        self.main_render_command_buffer = None;

        let queue_submit_semaphore =
            &self.render.queue_submit_semaphores[self.render.cur_image_index as usize];

        let mut submit_info = vk::SubmitInfo::default();

        let mut command_buffers: [vk::CommandBuffer; 2] = Default::default();
        command_buffers[0] = command_buffer;

        if let Some(memory_command_buffer) = self.props.device.memory_command_buffer.take() {
            let memory_command_buffer = memory_command_buffer.command_buffer;

            command_buffers[0] = memory_command_buffer;
            command_buffers[1] = command_buffer;
            submit_info = submit_info.command_buffers(&command_buffers[..]);
        } else {
            submit_info = submit_info.command_buffers(&command_buffers[..1]);
        }

        let wait_semaphores = [self
            .render
            .acquired_image_semaphore
            .semaphore(&mut self.current_frame_resources)];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let signal_semaphores =
            [queue_submit_semaphore.semaphore(&mut self.current_frame_resources)];
        submit_info = submit_info
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .signal_semaphores(&signal_semaphores);

        let mut timeline_submit_info: vk::TimelineSemaphoreSubmitInfo;
        let wait_counter: [u64; 1];
        let signal_counter: [u64; 1];

        if self.render.acquired_image_semaphore.is_timeline && self.ash_surf.surface.can_render() {
            wait_counter = [unsafe {
                self.props
                    .ash_vk
                    .vk_device
                    .device
                    .get_semaphore_counter_value(
                        self.render
                            .acquired_image_semaphore
                            .semaphore(&mut self.current_frame_resources),
                    )
                    .unwrap()
            }];
            signal_counter = [unsafe {
                self.props
                    .ash_vk
                    .vk_device
                    .device
                    .get_semaphore_counter_value(signal_semaphores[0])
                    .unwrap()
            } + 1];
            timeline_submit_info = vk::TimelineSemaphoreSubmitInfo::default()
                .wait_semaphore_values(&wait_counter)
                .signal_semaphore_values(&signal_counter);
            submit_info = submit_info.push_next(&mut timeline_submit_info);
        } else if !self.ash_surf.surface.can_render() {
            unsafe { self.props.device.ash_vk.device.device.device_wait_idle()? }
        }

        unsafe {
            self.props
                .ash_vk
                .vk_device
                .device
                .reset_fences(&[self.render.queue_submit_fences
                    [self.render.cur_image_index as usize]
                    .fence(&mut self.current_frame_resources)])
                .map_err(|err| anyhow!("could not reset fences {err}"))
        }?;

        unsafe {
            let queue = &self.props.queue.queues.lock();
            self.props.ash_vk.vk_device.device.queue_submit(
                queue.graphics_queue,
                &[submit_info],
                self.render.queue_submit_fences[self.render.cur_image_index as usize]
                    .fence(&mut self.current_frame_resources),
            )
        }
        .map_err(|err| anyhow!("Submitting to graphics queue failed: {err}"))?;

        std::mem::swap(
            &mut self.render.busy_acquire_image_semaphores[self.render.cur_image_index as usize],
            &mut self.render.acquired_image_semaphore,
        );

        let image_indices = [self.render.cur_image_index];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .image_indices(&image_indices);

        self.last_presented_swap_chain_image_index = self.render.cur_image_index;

        if !self.frame_fetchers.is_empty() {
            // TODO: removed cloning
            let keys: Vec<String> = self.frame_fetchers.keys().cloned().collect();
            for i in keys.iter() {
                // get current frame and fill the frame fetcher with it
                let fetch_index = self.frame_fetchers.get(i).unwrap().current_fetch_index();
                // ignore offscreen canvases that requested to skip this frame
                if let FetchCanvasIndex::Offscreen(index) = fetch_index
                    && self
                        .offscreen_canvases_frame_fetching_skips
                        .contains(&index)
                {
                    continue;
                }
                let img_data = self.get_presented_image_data_impl(fetch_index);
                if let Ok(img_data) = img_data {
                    let frame_fetcher = self.frame_fetchers.get(i).unwrap();
                    frame_fetcher.next_frame(img_data);
                }
            }
        }
        self.offscreen_canvases_frame_fetching_skips.clear();

        let queue_present_res = unsafe {
            let queue = &self.props.queue.queues.lock();
            self.ash_surf
                .vk_swap_chain_ash
                .queue_present(queue.present_queue, present_info)
        };

        let needs_recreate = if queue_present_res
            .is_err_and(|err| err == vk::Result::ERROR_OUT_OF_DATE_KHR)
        {
            Some(vk::Result::ERROR_OUT_OF_DATE_KHR)
        } else if queue_present_res.is_err_and(|err| err == vk::Result::ERROR_SURFACE_LOST_KHR) {
            let surface = self.create_fake_surface()?;
            log::warn!("surface lost after presenting queue, creating fake surface.");
            self.reinit_vulkan_swap_chain(|_| &surface)?;
            self.ash_surf.surface.replace(surface);
            self.recreate_swap_chain = false;
            self.prepare_frame()?;
            None
        } else {
            queue_present_res
                .map_err(|err| anyhow!("Presenting graphics queue failed: {err}"))?
                .then_some(vk::Result::SUBOPTIMAL_KHR)
        };

        if let Some(ty) = needs_recreate {
            if ty == vk::Result::ERROR_OUT_OF_DATE_KHR {
                self.ash_surf.vk_swap_chain_ash.out_of_date_ntf();
            }
            self.recreate_swap_chain = ty == vk::Result::ERROR_OUT_OF_DATE_KHR
                || match &self.render.onscreen.inner_type {
                    RenderSetupNativeType::Swapchain(swapchain) => {
                        swapchain.needs_recreate(&self.props.vk_gpu, &self.ash_surf.surface)
                    }
                    RenderSetupNativeType::Offscreen { .. } => true,
                };
            if self.recreate_swap_chain && is_verbose(&self.props.dbg) {
                info!(
                    "queue recreate swapchain because surface was {}.",
                    if ty == vk::Result::ERROR_OUT_OF_DATE_KHR {
                        "out of date"
                    } else {
                        "sub-optimal"
                    }
                );
            }
        }

        Ok(())
    }

    fn prepare_frame(&mut self) -> anyhow::Result<()> {
        if self.recreate_swap_chain {
            self.recreate_swap_chain = false;
            if is_verbose(&self.props.dbg) {
                info!("recreating swap chain requested by user (prepare frame).");
            }
            self.recreate_swap_chain()?;
        }

        let acquire_res = unsafe {
            self.ash_surf.vk_swap_chain_ash.acquire_next_image(
                u64::MAX,
                self.render
                    .acquired_image_semaphore
                    .semaphore(&mut self.current_frame_resources),
                vk::Fence::null(),
            )
        };

        if acquire_res.is_err_and(|err| err == vk::Result::ERROR_OUT_OF_DATE_KHR) {
            self.recreate_swap_chain = false;
            if is_verbose(&self.props.dbg) {
                info!("recreating swap chain requested by acquire next image (prepare frame).");
            }
            self.recreate_swap_chain()?;
            return self.prepare_frame();
        } else if acquire_res.is_err_and(|err| err == vk::Result::ERROR_SURFACE_LOST_KHR) {
            let surface = self.create_fake_surface()?;
            log::warn!("surface lost after acquiring next image, creating fake surface.");
            self.reinit_vulkan_swap_chain(|_| &surface)?;
            self.ash_surf.surface.replace(surface);
            self.recreate_swap_chain = false;
            self.prepare_frame()?;
            return Ok(());
        }

        let (next_image_index, is_suboptimal) =
            acquire_res.map_err(|err| anyhow!("Acquiring next image failed: {err}"))?;
        if is_suboptimal {
            self.recreate_swap_chain = match &self.render.onscreen.inner_type {
                RenderSetupNativeType::Swapchain(swapchain) => {
                    swapchain.needs_recreate(&self.props.vk_gpu, &self.ash_surf.surface)
                }
                RenderSetupNativeType::Offscreen { .. } => true,
            };
            if self.recreate_swap_chain && is_verbose(&self.props.dbg) {
                info!(
                    "recreating swap chain requested by acquire next image - suboptimal (prepare frame)."
                );
            }
        }

        self.render.cur_image_index = next_image_index;
        unsafe {
            self.props.ash_vk.vk_device.device.wait_for_fences(
                &[
                    self.render.queue_submit_fences[self.render.cur_image_index as usize]
                        .fence(&mut self.current_frame_resources),
                ],
                true,
                u64::MAX,
            )
        }?;

        // next frame
        self.cur_frame += 1;
        self.order_id_gen = 0;
        self.image_last_frame_check[self.render.cur_image_index as usize] = self.cur_frame;
        self.current_command_groups.clear();
        self.new_command_group(
            FrameCanvasIndex::Onscreen,
            0,
            if self.render.onscreen.multi_sampling.is_some() {
                RenderPassType::MultiSampling
            } else {
                RenderPassType::default()
            },
        )?;
        self.render.new_frame(&mut self.current_frame_resources)?;

        // check if older frames weren't used in a long time
        for frame_image_index in 0..self.image_last_frame_check.len() {
            let last_frame = self.image_last_frame_check[frame_image_index];
            if self.cur_frame - last_frame > self.render.onscreen.swap_chain_image_count() as u64 {
                unsafe {
                    self.props.ash_vk.vk_device.device.wait_for_fences(
                        &[self.render.queue_submit_fences[frame_image_index]
                            .fence(&mut self.current_frame_resources)],
                        true,
                        u64::MAX,
                    )
                }?;
                self.clear_frame_data(frame_image_index as u32);
                self.image_last_frame_check[frame_image_index] = self.cur_frame;
            }
        }

        // clear frame's memory data
        self.clear_frame_memory_usage();

        for thread in &self.render_threads {
            thread
                .events
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            thread
                .sender
                .send(RenderThreadEvent::ClearFrame(self.render.cur_image_index))?;
        }

        // prepare new frame_collection frame
        self.main_render_command_buffer = Some(self.props.command_pool.get_render_buffer(
            AutoCommandBufferType::Primary,
            &mut self.current_frame_resources.render,
        )?);
        self.frame.lock().new_frame(
            self.main_render_command_buffer
                .as_ref()
                .unwrap()
                .command_buffer,
        );
        Ok(())
    }

    fn pure_memory_frame(&mut self) -> anyhow::Result<()> {
        self.execute_memory_command_buffer();

        // reset streamed data
        self.upload_non_flushed_buffers();

        self.clear_frame_memory_usage();

        Ok(())
    }

    pub fn next_frame(&mut self) -> anyhow::Result<()> {
        if self.ash_surf.surface.can_render() {
            self.wait_frame()?;
            self.prepare_frame()?;
        }
        // else only execute the memory command buffer
        else {
            self.pure_memory_frame()?;
        }

        Ok(())
    }

    /************************
     * TEXTURES
     ************************/
    fn update_texture(
        &mut self,
        texture_slot: u128,
        format: vk::Format,
        data: &[u8],
        x_off: i64,
        y_off: i64,
        width: usize,
        height: usize,
        color_channel_count: usize,
    ) -> anyhow::Result<()> {
        let image_size: usize = width * height * color_channel_count;
        let mut staging_allocation = self
            .props
            .device
            .mem_allocator
            .lock()
            .get_staging_buffer_image(
                &self.props.device.mem,
                &self.props.device.vk_gpu.limits,
                data,
                image_size as u64,
            );
        if let Err(_) = staging_allocation {
            self.skip_frames_until_current_frame_is_used_again()?;
            staging_allocation = self
                .props
                .device
                .mem_allocator
                .lock()
                .get_staging_buffer_image(
                    &self.props.device.mem,
                    &self.props.device.vk_gpu.limits,
                    data,
                    image_size as u64,
                );
        }
        let staging_buffer = staging_allocation?;

        let tex = self
            .props
            .device
            .textures
            .get(&texture_slot)
            .ok_or(anyhow!("texture with that index does not exist"))?;
        match &tex.data {
            TextureData::Tex2D { img, .. } => {
                let img = img.clone();
                let mip_map_count = tex.mip_map_count;
                self.props
                    .device
                    .image_barrier(
                        &mut self.current_frame_resources,
                        &img,
                        0,
                        tex.mip_map_count as usize,
                        0,
                        1,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    )
                    .map_err(|err| {
                        anyhow!("updating texture failed when transitioning to transfer dst: {err}")
                    })?;
                let buffer = staging_buffer
                    .buffer(&mut self.current_frame_resources)
                    .as_ref()
                    .unwrap();
                self.props
                    .device
                    .copy_buffer_to_image(
                        &mut self.current_frame_resources,
                        buffer,
                        staging_buffer.heap_data.offset_to_align as u64,
                        &img,
                        x_off as i32,
                        y_off as i32,
                        width as u32,
                        height as u32,
                        1,
                    )
                    .map_err(|err| {
                        anyhow!("texture updating failed while copying buffer to image: {err}")
                    })?;

                if mip_map_count > 1 {
                    self.props
                        .device
                        .build_mipmaps(
                            &mut self.current_frame_resources,
                            &img,
                            format,
                            width,
                            height,
                            1,
                            mip_map_count as usize,
                        )
                        .map_err(|err| {
                            anyhow!("updating texture failed when building mipmaps: {err}")
                        })?;
                } else {
                    self.props.device.image_barrier(&mut self.current_frame_resources,
                        &img,
                        0,
                        1,
                        0,
                        1,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    ).map_err(|err| anyhow!("updating texture failed when transitioning back from transfer dst: {err}"))?;
                }
            }
            TextureData::Tex3D { .. } => panic!("not implemented for 3d textures"),
        }

        self.props.device.upload_and_free_staging_image_mem_block(
            &mut self.current_frame_resources,
            staging_buffer,
        );

        Ok(())
    }

    fn create_texture_cmd(
        &mut self,
        slot: u128,
        tex_format: vk::Format,
        upload_data: VulkanDeviceInternalMemory,
    ) -> anyhow::Result<()> {
        let image_index = slot;

        let VulkanAllocatorImageCacheEntryData {
            width,
            height,
            depth,
            is_3d_tex,
            mip_map_count,
            ..
        } = self
            .props
            .device
            .mem_allocator
            .lock()
            .mem_image_cache_entry(upload_data.mem.as_mut_ptr());

        let texture_data = if !is_3d_tex {
            match self.props.device.create_texture_image(
                &mut self.current_frame_resources,
                upload_data,
                tex_format,
                width,
                height,
                depth,
                mip_map_count,
            ) {
                Ok((img, img_mem)) => {
                    let img_format = tex_format;
                    let img_view = self.props.device.create_texture_image_view(
                        &mut self.current_frame_resources,
                        &img,
                        img_format,
                        vk::ImageViewType::TYPE_2D,
                        1,
                        mip_map_count,
                    );
                    let img_view = img_view?;

                    let descriptor = Device::create_new_textured_standard_descriptor_sets(
                        &self.props.device.ash_vk.device,
                        &self.props.device.layouts,
                        &self.props.device.standard_texture_descr_pool,
                        &img_view,
                    )?;
                    TextureData::Tex2D {
                        img,
                        _img_mem: img_mem,
                        _img_view: img_view,
                        vk_standard_textured_descr_set: descriptor,
                    }
                }
                Err(err) => {
                    return Err(err.into());
                }
            }
        } else {
            let image_3d_width = width;
            let image_3d_height = height;

            let (img_3d, img_mem_3d) = self.props.device.create_texture_image(
                &mut self.current_frame_resources,
                upload_data,
                tex_format,
                image_3d_width,
                image_3d_height,
                depth,
                mip_map_count,
            )?;
            let img_format = tex_format;
            let img_view = self.props.device.create_texture_image_view(
                &mut self.current_frame_resources,
                &img_3d,
                img_format,
                vk::ImageViewType::TYPE_2D_ARRAY,
                depth,
                mip_map_count,
            );
            let img_3d_view = img_view?;

            let descr = self
                .props
                .device
                .create_new_3d_textured_standard_descriptor_sets(&img_3d_view)?;

            TextureData::Tex3D {
                _img_3d: img_3d,
                _img_3d_mem: img_mem_3d,
                _img_3d_view: img_3d_view,
                vk_standard_3d_textured_descr_set: descr,
            }
        };

        let texture = TextureObject {
            data: texture_data,
            mip_map_count: mip_map_count as u32,
        };

        self.props.device.textures.insert(image_index, texture); // TODO better fix
        Ok(())
    }

    /************************
     * VULKAN SETUP CODE
     ************************/
    fn destroy_command_buffer(&mut self) {
        self.props.device.memory_command_buffer = None;
    }

    /*************
     * SWAP CHAIN
     **************/
    fn cleanup_vulkan<const IS_LAST_CLEANUP: bool>(&mut self) {
        self.image_last_frame_check.clear();

        self.frame_resources.clear();

        if IS_LAST_CLEANUP {
            self.props.device.mem_allocator.lock().destroy_caches();

            self.delete_presented_image_data_image();
        }

        self.destroy_command_buffer();
    }

    fn recreate_swap_chain(&mut self) -> anyhow::Result<()> {
        unsafe {
            let _g = self.props.queue.queues.lock();
            self.props
                .ash_vk
                .vk_device
                .device
                .device_wait_idle()
                .map_err(|err| anyhow!("wait idle wait while recreating swapchain {err}"))?
        };

        if is_verbose(&self.props.dbg) {
            info!("recreating swap chain.");
        }

        let old_swap_chain_image_count = self.render.onscreen.swap_chain_image_count();

        // set new multi sampling if it was requested
        if self.next_multi_sampling_count != u32::MAX {
            self.multi_sampling_count = self.next_multi_sampling_count;
            self.next_multi_sampling_count = u32::MAX;
        }

        if let Err(err) = self.reinit_vulkan_swap_chain(|s| s) {
            log::warn!(
                "error during swap chain recreation, trying to recover by creating a fake surface: {err}"
            );
            self.recreate_with_fake_surface()?;
        }

        if old_swap_chain_image_count != self.render.onscreen.swap_chain_image_count() {
            self.cleanup_vulkan::<false>();
            self.init_vulkan()?;
        }

        Ok(())
    }

    fn reinit_vulkan_swap_chain<'a>(
        &'a mut self,
        surf_func: impl FnOnce(&'a BackendSurface) -> &'a BackendSurface,
    ) -> anyhow::Result<()> {
        let shader_files = self.render.shader_compiler.shader_files.clone();
        let ty = self.render.shader_compiler.ty;
        let cache = self.render.shader_compiler.cache.clone();
        let fs = self.render.shader_compiler.fs.clone();

        // offscreen canvases stay as they are
        // cloning so we don't remove the existing offscreens if the setup fails.
        let offscreen_canvases = self.render.offscreens.clone();

        let surface = surf_func(&self.ash_surf.surface);
        let can_render = surface.can_render();
        let swapchain = Swapchain::new(
            &self.props.vk_gpu,
            surface,
            &mut self.ash_surf.vk_swap_chain_ash,
            &super::swapchain::SwapchainCreateOptions {
                vsync: self.props.gfx_vsync,
            },
            &self.props.dbg,
            (self.window_width, self.window_height),
        )?;
        self.render = RenderSetup::new(
            &self.props.device.ash_vk.device,
            &self.props.device.layouts,
            &self.props.custom_pipes.pipes,
            &self
                .pipeline_cache
                .as_ref()
                .map(|cache| cache.inner.clone()),
            &self.props.device.standard_texture_descr_pool,
            &self.props.device.mem_allocator,
            CompileThreadpoolsRef {
                one_by_one: &self.runtime_threadpool,
                async_full: &self.compile_threadpool,
            },
            swapchain,
            &self.ash_surf.vk_swap_chain_ash,
            ShaderCompiler::new_with_files(ty, cache, fs, shader_files),
            true,
            can_render && self.render_setup_queue_full_pipeline_creation,
            (self.multi_sampling_count > 0).then_some(self.multi_sampling_count),
        )?;

        self.render.offscreens = offscreen_canvases;

        self.last_presented_swap_chain_image_index = u32::MAX;

        for thread in &self.render_threads {
            thread
                .events
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            thread.sender.send(RenderThreadEvent::ClearFrames)?;
        }

        Ok(())
    }

    fn init_vulkan_with_io(&mut self) -> anyhow::Result<()> {
        self.image_last_frame_check
            .resize(self.render.onscreen.swap_chain_image_count(), 0);

        let onscreen = &self.render.onscreen;
        self.props
            .ash_vk
            .vk_device
            .phy_device
            .update_surface_texture_capabilities(onscreen.surf_format.format);

        Ok(())
    }

    fn init_vulkan(&mut self) -> anyhow::Result<()> {
        self.init_vulkan_with_io()
    }

    /************************
     * COMMAND IMPLEMENTATION
     ************************/
    fn cmd_texture_update(&mut self, cmd: &CommandTextureUpdate) -> anyhow::Result<()> {
        let index_tex = cmd.texture_index;

        self.update_texture(
            index_tex,
            vk::Format::R8G8B8A8_UNORM,
            &cmd.data,
            cmd.x as i64,
            cmd.y as i64,
            cmd.width as usize,
            cmd.height as usize,
            4,
        )
    }

    fn cmd_texture_destroy(&mut self, cmd: &CommandTextureDestroy) -> anyhow::Result<()> {
        let image_index = cmd.texture_index;
        self.props
            .device
            .textures
            .remove(&image_index)
            .ok_or(anyhow!("texture not found in vk backend"))?;

        Ok(())
    }

    fn cmd_texture_create(&mut self, cmd: CommandTextureCreate) -> anyhow::Result<()> {
        let texture_index = cmd.texture_index;

        let data_mem = cmd.data;
        let mut data_mem = self
            .props
            .device
            .mem_allocator
            .lock()
            .memory_to_internal_memory(data_mem);
        if let Err((mem, _)) = data_mem {
            self.skip_frames_until_current_frame_is_used_again()?;
            data_mem = self
                .props
                .device
                .mem_allocator
                .lock()
                .memory_to_internal_memory(mem);
        }
        let data_mem = data_mem.map_err(|(_, err)| err)?;

        self.create_texture_cmd(texture_index, vk::Format::R8G8B8A8_UNORM, data_mem)?;

        Ok(())
    }

    fn cmd_clear_fill_execute_buffer(
        render_execute_manager: &mut RenderCommandExecuteManager,
        cmd: &CommandClear,
    ) {
        render_execute_manager.clear_color_in_render_thread(cmd.force_clear, cmd.color);
        render_execute_manager.estimated_render_calls(0);
    }

    fn cmd_render_fill_execute_buffer(
        render_execute_manager: &mut RenderCommandExecuteManager,
        cmd: &CommandRender,
    ) {
        let address_mode_index: usize = get_address_mode_index(&cmd.state);
        match cmd.texture_index {
            StateTexture::Texture(texture_index) => {
                render_execute_manager.set_texture(0, texture_index, address_mode_index as u64);
            }
            StateTexture::ColorAttachmentOfPreviousPass => {
                render_execute_manager
                    .set_color_attachment_as_texture(0, address_mode_index as u64);
            }
            StateTexture::ColorAttachmentOfOffscreen(offscreen_id) => {
                render_execute_manager.set_offscreen_attachment_as_texture(
                    offscreen_id,
                    0,
                    address_mode_index as u64,
                );
            }
            StateTexture::None => {
                // nothing to do
            }
        }

        render_execute_manager.uses_index_buffer();

        render_execute_manager.estimated_render_calls(1);

        render_execute_manager.exec_buffer_fill_dynamic_states(&cmd.state);

        render_execute_manager.uses_stream_vertex_buffer(
            (cmd.vertices_offset * std::mem::size_of::<GlVertex>()) as u64,
        );
    }

    fn cmd_render_blurred_fill_execute_buffer(
        render_execute_manager: &mut RenderCommandExecuteManager,
        cmd: &CommandRender,
    ) {
        Self::cmd_render_fill_execute_buffer(render_execute_manager, cmd);
    }

    fn cmd_update_viewport(&mut self, cmd: &CommandUpdateViewport) -> anyhow::Result<()> {
        let viewport = self.render.get().native.swap_img_and_viewport_extent;
        if cmd.x != 0 || cmd.y != 0 || cmd.width != viewport.width || cmd.height != viewport.height
        {
            self.has_dynamic_viewport = true;
            self.dynamic_viewport_offset = vk::Offset2D { x: cmd.x, y: cmd.y };
            self.dynamic_viewport_size = vk::Extent2D {
                width: cmd.width,
                height: cmd.height,
            };
        } else {
            self.has_dynamic_viewport = false;
        }

        Ok(())
    }

    fn cmd_canvas_resized(&mut self, cmd: &CommandCanvasResized) -> anyhow::Result<()> {
        if is_verbose(&self.props.dbg) {
            info!("got resize event: {}x{}.", cmd.width, cmd.height);
        }

        self.window_width = cmd.width;
        self.window_height = cmd.height;

        Ok(())
    }

    fn cmd_vsync(&mut self, cmd: CommandVsync) -> anyhow::Result<()> {
        if is_verbose(&self.props.dbg) {
            info!("queueing swap chain recreation because vsync was changed");
        }
        self.props.gfx_vsync = cmd.on;
        self.recreate_swap_chain = true;

        Ok(())
    }

    fn cmd_mutli_sampling(&mut self, cmd: CommandMultiSampling) -> anyhow::Result<()> {
        if is_verbose(&self.props.dbg) {
            info!("queueing swap chain recreation because multi sampling was changed");
        }
        self.recreate_swap_chain = true;

        let sample_count = Device::get_sample_count(
            cmd.sample_count,
            &self.props.device.ash_vk.device.phy_device.limits,
        );
        self.next_multi_sampling_count = sample_count.as_raw();

        Ok(())
    }

    fn cmd_swap(&mut self) -> anyhow::Result<()> {
        self.next_frame()
    }

    fn update_buffer_impl(
        &mut self,
        mem: Arc<MemoryBlock>,
        buffer: Arc<Buffer>,
        update_data: Vec<u8>,
        update_regions: Vec<CommandUpdateBufferRegion>,
        access_flags: vk::AccessFlags,
        source_stage_flags: vk::PipelineStageFlags,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            !update_regions.is_empty(),
            anyhow!("copy regions shall not be empty.")
        );
        anyhow::ensure!(
            !update_regions.iter().any(|region| region.size == 0),
            anyhow!("copy regions sizes must be bigger than zero.")
        );

        let mut staging_allocation = self.props.device.mem_allocator.lock().get_staging_buffer(
            update_data.as_ptr() as _,
            update_data.len() as vk::DeviceSize,
        );

        if let Err(_) = staging_allocation {
            self.skip_frames_until_current_frame_is_used_again()?;
            staging_allocation = self.props.device.mem_allocator.lock().get_staging_buffer(
                update_data.as_ptr() as _,
                update_data.len() as vk::DeviceSize,
            );
        }
        let staging_buffer = staging_allocation?;

        let dst_buffer_align = mem.heap_data.offset_to_align;
        let src_buffer = staging_buffer
            .buffer(&mut self.current_frame_resources)
            .clone()
            .ok_or(anyhow!("staging mem had no buffer attached to it"))?;

        let min_dst_off = update_regions
            .iter()
            .map(|region| region.dst_offset)
            .min()
            .unwrap();
        let max_dst_off = update_regions
            .iter()
            .map(|region| region.dst_offset + region.size)
            .max()
            .unwrap();
        self.props.device.memory_barrier(
            &mut self.current_frame_resources,
            &buffer,
            min_dst_off as vk::DeviceSize + dst_buffer_align as vk::DeviceSize,
            (max_dst_off - min_dst_off) as vk::DeviceSize,
            access_flags,
            true,
            source_stage_flags,
        )?;
        self.props.device.copy_buffer(
            &mut self.current_frame_resources,
            &src_buffer,
            &buffer,
            &update_regions
                .into_iter()
                .map(|region| vk::BufferCopy {
                    src_offset: staging_buffer.heap_data.offset_to_align as vk::DeviceSize
                        + region.src_offset as vk::DeviceSize,
                    dst_offset: region.dst_offset as vk::DeviceSize
                        + dst_buffer_align as vk::DeviceSize,
                    size: region.size as vk::DeviceSize,
                })
                .collect::<Vec<_>>(),
        )?;
        self.props.device.memory_barrier(
            &mut self.current_frame_resources,
            &buffer,
            min_dst_off as vk::DeviceSize + dst_buffer_align as vk::DeviceSize,
            (max_dst_off - min_dst_off) as vk::DeviceSize,
            access_flags,
            false,
            source_stage_flags,
        )?;
        self.props
            .device
            .upload_and_free_staging_mem_block(&mut self.current_frame_resources, staging_buffer);

        Ok(())
    }

    fn cmd_create_buffer_object(&mut self, cmd: CommandCreateBufferObject) -> anyhow::Result<()> {
        let upload_data_size = cmd.upload_data.len();

        let data_mem = cmd.upload_data;
        let mut data_mem = self
            .props
            .device
            .mem_allocator
            .lock()
            .memory_to_internal_memory(data_mem);
        if let Err((mem, _)) = data_mem {
            data_mem = self
                .props
                .device
                .mem_allocator
                .lock()
                .memory_to_internal_memory(mem);
        }
        let data_mem = data_mem.map_err(|(_, err)| err)?;

        Ok(self.props.device.create_buffer_object(
            &mut self.current_frame_resources,
            cmd.buffer_index,
            data_mem,
            upload_data_size as vk::DeviceSize,
        )?)
    }

    fn cmd_recreate_buffer_object(
        &mut self,
        cmd: CommandRecreateBufferObject,
    ) -> anyhow::Result<()> {
        self.props.device.delete_buffer_object(cmd.buffer_index);

        let upload_data_size = cmd.upload_data.len();

        let data_mem = cmd.upload_data;
        let mut data_mem = self
            .props
            .device
            .mem_allocator
            .lock()
            .memory_to_internal_memory(data_mem);
        if let Err((mem, _)) = data_mem {
            data_mem = self
                .props
                .device
                .mem_allocator
                .lock()
                .memory_to_internal_memory(mem);
        }
        let data_mem = data_mem.map_err(|(_, err)| err)?;

        Ok(self.props.device.create_buffer_object(
            &mut self.current_frame_resources,
            cmd.buffer_index,
            data_mem,
            upload_data_size as vk::DeviceSize,
        )?)
    }

    fn cmd_update_buffer_object(&mut self, cmd: CommandUpdateBufferObject) -> anyhow::Result<()> {
        let buffer = self
            .props
            .device
            .buffer_objects
            .get(&cmd.buffer_index)
            .ok_or(anyhow!("buffer object with that index does not exist"))?;

        self.update_buffer_impl(
            buffer.buffer_object.mem.clone(),
            buffer.cur_buffer.clone(),
            cmd.update_data,
            cmd.update_regions,
            vk::AccessFlags::VERTEX_ATTRIBUTE_READ,
            vk::PipelineStageFlags::VERTEX_INPUT,
        )
    }

    fn cmd_delete_buffer_object(&mut self, cmd: &CommandDeleteBufferObject) -> anyhow::Result<()> {
        let buffer_index = cmd.buffer_index;
        self.props.device.delete_buffer_object(buffer_index);

        Ok(())
    }

    fn cmd_create_shader_storage(&mut self, cmd: CommandCreateShaderStorage) -> anyhow::Result<()> {
        let upload_data_size = cmd.upload_data.len();

        let data_mem = cmd.upload_data;
        let mut data_mem = self
            .props
            .device
            .mem_allocator
            .lock()
            .memory_to_internal_memory(data_mem);
        if let Err((mem, _)) = data_mem {
            data_mem = self
                .props
                .device
                .mem_allocator
                .lock()
                .memory_to_internal_memory(mem);
        }
        let data_mem = data_mem.map_err(|(_, err)| err)?;

        Ok(self.props.device.create_shader_storage_object(
            &mut self.current_frame_resources,
            cmd.shader_storage_index,
            data_mem,
            upload_data_size as vk::DeviceSize,
        )?)
    }

    fn cmd_update_shader_storage(&mut self, cmd: CommandUpdateShaderStorage) -> anyhow::Result<()> {
        let buffer = self
            .props
            .device
            .shader_storages
            .get(&cmd.shader_storage_index)
            .ok_or(anyhow!("shader storage with that index does not exist"))?;

        self.update_buffer_impl(
            buffer.buffer.buffer_object.mem.clone(),
            buffer.buffer.cur_buffer.clone(),
            cmd.update_data,
            cmd.update_regions,
            vk::AccessFlags::SHADER_READ,
            vk::PipelineStageFlags::VERTEX_SHADER,
        )
    }

    fn cmd_delete_shader_storage(
        &mut self,
        cmd: &CommandDeleteShaderStorage,
    ) -> anyhow::Result<()> {
        let index = cmd.shader_storage_index;
        self.props.device.delete_shader_storage(index);

        Ok(())
    }

    fn cmd_create_offscreen_canvas(
        &mut self,
        cmd: &CommandOffscreenCanvasCreate,
    ) -> anyhow::Result<()> {
        let offscreen_index = cmd.offscreen_index;

        self.render.create_offscreen_canvas(
            offscreen_index,
            cmd.width,
            cmd.height,
            cmd.has_multi_sampling,
            OffscreenCanvasCreateProps {
                device: &self.props.device.ash_vk.device,
                layouts: &self.props.device.layouts,
                custom_pipes: &self.props.custom_pipes.pipes,
                pipeline_cache: &self
                    .pipeline_cache
                    .as_ref()
                    .map(|cache| cache.inner.clone()),
                standard_texture_descr_pool: &self.props.device.standard_texture_descr_pool,
                mem_allocator: &self.props.device.mem_allocator,
                runtime_threadpool: CompileThreadpools {
                    one_by_one: self.runtime_threadpool.clone(),
                    async_full: self.compile_threadpool.clone(),
                },
                // For now never queue full compiles for offscreen canvases
                // It's slow and also `try_finish_compile` can currently
                // not really complete while the offscreen canvas is in use
                should_queue_full_compile: false,
            },
        )?;

        Ok(())
    }

    fn cmd_destroy_offscreen_canvas(
        &mut self,
        cmd: &CommandOffscreenCanvasDestroy,
    ) -> anyhow::Result<()> {
        let offscreen_index = cmd.offscreen_index;

        // make sure no commands use the offscreen canvas after this anymore
        if let Some(current_command_group) = self
            .current_command_groups
            .remove(&FrameCanvasIndex::Offscreen(offscreen_index))
        {
            Self::add_command_group(&mut self.command_groups, current_command_group);
        }
        self.handle_all_command_groups()?;
        self.render.destroy_offscreen_canvas(offscreen_index);

        Ok(())
    }

    fn cmd_skip_fetching_offscreen_canvas(
        &mut self,
        cmd: &CommandOffscreenCanvasSkipFetchingOnce,
    ) -> anyhow::Result<()> {
        let offscreen_index = cmd.offscreen_index;

        self.offscreen_canvases_frame_fetching_skips
            .insert(offscreen_index);

        Ok(())
    }

    fn cmd_indices_required_num_notify(
        &mut self,
        cmd: &CommandIndicesForQuadsRequiredNotify,
    ) -> anyhow::Result<()> {
        let quad_count = cmd.quad_count_required;
        if self.cur_render_index_primitive_count < quad_count {
            let mut upload_indices = Vec::<u32>::new();
            upload_indices.resize((quad_count * 6) as usize, Default::default());
            let mut primitive_count: u32 = 0;
            for i in (0..(quad_count as usize * 6)).step_by(6) {
                upload_indices[i] = primitive_count;
                upload_indices[i + 1] = primitive_count + 1;
                upload_indices[i + 2] = primitive_count + 2;
                upload_indices[i + 3] = primitive_count;
                upload_indices[i + 4] = primitive_count + 2;
                upload_indices[i + 5] = primitive_count + 3;
                primitive_count += 4;
            }
            (self.render_index_buffer, self.render_index_buffer_memory) =
                self.props.device.create_index_buffer(
                    &mut self.current_frame_resources,
                    upload_indices.as_ptr() as *const c_void,
                    upload_indices.len() * std::mem::size_of::<u32>(),
                )?;
            self.cur_render_index_primitive_count = quad_count;
        }

        Ok(())
    }

    fn buffer_object_fill_execute_buffer(
        render_execute_manager: &mut RenderCommandExecuteManager,
        state: &State,
        texture_index: &StateTexture,
        buffer_object_index: u128,
        draw_calls: usize,
    ) {
        render_execute_manager.set_vertex_buffer(buffer_object_index);

        let address_mode_index: usize = get_address_mode_index(state);
        match texture_index {
            StateTexture::Texture(texture_index) => {
                render_execute_manager.set_texture(0, *texture_index, address_mode_index as u64);
            }
            StateTexture::ColorAttachmentOfPreviousPass => {
                render_execute_manager
                    .set_color_attachment_as_texture(0, address_mode_index as u64);
            }
            StateTexture::ColorAttachmentOfOffscreen(offscreen_id) => {
                render_execute_manager.set_offscreen_attachment_as_texture(
                    *offscreen_id,
                    0,
                    address_mode_index as u64,
                );
            }
            StateTexture::None => {
                // nothing to do
            }
        }

        render_execute_manager.uses_index_buffer();

        render_execute_manager.estimated_render_calls(draw_calls as u64);

        render_execute_manager.exec_buffer_fill_dynamic_states(state);
    }

    fn cmd_render_quad_container_ex_fill_execute_buffer(
        render_execute_manager: &mut RenderCommandExecuteManager,
        cmd: &CommandRenderQuadContainer,
    ) {
        Self::buffer_object_fill_execute_buffer(
            render_execute_manager,
            &cmd.state,
            &cmd.texture_index,
            cmd.buffer_object_index,
            1,
        );
    }

    fn cmd_render_quad_container_as_sprite_multiple_fill_execute_buffer(
        render_execute_manager: &mut RenderCommandExecuteManager,
        cmd: &CommandRenderQuadContainerAsSpriteMultiple,
    ) {
        render_execute_manager.uses_stream_uniform_buffer(
            0,
            cmd.render_info_uniform_instance as u64,
            0,
        );

        Self::buffer_object_fill_execute_buffer(
            render_execute_manager,
            &cmd.state,
            &cmd.texture_index,
            cmd.buffer_object_index,
            ((cmd.instance_count - 1)
                / (GRAPHICS_MAX_UNIFORM_RENDER_COUNT
                    / (std::mem::size_of::<RenderSpriteInfo>() / GRAPHICS_DEFAULT_UNIFORM_SIZE)))
                + 1,
        );
    }

    fn init(&mut self) -> anyhow::Result<()> {
        self.init_vulkan_with_io()?;

        self.prepare_frame()?;

        Ok(())
    }

    fn create_initial_index_buffers(
        device: &mut Device,
        frame_resources: &mut FrameResources,
    ) -> anyhow::Result<InitialIndexBuffer> {
        let mut indices_upload: Vec<u32> =
            Vec::with_capacity(StreamDataMax::MaxVertices as usize / 4 * 6);
        let mut primitive_count: u32 = 0;
        for _ in (0..(StreamDataMax::MaxVertices as usize / 4 * 6)).step_by(6) {
            indices_upload.push(primitive_count);
            indices_upload.push(primitive_count + 1);
            indices_upload.push(primitive_count + 2);
            indices_upload.push(primitive_count);
            indices_upload.push(primitive_count + 2);
            indices_upload.push(primitive_count + 3);
            primitive_count += 4;
        }

        let (render_index_buffer, render_index_buffer_memory) = device
            .create_index_buffer(
                frame_resources,
                indices_upload.as_mut_ptr() as *mut c_void,
                std::mem::size_of::<u32>() * indices_upload.len(),
            )
            .map_err(|err| anyhow!("Failed to create index buffer: {err}"))?;

        let cur_render_index_primitive_count = StreamDataMax::MaxVertices as usize / 4;

        Ok((
            (render_index_buffer, render_index_buffer_memory),
            cur_render_index_primitive_count,
        ))
    }

    fn create_surface(
        entry: &ash::Entry,
        surface: BackendSurfaceAndHandles,
        instance: &ash::Instance,
        phy_gpu: &vk::PhysicalDevice,
        queue_family_index: u32,
        mem_allocator: &Arc<parking_lot::Mutex<VulkanAllocator>>,
    ) -> anyhow::Result<BackendSurface> {
        let surface = unsafe { surface.create_vk_surface(entry, instance, mem_allocator) }?;

        let is_supported =
            unsafe { surface.get_physical_device_surface_support(*phy_gpu, queue_family_index) }?;
        if !is_supported {
            return Err(anyhow!(
                "The device surface does not support presenting the framebuffer to a screen. (maybe the wrong GPU was selected?)"
            ));
        }

        Ok(surface)
    }

    pub fn get_main_thread_data(&self) -> VulkanMainThreadData {
        let phy_gpu = self.props.ash_vk.vk_device.phy_device.clone();
        let instance = phy_gpu.instance.clone();
        VulkanMainThreadData {
            instance,
            mem_allocator: self.props.device.mem_allocator.clone(),
            phy_gpu,
        }
    }

    pub fn main_thread_data(loading: &VulkanBackendLoading) -> VulkanMainThreadData {
        let instance = loading.props.ash_vk.vk_device.phy_device.instance.clone();
        let phy_gpu = loading.props.ash_vk.vk_device.phy_device.clone();
        let mem_allocator = loading.props.device.mem_allocator.clone();
        VulkanMainThreadData {
            instance,
            mem_allocator,
            phy_gpu,
        }
    }

    fn create_fake_surface(&self) -> anyhow::Result<BackendSurface> {
        let phy_gpu = &self.props.ash_vk.vk_device.phy_device;
        let instance = &phy_gpu.instance;
        unsafe {
            BackendWindow::create_fake_headless_surface().create_vk_surface(
                &instance.vk_entry,
                &instance.vk_instance,
                &self.props.device.mem_allocator,
            )
        }
    }

    pub fn try_create_surface_impl(
        instance: &Arc<Instance>,
        phy_gpu: &Arc<PhyDevice>,
        mem_allocator: &Arc<parking_lot::Mutex<VulkanAllocator>>,
        window: &BackendWindow,
        dbg: &ConfigDebug,
    ) -> anyhow::Result<BackendSurface> {
        let benchmark = Benchmark::new(dbg.bench);
        let surface = window.create_surface(&instance.vk_entry, &instance.vk_instance)?;
        let surface = Self::create_surface(
            &instance.vk_entry,
            surface,
            &instance.vk_instance,
            &phy_gpu.cur_device,
            phy_gpu.queue_node_index,
            mem_allocator,
        )?;
        benchmark.bench("creating vk surface");

        Ok(surface)
    }

    pub fn init_from_main_thread(
        data: VulkanMainThreadData,
        window: &BackendWindow,
        dbg: &ConfigDebug,
    ) -> anyhow::Result<VulkanMainThreadInit> {
        Ok(VulkanMainThreadInit {
            surface: Self::try_create_surface_impl(
                &data.instance,
                &data.phy_gpu,
                &data.mem_allocator,
                window,
                dbg,
            )?,
        })
    }

    pub fn set_from_main_thread(&mut self, data: VulkanMainThreadInit) -> anyhow::Result<()> {
        self.wait_frame()?;
        log::info!("new surface from main thred, recreating swapchain.");
        self.reinit_vulkan_swap_chain(|_| &data.surface)?;
        self.ash_surf.surface.replace(data.surface);
        self.recreate_swap_chain = false;
        self.prepare_frame()?;
        Ok(())
    }

    pub fn recreate_with_fake_surface(&mut self) -> anyhow::Result<()> {
        let surface = self.create_fake_surface()?;
        self.reinit_vulkan_swap_chain(|_| &surface)?;
        self.ash_surf.surface.replace(surface);
        Ok(())
    }
    pub fn surface_lost(&mut self) -> anyhow::Result<()> {
        self.wait_frame()?;
        log::warn!("surface lost, creating fake surface.");
        self.recreate_with_fake_surface()?;
        self.recreate_swap_chain = false;
        self.prepare_frame()?;
        Ok(())
    }

    pub fn new(
        mut loading: VulkanBackendLoading,
        loaded_io: VulkanBackendLoadedIo,
        runtime_threadpool: &Arc<rayon::ThreadPool>,

        main_thread_data: VulkanMainThreadInit,
        window_width: u32,
        window_height: u32,
        options: &Options,

        write_files: BackendWriteFiles,
    ) -> anyhow::Result<Box<Self>> {
        let benchmark = Benchmark::new(options.dbg.bench);

        let phy_gpu = &loading.props.ash_vk.vk_device.phy_device;
        let instance = &loading.props.ash_vk.vk_device.phy_device.instance;
        let surface = main_thread_data.surface;

        // thread count
        let thread_count = loading.props.thread_count;

        let shader_compiler = loaded_io.shader_compiler;
        benchmark.bench("getting compiled shaders");

        let pipeline_cache = PipelineCache::new(
            loading.props.device.ash_vk.device.clone(),
            loaded_io.pipeline_cache.as_ref(),
            write_files,
        )
        .ok();
        benchmark.bench("creating the pipeline cache");

        let mut swap_chain = surface.create_swapchain(
            instance,
            &loading.props.ash_vk.vk_device,
            &loading.props.queue,
        )?;
        benchmark.bench("creating vk swap chain");

        // ignore the uneven bit, only even multi sampling works
        let multi_sampling_count = options.gl.msaa_samples & 0xFFFFFFFE;

        let render_setup_queue_full_pipeline_creation = options.gl.full_pipeline_creation;

        let compile_threadpool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .thread_name(|i| format!("vk-compile{i}"))
                .num_threads(
                    // fast pcs get fast performance
                    if matches!(
                        phy_gpu.gpu_list.cur.ty,
                        graphics_types::gpu::GpuType::Discrete
                    ) {
                        std::thread::available_parallelism()
                            .map(|t| (t.get() / 2).max(2))
                            .unwrap_or(2)
                    } else {
                        1
                    },
                )
                .start_handler(|_| {
                    if let Err(err) = thread_priority::set_current_thread_priority(
                        thread_priority::ThreadPriority::Min,
                    ) {
                        log::info!("failed to apply thread priority to rayon builder: {err}");
                    }
                })
                .build()
                .unwrap(),
        );
        //let compile_threadpool = runtime_threadpool.clone();

        let render = RenderSetup::new(
            &loading.props.device.ash_vk.device,
            &loading.props.device.layouts,
            &loading.props.custom_pipes.pipes,
            &pipeline_cache.as_ref().map(|cache| cache.inner.clone()),
            &loading.props.device.standard_texture_descr_pool,
            &loading.props.device.mem_allocator,
            CompileThreadpoolsRef {
                one_by_one: runtime_threadpool,
                async_full: &compile_threadpool,
            },
            Swapchain::new(
                &loading.props.vk_gpu,
                &surface,
                &mut swap_chain,
                &super::swapchain::SwapchainCreateOptions {
                    vsync: loading.props.gfx_vsync,
                },
                &loading.props.dbg,
                (window_width, window_height),
            )?,
            &swap_chain,
            shader_compiler,
            true,
            render_setup_queue_full_pipeline_creation,
            (multi_sampling_count > 0).then_some(multi_sampling_count),
        )?;

        benchmark.bench("creating the vk render setup");

        let frame_resources_pool = FrameResourcesPool::new();
        let mut frame_resouces = FrameResources::new(Some(&frame_resources_pool));

        let ((render_index_buffer, render_index_buffer_memory), index_prim_count) =
            Self::create_initial_index_buffers(&mut loading.props.device, &mut frame_resouces)?;

        benchmark.bench("creating the vk render index buffer");

        let streamed_vertex_buffers_pool = StreamMemoryPool::new(
            loading.props.dbg.clone(),
            instance.clone(),
            loading.props.ash_vk.vk_device.clone(),
            phy_gpu.clone(),
            loading.props.device.mem.texture_memory_usage.clone(),
            loading.props.device.mem.buffer_memory_usage.clone(),
            loading.props.device.mem.stream_memory_usage.clone(),
            loading.props.device.mem.staging_memory_usage.clone(),
            vk::BufferUsageFlags::VERTEX_BUFFER,
            std::mem::size_of::<GlVertexTex3DStream>(),
            StreamDataMax::MaxVertices as usize,
            1,
        );

        let streamed_uniform_buffers_pool = StreamMemoryPool::new(
            loading.props.dbg.clone(),
            instance.clone(),
            loading.props.ash_vk.vk_device.clone(),
            phy_gpu.clone(),
            loading.props.device.mem.texture_memory_usage.clone(),
            loading.props.device.mem.buffer_memory_usage.clone(),
            loading.props.device.mem.stream_memory_usage.clone(),
            loading.props.device.mem.staging_memory_usage.clone(),
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            GRAPHICS_DEFAULT_UNIFORM_SIZE,
            GRAPHICS_MAX_UNIFORM_RENDER_COUNT,
            GRAPHICS_UNIFORM_INSTANCE_COUNT,
        );

        let cur_stream_vertex_buffer = StreamMemoryBlock::new(
            &streamed_vertex_buffers_pool.block_pool,
            streamed_vertex_buffers_pool.vec_pool.new(),
            streamed_vertex_buffers_pool.pool.clone(),
        );
        let cur_stream_uniform_buffers = StreamMemoryBlock::new(
            &streamed_uniform_buffers_pool.block_pool,
            streamed_uniform_buffers_pool.vec_pool.new(),
            streamed_uniform_buffers_pool.pool.clone(),
        );
        benchmark.bench("creating the vk streamed buffers & pools");

        let mut res = Box::new(Self {
            props: loading.props,
            ash_surf: VulkanBackendSurfaceAsh {
                vk_swap_chain_ash: swap_chain,
                surface,
            },
            runtime_threadpool: runtime_threadpool.clone(),
            compile_threadpool,

            streamed_vertex_buffers_pool,
            streamed_uniform_buffers_pool,

            in_use_data: VulkanInUseStreamData {
                cur_stream_vertex_buffer,
                cur_stream_uniform_buffers,
            },

            render_threads: Default::default(),
            render,

            multi_sampling_count,
            next_multi_sampling_count: Default::default(),

            render_setup_queue_full_pipeline_creation,

            render_index_buffer,
            render_index_buffer_memory,
            cur_render_index_primitive_count: index_prim_count as u64,

            cur_render_cmds_count_in_pipe: Default::default(),
            commands_in_pipe: Default::default(),

            last_render_thread_index: Default::default(),

            recreate_swap_chain: Default::default(),
            has_dynamic_viewport: Default::default(),
            dynamic_viewport_offset: Default::default(),
            dynamic_viewport_size: Default::default(),

            main_render_command_buffer: Default::default(),
            cur_frame: Default::default(),
            order_id_gen: Default::default(),
            image_last_frame_check: Default::default(),

            fetch_frame_buffer: Default::default(),
            last_presented_swap_chain_image_index: u32::MAX,
            frame_fetchers: Default::default(),
            frame_data_pool: MtPool::with_capacity(0),
            offscreen_canvases_frame_fetching_skips: Default::default(),

            frame: Frame::new(),

            window_width,
            window_height,
            clear_color: [
                options.gl.clear_color.r as f32 / 255.0,
                options.gl.clear_color.g as f32 / 255.0,
                options.gl.clear_color.b as f32 / 255.0,
                1.0,
            ],

            command_groups: Default::default(),
            current_command_groups: Default::default(),
            current_frame_resources: frame_resouces,
            frame_resources: Default::default(),

            frame_resources_pool,

            pipeline_cache,
        });
        benchmark.bench("creating vk backend instance");

        res.streamed_vertex_buffers_pool
            .try_alloc(|_, _, set_count: usize| Ok(vec![(); set_count]), 4 * 2)?;
        res.in_use_data.cur_stream_vertex_buffer =
            res.streamed_vertex_buffers_pool.try_get(1).unwrap();
        benchmark.bench("creating initial stream vertex buffers");
        res.uniform_stream_alloc_func(GRAPHICS_UNIFORM_INSTANCE_COUNT * 4 * 2)?;
        res.in_use_data.cur_stream_uniform_buffers = res
            .streamed_uniform_buffers_pool
            .try_get(GRAPHICS_UNIFORM_INSTANCE_COUNT)
            .unwrap();
        benchmark.bench("creating initial stream uniform buffers");

        // start threads
        assert!(
            thread_count >= 1,
            "At least one rendering thread must exist."
        );

        for i in 0..thread_count {
            let frame = res.frame.clone();
            let device = res.props.ash_vk.vk_device.clone();
            let queue_index = res.props.ash_vk.vk_device.phy_device.queue_node_index;
            let custom_pipes = res.props.custom_pipes.clone();

            let (sender, receiver) = unbounded();

            let events: Arc<AtomicUsize> = Default::default();
            let events_counter = events.clone();

            let thread = std::thread::Builder::new()
                .name(format!("vk-render {i}"))
                .spawn(move || {
                    Self::run_thread(
                        receiver,
                        events_counter,
                        frame,
                        device,
                        queue_index,
                        custom_pipes,
                    )
                })?;

            res.render_threads.push(Arc::new(RenderThread {
                sender,
                events,
                _thread: JoinThread::new(thread),
            }));
        }

        benchmark.bench("creating vk render threads");

        res.init()?;

        benchmark.bench("init vk backend instance");

        Ok(res)
    }

    /****************
     * RENDER THREADS
     *****************/

    fn run_thread(
        receiver: Receiver<RenderThreadEvent>,
        events_count: Arc<AtomicUsize>,
        frame: Arc<parking_lot::Mutex<Frame>>,
        device: Arc<LogicalDevice>,
        queue_family_index: u32,
        custom_pipes: Arc<VulkanCustomPipes>,
    ) {
        let command_pool = create_command_pools(device.clone(), queue_family_index, 1, 0, 5)
            .unwrap()
            .remove(0);

        let frame_resources_pool: Pool<Vec<RenderThreadFrameResources>> = Pool::with_capacity(16);
        let mut frame_resources: HashMap<u32, PoolVec<RenderThreadFrameResources>> =
            Default::default();

        let frame_resource_pool = RenderThreadFrameResourcesPool::new();

        while let Ok(event) = receiver.recv() {
            // set this to true, if you want to benchmark the render thread times
            let benchmark = Benchmark::new(false);

            let mut has_error_from_cmd = None;
            match event {
                RenderThreadEvent::ClearFrame(frame_index) => {
                    frame_resources.remove(&frame_index);
                }
                RenderThreadEvent::ClearFrames => {
                    frame_resources.clear();
                }
                RenderThreadEvent::Render((mut cmd_group, render)) => {
                    let mut frame_resource =
                        RenderThreadFrameResources::new(Some(&frame_resource_pool));
                    let command_buffer = command_pool
                        .get_render_buffer(
                            AutoCommandBufferType::Secondary {
                                render: &render,
                                cur_image_index: cmd_group.cur_frame_index,
                                render_pass_type: cmd_group.render_pass,
                                render_pass_frame_index: cmd_group.render_pass_index,
                                buffer_in_order_id: cmd_group.in_order_id,
                                canvas_index: cmd_group.canvas_index,
                                frame: &frame,
                            },
                            &mut frame_resource,
                        )
                        .unwrap();
                    for mut next_cmd in cmd_group.cmds.drain(..) {
                        let cmd = next_cmd.raw_render_command.take().unwrap();
                        if let Err(err) = command_cb_render(
                            &custom_pipes,
                            &device,
                            &render,
                            cmd_group.render_pass,
                            &cmd,
                            next_cmd,
                            &command_buffer,
                        ) {
                            // an error occured, the thread will not continue execution
                            has_error_from_cmd = Some(err);
                            break;
                        }
                    }

                    let resources = frame_resources
                        .entry(cmd_group.cur_frame_index)
                        .or_insert_with(|| frame_resources_pool.new());
                    resources.push(frame_resource);
                }
                RenderThreadEvent::Sync(sender) => sender.send(()).unwrap(),
            }
            if let Some(err) = has_error_from_cmd {
                log::error!("FATAL ERROR: {err}");
                panic!("TODO:")
            }

            benchmark.bench("vulkan render thread");

            events_count.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    pub fn get_stream_data(&mut self) -> anyhow::Result<VulkanInUseStreamData> {
        let cur_stream_vertex_buffer = self
            .streamed_vertex_buffers_pool
            .get(|_, _, set_count: usize| Ok(vec![(); set_count]), 1)?;

        self.uniform_stream_alloc_func(GRAPHICS_UNIFORM_INSTANCE_COUNT)?;
        let cur_stream_uniform_buffers = self
            .streamed_uniform_buffers_pool
            .try_get(GRAPHICS_UNIFORM_INSTANCE_COUNT)
            .ok_or_else(|| anyhow!("stream uniform buffer pool returned None"))?;

        Ok(VulkanInUseStreamData {
            cur_stream_vertex_buffer,
            cur_stream_uniform_buffers,
        })
    }
    pub fn set_stream_data_in_use(
        &mut self,
        stream_data: &GraphicsStreamedData,
        data: &VulkanInUseStreamData,
    ) -> anyhow::Result<()> {
        self.current_frame_resources
            .stream_vertex_buffers
            .push(data.cur_stream_vertex_buffer.clone());

        self.current_frame_resources
            .stream_uniform_buffers
            .push(data.cur_stream_uniform_buffers.clone());

        data.cur_stream_vertex_buffer.memories[0].flush(
            &mut self.current_frame_resources,
            self.props.vk_gpu.limits.non_coherent_mem_alignment,
            stream_data.vertices_count() * std::mem::size_of::<GlVertex>(),
            &mut self.props.device.non_flushed_memory_ranges,
        );
        let uniform_instance_count = stream_data.uniform_instance_count();
        for i in 0..uniform_instance_count {
            let usage_count = stream_data.uniform_used_count_of_instance(i);
            data.cur_stream_uniform_buffers.memories[i].flush(
                &mut self.current_frame_resources,
                self.props.vk_gpu.limits.non_coherent_mem_alignment,
                match usage_count {
                    GraphicsStreamedUniformDataType::Arbitrary {
                        element_size,
                        element_count,
                    } => element_count * element_size,
                    GraphicsStreamedUniformDataType::None => 0,
                },
                &mut self.props.device.non_flushed_memory_ranges,
            );
        }

        self.in_use_data = VulkanInUseStreamData {
            cur_stream_vertex_buffer: data.cur_stream_vertex_buffer.clone(),
            cur_stream_uniform_buffers: data.cur_stream_uniform_buffers.clone(),
        };

        Ok(())
    }

    pub fn create_mt_backend(data: &VulkanMainThreadData) -> VulkanBackendMt {
        VulkanBackendMt {
            mem_allocator: data.mem_allocator.clone(),
            flush_lock: Default::default(),
            gpus: data.phy_gpu.gpu_list.clone(),
        }
    }
}

impl DriverBackendInterface for VulkanBackend {
    fn attach_frame_fetcher(&mut self, name: String, fetcher: Arc<dyn BackendFrameFetcher>) {
        self.frame_fetchers.insert(name, fetcher);
    }

    fn detach_frame_fetcher(&mut self, name: String) {
        self.frame_fetchers.remove(&name);
    }

    fn run_command(&mut self, cmd: AllCommands) -> anyhow::Result<()> {
        let mut buffer = RenderCommandExecuteBuffer::default();
        buffer.viewport_size = self.render.get().native.swap_img_and_viewport_extent;

        let mut can_start_thread: bool = false;
        if let AllCommands::Render(render_cmd) = &cmd {
            let thread_index = ((self.cur_render_cmds_count_in_pipe * self.props.thread_count)
                / self.commands_in_pipe.max(1))
                % self.props.thread_count;

            if thread_index > self.last_render_thread_index {
                can_start_thread = true;
            }
            self.fill_execute_buffer(render_cmd, &mut buffer);
            self.cur_render_cmds_count_in_pipe += 1;
        }
        let mut is_misc_cmd = false;
        if let AllCommands::Misc(_) = cmd {
            is_misc_cmd = true;
        }
        if is_misc_cmd {
            if let AllCommands::Misc(cmd) = cmd {
                self.command_cb_misc(cmd)?;
            }
        } else if self.ash_surf.surface.can_render() {
            if let AllCommands::Render(render_cmd) = cmd {
                buffer.raw_render_command = Some(render_cmd)
            }
            if let Some(current_command_group) = self
                .current_command_groups
                .get_mut(&self.render.cur_canvas())
            {
                current_command_group.cmds.push(buffer);

                if can_start_thread {
                    let canvas_index = current_command_group.canvas_index;
                    let render_pass_index = current_command_group.render_pass_index;
                    let render_pass = current_command_group.render_pass;
                    self.new_command_group(canvas_index, render_pass_index, render_pass)?;
                }
            }
        }

        Ok(())
    }

    fn start_commands(&mut self, command_count: usize) {
        self.commands_in_pipe = command_count;
        self.cur_render_cmds_count_in_pipe = 0;
    }

    fn end_commands(&mut self) -> anyhow::Result<()> {
        self.commands_in_pipe = 0;
        self.last_render_thread_index = 0;

        Ok(())
    }
}

impl Drop for VulkanBackend {
    fn drop(&mut self) {
        unsafe {
            let _g = self.props.queue.queues.lock();
            self.props
                .ash_vk
                .vk_device
                .device
                .device_wait_idle()
                .unwrap()
        };

        self.cleanup_vulkan::<true>();

        // clean all images, buffers, buffer containers
        self.props.device.textures.clear();
        self.props.device.buffer_objects.clear();
    }
}

#[derive(Debug, Hiarc)]
pub struct VulkanBackendMt {
    pub mem_allocator: Arc<parking_lot::Mutex<VulkanAllocator>>,
    pub flush_lock: parking_lot::Mutex<()>,
    pub gpus: Arc<Gpus>,
}

#[derive(Debug)]
pub struct VulkanBackendDellocator {
    pub mem_allocator: Arc<parking_lot::Mutex<VulkanAllocator>>,
}

impl GraphicsBackendMemoryStaticCleaner for VulkanBackendDellocator {
    fn destroy(&self, mem: &'static mut [u8]) {
        self.mem_allocator.lock().free_mem_raw(mem.as_mut_ptr());
    }
}

impl GraphicsBackendMtInterface for VulkanBackendMt {
    fn mem_alloc(
        &self,
        alloc_type: GraphicsMemoryAllocationType,
        mode: GraphicsMemoryAllocationMode,
    ) -> GraphicsBackendMemory {
        if matches!(mode, GraphicsMemoryAllocationMode::Lazy) {
            return mem_alloc_lazy(alloc_type);
        }

        let buffer_data: *const c_void = std::ptr::null();
        let allocator_clone = self.mem_allocator.clone();
        let mut allocator = self.mem_allocator.lock();
        GraphicsBackendMemory::new(
            match alloc_type {
                GraphicsMemoryAllocationType::VertexBuffer { required_size } => {
                    let res = allocator.get_staging_buffer_for_mem_alloc(
                        buffer_data,
                        required_size.get() as vk::DeviceSize,
                    );
                    match res {
                        Ok(res) => {
                            GraphicsBackendMemoryAllocation::Static(GraphicsBackendMemoryStatic {
                                mem: Some(res),
                                deallocator: Some(Box::new(VulkanBackendDellocator {
                                    mem_allocator: allocator_clone,
                                })),
                            })
                        }
                        Err(_) => {
                            // go to slow memory as backup
                            let mut res = Vec::new();
                            res.resize(required_size.get(), Default::default());
                            GraphicsBackendMemoryAllocation::Vector(res)
                        }
                    }
                }
                GraphicsMemoryAllocationType::ShaderStorage { required_size } => {
                    let res = allocator.get_staging_buffer_for_shader_storage_mem_alloc(
                        buffer_data,
                        required_size.get() as vk::DeviceSize,
                    );
                    match res {
                        Ok(res) => {
                            GraphicsBackendMemoryAllocation::Static(GraphicsBackendMemoryStatic {
                                mem: Some(res),
                                deallocator: Some(Box::new(VulkanBackendDellocator {
                                    mem_allocator: allocator_clone,
                                })),
                            })
                        }
                        Err(_) => {
                            // go to slow memory as backup
                            let mut res = Vec::new();
                            res.resize(required_size.get(), Default::default());
                            GraphicsBackendMemoryAllocation::Vector(res)
                        }
                    }
                }
                GraphicsMemoryAllocationType::TextureRgbaU8 {
                    width,
                    height,
                    flags,
                } => {
                    let res = allocator.get_staging_buffer_image_for_mem_alloc(
                        buffer_data,
                        width.get(),
                        height.get(),
                        1,
                        false,
                        flags,
                    );
                    match res {
                        Ok(res) => {
                            GraphicsBackendMemoryAllocation::Static(GraphicsBackendMemoryStatic {
                                mem: Some(res),
                                deallocator: Some(Box::new(VulkanBackendDellocator {
                                    mem_allocator: allocator_clone,
                                })),
                            })
                        }
                        Err(_) => {
                            // go to slow memory as backup
                            let mut res = Vec::new();
                            res.resize(width.get() * height.get() * 4, Default::default());
                            GraphicsBackendMemoryAllocation::Vector(res)
                        }
                    }
                }
                GraphicsMemoryAllocationType::TextureRgbaU82dArray {
                    width,
                    height,
                    depth,
                    flags,
                } => {
                    let res = allocator.get_staging_buffer_image_for_mem_alloc(
                        buffer_data,
                        width.get(),
                        height.get(),
                        depth.get(),
                        true,
                        flags,
                    );
                    match res {
                        Ok(res) => {
                            GraphicsBackendMemoryAllocation::Static(GraphicsBackendMemoryStatic {
                                mem: Some(res),
                                deallocator: Some(Box::new(VulkanBackendDellocator {
                                    mem_allocator: allocator_clone,
                                })),
                            })
                        }
                        Err(_) => {
                            // go to slow memory as backup
                            let mut res = Vec::new();
                            res.resize(
                                width.get() * height.get() * depth.get() * 4,
                                Default::default(),
                            );
                            GraphicsBackendMemoryAllocation::Vector(res)
                        }
                    }
                }
            },
            alloc_type,
        )
    }

    fn try_flush_mem(
        &self,
        mem: &mut GraphicsBackendMemory,
        do_expensive_flushing: bool,
    ) -> anyhow::Result<()> {
        // make sure only one flush at a time happens
        let _lock = self.flush_lock.lock();
        let res = self
            .mem_allocator
            .lock()
            .try_flush_mem(mem, do_expensive_flushing)?;
        if let Some((fence, command_buffer, device)) = res {
            unsafe {
                device.wait_for_fences(&[fence], true, u64::MAX)?;
                device.reset_command_buffer(
                    command_buffer,
                    vk::CommandBufferResetFlags::RELEASE_RESOURCES,
                )
            }?;
        }
        Ok(())
    }
}
