use std::{
    collections::{HashMap, HashSet},
    marker::PhantomData,
    num::NonZeroUsize,
};

use graphics::handles::{
    backend::backend::GraphicsBackendHandle, canvas::canvas::GraphicsCanvasHandle,
    stream::stream::GraphicsStreamHandle,
};
use graphics_backend_traits::plugin::{
    GraphicsBufferObjectAccess, GraphicsObjectRewriteFunc, GraphicsShaderStorageAccess,
};
use graphics_types::{
    commands::{
        AllCommands, CommandDeleteBufferObject, CommandRender, CommandTextureDestroy, CommandsMisc,
        CommandsRender, CommandsRenderMod, CommandsRenderQuadContainer, CommandsRenderStream,
        PrimType, RenderSpriteInfo,
    },
    rendering::{GlVertex, StateTexture, StateTexture2dArray},
};
use hiarc::{hiarc_safer_refcell, Hiarc};

#[derive(Debug, Hiarc)]
struct TextureContainer {}

#[derive(Debug, Hiarc)]
struct BufferObject {
    alloc_size: usize,
}

#[derive(Debug, Hiarc)]
struct ShaderStorage {
    alloc_size: usize,
}

/// rewrites graphics commands to make them use resource indices
/// that are not colliding with host indices
/// Important: Does not validate correctness in any kind.
#[hiarc_safer_refcell]
#[derive(Debug, Hiarc)]
pub struct GraphicsContainersAPI {
    textures: HashMap<u128, TextureContainer>,
    buffers: HashMap<u128, BufferObject>,
    shader_storages: HashMap<u128, ShaderStorage>,
    offscreen_canvases: HashSet<u128>,

    index_buffer_quad_count: u64,

    graphics_backend: GraphicsBackendHandle,
    id_offset: u128,

    _pc: PhantomData<GraphicsCanvasHandle>,
}

#[hiarc_safer_refcell]
impl GraphicsContainersAPI {
    pub fn new(id_offset: u128, graphics_backend: GraphicsBackendHandle) -> Self {
        Self {
            textures: Default::default(),
            buffers: Default::default(),
            shader_storages: Default::default(),
            offscreen_canvases: Default::default(),
            index_buffer_quad_count: 0,
            id_offset,
            graphics_backend,

            _pc: Default::default(),
        }
    }

    fn process_texture(&self, texture_index: &mut StateTexture) {
        if let StateTexture::Texture(index) = texture_index {
            assert!(*index < u64::MAX as u128, "invalid index");
            let real_index = *index + self.id_offset;

            assert!(
                self.textures.contains_key(&real_index),
                "texture does not exists, this is not allowed"
            );
            *texture_index = StateTexture::Texture(real_index);
        }
    }

    fn process_texture_2d_array(&self, texture_index: &mut StateTexture2dArray) {
        if let StateTexture2dArray::Texture(index) = texture_index {
            assert!(*index < u64::MAX as u128, "invalid index");
            let real_index = *index + self.id_offset;

            assert!(
                self.textures.contains_key(&real_index),
                "texture does not exists, this is not allowed"
            );
            *texture_index = StateTexture2dArray::Texture(real_index);
        }
    }

    fn process_buffer_object(&self, index: &mut u128) {
        assert!(*index < u64::MAX as u128, "invalid index");
        let real_index = *index + self.id_offset;

        assert!(
            self.buffers.contains_key(&real_index),
            "buffer object does not exists, this is not allowed"
        );
        *index = real_index;
    }

    fn process_shader_storage(&self, index: &mut u128) {
        assert!(*index < u64::MAX as u128, "invalid index");
        let real_index = *index + self.id_offset;

        assert!(
            self.shader_storages.contains_key(&real_index),
            "shader storage does not exists, this is not allowed"
        );
        *index = real_index;
    }

