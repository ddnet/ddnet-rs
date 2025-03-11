use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::{atomic::AtomicU64, Arc},
};

use base_io::io::IoFileSys;
use config::config::{ConfigBackend, ConfigDebug, ConfigGfx};
use graphics_backend_traits::{
    frame_fetcher_plugin::BackendFrameFetcher,
    plugin::{BackendCustomPipeline, GraphicsObjectRewriteFunc},
    traits::{GraphicsBackendInterface, GraphicsBackendMtInterface},
    types::BackendCommands,
};
use graphics_base_traits::traits::{GraphicsStreamVertices, GraphicsStreamedData};
use hiarc::Hiarc;
use native::native::NativeImpl;
use pool::{mixed_pool::PoolSyncPoint, mt_datatypes::PoolVec};

use crate::{
    backend_thread::{BackendThread, BackendThreadInitData},
    backends::vulkan::vulkan::{VulkanBackendLoadedIo, VulkanBackendLoadingIo},
    window::{BackendDisplayRequirements, BackendRawDisplayHandle, BackendWindow},
};

use base::benchmark::Benchmark;

use super::backend_mt::GraphicsBackendMultiThreaded;

use graphics_types::{
    commands::{AllCommands, CommandUpdateViewport, CommandsMisc},
    gpu::Gpus,
    types::{
        GraphicsBackendMemory, GraphicsMemoryAllocationMode, GraphicsMemoryAllocationType,
        WindowProps,
    },
};

pub use parking_lot::RwLock;

pub type CustomPipelines = Arc<RwLock<Vec<Box<dyn BackendCustomPipeline>>>>;

#[derive(Debug)]
enum GraphicsBackendLoadingIoType {
    Vulkan(VulkanBackendLoadingIo),
    Null,
}

#[derive(Debug)]
pub struct GraphicsBackendIoLoading {
    backend_io: GraphicsBackendLoadingIoType,
}

impl GraphicsBackendIoLoading {
    pub fn new(config_gfx: &ConfigGfx, io: &IoFileSys) -> Self {
        Self {
            backend_io: match config_gfx.backend.to_ascii_lowercase().as_str() {
                "null" => GraphicsBackendLoadingIoType::Null,
                // "vulkan"
                _ => GraphicsBackendLoadingIoType::Vulkan(VulkanBackendLoadingIo::new(io)),
            },
        }
    }
}

#[derive(Debug, Hiarc, Clone)]
pub struct GraphicsBackendMemoryUsage {
    pub texture_memory_usage: Arc<AtomicU64>,
    pub buffer_memory_usage: Arc<AtomicU64>,
    pub stream_memory_usage: Arc<AtomicU64>,
    pub staging_memory_usage: Arc<AtomicU64>,
}

#[derive(Debug)]
pub struct GraphicsBackendLoading {
    memory_usage: GraphicsBackendMemoryUsage,

    backend: BackendThread,

    custom_pipes: Option<CustomPipelines>,

    config_dbg: ConfigDebug,
    config_gl: ConfigBackend,
}

impl GraphicsBackendLoading {
    pub fn new(
        config_gfx: &ConfigGfx,
        config_dbg: &ConfigDebug,
        config_gl: &ConfigBackend,
        raw_display_handle: BackendRawDisplayHandle,
        custom_pipes: Option<CustomPipelines>,
        io: IoFileSys,
    ) -> anyhow::Result<Self> {
        let backend = &config_gfx.backend;

        let benchmark = Benchmark::new(config_dbg.bench);

        let texture_memory_usage: Arc<AtomicU64> = Default::default();
        let buffer_memory_usage: Arc<AtomicU64> = Default::default();
        let stream_memory_usage: Arc<AtomicU64> = Default::default();
        let staging_memory_usage: Arc<AtomicU64> = Default::default();

        let backend = BackendThread::new(
            backend.clone(),
            BackendDisplayRequirements {
                extensions: raw_display_handle.enumerate_required_vk_extensions()?,
                is_headless: raw_display_handle.is_headless(),
            },
            *config_dbg,
            config_gl.clone(),
            custom_pipes.clone(),
            texture_memory_usage.clone(),
            buffer_memory_usage.clone(),
            stream_memory_usage.clone(),
            staging_memory_usage.clone(),
            io,
        )?;
        benchmark.bench("initializing the backend instance");

        Ok(Self {
            memory_usage: GraphicsBackendMemoryUsage {
                texture_memory_usage,
                buffer_memory_usage,
                stream_memory_usage,
                staging_memory_usage,
            },

            backend,
            custom_pipes,

            config_dbg: *config_dbg,
            config_gl: config_gl.clone(),
        })
    }
}

#[derive(Debug, Hiarc)]
pub struct GraphicsBackendBase {
    backend: BackendThread,
    backend_mt: Arc<GraphicsBackendMultiThreaded>,

    backend_cmds_in_use: Vec<AllCommands>,

