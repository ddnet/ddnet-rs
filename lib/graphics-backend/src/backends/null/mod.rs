use anyhow::anyhow;
use graphics_backend_traits::traits::{DriverBackendInterface, GraphicsBackendMtInterface};
use graphics_types::{
    commands::AllCommands,
    types::{
        GraphicsBackendMemory, GraphicsBackendMemoryAllocation, GraphicsMemoryAllocationMode,
        GraphicsMemoryAllocationType,
    },
};
use hiarc::Hiarc;

#[derive(Debug, Hiarc)]
pub struct NullBackend {}

impl NullBackend {
    pub fn get_mt_backend() -> NullBackendMt {
        NullBackendMt {}
    }
}

impl DriverBackendInterface for NullBackend {
    fn attach_frame_fetcher(
        &mut self,
        _name: String,
        _fetcher: std::sync::Arc<
            dyn graphics_backend_traits::frame_fetcher_plugin::BackendFrameFetcher,
        >,
    ) {
        // do nothing
    }

    fn detach_frame_fetcher(&mut self, _name: String) {
        // do nothing
    }

    fn run_command(&mut self, _cmd: AllCommands) -> anyhow::Result<()> {
        // nothing to do
        Ok(())
    }

    fn start_commands(&mut self, _command_count: usize) {
        // nothing to do
    }

    fn end_commands(&mut self) -> anyhow::Result<()> {
        // nothing to do
        Ok(())
    }
}

pub fn mem_alloc_lazy(alloc_type: GraphicsMemoryAllocationType) -> GraphicsBackendMemory {
    let mut mem: Vec<u8> = Default::default();
    match alloc_type {
        GraphicsMemoryAllocationType::TextureRgbaU8 { width, height, .. } => {
            mem.resize(width.get() * height.get() * 4, Default::default())
        }
        GraphicsMemoryAllocationType::TextureRgbaU82dArray {
            width,
            height,
            depth,
            ..
        } => mem.resize(
            width.get() * height.get() * depth.get() * 4,
            Default::default(),
        ),
        GraphicsMemoryAllocationType::VertexBuffer { required_size } => {
            mem.resize(required_size.get(), Default::default())
        }
        GraphicsMemoryAllocationType::ShaderStorage { required_size } => {
            mem.resize(required_size.get(), Default::default())
        }
    }
    GraphicsBackendMemory::new(GraphicsBackendMemoryAllocation::Vector(mem), alloc_type)
}

#[derive(Debug, Hiarc)]
pub struct NullBackendMt {}

impl GraphicsBackendMtInterface for NullBackendMt {
    fn mem_alloc(
        &self,
        alloc_type: GraphicsMemoryAllocationType,
        _mode: GraphicsMemoryAllocationMode,
    ) -> GraphicsBackendMemory {
        mem_alloc_lazy(alloc_type)
    }

    fn try_flush_mem(
        &self,
        _mem: &mut GraphicsBackendMemory,
        _do_expensive_flushing: bool,
    ) -> anyhow::Result<()> {
        Err(anyhow!("this operation is not supported."))
    }
}
