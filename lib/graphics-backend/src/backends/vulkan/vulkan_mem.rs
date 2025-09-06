use std::{
    num::NonZeroUsize,
    sync::{Arc, atomic::AtomicU64},
};

use anyhow::anyhow;
use ash::vk;
use config::config::AtomicGfxDebugModes;
use hiarc::Hiarc;
use libc::c_void;
use thiserror::Error;

use super::{
    buffer::Buffer,
    instance::Instance,
    logical_device::LogicalDevice,
    mapped_memory::{MappedMemory, MappedMemoryOffset},
    memory::{
        MemoryBlock, MemoryCache, MemoryHeapForVkMemory, MemoryHeapQueueElement, MemoryImageBlock,
        SMemoryHeapType,
    },
    memory_block::DeviceMemoryBlock,
    phy_device::PhyDevice,
    vulkan_types::MemoryBlockType,
};

#[derive(Debug, Error, Copy, Clone)]
pub enum MemoryAllocationError {
    #[error("Host is out of memory.")]
    OutOfHostMem,
    #[error("Device is out of memory.")]
    OutOfDeviceMem,
    #[error("Not specifically handled vulkan result {0}.")]
    VkError(vk::Result),
}

#[derive(Debug, Error)]
pub enum BufferAllocationError {
    #[error("Buffer creation failed.")]
    BufferCreationFailed,
    #[error("Memory allocation failed: {0}")]
    MemoryAllocationError(MemoryAllocationError),
    #[error("Binding memory to buffer failed.")]
    BindMemoryToBufferFailed,
    #[error("Mapping raw memory failed.")]
    MapMemoryFailed,
    #[error("Heap allocation failed.")]
    HeapAllocationFailed,
    #[error("Memory related operation failed: {0}")]
    MemoryRelatedOperationFailed(anyhow::Error),
}

impl From<MemoryAllocationError> for BufferAllocationError {
    fn from(value: MemoryAllocationError) -> Self {
        Self::MemoryAllocationError(value)
    }
}

impl From<vk::Result> for BufferAllocationError {
    fn from(_value: vk::Result) -> Self {
        Self::BufferCreationFailed
    }
}

#[derive(Debug, Error)]
pub enum ImageAllocationError {
    #[error("Image creation failed.")]
    ImageCreationFailed,
    #[error("Memory allocation failed: {0}")]
    MemoryAllocationError(MemoryAllocationError),
    #[error("Binding memory to image failed ({0}).")]
    BindMemoryToImageFailed(vk::Result),
    #[error("Mapping raw memory failed.")]
    MapMemoryFailed,
    #[error("Heap allocation failed.")]
    HeapAllocationFailed,
    #[error("Memory related operation failed.")]
    MemoryRelatedOperationFailed,
    #[error("Buffer allocation for copying failed: {0}")]
    BufferAllocationError(BufferAllocationError),
    #[error("Image dimensions too big.")]
    ImageDimensionsTooBig,
    #[error("No memory heap was found to allocate the required memory for the image.")]
    NoMemoryHeap,
}

impl From<MemoryAllocationError> for ImageAllocationError {
    fn from(value: MemoryAllocationError) -> Self {
        Self::MemoryAllocationError(value)
    }
}

impl From<vk::Result> for ImageAllocationError {
    fn from(_value: vk::Result) -> Self {
        Self::ImageCreationFailed
    }
}

impl From<BufferAllocationError> for ImageAllocationError {
    fn from(value: BufferAllocationError) -> Self {
        Self::BufferAllocationError(value)
    }
}

#[derive(Debug, Error)]
pub enum AllocationError {
    #[error("Image allocation error.")]
    Image(ImageAllocationError),
    #[error("Buffer allocation error.")]
    Buffer(BufferAllocationError),
    #[error("Memory allocation error.")]
    Memory(MemoryAllocationError),
}

impl From<ImageAllocationError> for AllocationError {
    fn from(value: ImageAllocationError) -> Self {
        Self::Image(value)
    }
}

