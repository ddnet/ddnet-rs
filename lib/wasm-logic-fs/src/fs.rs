use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use base_io::{io::Io, runtime::IoRuntimeTask};
use base_io_traits::fs_traits::{FileSystemEntryTy, HashMap};
use sendable::SendOption;
use wasm_runtime_types::{RawBytesEnv, read_param, write_result};
use wasmer::{AsStoreRef, Function, FunctionEnv, FunctionEnvMut, Imports, Store, imports};

type DirTasks = HashMap<u64, IoRuntimeTask<HashMap<PathBuf, Vec<u8>>>>;

pub struct WasmFileSystemLogicImpl {
    pub io: Io,
    tasks: RefCell<HashMap<u64, IoRuntimeTask<Vec<u8>>>>,
    write_tasks: RefCell<HashMap<u64, IoRuntimeTask<()>>>,
    create_dir_tasks: RefCell<HashMap<u64, IoRuntimeTask<()>>>,
    dir_tasks: RefCell<DirTasks>,
    entries_tasks: RefCell<HashMap<u64, IoRuntimeTask<HashMap<String, FileSystemEntryTy>>>>,
}

impl WasmFileSystemLogicImpl {
    fn new(io: Io) -> Self {
        Self {
            io,
            tasks: Default::default(),
            write_tasks: Default::default(),
            create_dir_tasks: Default::default(),
            dir_tasks: Default::default(),
            entries_tasks: Default::default(),
        }
    }

    fn read_file(&self, file_id: u64, file_path: &Path) -> Option<Result<Vec<u8>, String>> {
        let mut tasks = self.tasks.borrow_mut();
        match tasks.get(&file_id) {
            Some(task) => {
                if task.is_finished() {
                    let task = tasks.remove(&file_id).unwrap();
                    Some(task.get().map_err(|err| err.to_string()))
                } else {
                    None
                }
            }
            None => {
                let fs = self.io.fs.clone();
                let file_path_str = file_path.to_path_buf();
                let task = self
                    .io
                    .rt
                    .spawn(async move { Ok(fs.read_file(&file_path_str).await?) });
                tasks.insert(file_id, task);
                None
            }
        }
    }

    fn write_file(
        &self,
        file_id: u64,
        file_path: &Path,
        data: Vec<u8>,
    ) -> Option<Result<(), String>> {
        let mut tasks = self.write_tasks.borrow_mut();
        match tasks.get(&file_id) {
            Some(task) => {
                if task.is_finished() {
                    let task = tasks.remove(&file_id).unwrap();
                    Some(task.get().map_err(|err| err.to_string()))
                } else {
                    None
                }
            }
            None => {
                let fs = self.io.fs.clone();
                let file_path_str = file_path.to_path_buf();
                let task = self
                    .io
                    .rt
                    .spawn(async move { Ok(fs.write_file(&file_path_str, data).await?) });
                tasks.insert(file_id, task);
                None
            }
        }
    }

    fn create_dir(&self, file_id: u64, dir_path: &Path) -> Option<Result<(), String>> {
        let mut tasks = self.create_dir_tasks.borrow_mut();
        match tasks.get(&file_id) {
            Some(task) => {
                if task.is_finished() {
                    let task = tasks.remove(&file_id).unwrap();
                    Some(task.get().map_err(|err| err.to_string()))
                } else {
                    None
                }
            }
            None => {
                let fs = self.io.fs.clone();
                let dir_path_str = dir_path.to_path_buf();
                let task = self
                    .io
                    .rt
                    .spawn(async move { Ok(fs.create_dir(&dir_path_str).await?) });
                tasks.insert(file_id, task);
                None
            }
        }
    }

    fn files_in_dir_recursive(
        &self,
        file_id: u64,
        path: &Path,
    ) -> Option<Result<HashMap<PathBuf, Vec<u8>>, String>> {
        let mut tasks = self.dir_tasks.borrow_mut();
        match tasks.get(&file_id) {
            Some(task) => {
                if task.is_finished() {
                    let task = tasks.remove(&file_id).unwrap();
                    Some(task.get().map_err(|err| err.to_string()))
                } else {
                    None
                }
            }
            None => {
                let fs = self.io.fs.clone();
                let path_str = path.to_path_buf();
                let task = self
                    .io
                    .rt
                    .spawn(async move { fs.files_in_dir_recursive(&path_str).await });
                tasks.insert(file_id, task);
                None
            }
        }
    }

    fn entries_in_dir(
        &self,
        file_id: u64,
        path: &Path,
    ) -> Option<Result<HashMap<String, FileSystemEntryTy>, String>> {
        let mut tasks = self.entries_tasks.borrow_mut();
        match tasks.get(&file_id) {
            Some(task) => {
                if task.is_finished() {
                    let task = tasks.remove(&file_id).unwrap();
                    Some(task.get().map_err(|err| err.to_string()))
                } else {
                    None
                }
            }
            None => {
                let fs = self.io.fs.clone();
                let path_str = path.to_path_buf();
                let task = self
                    .io
                    .rt
                    .spawn(async move { fs.entries_in_dir(&path_str).await });
                tasks.insert(file_id, task);
                None
            }
        }
    }
}

