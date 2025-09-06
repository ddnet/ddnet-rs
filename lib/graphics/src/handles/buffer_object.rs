pub mod buffer_object {
    use graphics_types::{
        commands::{
            AllCommands, CommandCreateBufferObject, CommandDeleteBufferObject,
            CommandRecreateBufferObject, CommandUpdateBufferObject,
            CommandUpdateBufferObjectRegion, CommandsMisc,
        },
        types::{GraphicsBackendMemory, GraphicsMemoryAllocationType},
    };
    use hiarc::{Hiarc, hiarc_safer_rc_refcell};

    use crate::handles::backend::backend::GraphicsBackendHandle;

    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc)]
    pub struct GraphicsBufferObjectHandle {
        id_gen: u128,

        backend_handle: GraphicsBackendHandle,
    }

    #[hiarc_safer_rc_refcell]
    impl GraphicsBufferObjectHandle {
        pub fn new(backend_handle: GraphicsBackendHandle) -> Self {
            Self {
                id_gen: Default::default(),

                backend_handle,
            }
        }

        pub fn create_buffer_object(&mut self, upload_data: GraphicsBackendMemory) -> BufferObject {
            self.id_gen += 1;
            let index = self.id_gen;

            let cmd = CommandCreateBufferObject {
                buffer_index: index,
                upload_data,
            };

            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::CreateBufferObject(cmd)));

            BufferObject::new(index, self.backend_handle.clone())
        }

        pub fn create_buffer_object_slow(&mut self, upload_data: Vec<u8>) -> BufferObject {
            let mut buffer_mem =
                self.backend_handle
                    .mem_alloc(GraphicsMemoryAllocationType::VertexBuffer {
                        required_size: upload_data.len().try_into().unwrap(),
                    });
            buffer_mem.copy_from_slice(&upload_data);
            self.create_buffer_object(buffer_mem)
        }
    }

    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc)]
    pub struct BufferObject {
        index: u128,
        backend_handle: GraphicsBackendHandle,
    }

    #[hiarc_safer_rc_refcell]
    impl Drop for BufferObject {
        fn drop(&mut self) {
            let cmd = CommandDeleteBufferObject {
                buffer_index: self.index,
            };

            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::DeleteBufferObject(cmd)));
        }
    }

    #[hiarc_safer_rc_refcell]
    impl BufferObject {
        pub fn new(index: u128, backend_handle: GraphicsBackendHandle) -> Self {
            Self {
                index,
                backend_handle,
            }
        }

        /// updates the buffer object with specific limitations:
        /// - all commands that use this buffer object before this command was issued __might__ see the buffer update too
        /// - all commands that are issued after this update are guaranteed to see the buffer update
        pub fn update_buffer_object(
            &self,
            update_data: Vec<u8>,
            update_regions: Vec<CommandUpdateBufferObjectRegion>,
        ) {
            let cmd = CommandUpdateBufferObject {
                buffer_index: self.index,
                update_data,
                update_regions,
            };

            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::UpdateBufferObject(cmd)));
        }

        pub fn recreate_buffer_object(&self, upload_data: GraphicsBackendMemory) {
            let cmd = CommandRecreateBufferObject {
                buffer_index: self.index,
                upload_data,
            };

            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::RecreateBufferObject(cmd)));
        }

        pub fn recreate_buffer_object_slow(&self, upload_data: Vec<u8>) {
            let mut buffer_mem =
                self.backend_handle
                    .mem_alloc(GraphicsMemoryAllocationType::VertexBuffer {
                        required_size: upload_data.len().try_into().unwrap(),
                    });
            buffer_mem.copy_from_slice(&upload_data);
            self.recreate_buffer_object(buffer_mem)
        }

        pub fn get_index_unsafe(&self) -> u128 {
            self.index
        }
    }
}
