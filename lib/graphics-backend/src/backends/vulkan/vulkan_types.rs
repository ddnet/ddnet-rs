use std::sync::{atomic::AtomicUsize, Arc};

use arc_swap::ArcSwap;
use ash::vk;
use base::join_thread::JoinThread;
use crossbeam::channel::Sender;
use graphics_backend_traits::plugin::SamplerAddressMode;
use graphics_types::commands::StreamDataMax;
use hiarc::Hiarc;
use num_derive::FromPrimitive;
use strum::EnumCount;

use crate::backends::vulkan::render_group::{ColorWriteMaskType, StencilOpType};

use super::{
    buffer::Buffer,
    compiler::compiler::ShaderCompiler,
    descriptor_pool::DescriptorPool,
    descriptor_set::{DescriptorSet, DescriptorSets},
    frame::FrameCanvasIndex,
    image::Image,
    image_view::ImageView,
    logical_device::LogicalDevice,
    memory::{MemoryBlock, MemoryImageBlock},
    pipeline_cache::PipelineCacheInner,
    pipeline_manager::PipelineCreationAttributes,
    pipelines::Pipelines,
    render_fill_manager::RenderCommandExecuteBuffer,
    render_pass::CanvasSetup,
    vulkan_allocator::VulkanAllocator,
};

#[derive(Debug, Hiarc, Copy, Clone, PartialEq)]
pub enum MemoryBlockType {
    Texture = 0,
    Buffer,
    Stream,
    Staging,
}

/************************
 * STRUCT DEFINITIONS
 ************************/
#[derive(Debug, Hiarc, Clone, Copy)]
pub enum DescriptorPoolType {
    CombineImgageAndSampler,
    Image,
    Sampler,
    Uniform,
    ShaderStorage,
}

#[derive(Debug, Clone, Hiarc)]
pub struct DeviceDescriptorPools {
    pub pools: Vec<Arc<DescriptorPool>>,
    pub default_alloc_size: vk::DeviceSize,
    pub pool_ty: DescriptorPoolType,
}

impl DeviceDescriptorPools {
    pub fn new(
        device: &Arc<LogicalDevice>,
        default_alloc_size: vk::DeviceSize,
        pool_ty: DescriptorPoolType,
    ) -> anyhow::Result<Arc<parking_lot::Mutex<Self>>> {
        let mut pool = DeviceDescriptorPools {
            pools: Default::default(),
            default_alloc_size,
            pool_ty,
        };
        VulkanAllocator::allocate_descriptor_pool(
            device,
            &mut pool,
            StreamDataMax::MaxTextures as usize,
        )?;
        Ok(Arc::new(parking_lot::Mutex::new(pool)))
    }
}

#[derive(Debug, Hiarc)]
pub enum TextureData {
    Tex2D {
        img: Arc<Image>,
        // RAII
        _img_mem: MemoryImageBlock,
        // RAII
        _img_view: Arc<ImageView>,

        vk_standard_textured_descr_set: Arc<DescriptorSets>,
    },
    Tex3D {
        // RAII
        _img_3d: Arc<Image>,
        // RAII
        _img_3d_mem: MemoryImageBlock,
        // RAII
        _img_3d_view: Arc<ImageView>,

        vk_standard_3d_textured_descr_set: Arc<DescriptorSets>,
    },
}

impl TextureData {
    pub fn unwrap_3d_descr(&self) -> &Arc<DescriptorSets> {
        match self {
            TextureData::Tex2D { .. } => panic!("not a 3d texture"),
            TextureData::Tex3D {
                vk_standard_3d_textured_descr_set,
                ..
            } => vk_standard_3d_textured_descr_set,
        }
    }

