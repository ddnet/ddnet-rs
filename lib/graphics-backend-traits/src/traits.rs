use std::{fmt::Debug, sync::Arc};

use graphics_base_traits::traits::GraphicsStreamedData;
use graphics_types::{
    commands::AllCommands,
    gpu::Gpus,
    types::{GraphicsBackendMemory, GraphicsMemoryAllocationMode, GraphicsMemoryAllocationType},
};
use pool::{mixed_pool::PoolSyncPoint, mt_datatypes::PoolVec};

use crate::{
    frame_fetcher_plugin::BackendFrameFetcher, plugin::GraphicsObjectRewriteFunc,
    types::BackendCommands,
};

pub trait DriverBackendInterface: Debug {
    fn attach_frame_fetcher(&mut self, name: String, fetcher: Arc<dyn BackendFrameFetcher>);
    fn detach_frame_fetcher(&mut self, name: String);

    fn run_command(&mut self, cmd: AllCommands) -> anyhow::Result<()>;

    fn start_commands(&mut self, command_count: usize);

    fn end_commands(&mut self) -> anyhow::Result<()>;
}

pub trait GraphicsBackendInterface: Debug + 'static {
    /// Runs a backend buffer and swaps out the buffers the next to use
    fn run_cmds(&self, buffer: &BackendCommands, stream_data: &GraphicsStreamedData);

    fn check_mod_cmd(
        &self,
        mod_name: &str,
        cmd: &mut PoolVec<u8>,
        f: &dyn Fn(GraphicsObjectRewriteFunc),
    );

    /// The alloc_type is useful to reduce memory footprint by
    /// putting similar memory types into the same heap
    fn mem_alloc(&self, alloc_type: GraphicsMemoryAllocationType) -> GraphicsBackendMemory;

    /// This only throws errors if the driver backend crashed
    fn attach_frame_fetcher(
        &self,
        name: String,
        fetcher: Arc<dyn BackendFrameFetcher>,
    ) -> anyhow::Result<()>;
    /// This only throws errors if the driver backend crashed
    fn detach_frame_fetcher(&self, name: String) -> anyhow::Result<()>;

    /// Wait for the backend to become idle
    fn wait_idle(&self) -> anyhow::Result<()>;

    /// add a pool sync pointer inside the _potential_ mutex inside the [`GraphicsBackendInterface::run_cmds`] call
    /// sync points can not be removed, so call carefully
    fn add_sync_point(&self, sync_point: Box<dyn PoolSyncPoint>);

    fn gpus(&self) -> Arc<Gpus>;

    fn get_backend_mt(&self) -> Arc<dyn GraphicsBackendMtInterface + Sync + Send + 'static>;
}

pub trait GraphicsBackendMtInterface: Debug {
    /// Allocate new graphics backend memory.
    ///
    /// The `alloc_type` defines the type of memory that is about to be allocated.
    /// This is useful to reduce memory footprint by putting similar memory types
    /// into the same heap.
    ///
    /// If the `mode` is [`GraphicsMemoryAllocationMode::Immediate`], then
    /// the backend tries to allocate driver memory + VRAM. But the backend is allowed to
    /// always fallback to [`GraphicsMemoryAllocationMode::Lazy`] mode if former is
    /// not available.
    /// If `mode` is [`GraphicsMemoryAllocationMode::Lazy`] it will usually only consume
    /// system memory (no VRAM).
    ///
    fn mem_alloc(
        &self,
        alloc_type: GraphicsMemoryAllocationType,
        mode: GraphicsMemoryAllocationMode,
    ) -> GraphicsBackendMemory;

    /// Tries to flush backend memory, which allows to skip doing so
    /// on runtime on the main thread. It's important however to understand
    /// that the backend can have various reasons not to flush in this moment.
    /// In any case the memory should still be applied to the graphics implementation
    /// normally (create texture, create buffer).
    /// The rule of thumb is to always try to call this, when you are loading inside
    /// a thread
    /// `do_expensive_flushing` determines if the implementation is allowed to use an expensive flushing mechanism
    /// this is generally recommended since this function should only be called from a function anyway,
    /// but it might cost more performance in some cases which makes it undesirable for situations where
    /// it is better to have fewest possible runtime (e.g. loading a map, which is required to proceed the game)
    /// ### Note: any memory related operations after flushing might be ignore until another flush operation
    /// ### is called. In other words, if you tried to flushed manually once, you always have to flush.
    /// ### The implementation has no mechanism to know if memory was changed afterwards.
    /// ### If you don't flush at all, the backend will do it automatically.
    fn try_flush_mem(
        &self,
        mem: &mut GraphicsBackendMemory,
        do_expensive_flushing: bool,
    ) -> anyhow::Result<()>;
}