pub struct WasmFileSystemLogic(pub Arc<Mutex<SendOption<WasmFileSystemLogicImpl>>>);

impl WasmFileSystemLogic {
    pub fn new(io: Io) -> Self {
        Self(Arc::new(Mutex::new(SendOption::new(Some(
            WasmFileSystemLogicImpl::new(io),
        )))))
    }

    pub fn get_wasm_logic_imports(
        &self,
        store: &mut Store,
        raw_bytes_env: &FunctionEnv<Arc<RawBytesEnv>>,
    ) -> Imports {
        fn read_file(
            logic_clone: &Arc<Mutex<SendOption<WasmFileSystemLogicImpl>>>,
            mut env: FunctionEnvMut<Arc<RawBytesEnv>>,
        ) {
            let (data, mut store) = env.data_and_store_mut();
            let (mut param0, instance) = data.param_index_mut();
            let file_path: PathBuf = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                0,
            );
            let file_id: u64 = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                1,
            );

            let file = logic_clone
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .read_file(file_id, &file_path);
            write_result(instance.as_ref().unwrap(), &mut store, &file);
        }

        fn write_file(
            logic_clone: &Arc<Mutex<SendOption<WasmFileSystemLogicImpl>>>,
            mut env: FunctionEnvMut<Arc<RawBytesEnv>>,
        ) {
            let (data, mut store) = env.data_and_store_mut();
            let (mut param0, instance) = data.param_index_mut();
            let file_path: PathBuf = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                0,
            );
            let data: Vec<u8> = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                1,
            );
            let file_id: u64 = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                2,
            );

            let file = logic_clone
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .write_file(file_id, &file_path, data);
            write_result(instance.as_ref().unwrap(), &mut store, &file);
        }

        fn create_dir(
            logic_clone: &Arc<Mutex<SendOption<WasmFileSystemLogicImpl>>>,
            mut env: FunctionEnvMut<Arc<RawBytesEnv>>,
        ) {
            let (data, mut store) = env.data_and_store_mut();
            let (mut param0, instance) = data.param_index_mut();
            let dir_path: PathBuf = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                0,
            );
            let file_id: u64 = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                1,
            );

            let file = logic_clone
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .create_dir(file_id, &dir_path);
            write_result(instance.as_ref().unwrap(), &mut store, &file);
        }

        fn files_in_dir_recursive(
            logic_clone: &Arc<Mutex<SendOption<WasmFileSystemLogicImpl>>>,
            mut env: FunctionEnvMut<Arc<RawBytesEnv>>,
        ) {
            let (data, mut store) = env.data_and_store_mut();
            let (mut param0, instance) = data.param_index_mut();
            let dir_path: PathBuf = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                0,
            );
            let file_id: u64 = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                1,
            );

            let file = logic_clone
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .files_in_dir_recursive(file_id, &dir_path);
            write_result(instance.as_ref().unwrap(), &mut store, &file);
        }

        fn entries_in_dir(
            logic_clone: &Arc<Mutex<SendOption<WasmFileSystemLogicImpl>>>,
            mut env: FunctionEnvMut<Arc<RawBytesEnv>>,
        ) {
            let (data, mut store) = env.data_and_store_mut();
            let (mut param0, instance) = data.param_index_mut();
            let dir_path: PathBuf = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                0,
            );
            let file_id: u64 = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                1,
            );

            let file = logic_clone
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .entries_in_dir(file_id, &dir_path);
            write_result(instance.as_ref().unwrap(), &mut store, &file);
        }

        let logic = self.0.clone();
        let logic2 = self.0.clone();
        let logic3 = self.0.clone();
        let logic4 = self.0.clone();
        let logic5 = self.0.clone();

        imports! {
            "env" => {
                "api_read_file" => Function::new_typed_with_env(store, raw_bytes_env, move |env: FunctionEnvMut<Arc<RawBytesEnv>>| read_file(&logic, env)),
                "api_write_file" => Function::new_typed_with_env(store, raw_bytes_env, move |env: FunctionEnvMut<Arc<RawBytesEnv>>| write_file(&logic2, env)),
                "api_create_dir" => Function::new_typed_with_env(store, raw_bytes_env, move |env: FunctionEnvMut<Arc<RawBytesEnv>>| create_dir(&logic3, env)),
                "api_files_in_dir_recursive" => Function::new_typed_with_env(store, raw_bytes_env, move |env: FunctionEnvMut<Arc<RawBytesEnv>>| files_in_dir_recursive(&logic4, env)),
                "api_entries_in_dir" => Function::new_typed_with_env(store, raw_bytes_env, move |env: FunctionEnvMut<Arc<RawBytesEnv>>| entries_in_dir(&logic5, env)),
            }
        }
    }
}
