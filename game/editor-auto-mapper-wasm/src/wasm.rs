use api_wasm_macros::wasm_mod_prepare_auto_mapper;

#[wasm_mod_prepare_auto_mapper]
pub mod wasm {
    use anyhow::anyhow;
    use api_wasm_macros::wasm_func_auto_call;
    use wasm_runtime::{MemoryLimit, WasmManager, WasmManagerModuleType};
    use wasmer::Module;

    use editor_interface::auto_mapper::{
        AutoMapperInputModes, AutoMapperInterface, AutoMapperModes, AutoMapperOutputModes,
    };

    pub struct AutoMapperWasm {
        wasm_manager: WasmManager,
    }

    #[constructor]
    impl AutoMapperWasm {
        pub fn new(wasm_module: &Vec<u8>) -> anyhow::Result<Self> {
            let wasm_manager = WasmManager::new(
                WasmManagerModuleType::FromClosure(|store| {
                    match unsafe { Module::deserialize(store, wasm_module.as_slice()) } {
                        Ok(module) => Ok(module),
                        Err(err) => Err(anyhow!(err)),
                    }
                }),
                |_, _| None,
                MemoryLimit::OneGibiByte,
            )?;
            wasm_manager.run_by_name::<()>("auto_mapper_new").unwrap();
            wasm_manager
                .get_result_as::<Result<(), String>>()
                .map_err(|err| anyhow::anyhow!(err))?;

            Ok(Self { wasm_manager })
        }
    }

    impl AutoMapperInterface for AutoMapperWasm {
        #[wasm_func_auto_call]
        fn supported_modes(&self) -> Vec<AutoMapperModes> {}

        #[wasm_func_auto_call]
        fn run(
            &mut self,
            seed: u64,
            input: AutoMapperInputModes,
        ) -> Result<AutoMapperOutputModes, String> {
        }
    }

    impl Drop for AutoMapperWasm {
        fn drop(&mut self) {
            self.wasm_manager
                .run_by_name::<()>("auto_mapper_drop")
                .unwrap();
        }
    }
}