impl From<BufferAllocationError> for AllocationError {
    fn from(value: BufferAllocationError) -> Self {
        Self::Buffer(value)
    }
}

impl From<MemoryAllocationError> for AllocationError {
    fn from(value: MemoryAllocationError) -> Self {
        Self::Memory(value)
    }
}

#[derive(Clone, Hiarc)]
pub struct Memory {
    #[hiarc_skip_unsafe]
    dbg: Arc<AtomicGfxDebugModes>,

    instance: Arc<Instance>,
    pub logical_device: Arc<LogicalDevice>,
    vk_gpu: Arc<PhyDevice>,

    pub(crate) texture_memory_usage: Arc<AtomicU64>,
    pub(crate) buffer_memory_usage: Arc<AtomicU64>,
    pub(crate) stream_memory_usage: Arc<AtomicU64>,
    pub(crate) staging_memory_usage: Arc<AtomicU64>,
}

impl std::fmt::Debug for Memory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Memory")
            .field("dbg", &self.dbg)
            .field("vk_gpu", &self.vk_gpu)
            .field("texture_memory_usage", &self.texture_memory_usage)
            .field("buffer_memory_usage", &self.buffer_memory_usage)
            .field("stream_memory_usage", &self.stream_memory_usage)
            .field("staging_memory_usage", &self.staging_memory_usage)
            .finish()
    }
}

impl Memory {
    pub fn new(
        dbg: Arc<AtomicGfxDebugModes>,

        instance: Arc<Instance>,
        device: Arc<LogicalDevice>,
        vk_gpu: Arc<PhyDevice>,

        texture_memory_usage: Arc<AtomicU64>,
        buffer_memory_usage: Arc<AtomicU64>,
        stream_memory_usage: Arc<AtomicU64>,
        staging_memory_usage: Arc<AtomicU64>,
    ) -> Self {
        Self {
            dbg,
            instance,
            logical_device: device.clone(),
            vk_gpu,
            texture_memory_usage,
            buffer_memory_usage,
            stream_memory_usage,
            staging_memory_usage,
        }
    }

    /************************
     * MEMORY MANAGEMENT
     ************************/
    pub fn find_memory_type(
        &self,
        phy_device: vk::PhysicalDevice,
        type_filter: u32,
        properties: vk::MemoryPropertyFlags,
    ) -> anyhow::Result<u32> {
        let mem_properties: vk::PhysicalDeviceMemoryProperties = unsafe {
            self.instance
                .vk_instance
                .get_physical_device_memory_properties(phy_device)
        };

        for i in 0..mem_properties.memory_type_count {
            if (type_filter & (1 << i)) != 0
                && (mem_properties.memory_types[i as usize].property_flags & properties)
                    == properties
            {
                return Ok(i);
            }
        }

        Err(anyhow!(
            "Could not find a matching memory type for type bits: \
            {type_filter} & {properties:?}, mem_properties: {:?}",
            mem_properties
        ))
    }

    fn allocate_vulkan_memory(
        &self,
        allocate_info: vk::MemoryAllocateInfo,
        usage_type: MemoryBlockType,
    ) -> anyhow::Result<Arc<DeviceMemoryBlock>, MemoryAllocationError> {
        let res = DeviceMemoryBlock::new(self.logical_device.clone(), allocate_info, usage_type);
        if let Err(err) = res {
            // TODO  dbg_msg("vulkan", "vulkan memory allocation failed, trying to recover.");
            match err {
                vk::Result::ERROR_OUT_OF_HOST_MEMORY => Err(MemoryAllocationError::OutOfHostMem),
                vk::Result::ERROR_OUT_OF_DEVICE_MEMORY => {
                    Err(MemoryAllocationError::OutOfDeviceMem)
                }
                _ => Err(MemoryAllocationError::VkError(err)),
            }
        } else {
            Ok(res.unwrap())
        }
    }

