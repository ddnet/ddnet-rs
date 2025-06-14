use std::{
    collections::HashMap,
    rc::Rc,
    sync::{atomic::AtomicU64, Arc},
};

use anyhow::anyhow;
use ash::vk;
use config::config::AtomicGfxDebugModes;
use hiarc::Hiarc;
use libc::c_void;
use strum::EnumCount;

use super::{
    barriers::{image_barrier, memory_barrier},
    buffer::Buffer,
    command_pool::{AutoCommandBuffer, AutoCommandBufferType, CommandPool},
    descriptor_layout::DescriptorSetLayout,
    descriptor_set::DescriptorSets,
    frame_resources::FrameResources,
    image::Image,
    image_view::ImageView,
    instance::Instance,
    logical_device::LogicalDevice,
    memory::{MemoryBlock, MemoryHeapQueueElement, MemoryImageBlock},
    memory_block::DeviceMemoryBlock,
    phy_device::PhyDevice,
    queue::Queue,
    sampler::Sampler,
    utils::{
        build_mipmaps, complete_buffer_object, complete_shader_storage_object, complete_texture,
        copy_buffer, copy_buffer_to_image, get_memory_range,
    },
    vulkan_allocator::{FlushType, VulkanAllocator, VulkanDeviceInternalMemory},
    vulkan_limits::Limits,
    vulkan_mem::{BufferAllocationError, ImageAllocationError, Memory},
    vulkan_types::{
        BufferObject, BufferObjectMem, DescriptorPoolType, DeviceDescriptorPools, MemoryBlockType,
        ShaderStorage, SupportedSamplerTypes, TextureObject,
    },
    Options,
};

type TextureSamplersRes = (
    (Arc<Sampler>, Arc<DescriptorSetLayout>),
    (Arc<Sampler>, Arc<DescriptorSetLayout>),
    (Arc<Sampler>, Arc<DescriptorSetLayout>),
);

type DescriptorSetsRes = (
    Arc<DescriptorSets>,
    Arc<DescriptorSets>,
    Arc<DescriptorSets>,
);

#[derive(Debug, Hiarc)]
pub struct DeviceAsh {
    pub device: Arc<LogicalDevice>,
}

#[derive(Debug, Hiarc, Clone)]
pub struct DescriptorLayouts {
    pub standard_textured_descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub standard_2d_texture_array_descriptor_set_layout: Arc<DescriptorSetLayout>,

    pub vertex_uniform_descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub vertex_fragment_uniform_descriptor_set_layout: Arc<DescriptorSetLayout>,

    pub vertex_shader_storage_descriptor_set_layout: Arc<DescriptorSetLayout>,

    pub samplers_layouts: Arc<[Arc<DescriptorSetLayout>; SupportedSamplerTypes::COUNT]>,
}

#[derive(Debug, Hiarc)]
pub struct Device {
    pub mem: Memory,
    pub mem_allocator: Arc<parking_lot::Mutex<VulkanAllocator>>,

    pub ash_vk: DeviceAsh,

    pub vk_gpu: Arc<PhyDevice>,

    #[hiarc_skip_unsafe]
    pub non_flushed_memory_ranges: Vec<vk::MappedMemoryRange<'static>>,

    pub samplers: Arc<[(Arc<Sampler>, Arc<DescriptorSets>); SupportedSamplerTypes::COUNT]>,

    pub textures: HashMap<u128, TextureObject>,
    pub buffer_objects: HashMap<u128, BufferObject>,
    pub shader_storages: HashMap<u128, ShaderStorage>,

    pub standard_texture_descr_pool: Arc<parking_lot::Mutex<DeviceDescriptorPools>>,

    pub layouts: DescriptorLayouts,

    // command buffers
    pub command_pool: Rc<CommandPool>,
    pub memory_command_buffer: Option<AutoCommandBuffer>,

    pub is_headless: bool,
}

impl Device {
    fn create_vertex_uniform_descriptor_set_layout(
        device: &Arc<LogicalDevice>,
    ) -> anyhow::Result<Arc<DescriptorSetLayout>> {
        VulkanAllocator::create_uniform_descriptor_set_layout(device, vk::ShaderStageFlags::VERTEX)
    }

    fn create_vertex_fragment_uniform_descriptor_set_layout(
        device: &Arc<LogicalDevice>,
    ) -> anyhow::Result<Arc<DescriptorSetLayout>> {
        VulkanAllocator::create_uniform_descriptor_set_layout(
            device,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
        )
    }

