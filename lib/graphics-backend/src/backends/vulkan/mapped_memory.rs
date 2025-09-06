use std::sync::Arc;

use ash::vk;
use hiarc::Hiarc;

use super::{
    frame_resources::FrameResources, logical_device::LogicalDevice, memory_block::DeviceMemoryBlock,
};

#[derive(Debug, Hiarc)]
pub struct MappedMemory {
    mapped_mem: *mut u8,

    mem: Arc<DeviceMemoryBlock>,

    device: Arc<LogicalDevice>,
}

unsafe impl Send for MappedMemory {}
unsafe impl Sync for MappedMemory {}

impl MappedMemory {
    pub fn new(
        device: Arc<LogicalDevice>,
        mem: Arc<DeviceMemoryBlock>,
        offset: vk::DeviceSize,
    ) -> anyhow::Result<Arc<Self>> {
        let mapped_mem = unsafe {
            device.device.map_memory(
                mem.mem(&mut FrameResources::new(None)),
                offset,
                vk::WHOLE_SIZE,
                vk::MemoryMapFlags::empty(),
            )
        }? as *mut u8;
        Ok(Arc::new(Self {
            mapped_mem,
            device,
            mem,
        }))
    }

    pub unsafe fn get_mem(&self) -> *mut u8 {
        self.mapped_mem
    }
}

impl Drop for MappedMemory {
    fn drop(&mut self) {
        unsafe {
            self.device
                .device
                .unmap_memory(self.mem.mem(&mut FrameResources::new(None)));
        }
    }
}

#[derive(Debug, Hiarc, Clone)]
pub struct MappedMemoryOffset {
    mem: Arc<MappedMemory>,
    offset: isize,
}

impl MappedMemoryOffset {
    pub fn new(mem: Arc<MappedMemory>, offset: isize) -> Self {
        Self { mem, offset }
    }

    pub fn offset(&self, offset: isize) -> Self {
        Self {
            mem: self.mem.clone(),
            offset: self.offset + offset,
        }
    }

    pub unsafe fn get_mem_typed<T>(&self, required_instance_count: usize) -> &'static mut [T] {
        unsafe {
            std::slice::from_raw_parts_mut::<T>(
                self.mem.get_mem().offset(self.offset) as *mut _,
                required_instance_count,
            )
        }
    }

    pub unsafe fn get_mem(&self, required_size: usize) -> &'static mut [u8] {
        unsafe { self.get_mem_typed(required_size) }
    }
}