    fn check_render_cmd(
        &self,
        stream_handle: &GraphicsStreamHandle,
        cmd: &mut CommandRender,
        vertices_offset_before_commands: usize,
    ) {
        self.process_texture(&mut cmd.texture_index);

        let count = stream_handle.stream_data().vertices_count();
        let vert_per_prim = match cmd.prim_type {
            PrimType::Lines => 2,
            PrimType::Quads => 4,
            PrimType::Triangles => 3,
        };
        cmd.vertices_offset = cmd
            .vertices_offset
            .checked_add(vertices_offset_before_commands)
            .unwrap();
        assert!(
            count >= cmd.prim_count * vert_per_prim + cmd.vertices_offset,
            "Not enough vertices in the stream handle."
        );
    }

    fn handle_misc_cmd(&mut self, cmd: &mut CommandsMisc, canvas_handle: &GraphicsCanvasHandle) {
        match cmd {
            CommandsMisc::TextureCreate(cmd) => {
                assert!(cmd.texture_index < u64::MAX as u128, "invalid index");
                let real_index = cmd.texture_index + self.id_offset;
                cmd.texture_index = real_index;
                assert!(
                    !self.textures.contains_key(&real_index),
                    "texture already exists, this is not allowed"
                );
                self.textures.insert(real_index, TextureContainer {});
            }
            CommandsMisc::TextureDestroy(cmd) => {
                assert!(cmd.texture_index < u64::MAX as u128, "invalid index");
                let real_index = cmd.texture_index + self.id_offset;
                cmd.texture_index = real_index;
                assert!(
                    self.textures.contains_key(&real_index),
                    "texture does not exists, this is not allowed"
                );
                self.textures.remove(&real_index);
            }
            CommandsMisc::TextureUpdate(cmd) => {
                assert!(cmd.texture_index < u64::MAX as u128, "invalid index");
                let real_index = cmd.texture_index + self.id_offset;
                cmd.texture_index = real_index;
                assert!(
                    self.textures.contains_key(&real_index),
                    "texture does not exists, this is not allowed"
                );
            }
            // TODO: check out of bounds? Commands::TextureUpdate(cmd) => {}
            CommandsMisc::CreateBufferObject(cmd) => {
                assert!(cmd.buffer_index < u64::MAX as u128, "invalid index");
                let real_index = cmd.buffer_index + self.id_offset;
                cmd.buffer_index = real_index;
                assert!(
                    !self.buffers.contains_key(&real_index),
                    "buffer already exists, this is not allowed"
                );
                self.buffers.insert(
                    real_index,
                    BufferObject {
                        alloc_size: cmd.upload_data.len(),
                    },
                );
            }
            CommandsMisc::RecreateBufferObject(cmd) => {
                assert!(cmd.buffer_index < u64::MAX as u128, "invalid index");
                let real_index = cmd.buffer_index + self.id_offset;
                cmd.buffer_index = real_index;
                assert!(
                    self.buffers.contains_key(&real_index),
                    "buffer does not exists, this is not allowed"
                );
                self.buffers.get_mut(&real_index).unwrap().alloc_size = cmd.upload_data.len();
            }
            CommandsMisc::DeleteBufferObject(cmd) => {
                assert!(cmd.buffer_index < u64::MAX as u128, "invalid index");
                let real_index = cmd.buffer_index + self.id_offset;
                cmd.buffer_index = real_index;
                assert!(
                    self.buffers.contains_key(&real_index),
                    "buffer does not exists, this is not allowed"
                );
                self.buffers.remove(&real_index);
            }
            CommandsMisc::UpdateBufferObject(_) => todo!(),
            CommandsMisc::CreateShaderStorage(cmd) => {
                assert!(cmd.shader_storage_index < u64::MAX as u128, "invalid index");
                let real_index = cmd.shader_storage_index + self.id_offset;
                cmd.shader_storage_index = real_index;
                assert!(
                    !self.shader_storages.contains_key(&real_index),
                    "shader storage already exists, this is not allowed"
                );
                self.shader_storages.insert(
                    real_index,
                    ShaderStorage {
                        alloc_size: cmd.upload_data.len(),
                    },
                );
            }
            CommandsMisc::DeleteShaderStorage(cmd) => {
                assert!(cmd.shader_storage_index < u64::MAX as u128, "invalid index");
                let real_index = cmd.shader_storage_index + self.id_offset;
                cmd.shader_storage_index = real_index;
                assert!(
                    self.shader_storages.contains_key(&real_index),
                    "shader storage does not exists, this is not allowed"
                );
                self.shader_storages.remove(&real_index);
            }
            CommandsMisc::UpdateShaderStorage(_) => todo!(),
            CommandsMisc::OffscreenCanvasCreate(cmd) => {
                assert!(cmd.offscreen_index < u64::MAX as u128, "invalid index");
                assert!(
                    cmd.width <= 4096 && cmd.height <= 4096,
                    "widths & heights above 4096 pixels are currently not allowed."
                );
                assert!(
                    cmd.has_multi_sampling.is_none_or(|s| s < 64),
                    "msaa above x64 is not allowed."
                );
                let real_index = cmd.offscreen_index + self.id_offset;
                cmd.offscreen_index = real_index;
                assert!(
                    !self.offscreen_canvases.contains(&real_index),
                    "offscreen canvas already exists, this is not allowed"
                );
                self.offscreen_canvases.insert(real_index);
            }
            CommandsMisc::OffscreenCanvasDestroy(cmd) => {
                assert!(cmd.offscreen_index < u64::MAX as u128, "invalid index");
                let real_index = cmd.offscreen_index + self.id_offset;
                cmd.offscreen_index = real_index;
                assert!(
                    self.offscreen_canvases.contains(&real_index),
                    "offscreen canvas does not exists, this is not allowed"
                );
                self.offscreen_canvases.remove(&real_index);
            }
            CommandsMisc::OffscreenCanvasSkipFetchingOnce(cmd) => {
                assert!(cmd.offscreen_index < u64::MAX as u128, "invalid index");
                let real_index = cmd.offscreen_index + self.id_offset;
                cmd.offscreen_index = real_index;
                assert!(
                    self.offscreen_canvases.contains(&real_index),
                    "offscreen canvas does not exists, this is not allowed"
                );
            }
            CommandsMisc::IndicesForQuadsRequiredNotify(cmd) => {
                assert!(cmd.quad_count_required <= (u32::MAX / 6) as u64);
                self.index_buffer_quad_count =
                    self.index_buffer_quad_count.max(cmd.quad_count_required);
            }
            CommandsMisc::Swap => todo!(),
            CommandsMisc::NextSwitchPass => {
                // backends job to verify correctness
            }
            CommandsMisc::ConsumeMultiSamplingTargets => {
                // backends job to verify correctness
            }
            CommandsMisc::SwitchCanvas(_) => todo!(),
            CommandsMisc::UpdateViewport(cmd) => {
                assert!(!cmd.by_resize);

                assert!(
                    cmd.x >= 0
                        && cmd.width > 0
                        && (cmd.x as u32) < canvas_handle.canvas_width()
                        && (cmd.x as u32).checked_add(cmd.width).unwrap()
                            <= canvas_handle.canvas_width()
                );
                assert!(
                    cmd.y >= 0
                        && cmd.height > 0
                        && (cmd.y as u32) < canvas_handle.canvas_height()
                        && (cmd.y as u32).checked_add(cmd.height).unwrap()
                            <= canvas_handle.canvas_height()
                );
            }
            CommandsMisc::Multisampling(cmd) => {
                // Nothing more that can be done, the backend has to be safe for this one.
                assert!(
                    cmd.sample_count < 64 && (cmd.sample_count == 1 || cmd.sample_count % 2 == 0)
                );
            }
            CommandsMisc::VSync(_) => {
                // Nothing to do
            }
        }
    }