    fn create_vertex_shader_storage_descriptor_set_layout(
        device: &Arc<LogicalDevice>,
    ) -> anyhow::Result<Arc<DescriptorSetLayout>> {
        VulkanAllocator::create_shader_storage_read_only_descriptor_set_layout(
            device,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
        )
    }

    fn create_descriptor_set_layouts(
        device: &Arc<LogicalDevice>,
    ) -> anyhow::Result<(Arc<DescriptorSetLayout>, Arc<DescriptorSetLayout>)> {
        let mut sampler_layout_binding = vk::DescriptorSetLayoutBinding::default();
        sampler_layout_binding.binding = 0;
        sampler_layout_binding.descriptor_count = 1;
        sampler_layout_binding.descriptor_type = vk::DescriptorType::SAMPLED_IMAGE;
        sampler_layout_binding.stage_flags = vk::ShaderStageFlags::FRAGMENT;

        let layout_bindings = [sampler_layout_binding];

        let layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&layout_bindings);

        let standard_textured_descriptor_set_layout =
            DescriptorSetLayout::new(device.clone(), &layout_info)?;

        let standard_3d_textured_descriptor_set_layout =
            DescriptorSetLayout::new(device.clone(), &layout_info)?;
        Ok((
            standard_textured_descriptor_set_layout,
            standard_3d_textured_descriptor_set_layout,
        ))
    }

    fn create_texture_samplers(
        device: &Arc<LogicalDevice>,
        limits: &Limits,
        global_texture_lod_bias: f64,
    ) -> anyhow::Result<TextureSamplersRes> {
        Ok((
            Device::create_texture_samplers_impl(
                device,
                limits.max_sampler_anisotropy,
                global_texture_lod_bias,
                vk::SamplerAddressMode::REPEAT,
                vk::SamplerAddressMode::REPEAT,
                vk::SamplerAddressMode::REPEAT,
            )?,
            Device::create_texture_samplers_impl(
                device,
                limits.max_sampler_anisotropy,
                global_texture_lod_bias,
                vk::SamplerAddressMode::CLAMP_TO_EDGE,
                vk::SamplerAddressMode::CLAMP_TO_EDGE,
                vk::SamplerAddressMode::CLAMP_TO_EDGE,
            )?,
            Device::create_texture_samplers_impl(
                device,
                limits.max_sampler_anisotropy,
                global_texture_lod_bias,
                vk::SamplerAddressMode::CLAMP_TO_EDGE,
                vk::SamplerAddressMode::CLAMP_TO_EDGE,
                vk::SamplerAddressMode::MIRRORED_REPEAT,
            )?,
        ))
    }

    pub fn new(
        dbg: Arc<AtomicGfxDebugModes>,
        instance: Arc<Instance>,
        device: Arc<LogicalDevice>,
        vk_gpu: Arc<PhyDevice>,

        graphics_queue: Arc<Queue>,

        texture_memory_usage: Arc<AtomicU64>,
        buffer_memory_usage: Arc<AtomicU64>,
        stream_memory_usage: Arc<AtomicU64>,
        staging_memory_usage: Arc<AtomicU64>,

        is_headless: bool,
        options: &Options,

        command_pool: Rc<CommandPool>,
    ) -> anyhow::Result<Self> {
        let (repeat, clamp_to_edge, texture_2d_array) = Self::create_texture_samplers(
            &device,
            &device.phy_device.limits,
            options.gl.global_texture_lod_bias,
        )?;

        let samplers: [Arc<Sampler>; SupportedSamplerTypes::COUNT] =
            [repeat.0, clamp_to_edge.0, texture_2d_array.0];

        let sampler_layouts: [Arc<DescriptorSetLayout>; SupportedSamplerTypes::COUNT] =
            [repeat.1, clamp_to_edge.1, texture_2d_array.1];

        let sampler_descr_pool = DeviceDescriptorPools::new(
            &device,
            SupportedSamplerTypes::COUNT as vk::DeviceSize,
            DescriptorPoolType::Sampler,
        )?;
        let (repeat_set, clamp_to_edge_set, texture_2d_set) =
            Self::create_new_sampler_descriptor_sets(
                &device,
                &sampler_layouts,
                &sampler_descr_pool,
                &samplers,
            )?;

        let vertex_uniform_descriptor_set_layout =
            Self::create_vertex_uniform_descriptor_set_layout(&device)?;
        let vertex_fragment_uniform_descriptor_set_layout =
            Self::create_vertex_fragment_uniform_descriptor_set_layout(&device)?;
        let vertex_shader_storage_descriptor_set_layout =
            Self::create_vertex_shader_storage_descriptor_set_layout(&device)?;

        let (standard_textured_descriptor_set_layout, standard_3d_textured_descriptor_set_layout) =
            Self::create_descriptor_set_layouts(&device)?;

        let mem = Memory::new(
            dbg.clone(),
            instance.clone(),
            device.clone(),
            vk_gpu.clone(),
            texture_memory_usage,
            buffer_memory_usage,
            stream_memory_usage,
            staging_memory_usage,
        );

        Ok(Self {
            mem: mem.clone(),
            mem_allocator: VulkanAllocator::new(
                device.clone(),
                mem.clone(),
                vk_gpu.limits.clone(),
                graphics_queue,
                vertex_shader_storage_descriptor_set_layout.clone(),
            )?,

            ash_vk: DeviceAsh {
                device: device.clone(),
            },

            vk_gpu,
            non_flushed_memory_ranges: Default::default(),
            samplers: Arc::new([
                (samplers[0].clone(), repeat_set),
                (samplers[1].clone(), clamp_to_edge_set),
                (samplers[2].clone(), texture_2d_set),
            ]),
            textures: Default::default(),
            shader_storages: Default::default(),

            buffer_objects: Default::default(),
            standard_texture_descr_pool: DeviceDescriptorPools::new(
                &device,
                1024,
                DescriptorPoolType::Image,
            )?,

            layouts: DescriptorLayouts {
                standard_textured_descriptor_set_layout,
                standard_2d_texture_array_descriptor_set_layout:
                    standard_3d_textured_descriptor_set_layout,
                vertex_uniform_descriptor_set_layout,
                vertex_fragment_uniform_descriptor_set_layout,
                vertex_shader_storage_descriptor_set_layout,
                samplers_layouts: Arc::new(sampler_layouts),
            },

            memory_command_buffer: Default::default(),

            command_pool,

            is_headless,
        })
    }

    /************************
     * MEMORY MANAGEMENT
     ************************/
    pub fn prepare_staging_mem_range_impl(
        &mut self,
        frame_resources: &mut FrameResources,
        buffer_mem: &Arc<DeviceMemoryBlock>,
        heap_data: &MemoryHeapQueueElement,
    ) {
        let upload_range =
            get_memory_range(frame_resources, buffer_mem, heap_data, &self.vk_gpu.limits);

        self.non_flushed_memory_ranges.push(upload_range);
    }

    pub fn prepare_staging_mem_range(
        &mut self,
        frame_resources: &mut FrameResources,
        block: &Arc<MemoryBlock>,
    ) {
        let block_mem = block.buffer_mem(frame_resources);
        self.prepare_staging_mem_range_impl(frame_resources, block_mem, &block.heap_data);
    }

    pub fn upload_and_free_staging_mem_block(
        &mut self,
        frame_resources: &mut FrameResources,
        block: Arc<MemoryBlock>,
    ) {
        self.prepare_staging_mem_range(frame_resources, &block);
    }

    pub fn upload_and_free_staging_image_mem_block(
        &mut self,
        frame_resources: &mut FrameResources,
        block: Arc<MemoryBlock>,
    ) {
        self.prepare_staging_mem_range(frame_resources, &block);
    }

    pub fn get_memory_command_buffer(
        &mut self,
        frame_resources: &mut FrameResources,
    ) -> anyhow::Result<&AutoCommandBuffer> {
        if let Some(ref memory_command_buffer) = self.memory_command_buffer {
            Ok(memory_command_buffer)
        } else {
            let command_buffer = self
                .command_pool
                .get_render_buffer(AutoCommandBufferType::Primary, &mut frame_resources.render)?;
            self.memory_command_buffer = Some(command_buffer);
            Ok(self.memory_command_buffer.as_ref().unwrap())
        }
    }

    pub fn memory_barrier(
        &mut self,
        frame_resources: &mut FrameResources,
        buffer: &Arc<Buffer>,
        offset: vk::DeviceSize,
        size: vk::DeviceSize,
        buffer_access_type: vk::AccessFlags,
        before_command: bool,
        source_stage_flags: vk::PipelineStageFlags,
    ) -> anyhow::Result<()> {
        let mem_command_buffer = self
            .get_memory_command_buffer(frame_resources)?
            .command_buffer;

        memory_barrier(
            frame_resources,
            &self.ash_vk.device,
            mem_command_buffer,
            buffer,
            offset,
            size,
            buffer_access_type,
            before_command,
            source_stage_flags,
        )
    }

    /************************
     * TEXTURES
     ************************/

    pub fn build_mipmaps(
        &mut self,
        frame_resources: &mut FrameResources,
        image: &Arc<Image>,
        image_format: vk::Format,
        width: usize,
        height: usize,
        depth: usize,
        mip_map_level_count: usize,
    ) -> anyhow::Result<()> {
        let mem_command_buffer = self
            .get_memory_command_buffer(frame_resources)?
            .command_buffer;

        build_mipmaps(
            frame_resources,
            &self.ash_vk.device,
            mem_command_buffer,
            image,
            image_format,
            width,
            height,
            depth,
            mip_map_level_count,
        )
    }

    pub fn create_texture_image(
        &mut self,
        frame_resources: &mut FrameResources,
        upload_data: VulkanDeviceInternalMemory,
        format: vk::Format,
        width: usize,
        height: usize,
        depth: usize,
        mip_map_level_count: usize,
    ) -> anyhow::Result<(Arc<Image>, MemoryImageBlock), ImageAllocationError> {
        let allocator_dummy = self.mem_allocator.clone();
        let mut allocator = allocator_dummy.lock();
        let buffer_block_res =
            allocator.get_and_remove_mem_block_image(upload_data.mem.as_mut_ptr());

        let staging_and_image_buffer = buffer_block_res.unwrap();

        let (new_image, image_mem) = (
            staging_and_image_buffer.img,
            staging_and_image_buffer.img_mem,
        );

        let staging_buffer = staging_and_image_buffer.staging;
        // if not yet flushed. flush it
        if let FlushType::None = staging_and_image_buffer.is_flushed {
            let block_mem = staging_buffer.buffer_mem(frame_resources);
            self.prepare_staging_mem_range_impl(
                frame_resources,
                block_mem,
                &staging_buffer.heap_data,
            );
        }
        if let FlushType::StagingBufferFlushed | FlushType::None =
            staging_and_image_buffer.is_flushed
        {
            let mem_command_buffer = self
                .get_memory_command_buffer(frame_resources)
                .map_err(|_| ImageAllocationError::MemoryRelatedOperationFailed)?
                .command_buffer;

            complete_texture(
                frame_resources,
                &self.ash_vk.device,
                mem_command_buffer,
                &staging_buffer,
                &new_image,
                format,
                width,
                height,
                depth,
                mip_map_level_count,
            )?;
        }

        Ok((new_image, image_mem))
    }

    pub fn create_texture_image_view(
        &self,
        frame_resources: &mut FrameResources,
        tex_image: &Arc<Image>,
        img_format: vk::Format,
        view_type: vk::ImageViewType,
        depth: usize,
        mip_map_level_count: usize,
    ) -> anyhow::Result<Arc<ImageView>> {
        Self::create_image_view(
            &self.ash_vk.device,
            frame_resources,
            tex_image,
            img_format,
            view_type,
            depth,
            mip_map_level_count,
            vk::ImageAspectFlags::COLOR,
        )
    }

    fn create_sampler_descriptor_set_layouts(
        device: &Arc<LogicalDevice>,
        sampler: &Arc<Sampler>,
    ) -> anyhow::Result<Arc<DescriptorSetLayout>> {
        let mut sampler_layout_binding = vk::DescriptorSetLayoutBinding::default();
        sampler_layout_binding.binding = 0;
        sampler_layout_binding.descriptor_count = 1;
        sampler_layout_binding.descriptor_type = vk::DescriptorType::SAMPLER;
        sampler_layout_binding.stage_flags = vk::ShaderStageFlags::FRAGMENT;

        let layout_bindings = [sampler_layout_binding];

        let layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&layout_bindings);

        let sampler_descr_set_layout = DescriptorSetLayout::new_with_immutable_sampler(
            device.clone(),
            &layout_info,
            sampler.clone(),
        )?;
        Ok(sampler_descr_set_layout)
    }

    pub fn create_texture_samplers_impl(
        device: &Arc<LogicalDevice>,
        max_sampler_anisotropy: u32,
        global_texture_lod_bias: f64,
        addr_mode_u: vk::SamplerAddressMode,
        addr_mode_v: vk::SamplerAddressMode,
        addr_mode_w: vk::SamplerAddressMode,
    ) -> anyhow::Result<(Arc<Sampler>, Arc<DescriptorSetLayout>)> {
        let sampler = Sampler::new(
            device,
            max_sampler_anisotropy,
            global_texture_lod_bias,
            addr_mode_u,
            addr_mode_v,
            addr_mode_w,
        )?;
        let descr_layout = Self::create_sampler_descriptor_set_layouts(device, &sampler)?;
        Ok((sampler, descr_layout))
    }

    pub fn create_image_view(
        device: &Arc<LogicalDevice>,
        frame_resources: &mut FrameResources,
        image: &Arc<Image>,
        format: vk::Format,
        view_type: vk::ImageViewType,
        depth: usize,
        mip_map_level_count: usize,
        aspect_mask: vk::ImageAspectFlags,
    ) -> anyhow::Result<Arc<ImageView>> {
        let mut view_create_info = vk::ImageViewCreateInfo::default();
        view_create_info.image = image.img(frame_resources);
        view_create_info.view_type = view_type;
        view_create_info.format = format;
        view_create_info.subresource_range.aspect_mask = aspect_mask;
        view_create_info.subresource_range.base_mip_level = 0;
        view_create_info.subresource_range.level_count = mip_map_level_count as u32;
        view_create_info.subresource_range.base_array_layer = 0;
        view_create_info.subresource_range.layer_count = depth as u32;

        Ok(ImageView::new(
            device.clone(),
            image.clone(),
            view_create_info,
        )?)
    }

    pub fn get_max_sample_count(limits: &Limits) -> vk::SampleCountFlags {
        if !(limits.max_multi_sample & vk::SampleCountFlags::TYPE_64).is_empty() {
            return vk::SampleCountFlags::TYPE_64;
        } else if !(limits.max_multi_sample & vk::SampleCountFlags::TYPE_32).is_empty() {
            return vk::SampleCountFlags::TYPE_32;
        } else if !(limits.max_multi_sample & vk::SampleCountFlags::TYPE_16).is_empty() {
            return vk::SampleCountFlags::TYPE_16;
        } else if !(limits.max_multi_sample & vk::SampleCountFlags::TYPE_8).is_empty() {
            return vk::SampleCountFlags::TYPE_8;
        } else if !(limits.max_multi_sample & vk::SampleCountFlags::TYPE_4).is_empty() {
            return vk::SampleCountFlags::TYPE_4;
        } else if !(limits.max_multi_sample & vk::SampleCountFlags::TYPE_2).is_empty() {
            return vk::SampleCountFlags::TYPE_2;
        }

        vk::SampleCountFlags::TYPE_1
    }

    pub fn get_sample_count(ms_count: u32, limits: &Limits) -> vk::SampleCountFlags {
        let max_sample_count: vk::SampleCountFlags = Self::get_max_sample_count(limits);
        if ms_count >= 64 && max_sample_count >= vk::SampleCountFlags::TYPE_64 {
            return vk::SampleCountFlags::TYPE_64;
        } else if ms_count >= 32 && max_sample_count >= vk::SampleCountFlags::TYPE_32 {
            return vk::SampleCountFlags::TYPE_32;
        } else if ms_count >= 16 && max_sample_count >= vk::SampleCountFlags::TYPE_16 {
            return vk::SampleCountFlags::TYPE_16;
        } else if ms_count >= 8 && max_sample_count >= vk::SampleCountFlags::TYPE_8 {
            return vk::SampleCountFlags::TYPE_8;
        } else if ms_count >= 4 && max_sample_count >= vk::SampleCountFlags::TYPE_4 {
            return vk::SampleCountFlags::TYPE_4;
        } else if ms_count >= 2 && max_sample_count >= vk::SampleCountFlags::TYPE_2 {
            return vk::SampleCountFlags::TYPE_2;
        }

        vk::SampleCountFlags::TYPE_1
    }

    pub fn image_barrier(
        &mut self,
        frame_resources: &mut FrameResources,
        image: &Arc<Image>,
        mip_map_base: usize,
        mip_map_count: usize,
        layer_base: usize,
        layer_count: usize,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) -> anyhow::Result<()> {
        let mem_command_buffer = self
            .get_memory_command_buffer(frame_resources)
            .map_err(|err| {
                anyhow!("image barrier failed while getting memory command buffer: {err}")
            })?
            .command_buffer;

        image_barrier(
            frame_resources,
            &self.ash_vk.device,
            mem_command_buffer,
            image,
            mip_map_base,
            mip_map_count,
            layer_base,
            layer_count,
            old_layout,
            new_layout,
        )
    }

    pub fn copy_buffer_to_image(
        &mut self,
        frame_resources: &mut FrameResources,
        buffer: &Arc<Buffer>,
        buffer_offset: vk::DeviceSize,
        image: &Arc<Image>,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        depth: usize,
    ) -> anyhow::Result<()> {
        let command_buffer = self
            .get_memory_command_buffer(frame_resources)
            .map_err(|err| {
                anyhow!("copy buffer to image failed during getting memory command buffer: {err}")
            })?
            .command_buffer;

        copy_buffer_to_image(
            frame_resources,
            &self.ash_vk.device,
            command_buffer,
            buffer,
            buffer_offset,
            image,
            x,
            y,
            width,
            height,
            depth,
        )
    }

    /************************
     * STREAM BUFFERS SETUP
     ************************/
    pub fn create_index_buffer(
        &mut self,
        frame_resources: &mut FrameResources,
        ptr_raw_data: *const c_void,
        data_size: usize,
    ) -> anyhow::Result<(Arc<Buffer>, Arc<DeviceMemoryBlock>), BufferAllocationError> {
        let buffer_data_size = data_size as vk::DeviceSize;

        let staging_buffer = self
            .mem_allocator
            .lock()
            .get_staging_buffer(ptr_raw_data, data_size as u64)?;

        let (vertex_buffer, vertex_buffer_memory) = self.mem.create_buffer(
            buffer_data_size,
            MemoryBlockType::Buffer,
            vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::INDEX_BUFFER,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;

        self.memory_barrier(
            frame_resources,
            &vertex_buffer,
            0,
            buffer_data_size,
            vk::AccessFlags::INDEX_READ,
            true,
            vk::PipelineStageFlags::VERTEX_INPUT,
        )
        .map_err(BufferAllocationError::MemoryRelatedOperationFailed)?;

        let buffer = staging_buffer.buffer(frame_resources).as_ref().unwrap();
        self.copy_buffer(
            frame_resources,
            buffer,
            &vertex_buffer,
            &[vk::BufferCopy {
                src_offset: staging_buffer.heap_data.offset_to_align as u64,
                dst_offset: 0,
                size: buffer_data_size,
            }],
        )
        .map_err(BufferAllocationError::MemoryRelatedOperationFailed)?;
        self.memory_barrier(
            frame_resources,
            &vertex_buffer,
            0,
            buffer_data_size,
            vk::AccessFlags::INDEX_READ,
            false,
            vk::PipelineStageFlags::VERTEX_INPUT,
        )
        .map_err(BufferAllocationError::MemoryRelatedOperationFailed)?;

        self.upload_and_free_staging_mem_block(frame_resources, staging_buffer);

        Ok((vertex_buffer, vertex_buffer_memory))
    }
    /************************
     * BUFFERS
     ************************/
    pub fn create_buffer_object(
        &mut self,
        frame_resources: &mut FrameResources,
        buffer_index: u128,
        upload_data: VulkanDeviceInternalMemory,
        buffer_data_size: vk::DeviceSize,
    ) -> anyhow::Result<(), BufferAllocationError> {
        let tmp_allocator = self.mem_allocator.clone();
        let mut allocator = tmp_allocator.lock();

        let staging_and_device_buffer = allocator
            .get_and_remove_mem_block(upload_data.mem.as_mut_ptr())
            .unwrap();

        let mem = staging_and_device_buffer.device;

        let staging_buffer = staging_and_device_buffer.staging;
        // if not yet flushed, flush it
        if let FlushType::None = staging_and_device_buffer.is_flushed {
            let block_mem = staging_buffer.buffer_mem(frame_resources);
            self.prepare_staging_mem_range_impl(
                frame_resources,
                block_mem,
                &staging_buffer.heap_data,
            );
        }
        if let FlushType::StagingBufferFlushed | FlushType::None =
            staging_and_device_buffer.is_flushed
        {
            let command_buffer = self
                .get_memory_command_buffer(frame_resources)
                .map_err(BufferAllocationError::MemoryRelatedOperationFailed)?
                .command_buffer;

            complete_buffer_object(
                frame_resources,
                &self.ash_vk.device,
                command_buffer,
                &staging_buffer,
                &mem,
                buffer_data_size,
            )?;
        }

        let vertex_buffer = mem.buffer(frame_resources).clone().unwrap();
        let buffer_offset = mem.heap_data.offset_to_align;
        self.buffer_objects.insert(
            buffer_index,
            BufferObject {
                buffer_object: BufferObjectMem { mem },
                cur_buffer: vertex_buffer,
                cur_buffer_offset: buffer_offset,
            },
        );

        Ok(())
    }

    pub fn delete_buffer_object(&mut self, buffer_index: u128) {
        self.buffer_objects.remove(&buffer_index).unwrap();
    }

    pub fn copy_buffer(
        &mut self,
        frame_resources: &mut FrameResources,
        src_buffer: &Arc<Buffer>,
        dst_buffer: &Arc<Buffer>,
        copy_regions: &[vk::BufferCopy],
    ) -> anyhow::Result<()> {
        let command_buffer = self
            .get_memory_command_buffer(frame_resources)
            .map_err(|err| {
                anyhow!("copy buffer failed while getting memory command buffer: {err}")
            })?
            .command_buffer;

        copy_buffer(
            frame_resources,
            &self.ash_vk.device,
            command_buffer,
            src_buffer,
            dst_buffer,
            copy_regions,
        )
    }

    pub fn create_shader_storage_object(
        &mut self,
        frame_resources: &mut FrameResources,
        shader_storage_index: u128,
        upload_data: VulkanDeviceInternalMemory,
        buffer_data_size: vk::DeviceSize,
    ) -> anyhow::Result<(), BufferAllocationError> {
        let tmp_allocator = self.mem_allocator.clone();
        let mut allocator = tmp_allocator.lock();

        let staging_and_device_buffer = allocator
            .get_and_remove_shader_storage_mem_block(upload_data.mem.as_mut_ptr())
            .unwrap();

        let mem = staging_and_device_buffer.base.device;
        let mut descriptor = staging_and_device_buffer.descriptor_sets;

        let staging_buffer = staging_and_device_buffer.base.staging;
        // if not yet flushed, flush it
        if let FlushType::None = staging_and_device_buffer.base.is_flushed {
            let block_mem = staging_buffer.buffer_mem(frame_resources);
            self.prepare_staging_mem_range_impl(
                frame_resources,
                block_mem,
                &staging_buffer.heap_data,
            );
        }
        if let FlushType::StagingBufferFlushed | FlushType::None =
            staging_and_device_buffer.base.is_flushed
        {
            let command_buffer = self
                .get_memory_command_buffer(frame_resources)
                .map_err(BufferAllocationError::MemoryRelatedOperationFailed)?
                .command_buffer;

            complete_shader_storage_object(
                frame_resources,
                &self.ash_vk.device,
                command_buffer,
                &staging_buffer,
                &mem,
                buffer_data_size,
            )?;

            let mut descriptors = VulkanAllocator::create_shader_storage_descriptor_sets(
                &self.ash_vk.device,
                &allocator.shader_storage_descr_pools,
                &self.layouts.vertex_shader_storage_descriptor_set_layout,
                1,
                &mem.buffer(frame_resources).clone().unwrap(),
                mem.heap_data.allocation_size,
                mem.heap_data.offset_to_align as vk::DeviceSize,
            )
            .map_err(BufferAllocationError::MemoryRelatedOperationFailed)?;
            descriptor = Some(descriptors.remove(0));
        }

        let vertex_buffer = mem.buffer(frame_resources).clone().unwrap();
        let buffer_offset = mem.heap_data.offset_to_align;
        self.shader_storages.insert(
            shader_storage_index,
            ShaderStorage {
                buffer: BufferObject {
                    buffer_object: BufferObjectMem { mem },
                    cur_buffer: vertex_buffer,
                    cur_buffer_offset: buffer_offset,
                },
                descriptor: descriptor.unwrap(),
            },
        );

        Ok(())
    }

    pub fn delete_shader_storage(&mut self, index: u128) {
        self.shader_storages.remove(&index).unwrap();
    }

    pub fn create_new_textured_descriptor_sets_impl(
        device: &Arc<LogicalDevice>,
        layouts: &DescriptorLayouts,
        standard_texture_descr_pool: &Arc<parking_lot::Mutex<DeviceDescriptorPools>>,
        img_view: &Arc<ImageView>,
    ) -> anyhow::Result<Arc<DescriptorSets>, ImageAllocationError> {
        let res = VulkanAllocator::get_descriptor_pool_for_alloc(
            device,
            standard_texture_descr_pool,
            1,
            &layouts.standard_textured_descriptor_set_layout,
        );
        if res.is_err() {
            return Err(ImageAllocationError::MemoryRelatedOperationFailed);
        }
        let descr_set = res.unwrap().remove(0);

        descr_set.assign_texture(img_view);

        Ok(descr_set)
    }

    pub fn create_new_textured_standard_descriptor_sets(
        device: &Arc<LogicalDevice>,
        layouts: &DescriptorLayouts,
        standard_texture_descr_pool: &Arc<parking_lot::Mutex<DeviceDescriptorPools>>,
        img_view: &Arc<ImageView>,
    ) -> anyhow::Result<Arc<DescriptorSets>, ImageAllocationError> {
        let set = Self::create_new_textured_descriptor_sets_impl(
            device,
            layouts,
            standard_texture_descr_pool,
            img_view,
        )?;
        Ok(set)
    }

    pub fn create_new_3d_textured_standard_descriptor_sets(
        &self,
        img_3d_view: &Arc<ImageView>,
    ) -> anyhow::Result<Arc<DescriptorSets>, ImageAllocationError> {
        let res = VulkanAllocator::get_descriptor_pool_for_alloc(
            &self.ash_vk.device,
            &self.standard_texture_descr_pool,
            1,
            &self.layouts.standard_2d_texture_array_descriptor_set_layout,
        );
        if res.is_err() {
            return Err(ImageAllocationError::MemoryRelatedOperationFailed);
        }

        let descr_set = res.unwrap().remove(0);

        descr_set.assign_texture(img_3d_view);

        Ok(descr_set)
    }

    pub fn create_new_sampler_descriptor_set(
        device: &Arc<LogicalDevice>,
        layouts: &[Arc<DescriptorSetLayout>; SupportedSamplerTypes::COUNT],
        sampler_descr_pool: &Arc<parking_lot::Mutex<DeviceDescriptorPools>>,
        samplers: &[Arc<Sampler>; SupportedSamplerTypes::COUNT],
        address_mode: SupportedSamplerTypes,
    ) -> anyhow::Result<Arc<DescriptorSets>, ImageAllocationError> {
        let res = VulkanAllocator::get_descriptor_pool_for_alloc(
            device,
            sampler_descr_pool,
            1,
            &layouts[address_mode as usize],
        );
        if res.is_err() {
            return Err(ImageAllocationError::MemoryRelatedOperationFailed);
        }
        let descr_set = res.unwrap().remove(0);

        descr_set.assign_sampler(&samplers[address_mode as usize]);

        Ok(descr_set)
    }

    pub fn create_new_sampler_descriptor_sets(
        device: &Arc<LogicalDevice>,
        layouts: &[Arc<DescriptorSetLayout>; SupportedSamplerTypes::COUNT],
        sampler_descr_pool: &Arc<parking_lot::Mutex<DeviceDescriptorPools>>,
        samplers: &[Arc<Sampler>; SupportedSamplerTypes::COUNT],
    ) -> anyhow::Result<DescriptorSetsRes, ImageAllocationError> {
        Ok((
            Self::create_new_sampler_descriptor_set(
                device,
                layouts,
                sampler_descr_pool,
                samplers,
                SupportedSamplerTypes::Repeat,
            )?,
            Self::create_new_sampler_descriptor_set(
                device,
                layouts,
                sampler_descr_pool,
                samplers,
                SupportedSamplerTypes::ClampToEdge,
            )?,
            Self::create_new_sampler_descriptor_set(
                device,
                layouts,
                sampler_descr_pool,
                samplers,
                SupportedSamplerTypes::Texture2dArray,
            )?,
        ))
    }
}
