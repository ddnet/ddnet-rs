use std::sync::Arc;

use ash::vk;
use hiarc::Hiarc;

use super::{
    frame_resources::FrameResources, logical_device::LogicalDevice, vulkan_dbg::is_verbose,
    vulkan_types::MemoryBlockType,
};

fn verbose_allocated_memory(size: vk::DeviceSize, mem_usage: MemoryBlockType) {
    let usage_str = match mem_usage {
        MemoryBlockType::Texture => "texture",
        MemoryBlockType::Buffer => "buffer",
        MemoryBlockType::Stream => "stream",
        MemoryBlockType::Staging => "staging buffer",
    };
    log::info!(
        target: "vulkan",
        "allocated chunk of memory with size: {size} ({usage_str})"
    );
}

fn verbose_deallocated_memory(size: vk::DeviceSize, mem_usage: MemoryBlockType) {
    let usage_str = match mem_usage {
        MemoryBlockType::Texture => "texture",
        MemoryBlockType::Buffer => "buffer",
        MemoryBlockType::Stream => "stream",
        MemoryBlockType::Staging => "staging buffer",
    };
    log::info!(
        target: "vulkan",
        "deallocated chunk of memory with size: {size} ({usage_str})"
    );
}

#[derive(Debug, Clone, Hiarc)]
pub struct DeviceMemoryBlock {
    #[hiarc_skip_unsafe]
    mem: vk::DeviceMemory,
    size: vk::DeviceSize,
    usage_type: MemoryBlockType,

    device: Arc<LogicalDevice>,
}

impl DeviceMemoryBlock {
    pub fn new(
        device: Arc<LogicalDevice>,
        mem_alloc_info: vk::MemoryAllocateInfo,
        usage_type: MemoryBlockType,
    ) -> anyhow::Result<Arc<Self>, vk::Result> {
        let size = mem_alloc_info.allocation_size;
        match usage_type {
            MemoryBlockType::Texture => {
                device
                    .texture_memory_usage
                    .fetch_add(size, std::sync::atomic::Ordering::Relaxed);
            }
            MemoryBlockType::Buffer => {
                device
                    .buffer_memory_usage
                    .fetch_add(size, std::sync::atomic::Ordering::Relaxed);
            }
            MemoryBlockType::Stream => {
                device
                    .stream_memory_usage
                    .fetch_add(size, std::sync::atomic::Ordering::Relaxed);
            }
            MemoryBlockType::Staging => {
                device
                    .staging_memory_usage
                    .fetch_add(size, std::sync::atomic::Ordering::Relaxed);
            }
        };
        if is_verbose(&device.dbg) {
            verbose_allocated_memory(mem_alloc_info.allocation_size, usage_type);
        }

        let mem = unsafe { device.device.allocate_memory(&mem_alloc_info, None) }?;

        Ok(Arc::new(Self {
            device,

            mem,
            size,
            usage_type,
        }))
    }

    pub fn size(&self) -> vk::DeviceSize {
        self.size
    }

    pub fn mem(self: &Arc<Self>, frame_resources: &mut FrameResources) -> vk::DeviceMemory {
        frame_resources.device_memory.push(self.clone());

        self.mem
    }
}

impl Drop for DeviceMemoryBlock {
    fn drop(&mut self) {
        match self.usage_type {
            MemoryBlockType::Texture => {
                self.device
                    .texture_memory_usage
                    .fetch_sub(self.size, std::sync::atomic::Ordering::Relaxed);
            }
            MemoryBlockType::Buffer => {
                self.device
                    .buffer_memory_usage
                    .fetch_sub(self.size, std::sync::atomic::Ordering::Relaxed);
            }
            MemoryBlockType::Stream => {
                self.device
                    .stream_memory_usage
                    .fetch_sub(self.size, std::sync::atomic::Ordering::Relaxed);
            }
            MemoryBlockType::Staging => {
                self.device
                    .staging_memory_usage
                    .fetch_sub(self.size, std::sync::atomic::Ordering::Relaxed);
            }
        };

        if is_verbose(&self.device.dbg) {
            verbose_deallocated_memory(self.size, self.usage_type);
        }

        unsafe {
            self.device.device.free_memory(self.mem, None);
        }
    }
}
