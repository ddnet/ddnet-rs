use std::{fmt::Debug, num::NonZeroUsize};

use bitflags::bitflags;
use graphics_types::rendering::{State, StateTexture, StateTexture2dArray};
use hiarc::Hiarc;
use pool::mt_datatypes::PoolVec;

#[derive(Debug)]
pub enum BackendVertexFormat {
    /// float
    Vec4,
    Vec3,
    Vec2,
    /// unsigned byte - normalized float
    UbVec4Normalized,
    // unsigned byte
    UbVec2,
    // unsigned byte
    UbVec4,
    // unsigned short
    UsVec2,
}

#[derive(Debug)]

pub struct BackendVertexInputAttributeDescription {
    pub location: u32,
    pub binding: u32,
    pub format: BackendVertexFormat,
    pub offset: u32,
}

/// for every address mode, there is a corresponding resource descriptor ([`BackendResourceDescription`])
#[derive(Debug, Clone, Copy)]
pub enum SamplerAddressMode {
    /// repeat on uv
    Repeat = 0,
    /// clamp uv
    ClampToEdge,
    /// clamp uv, mirror repeat r
    Texture2DArray,
}

/// the resource descriptors are pre-defined sets of descriptors that
/// can bind various types of resources.
///
/// Every descriptor uses at least one set and one binding (for wgsl that is `@group @binding`, glsl: `set = , binding = `).
/// Note: every resource uses a custom amount of sets & bindings, see the individual enum variants for details.
/// Using one of the descriptors consumes the said amount of sets, so following sets should add the previous amount of sets as offset in the shader
///
/// # Example
///
/// ```no_run
/// // create the pipeline layouts
/// set_layouts = vec![BackendResourceDescription::Fragment2DTexture, BackendResourceDescription::VertexUniformBuffer];
/// ```
///
/// in wgsl:
/// ```wgsl
/// // no resource was used yet, our texture gets group 0 & group 1
/// @group(0) @binding(0) var texture: texture_2d<f32>;
/// @group(1) @binding(0) var sampler: sampler;
/// // group offset of 2, because the texture resource descriptor was used earlier in the vec![] construction
/// @group(2) @binding(0) var<uniform> my_uniform_data: BufferObjectStruct;
/// ```
#[derive(Debug, Clone, Copy)]
pub enum BackendResourceDescription {
    /// normal 2d texture
    /// sets: 2 (slot 1: texture, slot 2: sampler)
    /// bindings: 1
    Fragment2DTexture,
    /// normal 2d texture array
    /// sets: 2 (slot 1: texture, slot 2: sampler)
    /// bindings: 1
    Fragment2DArrayTexture,
    /// a uniform buffer for vertex shader
    /// sets: 1
    /// bindings: 1
    VertexUniformBuffer,
    /// a uniform buffer for vertex and fragment shader
    /// sets: 1
    /// bindings: 1
    VertexFragmentUniformBuffer,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct BackendShaderStage: u32 {
        const VERTEX = 0b1;
        const FRAGMENT = 0b10;
    }
}

#[derive(Debug, Clone, Copy)]

pub struct BackendPushConstant {
    pub stage_flags: BackendShaderStage,
    pub offset: u32,
    pub size: u32,
}

pub type BackendDeviceSize = u64;

#[derive(Debug)]
pub struct BackendPipelineLayout {
    pub vertex_attributes: Vec<BackendVertexInputAttributeDescription>,
    pub descriptor_layouts: Vec<BackendResourceDescription>,
    pub push_constants: Vec<BackendPushConstant>,
    pub stride: BackendDeviceSize,
    pub geometry_is_line: bool,
}

pub trait BackendRenderExecuteInterface {
    fn get_address_mode_index(&self, state: &State) -> u64;

    fn estimated_render_calls(&mut self, estimated_render_call_count: u64);

    fn set_texture(&mut self, index: u64, texture_index: u128, address_mode_index: u64);

    /// the color attachment of the previous render pass
    fn set_color_attachment_as_texture(&mut self, index: u64, address_mode_index: u64);

    /// the color attachment of an offscreen buffer
    fn set_offscreen_attachment_as_texture(
        &mut self,
        offscreen_id: u128,
        index: u64,
        address_mode_index: u64,
    );

    fn set_texture_3d(&mut self, index: u64, texture_index: u128);

    fn uses_stream_vertex_buffer(&mut self, offset: u64);

    fn uses_stream_uniform_buffer(
        &mut self,
        uniform_index: u64,
        stream_instance_index: u64,
        uniform_descriptor_index: u64,
    );

    fn uses_index_buffer(&mut self);

    fn exec_buffer_fill_dynamic_states(&mut self, state: &State);

    fn set_vertex_buffer(&mut self, buffer_object_index: u128);
    fn set_vertex_buffer_with_offset(&mut self, buffer_object_index: u128, offset: usize);
}

pub enum SubRenderPassAttributes {
    StandardPipeline,
    StandardLinePipeline,
    StandardBlurPipeline,
    Standard3dPipeline,
    BlurPipeline,
    PrimExPipeline,
    PrimExRotationlessPipeline,
    SpriteMultiPipeline,
    /// pipeline name
    Additional(u64),
}