    fn check_buffer(
        &self,
        real_index: u128,
        quad_offset: usize,
        quad_num: usize,
        byte_offset: usize,
        single_vertex_byte_size: usize,
        allowed_alignment: NonZeroUsize,
    ) {
        let buffer = self.buffers.get(&real_index).unwrap();
        // check that offset and draw num are multiples of the index buffer
        let pre_cond = quad_offset as u64 <= u64::MAX / std::mem::size_of::<u32>() as u64 / 6
            && quad_num as u64 <= u32::MAX as u64 / 6
            && (quad_offset as u64 + quad_num as u64) <= self.index_buffer_quad_count;

        // common min alignment
        assert!(byte_offset % allowed_alignment.get() == 0);

        // make sure the buffer is big enough
        let number_of_quads = quad_offset.checked_add(quad_num).unwrap();
        assert!(
            pre_cond
                && buffer.alloc_size
                    >= single_vertex_byte_size
                        .checked_mul(4)
                        .unwrap()
                        .checked_mul(number_of_quads)
                        .unwrap()
                        .checked_add(byte_offset)
                        .unwrap()
        );
    }

    fn check_shader_storage(
        &self,
        real_index: u128,
        quad_offset: usize,
        quad_num: usize,
        single_entry_byte_size: usize,
        allowed_alignment: NonZeroUsize,
    ) {
        let shader_storage = self.shader_storages.get(&real_index).unwrap();
        // check that offset and draw num are multiples of the index buffer
        let pre_cond = quad_offset as u64 <= u64::MAX / std::mem::size_of::<u32>() as u64 / 6
            && quad_num as u64 <= u32::MAX as u64 / 6
            && (quad_offset as u64 + quad_num as u64) <= self.index_buffer_quad_count;

        // common min alignment
        assert!(allowed_alignment.get() % 2 == 0);

        // make sure the shader storage is big enough
        let number_of_quads = quad_offset.checked_add(quad_num).unwrap();
        assert!(
            pre_cond
                && shader_storage.alloc_size
                    >= single_entry_byte_size.checked_mul(number_of_quads).unwrap()
        );
    }

