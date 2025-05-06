use std::sync::Arc;

use base_io_traits::fs_traits::FileSystemInterface;
use cache::Cache;
use editor_interface::auto_mapper::{
    AutoMapperInputModes, AutoMapperInterface, AutoMapperModes, AutoMapperOutputModes,
};
use wasm_runtime::WasmManager;

use super::wasm::wasm::AutoMapperWasm;

pub type WasmModule = Vec<u8>;

pub struct AutoMapperWasmManager {
    auto_mapper: AutoMapperWasm,
}

pub const AUTO_MAPPER_MODS_PATH: &str = "editor/rules";

impl AutoMapperWasmManager {
    pub async fn load_module(
        fs: &Arc<dyn FileSystemInterface>,
        file: Vec<u8>,
    ) -> anyhow::Result<Vec<u8>> {
        let cache = Arc::new(Cache::<20250506>::new_async(AUTO_MAPPER_MODS_PATH, fs).await);
        cache
            .load_from_binary(file, |wasm_bytes| {
                Box::pin(async move {
                    Ok(WasmManager::compile_module(&wasm_bytes)?
                        .serialize()?
                        .to_vec())
                })
            })
            .await
    }

    pub fn new(wasm_module: WasmModule) -> anyhow::Result<Self> {
        let auto_mapper = AutoMapperWasm::new(&wasm_module)?;

        Ok(Self { auto_mapper })
    }
}

impl AutoMapperInterface for AutoMapperWasmManager {
    fn supported_modes(&self) -> Vec<AutoMapperModes> {
        self.auto_mapper.supported_modes()
    }

    fn run(
        &mut self,
        seed: u64,
        input: AutoMapperInputModes,
    ) -> Result<AutoMapperOutputModes, String> {
        self.auto_mapper.run(seed, input)
    }
}