#[derive(Debug)]
pub struct BackendExtent2D {
    pub width: u32,
    pub height: u32,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct BackendImageAspectFlags: u32 {
        const COLOR = 0b1;
        const STENCIL = 0b10;
    }
}

#[derive(Debug)]
pub enum BackendClearValue {
    Color([f32; 4]),
    Stencil(u32),
}

#[derive(Debug)]
pub struct BackendClearAttachment {
    pub aspect_mask: BackendImageAspectFlags,
    pub color_attachment: u32,
    pub clear_value: BackendClearValue,
}

#[derive(Debug)]
pub struct BackendOffset2D {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug)]
pub struct BackendRect2D {
    pub offset: BackendOffset2D,
    pub extent: BackendExtent2D,
}

#[derive(Debug)]
pub struct BackendClearRect {
    pub rect: BackendRect2D,
    pub base_array_layer: u32,
    pub layer_count: u32,
}

pub trait BackendRenderInterface {
    fn get_state_matrix(&self, state: &State, matrix: &mut [f32; 4 * 2]);

    fn get_address_mode_index(&self, state: &State) -> u64;

    fn bind_pipeline(
        &mut self,
        state: &State,
        texture_index: &StateTexture,
        pipe_name: SubRenderPassAttributes,
    );

    fn bind_pipeline_2d_array_texture(
        &mut self,
        state: &State,
        texture_index: &StateTexture2dArray,
        pipe_name: SubRenderPassAttributes,
    );

    fn bind_vertex_buffer(&self);

    fn bind_index_buffer(&self, index_offset: BackendDeviceSize);

    fn bind_texture_descriptor_sets(&self, first_set: u32, descriptor_index: u64);

    fn bind_uniform_descriptor_sets(&self, first_set: u32, descriptor_index: u64);

    fn push_constants(&self, stage_flags: BackendShaderStage, offset: u32, constants: &[u8]);

    fn draw_indexed(
        &self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    );

    fn draw(&self, vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance: u32);

    fn is_textured(&self) -> bool;

    fn viewport_size(&self) -> BackendExtent2D;

    fn clear_attachments(&self, attachments: &[BackendClearAttachment], rects: &[BackendClearRect]);
}

#[derive(Debug, Hiarc)]
pub enum GraphicsBufferObjectAccess {
    Quad {
        /// How many quads are skipped in the index buffer before rendering.
        quad_offset: usize,
        /// How many quads are expected to be drawn
        quad_count: usize,
        /// A skip in the vertex buffer by byte offset
        buffer_byte_offset: usize,
        /// The size of a single vertex for this quad
        vertex_byte_size: usize,
        /// The alignment of the vertex buffer's vertex data type.
        alignment: NonZeroUsize,
    },
}

#[derive(Debug)]
pub struct GraphicsBufferObjectAccessAndRewrite<'a> {
    pub buffer_object_index: &'a mut u128,
    pub accesses: PoolVec<GraphicsBufferObjectAccess>,
}

#[derive(Debug)]
pub struct GraphicsUniformAccessAndRewrite<'a> {
    pub index: &'a mut usize,
    /// Number of elements in the uniform instance
    pub instance_count: usize,
    /// Byte size of a single uniform instance
    pub single_instance_byte_size: usize,
}

#[derive(Debug)]
pub struct GraphicsObjectRewriteFunc<'a> {
    pub textures: &'a mut [&'a mut StateTexture],
    pub textures_2d_array: &'a mut [&'a mut StateTexture2dArray],
    pub buffer_objects: &'a mut [GraphicsBufferObjectAccessAndRewrite<'a>],
    pub uniform_instances: &'a mut [GraphicsUniformAccessAndRewrite<'a>],
}

pub trait BackendCustomPipeline: Debug + Sync + Send {
    /// the name to which commands are related to
    /// it's recommanded to do it in a syntax like this:
    /// "<mod>::<name>"
    /// Note: don't use "intern" or "internal" for <mod>, reserved names
    fn pipe_name(&self) -> String;

    /// number of custom pipelines this instance should create
    fn pipeline_count(&self) -> u64;

    /// pipeline indices in order
    /// `name_of_first` is the name of the first custom pipeline
    /// the name of the last pipeline is `name_of_first` + `self.pipeline_count()` - 1
    fn pipeline_names(&mut self, name_of_first: u64);

    /// pipeline layout for a named pipeline
    fn pipe_layout_of(&self, name: u64, is_textured: bool) -> BackendPipelineLayout;

    /// names of the shaders used by a named pipeline
    /// Value of `None` would indicate that this pipeline will not be loaded at all
    /// (e.g. if your pipeline only supports textured mode)
    fn pipe_shader_names(&self, name: u64, is_textured: bool) -> Option<(String, String)>;

    fn fill_exec_buffer(
        &self,
        cmd: &PoolVec<u8>,
        render_execute: &mut dyn BackendRenderExecuteInterface,
    );

    fn render(
        &self,
        cmd: &PoolVec<u8>,
        render: &mut dyn BackendRenderInterface,
    ) -> anyhow::Result<()>;

    /// for modding purposes this function must be able to:
    /// - decode the command
    /// - call `f` with all texture indices & buffer object indices
    /// - encode the command back
    fn rewrite_texture_and_buffer_object_indices(
        &self,
        cmd: &mut PoolVec<u8>,
        f: &dyn Fn(GraphicsObjectRewriteFunc),
    );
}