    fn check_and_change_uniform_instance(
        &self,
        stream_handle: &GraphicsStreamHandle,
        uniform_instance: &mut usize,
        instances_to_draw: usize,
        byte_size_of_single_instance: usize,
        uniform_offset_before_commands: usize,
    ) {
        *uniform_instance += uniform_offset_before_commands;
        assert!(byte_size_of_single_instance % 4 == 0);
        assert!(
            stream_handle.stream_data().uniform_instance_count() > *uniform_instance
                && stream_handle
                    .stream_data()
                    .uniform_byte_size(*uniform_instance)
                    >= instances_to_draw * byte_size_of_single_instance
        );
    }

    fn handle_render_cmd(
        &mut self,
        stream_handle: &GraphicsStreamHandle,
        cmd: &mut CommandsRender,
        vertices_offset_before_commands: usize,
        uniform_offset_before_commands: usize,
    ) {
        match cmd {
            CommandsRender::QuadContainer(cmd) => match cmd {
                CommandsRenderQuadContainer::Render(cmd) => {
                    assert!(cmd.buffer_object_index < u64::MAX as u128, "invalid index");
                    let real_index = cmd.buffer_object_index + self.id_offset;
                    cmd.buffer_object_index = real_index;
                    self.check_buffer(
                        real_index,
                        cmd.quad_offset,
                        cmd.quad_num,
                        0,
                        std::mem::size_of::<GlVertex>(),
                        4.try_into().unwrap(),
                    );
                    self.process_texture(&mut cmd.texture_index);
                }
                CommandsRenderQuadContainer::RenderAsSpriteMultiple(cmd) => {
                    assert!(cmd.buffer_object_index < u64::MAX as u128, "invalid index");
                    let real_index = cmd.buffer_object_index + self.id_offset;
                    cmd.buffer_object_index = real_index;
                    self.check_buffer(
                        real_index,
                        cmd.quad_offset,
                        cmd.quad_num,
                        0,
                        std::mem::size_of::<GlVertex>(),
                        4.try_into().unwrap(),
                    );
                    self.check_and_change_uniform_instance(
                        stream_handle,
                        &mut cmd.render_info_uniform_instance,
                        cmd.instance_count,
                        std::mem::size_of::<RenderSpriteInfo>(),
                        uniform_offset_before_commands,
                    );
                    self.process_texture(&mut cmd.texture_index);
                }
            },
            CommandsRender::Stream(cmd) => match cmd {
                CommandsRenderStream::Render(cmd) => {
                    self.check_render_cmd(stream_handle, cmd, vertices_offset_before_commands)
                }
                CommandsRenderStream::RenderBlurred { cmd, .. } => {
                    self.check_render_cmd(stream_handle, cmd, vertices_offset_before_commands)
                }
            },
            CommandsRender::Clear(_) => {
                todo!()
            }
            CommandsRender::Mod(CommandsRenderMod { mod_name, cmd }) => {
                self.graphics_backend.check_mod_cmd(
                    mod_name,
                    cmd,
                    &|GraphicsObjectRewriteFunc {
                          textures,
                          textures_2d_array,
                          buffer_objects,
                          uniform_instances,
                          shader_storages,
                      }| {
                        for texture in textures {
                            self.process_texture(texture)
                        }
                        for texture_2d_array in textures_2d_array {
                            self.process_texture_2d_array(texture_2d_array)
                        }
                        for buffer_object in buffer_objects {
                            self.process_buffer_object(buffer_object.buffer_object_index);
                            for access in buffer_object.accesses.iter() {
                                match access {
                                    GraphicsBufferObjectAccess::Quad {
                                        quad_offset,
                                        quad_count,
                                        buffer_byte_offset,
                                        vertex_byte_size,
                                        alignment,
                                    } => {
                                        self.check_buffer(
                                            *buffer_object.buffer_object_index,
                                            *quad_offset,
                                            *quad_count,
                                            *buffer_byte_offset,
                                            *vertex_byte_size,
                                            *alignment,
                                        );
                                    }
                                }
                            }
                        }
                        for uniform_instance in uniform_instances {
                            self.check_and_change_uniform_instance(
                                stream_handle,
                                uniform_instance.index,
                                uniform_instance.instance_count,
                                uniform_instance.single_instance_byte_size,
                                uniform_offset_before_commands,
                            )
                        }
                        for shader_storage in shader_storages {
                            self.process_shader_storage(shader_storage.shader_storage_index);
                            for access in shader_storage.accesses.iter() {
                                match access {
                                    GraphicsShaderStorageAccess::IndicedQuad {
                                        quad_offset,
                                        quad_count,
                                        entry_byte_size,
                                        alignment,
                                    } => {
                                        self.check_shader_storage(
                                            *shader_storage.shader_storage_index,
                                            *quad_offset,
                                            *quad_count,
                                            *entry_byte_size,
                                            *alignment,
                                        );
                                    }
                                }
                            }
                        }
                    },
                );
            }
        }
    }