    pub fn get_block_impl<
        const MEMORY_BLOCK_SIZE: i64,
        const BLOCK_COUNT: usize,
        const REQUIRES_MAPPING: bool,
        FAlloc,
    >(
        &self,
        memory_cache: &Arc<parking_lot::Mutex<MemoryCache>>,
        buffer_usage: vk::BufferUsageFlags,
        buffer_properties: vk::MemoryPropertyFlags,
        buffer_data: *const c_void,
        required_size: vk::DeviceSize,
        target_alignment: vk::DeviceSize,
        allocate_block: FAlloc,
        memory_requirement_bits: u32,
    ) -> anyhow::Result<Arc<MemoryBlock>, BufferAllocationError>
    where
        FAlloc: FnOnce(
            vk::DeviceSize,
            MemoryBlockType,
            vk::BufferUsageFlags,
            vk::MemoryPropertyFlags,
            u32, // memory requirement bits
        ) -> anyhow::Result<
            (Option<Arc<Buffer>>, Arc<DeviceMemoryBlock>),
            BufferAllocationError,
        >,
    {
        // if the required size is in the region of a single memory block
        // try to find it or create it
        if required_size < MEMORY_BLOCK_SIZE as vk::DeviceSize {
            let create_or_find_cache_block = || {
                let mut found_allocation = false;
                let mut allocated_mem = None;
                let mut tmp_block_memory = None;
                let cache_heap: Option<(Option<Arc<Buffer>>, Option<MappedMemoryOffset>)>;
                let mut found_id = 0;
                // try to allocate the memory inside existing heaps
                let mut memory_cache_guard = memory_cache.lock();
                let heaps = &mut memory_cache_guard.memory_heaps;
                for (id, heap) in heaps.iter_mut() {
                    allocated_mem = heap
                        .heap
                        .allocate(required_size as usize, target_alignment as usize);
                    if allocated_mem.is_some() {
                        tmp_block_memory = Some(heap.buffer_mem.clone());
                        found_allocation = true;
                        found_id = *id;
                        break;
                    }
                }
                drop(memory_cache_guard);
                // if no heap was suited, we create a new block
                if !found_allocation {
                    let block_allocation_size = MEMORY_BLOCK_SIZE as u64 * BLOCK_COUNT as u64;
                    let buffer_allocation = allocate_block(
                        block_allocation_size,
                        if REQUIRES_MAPPING {
                            MemoryBlockType::Staging
                        } else {
                            MemoryBlockType::Buffer
                        },
                        buffer_usage,
                        buffer_properties,
                        memory_requirement_bits,
                    );
                    let (tmp_buffer, res_block) = buffer_allocation?;
                    tmp_block_memory = Some(res_block.clone());

                    let mut mapped_data_as_ptr: Option<MappedMemoryOffset> = None;

                    if REQUIRES_MAPPING {
                        let unmapped = MappedMemory::new(
                            self.logical_device.clone(),
                            tmp_block_memory.as_ref().unwrap().clone(),
                            0,
                        );
                        match unmapped {
                            Err(_) => {
                                // TODO: add case for image
                                return Err(BufferAllocationError::BindMemoryToBufferFailed);
                            }
                            Ok(mapped_mem) => {
                                mapped_data_as_ptr = Some(MappedMemoryOffset::new(mapped_mem, 0));
                            }
                        };
                    }

                    let mut new_heap = MemoryHeapForVkMemory::new(
                        tmp_buffer,
                        res_block,
                        mapped_data_as_ptr,
                        MEMORY_BLOCK_SIZE as usize * BLOCK_COUNT,
                        0,
                    );

                    cache_heap = Some((new_heap.buffer.clone(), new_heap.mapped_buffer.clone()));

                    let mut memory_cache = memory_cache.lock();
                    memory_cache.heap_id_gen += 1;
                    found_id = memory_cache.heap_id_gen;
                    allocated_mem = new_heap
                        .heap
                        .allocate(required_size as usize, target_alignment as usize);
                    if allocated_mem.is_none() {
                        // TODO: add case for image
                        return Err(BufferAllocationError::HeapAllocationFailed);
                    }
                    assert!(
                        memory_cache
                            .memory_heaps
                            .insert(found_id, new_heap)
                            .is_none()
                    );
                } else {
                    let memory_cache = memory_cache.lock();
                    let heap = memory_cache.memory_heaps.get(&found_id).unwrap();
                    cache_heap = Some((heap.buffer.clone(), heap.mapped_buffer.clone()));
                }

                let (buffer, mapped_buffer) = cache_heap.unwrap();
                let allocated_mem = allocated_mem.unwrap();
                let mem_offset = allocated_mem.offset_to_align as isize;
                assert!(found_id != 0);
                let res_block = MemoryBlock::new(
                    allocated_mem,
                    tmp_block_memory.unwrap(),
                    buffer,
                    if REQUIRES_MAPPING {
                        let mem = mapped_buffer.clone().unwrap();
                        Some(mem.offset(mem_offset))
                    } else {
                        None
                    },
                    SMemoryHeapType::Cached {
                        heap: memory_cache.clone(),
                        id: found_id,
                    },
                );

                if REQUIRES_MAPPING && !buffer_data.is_null() {
                    let mem = res_block.mapped_buffer.as_ref().unwrap();
                    unsafe {
                        libc::memcpy(
                            mem.get_mem(required_size as usize) as *mut _ as *mut _,
                            buffer_data,
                            required_size as usize,
                        );
                    }
                }
                Ok(res_block)
            };
            create_or_find_cache_block()
        } else {
            let block_allocation = allocate_block(
                required_size,
                if REQUIRES_MAPPING {
                    MemoryBlockType::Staging
                } else {
                    MemoryBlockType::Buffer
                },
                buffer_usage,
                buffer_properties,
                memory_requirement_bits,
            );
            let (tmp_buffer, tmp_block_memory) = block_allocation?;

            let mut mapped_data = None;
            if REQUIRES_MAPPING {
                unsafe {
                    mapped_data = Some(
                        MappedMemory::new(self.logical_device.clone(), tmp_block_memory.clone(), 0)
                            .unwrap(),
                    );
                    if !buffer_data.is_null() {
                        libc::memcpy(
                            mapped_data.as_ref().unwrap().get_mem() as *mut _,
                            buffer_data,
                            required_size as usize,
                        );
                    }
                }
            }

            let mut heap_data = MemoryHeapQueueElement::new(NonZeroUsize::new(1).unwrap());
            heap_data.offset_to_align = 0;
            heap_data.allocation_size = required_size as usize;
            let res_block = MemoryBlock::new(
                heap_data,
                tmp_block_memory,
                tmp_buffer,
                mapped_data.map(|i| MappedMemoryOffset::new(i, 0)),
                SMemoryHeapType::None,
            );

            Ok(res_block)
        }
    }

