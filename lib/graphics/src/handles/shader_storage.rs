pub mod shader_storage {
    use graphics_types::{
        commands::{
            AllCommands, CommandCreateShaderStorage, CommandDeleteShaderStorage,
            CommandUpdateShaderStorage, CommandUpdateShaderStorageRegion, CommandsMisc,
        },
        types::GraphicsBackendMemory,
    };
    use hiarc::{Hiarc, hiarc_safer_rc_refcell};

    use crate::handles::backend::backend::GraphicsBackendHandle;

    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc)]
    pub struct GraphicsShaderStorageHandle {
        id_gen: u128,

        backend_handle: GraphicsBackendHandle,
    }

    #[hiarc_safer_rc_refcell]
    impl GraphicsShaderStorageHandle {
        pub fn new(backend_handle: GraphicsBackendHandle) -> Self {
            Self {
                id_gen: Default::default(),

                backend_handle,
            }
        }

        pub fn create_shader_storage(
            &mut self,
            upload_data: GraphicsBackendMemory,
        ) -> ShaderStorage {
            self.id_gen += 1;
            let index = self.id_gen;

            let cmd = CommandCreateShaderStorage {
                shader_storage_index: index,
                upload_data,
            };

            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::CreateShaderStorage(cmd)));

            ShaderStorage::new(index, self.backend_handle.clone())
        }
    }

    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc)]
    pub struct ShaderStorage {
        index: u128,
        backend_handle: GraphicsBackendHandle,
    }

    #[hiarc_safer_rc_refcell]
    impl Drop for ShaderStorage {
        fn drop(&mut self) {
            let cmd = CommandDeleteShaderStorage {
                shader_storage_index: self.index,
            };

            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::DeleteShaderStorage(cmd)));
        }
    }

    #[hiarc_safer_rc_refcell]
    impl ShaderStorage {
        pub fn new(index: u128, backend_handle: GraphicsBackendHandle) -> Self {
            Self {
                index,
                backend_handle,
            }
        }

        /// updates the shader storage object with specific limitations:
        /// - all commands that use this shader storage object before this command was issued __might__ see the shader storage update too
        /// - all commands that are issued after this update are guaranteed to see the shader storage update
        pub fn update_shader_storage(
            &self,
            update_data: Vec<u8>,
            update_regions: Vec<CommandUpdateShaderStorageRegion>,
        ) {
            let cmd = CommandUpdateShaderStorage {
                shader_storage_index: self.index,
                update_data,
                update_regions,
            };

            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::UpdateShaderStorage(cmd)));
        }

        pub fn get_index_unsafe(&self) -> u128 {
            self.index
        }
    }
}