    pub fn unwrap_2d_descr(&self) -> &Arc<DescriptorSets> {
        match self {
            TextureData::Tex2D {
                vk_standard_textured_descr_set,
                ..
            } => vk_standard_textured_descr_set,
            TextureData::Tex3D { .. } => panic!("not a 2d texture"),
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct TextureObject {
    pub data: TextureData,

    pub mip_map_count: u32,
}

#[derive(Debug, Hiarc)]
pub struct BufferObjectMem {
    pub mem: Arc<MemoryBlock>,
}

#[derive(Debug, Hiarc)]
pub struct BufferObject {
    pub buffer_object: BufferObjectMem,

    pub cur_buffer: Arc<Buffer>,
    pub cur_buffer_offset: usize,
}

#[derive(Debug, Hiarc)]
pub struct ShaderStorage {
    pub buffer: BufferObject,

    pub descriptor: Arc<DescriptorSets>,
}

#[derive(Debug, Hiarc)]
pub struct StreamedUniformBuffer {
    pub uniform_sets: [Arc<DescriptorSet>; 2],
}

#[derive(Debug)]
pub struct ShaderModule {
    pub vert_shader_module: vk::ShaderModule,
    pub frag_shader_module: vk::ShaderModule,

    vk_device: Arc<LogicalDevice>,
}

impl ShaderModule {
    pub fn new(
        vert_shader_module: vk::ShaderModule,
        frag_shader_module: vk::ShaderModule,
        vk_device: &Arc<LogicalDevice>,
    ) -> Self {
        Self {
            vert_shader_module,
            frag_shader_module,
            vk_device: vk_device.clone(),
        }
    }
}

impl Drop for ShaderModule {
    fn drop(&mut self) {
        if self.vert_shader_module != vk::ShaderModule::null() {
            unsafe {
                self.vk_device
                    .device
                    .destroy_shader_module(self.vert_shader_module, None);
            }
        }

        if self.frag_shader_module != vk::ShaderModule::null() {
            unsafe {
                self.vk_device
                    .device
                    .destroy_shader_module(self.frag_shader_module, None);
            }
        }
    }
}

#[derive(FromPrimitive, Copy, Clone, EnumCount)]
#[repr(u32)]
pub enum SupportedAddressModes {
    Repeat = 0,
    ClampEdges = 1,
}

#[derive(Debug, Hiarc, FromPrimitive, Copy, Clone, PartialEq, EnumCount)]
#[repr(u32)]
pub enum SupportedBlendModes {
    Alpha = 0,
    None = 1,
    Additive = 2,
}

#[derive(Debug, Hiarc, FromPrimitive, Copy, Clone, PartialEq, EnumCount)]
#[repr(u32)]
pub enum CanvasClipModes {
    None = 0,
    DynamicScissorAndViewport = 1,
}

const MAX_TEXTURE_MODES: usize = 2;

#[derive(Debug, Hiarc, Default, Copy, Clone, PartialEq)]
pub enum RenderPassSubType {
    #[default]
    Single = 0,
    // switch around 2 framebuffers to use each other as
    // input attachments
    Switching1,
    Switching2,
}

#[derive(Debug, Hiarc, Copy, Clone, PartialEq)]
pub enum RenderPassType {
    Normal(RenderPassSubType),
    MultiSampling,
}

impl Default for RenderPassType {
    fn default() -> Self {
        Self::Normal(Default::default())
    }
}

#[derive(Debug, Hiarc, Clone)]
pub struct PipelineCreationAttributesEx {
    pub address_mode_index: usize,
    pub is_textured: bool,
}

#[derive(Debug, Hiarc, Clone)]
pub struct PipelineCreationProps {
    pub attr: PipelineCreationAttributes,
    pub attr_ex: PipelineCreationAttributesEx,
}

#[derive(Debug, Hiarc, Clone)]
pub struct PipelineCreationOneByOne {
    pub multi_sampling_count: u32,
    pub device: Arc<LogicalDevice>,
    pub shader_compiler: Arc<ShaderCompiler>,
    #[hiarc_skip_unsafe]
    pub swapchain_extent: vk::Extent2D,
    #[hiarc_skip_unsafe]
    pub render_pass: vk::RenderPass,

    pub pipeline_cache: Option<Arc<PipelineCacheInner>>,
}

#[derive(Debug, Hiarc, Default)]
pub enum PipelineContainerItem {
    Normal {
        pipeline: Pipelines,
    },
    MaybeUninit {
        pipeline_and_layout: ArcSwap<Option<Pipelines>>,

        pipe_create_mutex: Arc<parking_lot::Mutex<()>>,

        creation_props: Box<PipelineCreationProps>,
        creation_data: PipelineCreationOneByOne,
    },
    #[default]
    None,
}

#[derive(Debug, Hiarc, Clone)]
pub enum PipelineContainerCreateMode {
    AtOnce,
    OneByOne(PipelineCreationOneByOne),
}

pub type PipelinesSampler = [PipelineContainerItem; SupportedSamplerTypes::COUNT];
pub type PipelinesColorMasks = [PipelinesSampler; ColorWriteMaskType::COUNT];

#[derive(Debug, Hiarc)]
pub struct PipelineContainer {
    // 3 blend modes - 2 viewport & scissor modes - 2 texture modes - 4 stencil modes - 3 color mask types - 3 sampler modes
    pub pipelines: Box<
        [[[[PipelinesColorMasks; StencilOpType::COUNT]; MAX_TEXTURE_MODES]; CanvasClipModes::COUNT];
            SupportedBlendModes::COUNT],
    >,

    pub(crate) mode: PipelineContainerCreateMode,
}

impl PipelineContainer {
    pub fn new(mode: PipelineContainerCreateMode) -> Self {
        Self {
            pipelines: Default::default(),
            mode,
        }
    }
}

#[derive(Debug, FromPrimitive, Copy, Clone, PartialEq, EnumCount)]
#[repr(u32)]
pub enum SupportedSamplerTypes {
    Repeat = 0,
    ClampToEdge,
    Texture2dArray,
}

impl From<SupportedSamplerTypes> for SamplerAddressMode {
    fn from(val: SupportedSamplerTypes) -> Self {
        match val {
            SupportedSamplerTypes::Repeat => SamplerAddressMode::Repeat,
            SupportedSamplerTypes::ClampToEdge => SamplerAddressMode::ClampToEdge,
            SupportedSamplerTypes::Texture2dArray => SamplerAddressMode::Texture2dArray,
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct SwapChainImageBase {
    pub image: Arc<Image>,
    pub img_mem: MemoryImageBlock,
    pub img_view: Arc<ImageView>,
}

#[derive(Debug, Hiarc)]
pub struct SwapChainImageFull {
    pub base: SwapChainImageBase,

    pub texture_descr_sets: Arc<DescriptorSets>,
}

#[derive(Debug, Hiarc, Default)]
pub struct ThreadCommandGroup {
    pub render_pass: RenderPassType,

    pub render_pass_index: usize,
    pub canvas_index: FrameCanvasIndex,

    pub cur_frame_index: u32,

    pub in_order_id: usize,

    pub cmds: Vec<RenderCommandExecuteBuffer>,
}

#[derive(Debug, Hiarc)]
pub enum RenderThreadEvent {
    ClearFrame(u32),
    ClearFrames,
    Render((ThreadCommandGroup, Arc<CanvasSetup>)),
    Sync(Sender<()>),
}

#[derive(Debug, Hiarc)]
pub struct RenderThread {
    pub sender: Sender<RenderThreadEvent>,
    pub events: Arc<AtomicUsize>,
    pub _thread: JoinThread<()>,
}