    window_props: WindowProps,
    memory_usage: GraphicsBackendMemoryUsage,

    #[hiarc_skip_unsafe]
    custom_pipes: Option<CustomPipelines>,
    pipeline_names: HashMap<String, usize>,
}

impl GraphicsBackendBase {
    /// returns the base and the stream_data
    pub fn new(
        io_loading: GraphicsBackendIoLoading,
        backend_loading: GraphicsBackendLoading,
        runtime_threadpool: &Arc<rayon::ThreadPool>,
        window: BackendWindow,
    ) -> anyhow::Result<(Self, GraphicsStreamedData)> {
        let benchmark = Benchmark::new(backend_loading.config_dbg.bench);

        let window = &window;
        let size = window.inner_size();
        let scale_factor = window.scale_factor();

        // get window & canvas properties
        let (canvas_width, canvas_height) = (size.width, size.height);

        let (window_width, window_height) = (
            canvas_width as f64 / scale_factor,
            canvas_height as f64 / scale_factor,
        );

        let backend_mt = backend_loading.backend.init(
            match io_loading.backend_io {
                GraphicsBackendLoadingIoType::Vulkan(data) => BackendThreadInitData::Vulkan {
                    data: VulkanBackendLoadedIo {
                        shader_compiler: data.shader_compiler.get_storage()?,
                        pipeline_cache: data.pipeline_cache.get_storage()?,
                    },
                    runtime_threadpool: runtime_threadpool.clone(),
                    window_width: size.width,
                    window_height: size.height,
                    dbg: backend_loading.config_dbg,
                    gl: backend_loading.config_gl.clone(),
                },
                GraphicsBackendLoadingIoType::Null => BackendThreadInitData::Null,
            },
            &backend_loading.config_dbg,
            window,
        )?;
        benchmark.bench("gl backend loading");

        let backend_mt = Arc::new(GraphicsBackendMultiThreaded { backend_mt });

        // clear first frame
        let cmd_swap = CommandsMisc::Swap;

        let buffer = BackendCommands::default();
        buffer.add_cmd(AllCommands::Misc(cmd_swap));
        let stream_data: GraphicsStreamedData = GraphicsStreamedData::new(
            GraphicsStreamVertices::Vec(Default::default()),
            PoolVec::new_without_pool(),
        );

        let mut pipeline_names: HashMap<String, usize> = Default::default();
        if let Some(custom_pipes) = &backend_loading.custom_pipes {
            let pipes = custom_pipes.read();
            for (index, pipe) in pipes.iter().enumerate() {
                pipeline_names.insert(pipe.pipe_name(), index);
            }
        }

        let mut res = GraphicsBackendBase {
            backend: backend_loading.backend,
            backend_mt,

            backend_cmds_in_use: Default::default(),

            window_props: WindowProps {
                window_width,
                window_height,
                canvas_width,
                canvas_height,
            },
            memory_usage: backend_loading.memory_usage,

            custom_pipes: backend_loading.custom_pipes,
            pipeline_names,
        };
        res.run_cmds(&buffer, &stream_data)?;
        benchmark.bench("gl first swap");

        Ok((res, stream_data))
    }

    fn run_cmds_impl(
        &mut self,
        buffer: &BackendCommands,
        stream_data: &GraphicsStreamedData,
    ) -> anyhow::Result<()> {
        self.backend_cmds_in_use.clear();
        buffer.replace(&mut self.backend_cmds_in_use);

        self.backend
            .run_cmds(stream_data, &mut self.backend_cmds_in_use)?;

        Ok(())
    }

    pub fn get_window_props(&self) -> WindowProps {
        self.window_props
    }

    #[must_use]
    fn resized(
        &mut self,
        buffer: &BackendCommands,
        stream_data: &GraphicsStreamedData,
        window_handling: &dyn NativeImpl,
        new_width: u32,
        new_height: u32,
    ) -> WindowProps {
        self.wait_idle().unwrap();
        let cmd_viewport = CommandsMisc::UpdateViewport(CommandUpdateViewport {
            x: 0,
            y: 0,
            width: new_width,
            height: new_height,
            by_resize: true,
        });

        buffer.add_cmd(AllCommands::Misc(cmd_viewport));
        self.run_cmds_impl(buffer, stream_data).unwrap(); // TODO: unwrap here?

        let scale_factor = window_handling
            .borrow_window()
            .scale_factor()
            .clamp(0.0001, f64::MAX);

        self.window_props.window_width = new_width as f64 / scale_factor;
        self.window_props.window_height = new_height as f64 / scale_factor;
        self.window_props.canvas_width = new_width;
        self.window_props.canvas_height = new_height;

        self.window_props
    }

    fn window_created_ntfy(&self, window: BackendWindow, dbg: &ConfigDebug) -> anyhow::Result<()> {
        self.backend.window_created_ntfy(window, dbg)
    }

    fn window_destroyed_ntfy(&self) -> anyhow::Result<()> {
        self.backend.window_destroyed_ntfy()
    }

