use std::{rc::Rc, sync::Arc};

use hiarc::Hiarc;
use pool::{arc::PoolArc, datatypes::PoolVec, pool::Pool};

use crate::backends::vulkan::{fence::Fence, semaphore::Semaphore};

use super::{
    buffer::Buffer, command_buffer::CommandBuffers, descriptor_set::DescriptorSets, image::Image,
    image_view::ImageView, memory::MemoryBlock, memory_block::DeviceMemoryBlock,
    render_pass::CanvasSetup, sampler::Sampler, stream_memory_pool::StreamMemoryBlock,
    sub_render_pass::SubRenderPass, vulkan_types::StreamedUniformBuffer,
};

#[derive(Debug, Hiarc)]
pub struct RenderThreadFrameResources {
    pub command_buffers: PoolVec<Rc<CommandBuffers>>,
}

impl RenderThreadFrameResources {
    pub fn new(pool: Option<&RenderThreadFrameResourcesPool>) -> Self {
        if let Some(pool) = pool {
            Self {
                command_buffers: pool.command_buffers.new(),
            }
        } else {
            Self {
                command_buffers: PoolVec::new_without_pool(),
            }
        }
    }
}

/// resources that a single frame "holds",
/// in a sense that they are "in-use" and shall
/// not be deallocated, before the frame ends
#[derive(Debug, Hiarc)]
pub struct FrameResources {
    pub device_memory: PoolVec<Arc<DeviceMemoryBlock>>,
    pub buffers: PoolVec<Arc<Buffer>>,
    pub images: PoolVec<Arc<Image>>,
    pub image_views: PoolVec<Arc<ImageView>>,
    pub samplers: PoolVec<Arc<Sampler>>,
    pub descriptor_sets: PoolVec<Arc<DescriptorSets>>,

    pub memory_blocks: PoolVec<Arc<MemoryBlock>>,

    pub semaphores: PoolVec<Arc<Semaphore>>,
    pub fences: PoolVec<Arc<Fence>>,

    pub stream_vertex_buffers: PoolVec<PoolArc<StreamMemoryBlock<()>>>,
    pub stream_uniform_buffers: PoolVec<PoolArc<StreamMemoryBlock<StreamedUniformBuffer>>>,

    pub render_setups: PoolVec<Arc<CanvasSetup>>,

    pub render: RenderThreadFrameResources,

    pub sub_render_passes: PoolVec<Arc<Vec<SubRenderPass>>>,
}

impl FrameResources {
    pub fn new(pool: Option<&FrameResourcesPool>) -> Self {
        if let Some(pool) = pool {
            Self {
                device_memory: pool.device_memory.new(),
                buffers: pool.buffers.new(),
                images: pool.images.new(),
                image_views: pool.image_views.new(),
                samplers: pool.sampler.new(),
                descriptor_sets: pool.descriptor_sets.new(),
                memory_blocks: pool.memory_blocks.new(),
                semaphores: pool.semaphores.new(),
                fences: pool.fences.new(),
                stream_vertex_buffers: pool.stream_vertex_buffers.new(),
                stream_uniform_buffers: pool.stream_uniform_buffers.new(),
                render_setups: pool.render_setups.new(),
                render: RenderThreadFrameResources::new(Some(&pool.render)),
                sub_render_passes: pool.sub_render_passes.new(),
            }
        } else {
            Self {
                device_memory: PoolVec::new_without_pool(),
                buffers: PoolVec::new_without_pool(),
                images: PoolVec::new_without_pool(),
                image_views: PoolVec::new_without_pool(),
                samplers: PoolVec::new_without_pool(),
                descriptor_sets: PoolVec::new_without_pool(),
                memory_blocks: PoolVec::new_without_pool(),
                semaphores: PoolVec::new_without_pool(),
                fences: PoolVec::new_without_pool(),
                stream_vertex_buffers: PoolVec::new_without_pool(),
                stream_uniform_buffers: PoolVec::new_without_pool(),
                render_setups: PoolVec::new_without_pool(),
                render: RenderThreadFrameResources::new(None),
                sub_render_passes: PoolVec::new_without_pool(),
            }
        }
    }

    pub fn take(&mut self, pool: Option<&FrameResourcesPool>) -> Self {
        let mut res = FrameResources::new(pool);
        std::mem::swap(&mut res, self);
        res
    }
}

#[derive(Debug, Hiarc, Clone)]
pub struct RenderThreadFrameResourcesPool {
    pub command_buffers: Pool<Vec<Rc<CommandBuffers>>>,
}

impl Default for RenderThreadFrameResourcesPool {
    fn default() -> Self {
        Self {
            command_buffers: Pool::with_capacity(64),
        }
    }
}

impl RenderThreadFrameResourcesPool {
    pub fn new() -> Self {
        Default::default()
    }
}

/// resources that a single frame "holds",
/// in a sense that they are "in-use" and shall
/// not be deallocated, before the frame ends
#[derive(Debug, Hiarc)]
pub struct FrameResourcesPool {
    pub device_memory: Pool<Vec<Arc<DeviceMemoryBlock>>>,
    pub buffers: Pool<Vec<Arc<Buffer>>>,
    pub images: Pool<Vec<Arc<Image>>>,
    pub image_views: Pool<Vec<Arc<ImageView>>>,
    pub sampler: Pool<Vec<Arc<Sampler>>>,
    pub descriptor_sets: Pool<Vec<Arc<DescriptorSets>>>,

    pub memory_blocks: Pool<Vec<Arc<MemoryBlock>>>,

    pub semaphores: Pool<Vec<Arc<Semaphore>>>,
    pub fences: Pool<Vec<Arc<Fence>>>,

    pub stream_vertex_buffers: Pool<Vec<PoolArc<StreamMemoryBlock<()>>>>,
    pub stream_uniform_buffers: Pool<Vec<PoolArc<StreamMemoryBlock<StreamedUniformBuffer>>>>,

    pub render_setups: Pool<Vec<Arc<CanvasSetup>>>,

    pub render: RenderThreadFrameResourcesPool,

    pub sub_render_passes: Pool<Vec<Arc<Vec<SubRenderPass>>>>,
}

impl Default for FrameResourcesPool {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameResourcesPool {
    pub fn new() -> Self {
        Self {
            device_memory: Pool::with_capacity(64),
            buffers: Pool::with_capacity(64),
            images: Pool::with_capacity(64),
            image_views: Pool::with_capacity(64),
            sampler: Pool::with_capacity(4),
            descriptor_sets: Pool::with_capacity(64),
            memory_blocks: Pool::with_capacity(64),

            semaphores: Pool::with_capacity(8),
            fences: Pool::with_capacity(8),

            stream_vertex_buffers: Pool::with_capacity(8),
            stream_uniform_buffers: Pool::with_capacity(8),

            render_setups: Pool::with_capacity(4),

            render: RenderThreadFrameResourcesPool::new(),

            sub_render_passes: Pool::with_capacity(32),
        }
    }
}