    fn process_command(
        &mut self,
        stream_handle: &GraphicsStreamHandle,
        canvas_handle: &GraphicsCanvasHandle,
        cmd: &mut AllCommands,
        vertices_offset_before_commands: usize,
        uniform_offset_before_commands: usize,
    ) {
        match cmd {
            AllCommands::Render(cmd) => self.handle_render_cmd(
                stream_handle,
                cmd,
                vertices_offset_before_commands,
                uniform_offset_before_commands,
            ),
            AllCommands::Misc(cmd) => self.handle_misc_cmd(cmd, canvas_handle),
        }
    }

    pub fn process_commands(
        &mut self,
        stream_handle: &GraphicsStreamHandle,
        canvas_handle: &GraphicsCanvasHandle,
        cmds: &mut Vec<AllCommands>,
        vertices_offset_before_commands: usize,
        uniform_offset_before_commands: usize,
    ) {
        for cmd in cmds {
            self.process_command(
                stream_handle,
                canvas_handle,
                cmd,
                vertices_offset_before_commands,
                uniform_offset_before_commands,
            );
        }
    }
}

#[hiarc_safer_refcell]
impl Drop for GraphicsContainersAPI {
    fn drop(&mut self) {
        self.buffers.drain().for_each(|(buffer_index, _)| {
            self.graphics_backend
                .add_cmd(AllCommands::Misc(CommandsMisc::DeleteBufferObject(
                    CommandDeleteBufferObject { buffer_index },
                )))
        });

        self.textures.drain().for_each(|(texture_index, _)| {
            self.graphics_backend
                .add_cmd(AllCommands::Misc(CommandsMisc::TextureDestroy(
                    CommandTextureDestroy { texture_index },
                )))
        });
    }
}
