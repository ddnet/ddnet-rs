use api_wasm_macros::wasm_mod_prepare_editor;

#[wasm_mod_prepare_editor]
pub mod editor_wasm {
    use std::{path::PathBuf, rc::Rc};

    use anyhow::anyhow;
    use api_wasm_macros::wasm_func_auto_call;
    use base_io::io::Io;
    use config::config::ConfigEngine;
    use editor::editor::{EditorInterface, EditorResult};
    use egui::FontDefinitions;
    use graphics::graphics::graphics::Graphics;
    use graphics_backend::backend::GraphicsBackend;
    use sound::sound::SoundManager;
    use wasm_logic_fs::fs::WasmFileSystemLogic;
    use wasm_logic_graphics::WasmGraphicsLogic;
    use wasm_logic_http::http::WasmHttpLogic;
    use wasm_logic_sound::sound::WasmSoundLogic;
    use wasm_runtime::{MemoryLimit, WasmManager, WasmManagerModuleType};
    use wasmer::Module;

    pub struct EditorWasm {
        wasm_manager: WasmManager,
    }

    #[constructor]
    impl EditorWasm {
        pub fn new(
            sound: &SoundManager,
            graphics: &Graphics,
            backend: &Rc<GraphicsBackend>,
            io: &Io,
            font_data: &FontDefinitions,
            wasm_module: &[u8],
        ) -> Self {
            let sound_logic = WasmSoundLogic::new(u128::MAX / 2, sound);
            let graphics_logic = WasmGraphicsLogic::new(graphics, backend.clone(), u128::MAX / 2);
            let fs_logic = WasmFileSystemLogic::new(io.clone());
            let http_logic = WasmHttpLogic::new(io.clone());
            let wasm_manager: WasmManager = WasmManager::new(
                WasmManagerModuleType::FromClosure(|store| {
                    match unsafe { Module::deserialize(store, wasm_module) } {
                        Ok(module) => Ok(module),
                        Err(err) => Err(anyhow!(err)),
                    }
                }),
                |store, raw_bytes_env| {
                    let mut imports = graphics_logic.get_wasm_logic_imports(store, raw_bytes_env);
                    imports.extend(&fs_logic.get_wasm_logic_imports(store, raw_bytes_env));
                    imports.extend(&sound_logic.get_wasm_logic_imports(store, raw_bytes_env));
                    imports.extend(&http_logic.get_wasm_logic_imports(store, raw_bytes_env));
                    Some(imports)
                },
                MemoryLimit::OneGibiByte,
            )
            .unwrap();
            wasm_manager.add_param(0, &graphics.canvas_handle.window_props());
            wasm_manager.add_param(1, font_data);
            wasm_manager.run_by_name::<()>("editor_new").unwrap();

            Self { wasm_manager }
        }
    }

    impl EditorInterface for EditorWasm {
        #[wasm_func_auto_call]
        fn render(&mut self, input: egui::RawInput, config: &ConfigEngine) -> EditorResult {}

        #[wasm_func_auto_call]
        fn file_dropped(&mut self, file: PathBuf) {}

        #[wasm_func_auto_call]
        fn file_hovered(&mut self, file: Option<PathBuf>) {}
    }
}