    pub fn get_buffer_block_impl<
        const MEMORY_BLOCK_SIZE: i64,
        const BLOCK_COUNT: usize,
        const REQUIRES_MAPPING: bool,
    >(
        &self,
        memory_cache: &Arc<parking_lot::Mutex<MemoryCache>>,
        buffer_usage: vk::BufferUsageFlags,
        buffer_properties: vk::MemoryPropertyFlags,
        buffer_data: *const c_void,
        requized_size: vk::DeviceSize,
        target_alignment: vk::DeviceSize,
    ) -> anyhow::Result<Arc<MemoryBlock>, BufferAllocationError> {
        self.get_block_impl::<{ MEMORY_BLOCK_SIZE }, { BLOCK_COUNT }, { REQUIRES_MAPPING }, _>(
            memory_cache,
            buffer_usage,
            buffer_properties,
            buffer_data,
            requized_size,
            target_alignment,
            |required_size: vk::DeviceSize,
             mem_usage: MemoryBlockType,
             buffer_usage: vk::BufferUsageFlags,
             buffer_properties: vk::MemoryPropertyFlags,
             _| {
                let (buffer, mem) =
                    self.create_buffer(required_size, mem_usage, buffer_usage, buffer_properties)?;
                Ok((Some(buffer), mem))
            },
            0,
        )
    }

