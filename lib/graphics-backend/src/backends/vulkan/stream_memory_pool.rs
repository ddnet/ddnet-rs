use std::sync::{Arc, atomic::AtomicU64};

use ash::vk;
use config::config::AtomicGfxDebugModes;
use hiarc::Hiarc;
use parking_lot::Mutex;
use pool::{
    arc::{ArcPool, PoolArc},
    mt_datatypes::PoolVec as MtPoolVec,
    mt_pool::Pool as MtPool,
};

use super::{
    buffer::Buffer,
    frame_resources::FrameResources,
    instance::Instance,
    logical_device::LogicalDevice,
    mapped_memory::{MappedMemory, MappedMemoryOffset},
    memory_block::DeviceMemoryBlock,
    phy_device::PhyDevice,
    vulkan_mem::{BufferAllocationError, Memory},
    vulkan_types::MemoryBlockType,
};

#[derive(Debug, Hiarc)]
pub struct StreamMemory<T: Send + Sync> {
    pub buffer: Arc<Buffer>,
    mem: Arc<DeviceMemoryBlock>,
    pub offset: usize,
    pub mapped_memory: MappedMemoryOffset,

    pub user: T,
}

impl<T: Send + Sync> StreamMemory<T> {
    pub fn flush(
        &self,
        frame_resources: &mut FrameResources,
        non_coherent_mem_alignment: u64,
        flush_size: usize,
        non_flushed_memory_ranges: &mut Vec<vk::MappedMemoryRange>,
    ) {
        if flush_size > 0 {
            let mut mem_range = vk::MappedMemoryRange::default();
            mem_range.memory = self.mem.mem(frame_resources);
            mem_range.offset = self.offset as vk::DeviceSize;
            let alignment_mod = flush_size as vk::DeviceSize % non_coherent_mem_alignment;
            let mut alignment_req = non_coherent_mem_alignment - alignment_mod;
            if alignment_mod == 0 {
                alignment_req = 0;
            }
            mem_range.size = flush_size as u64 + alignment_req;

            if mem_range.offset + mem_range.size > self.mem.size() {
                mem_range.size = vk::WHOLE_SIZE;
            }

            non_flushed_memory_ranges.push(mem_range);
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct StreamMemoryBlock<T: Send + Sync> {
    pub memories: MtPoolVec<StreamMemory<T>>,
    pool: Arc<Mutex<Vec<StreamMemory<T>>>>,
}

impl<T: Send + Sync> StreamMemoryBlock<T> {
    pub fn new(
        block_pool: &ArcPool<StreamMemoryBlock<T>>,
        memories: MtPoolVec<StreamMemory<T>>,
        pool: Arc<Mutex<Vec<StreamMemory<T>>>>,
    ) -> PoolArc<Self> {
        block_pool.new_arc(Self { memories, pool })
    }
}

impl<T: Send + Sync> Drop for StreamMemoryBlock<T> {
    fn drop(&mut self) {
        self.pool.lock().append(&mut self.memories);
    }
}

#[derive(Debug, Hiarc)]
pub struct StreamMemoryPool<T: Send + Sync> {
    mem: Memory,

    pub pool: Arc<Mutex<Vec<StreamMemory<T>>>>,

    #[hiarc_skip_unsafe]
    usage: vk::BufferUsageFlags,

    size_of_instance: usize,
    instances_per_buffer: usize,
    buffers_per_allocation: usize,

    pub vec_pool: MtPool<Vec<StreamMemory<T>>>,
    pub block_pool: ArcPool<StreamMemoryBlock<T>>,
}

impl<T: Send + Sync> StreamMemoryPool<T> {
    pub fn new(
        dbg: Arc<AtomicGfxDebugModes>,

        instance: Arc<Instance>,
        device: Arc<LogicalDevice>,
        vk_gpu: Arc<PhyDevice>,

        texture_memory_usage: Arc<AtomicU64>,
        buffer_memory_usage: Arc<AtomicU64>,
        stream_memory_usage: Arc<AtomicU64>,
        staging_memory_usage: Arc<AtomicU64>,

        usage: vk::BufferUsageFlags,

        size_of_instance: usize,
        instances_per_buffer: usize,
        buffers_per_allocation: usize,
    ) -> Self {
        let vec_pool = MtPool::with_capacity(10);

        Self {
            mem: Memory::new(
                dbg,
                instance,
                device,
                vk_gpu,
                texture_memory_usage,
                buffer_memory_usage,
                stream_memory_usage,
                staging_memory_usage,
            ),
            usage,
            size_of_instance,
            instances_per_buffer,
            buffers_per_allocation,

            pool: Default::default(),

            vec_pool,
            block_pool: ArcPool::with_capacity(8),
        }
    }

    /// allocates only if pool is smaller than count
    pub fn try_alloc(
        &mut self,
        mut new_instance_func: impl FnMut(&Arc<Buffer>, vk::DeviceSize, usize) -> anyhow::Result<Vec<T>>,
        count: usize,
    ) -> anyhow::Result<(), BufferAllocationError> {
        let mut pool = self.pool.lock();
        while pool.len() < count {
            let alloc_amount = self.buffers_per_allocation.max(count - pool.len());
            // allocate new buffers
            let new_buffer_single_size =
                (self.size_of_instance * self.instances_per_buffer) as vk::DeviceSize;
            let new_buffer_size = (new_buffer_single_size * alloc_amount as u64) as vk::DeviceSize;
            let (stream_buffer, stream_buffer_memory) = self.mem.create_buffer(
                new_buffer_size,
                MemoryBlockType::Stream,
                self.usage,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_CACHED,
            )?;

            let ptr_mapped_data = MappedMemory::new(
                self.mem.logical_device.clone(),
                stream_buffer_memory.clone(),
                0,
            )
            .unwrap();
            let mut users = new_instance_func(&stream_buffer, 0, alloc_amount)
                .map_err(BufferAllocationError::MemoryRelatedOperationFailed)?;
            for (i, user) in users.drain(..).enumerate() {
                let offset = new_buffer_single_size * i as vk::DeviceSize;
                pool.push(StreamMemory {
                    buffer: stream_buffer.clone(),
                    mem: stream_buffer_memory.clone(),
                    offset: offset as usize,
                    mapped_memory: MappedMemoryOffset::new(
                        ptr_mapped_data.clone(),
                        offset as isize,
                    ),
                    user,
                });
            }
        }

        Ok(())
    }

    pub fn try_get(&mut self, count: usize) -> Option<PoolArc<StreamMemoryBlock<T>>> {
        let mut pool = self.pool.lock();
        let mut res = self.vec_pool.new();
        let pool_len = pool.len();
        if pool_len < count {
            None
        } else {
            res.extend(pool.drain(pool_len - count..pool_len));
            drop(pool);
            Some(StreamMemoryBlock::new(
                &self.block_pool,
                res,
                self.pool.clone(),
            ))
        }
    }

    pub fn get(
        &mut self,
        new_instance_func: impl FnMut(&Arc<Buffer>, vk::DeviceSize, usize) -> anyhow::Result<Vec<T>>,
        count: usize,
    ) -> anyhow::Result<PoolArc<StreamMemoryBlock<T>>, BufferAllocationError> {
        self.try_alloc(new_instance_func, count)?;

        Ok(self.try_get(count).unwrap())
    }
}