    fn run_cmds(
        &mut self,
        buffer: &BackendCommands,
        stream_data: &GraphicsStreamedData,
    ) -> anyhow::Result<()> {
        self.run_cmds_impl(buffer, stream_data)
    }

    fn mem_alloc(&self, alloc_type: GraphicsMemoryAllocationType) -> GraphicsBackendMemory {
        self.backend_mt
            .mem_alloc(alloc_type, GraphicsMemoryAllocationMode::Lazy)
    }

    fn attach_frame_fetcher(
        &mut self,
        name: String,
        fetcher: Arc<dyn BackendFrameFetcher>,
    ) -> anyhow::Result<()> {
        self.backend.attach_frame_fetcher(name, fetcher)
    }

    fn detach_frame_fetcher(&mut self, name: String) -> anyhow::Result<()> {
        self.backend.detach_frame_fetcher(name)
    }

    fn wait_idle(&mut self) -> anyhow::Result<()> {
        self.backend.wait_idle()
    }

    fn get_backend_mt(&self) -> Arc<dyn GraphicsBackendMtInterface + Sync + Send + 'static> {
        self.backend_mt.clone()
    }

    fn add_sync_point(&mut self, sync_point: Box<dyn PoolSyncPoint>) {
        self.backend.add_sync_point(sync_point);
    }

    fn check_mod_cmd(
        &self,
        mod_name: &str,
        cmd: &mut PoolVec<u8>,
        f: &dyn Fn(GraphicsObjectRewriteFunc),
    ) {
        let backend_plugins = self
            .custom_pipes
            .as_ref()
            .expect("no backend custom pipeline plugins registered.");
        let pipe_index = self
            .pipeline_names
            .get(mod_name)
            .expect("pipeline with that name not found");
        backend_plugins.read()[*pipe_index].rewrite_texture_and_buffer_object_indices(cmd, f)
    }
}

#[derive(Debug, Hiarc)]
pub struct GraphicsBackend(RefCell<GraphicsBackendBase>);

impl GraphicsBackend {
    pub fn new(backend_base: GraphicsBackendBase) -> Rc<Self> {
        Rc::new(Self(RefCell::new(backend_base)))
    }

    #[must_use]
    pub fn resized(
        &self,
        buffer: &BackendCommands,
        stream_data: &GraphicsStreamedData,
        window_handling: &dyn NativeImpl,
        new_width: u32,
        new_height: u32,
    ) -> WindowProps {
        self.0
            .borrow_mut()
            .resized(buffer, stream_data, window_handling, new_width, new_height)
    }

    #[must_use]
    pub fn memory_usage(&self) -> GraphicsBackendMemoryUsage {
        self.0.borrow().memory_usage.clone()
    }

    pub fn window_created_ntfy(
        &self,
        window: BackendWindow,
        dbg: &ConfigDebug,
    ) -> anyhow::Result<()> {
        self.0.borrow().window_created_ntfy(window, dbg)
    }

    pub fn window_destroyed_ntfy(&self) -> anyhow::Result<()> {
        self.0.borrow().window_destroyed_ntfy()
    }
}

impl GraphicsBackendInterface for GraphicsBackend {
    fn run_cmds(&self, buffer: &BackendCommands, stream_data: &GraphicsStreamedData) {
        self.0.borrow_mut().run_cmds(buffer, stream_data).unwrap(); // TODO: unwrap?
    }

    fn mem_alloc(&self, alloc_type: GraphicsMemoryAllocationType) -> GraphicsBackendMemory {
        self.0.borrow().mem_alloc(alloc_type)
    }

    fn attach_frame_fetcher(
        &self,
        name: String,
        fetcher: Arc<dyn graphics_backend_traits::frame_fetcher_plugin::BackendFrameFetcher>,
    ) -> anyhow::Result<()> {
        self.0.borrow_mut().attach_frame_fetcher(name, fetcher)
    }

    fn detach_frame_fetcher(&self, name: String) -> anyhow::Result<()> {
        self.0.borrow_mut().detach_frame_fetcher(name)
    }

    fn wait_idle(&self) -> anyhow::Result<()> {
        self.0.borrow_mut().wait_idle()
    }

    fn get_backend_mt(&self) -> Arc<dyn GraphicsBackendMtInterface + Sync + Send + 'static> {
        self.0.borrow().get_backend_mt()
    }

    fn add_sync_point(&self, sync_point: Box<dyn PoolSyncPoint>) {
        self.0.borrow_mut().add_sync_point(sync_point)
    }

    fn check_mod_cmd(
        &self,
        mod_name: &str,
        cmd: &mut PoolVec<u8>,
        f: &dyn Fn(GraphicsObjectRewriteFunc),
    ) {
        self.0.borrow().check_mod_cmd(mod_name, cmd, f)
    }

    fn gpus(&self) -> Arc<Gpus> {
        self.0.borrow().backend_mt.backend_mt.gpus()
    }
}
