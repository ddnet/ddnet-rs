use anyhow::anyhow;
use graphics_backend_traits::traits::GraphicsBackendMtInterface;
use graphics_types::types::{
    GraphicsBackendMemory, GraphicsBackendMemoryAllocation, GraphicsMemoryAllocationMode,
    GraphicsMemoryAllocationType,
};

/// The multi-threaded backend part to [`graphics::graphics_mt::GraphicsMultiThreaded`]
#[derive(Debug)]
pub struct GraphicsBackendMultiThreaded {}

impl Default for GraphicsBackendMultiThreaded {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphicsBackendMultiThreaded {
    pub fn new() -> Self {
        Self {}
    }
}

impl GraphicsBackendMtInterface for GraphicsBackendMultiThreaded {
    fn mem_alloc(
        &self,
        alloc_type: GraphicsMemoryAllocationType,
        _mode: GraphicsMemoryAllocationMode,
    ) -> GraphicsBackendMemory {
        GraphicsBackendMemory::new(
            match alloc_type {
                GraphicsMemoryAllocationType::TextureRgbaU8 { width, height, .. } => {
                    let mut res = Vec::new();
                    res.resize(width.get() * height.get() * 4, Default::default());
                    GraphicsBackendMemoryAllocation::Vector(res)
                }
                GraphicsMemoryAllocationType::TextureRgbaU82dArray {
                    width,
                    height,
                    depth,
                    ..
                } => {
                    let mut res = Vec::new();
                    res.resize(
                        width.get() * height.get() * depth.get() * 4,
                        Default::default(),
                    );
                    GraphicsBackendMemoryAllocation::Vector(res)
                }
                GraphicsMemoryAllocationType::VertexBuffer { required_size } => {
                    let mut res = Vec::new();
                    res.resize(required_size.get(), Default::default());
                    GraphicsBackendMemoryAllocation::Vector(res)
                }
                GraphicsMemoryAllocationType::ShaderStorage { required_size } => {
                    let mut res = Vec::new();
                    res.resize(required_size.get(), Default::default());
                    GraphicsBackendMemoryAllocation::Vector(res)
                }
            },
            alloc_type,
        )
    }

    fn try_flush_mem(
        &self,
        _mem: &mut GraphicsBackendMemory,
        _do_expensive_flushing: bool,
    ) -> anyhow::Result<()> {
        Err(anyhow!(
            "not supported inside a WASM module (this is not a bug)."
        ))
    }
}
