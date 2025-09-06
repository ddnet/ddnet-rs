use std::sync::{Arc, Mutex};

use sendable::SendOption;
use sound::{
    backend_handle::SoundBackendHandle, commands::SoundCommand, scene_handle::SoundSceneHandle,
    sound::SoundManager,
};
use wasm_runtime_types::{RawBytesEnv, read_param};
use wasmer::{AsStoreRef, Function, FunctionEnv, FunctionEnvMut, Imports, Store, imports};

use crate::checker::SoundCheckerApi;

pub struct WasmSoundLogicImpl {
    pub backend_handle: SoundBackendHandle,
    pub scene_handle: SoundSceneHandle,

    pub sound_api: SoundCheckerApi,
}

impl WasmSoundLogicImpl {
    fn new(id_offset: u128, sound: &SoundManager) -> Self {
        Self {
            backend_handle: sound.backend_handle.clone(),
            scene_handle: sound.scene_handle.clone(),

            sound_api: SoundCheckerApi::new(id_offset, sound.backend_handle.clone()),
        }
    }

    fn run_cmds(&self, mut cmds: Vec<SoundCommand>, actually_run_cmds: bool) {
        self.sound_api.process_commands(&mut cmds);

        self.backend_handle.add_cmds(&mut cmds);

        if actually_run_cmds {
            self.backend_handle.run_cmds();
        }
    }
}

pub struct WasmSoundLogic(pub Arc<Mutex<SendOption<WasmSoundLogicImpl>>>);

impl WasmSoundLogic {
    pub fn new(id_offset: u128, sound: &SoundManager) -> Self {
        Self(Arc::new(Mutex::new(SendOption::new(Some(
            WasmSoundLogicImpl::new(id_offset, sound),
        )))))
    }

    pub fn get_wasm_logic_imports(
        &self,
        store: &mut Store,
        raw_bytes_env: &FunctionEnv<Arc<RawBytesEnv>>,
    ) -> Imports {
        fn run_cmds(
            logic_clone: &Arc<Mutex<SendOption<WasmSoundLogicImpl>>>,
            mut env: FunctionEnvMut<Arc<RawBytesEnv>>,
        ) {
            let (data, store) = env.data_and_store_mut();
            let (mut param0, instance) = data.param_index_mut();
            let cmds = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                0,
            );
            let (mut param2, instance) = data.param_index_mut();
            let actually_run_cmds = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param2,
                1,
            );

            logic_clone
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .run_cmds(cmds, actually_run_cmds)
        }

        let logic = self.0.clone();

        imports! {
            "env" => {
                "sound_api_run_cmds" => Function::new_typed_with_env(store, raw_bytes_env, move |env: FunctionEnvMut<Arc<RawBytesEnv>>| run_cmds(&logic, env)),
            }
        }
    }
}