    pub fn create_buffer(
        &self,
        buffer_size: vk::DeviceSize,
        mem_usage: MemoryBlockType,
        buffer_usage: vk::BufferUsageFlags,
        memory_properties: vk::MemoryPropertyFlags,
    ) -> anyhow::Result<(Arc<Buffer>, Arc<DeviceMemoryBlock>), BufferAllocationError> {
        let mut buffer_info = vk::BufferCreateInfo::default();
        buffer_info.size = buffer_size;
        buffer_info.usage = buffer_usage;
        buffer_info.sharing_mode = vk::SharingMode::EXCLUSIVE;

        let created_buffer_res = Buffer::new(self.logical_device.clone(), buffer_info);
        if let Err(_) = created_buffer_res {
            return Err(BufferAllocationError::BufferCreationFailed);
        }
        let vk_buffer = created_buffer_res.unwrap();

        let mem_requirements = vk_buffer.get_buffer_memory_requirements();

        let mut mem_alloc_info = vk::MemoryAllocateInfo::default();
        mem_alloc_info.allocation_size = mem_requirements.size;
        mem_alloc_info.memory_type_index = self
            .find_memory_type(
                self.vk_gpu.cur_device,
                mem_requirements.memory_type_bits,
                memory_properties,
            )
            .map_err(BufferAllocationError::MemoryRelatedOperationFailed)?;

        let allocation = self.allocate_vulkan_memory(mem_alloc_info, mem_usage);
        if let Err(err) = &allocation {
            return Err(BufferAllocationError::MemoryAllocationError(*err));
        }
        let vk_buffer_memory = allocation.unwrap();

        let res = vk_buffer.bind(vk_buffer_memory.clone());
        if res.is_err() {
            return Err(BufferAllocationError::MapMemoryFailed);
        }

        Ok((vk_buffer, vk_buffer_memory))
    }

    fn get_image_memory_impl(
        &self,
        required_size: vk::DeviceSize,
        required_memory_type_bits: u32,

        buffer_properties: vk::MemoryPropertyFlags,
    ) -> anyhow::Result<Arc<DeviceMemoryBlock>, BufferAllocationError> {
        let mut mem_alloc_info = vk::MemoryAllocateInfo::default();
        mem_alloc_info.allocation_size = required_size;
        mem_alloc_info.memory_type_index = self
            .find_memory_type(
                self.vk_gpu.cur_device,
                required_memory_type_bits,
                buffer_properties,
            )
            .map_err(BufferAllocationError::MemoryRelatedOperationFailed)?;

        let allocation = self.allocate_vulkan_memory(mem_alloc_info, MemoryBlockType::Texture);
        if let Err(err) = &allocation {
            return Err(BufferAllocationError::MemoryAllocationError(*err));
        }
        let buffer_memory = allocation.unwrap();

        Ok(buffer_memory)
    }

    pub fn get_image_memory_block_impl<const MEMORY_BLOCK_SIZE: i64, const BLOCK_COUNT: usize>(
        &mut self,
        memory_cache: &Arc<parking_lot::Mutex<MemoryCache>>,
        buffer_properties: vk::MemoryPropertyFlags,
        required_size: vk::DeviceSize,
        required_alignment: vk::DeviceSize,
        required_memory_type_bits: u32,
    ) -> anyhow::Result<MemoryImageBlock, BufferAllocationError> {
        let base_block = self.get_block_impl::<{ MEMORY_BLOCK_SIZE }, { BLOCK_COUNT }, false, _>(
            memory_cache,
            vk::BufferUsageFlags::empty(),
            buffer_properties,
            std::ptr::null(),
            required_size,
            required_alignment,
            |required_size, _, _, buffer_properties, required_memory_type_bits| {
                let memory_block = self.get_image_memory_impl(
                    required_size,
                    required_memory_type_bits,
                    buffer_properties,
                )?;
                Ok((Default::default(), memory_block))
            },
            required_memory_type_bits,
        )?;

        let result_block = MemoryImageBlock {
            base: base_block,
            image_memory_bits: required_memory_type_bits,
        };

        Ok(result_block)
    }
}
