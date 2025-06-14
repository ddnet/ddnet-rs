use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, Mutex},
};

use base::hash::Hash;
use base_io::{io::Io, runtime::IoRuntimeTask};
use base_io_traits::http_traits::HttpError;
use bytes::Bytes;
use sendable::SendOption;
use url::Url;
use wasm_runtime_types::{read_param, write_result, RawBytesEnv};
use wasmer::{imports, AsStoreRef, Function, FunctionEnv, FunctionEnvMut, Imports, Store};

type PostTasks = HashMap<u64, IoRuntimeTask<Result<Vec<u8>, HttpError>>>;

pub struct WasmHttpLogicImpl {
    pub io: Io,
    tasks: RefCell<HashMap<u64, IoRuntimeTask<Result<String, HttpError>>>>,
    bin_tasks: RefCell<HashMap<u64, IoRuntimeTask<Result<Bytes, HttpError>>>>,
    post_tasks: RefCell<PostTasks>,
}

impl WasmHttpLogicImpl {
    fn new(io: Io) -> Self {
        Self {
            io,
            tasks: Default::default(),
            bin_tasks: Default::default(),
            post_tasks: Default::default(),
        }
    }

    fn download_text(&self, task_id: u64, url: Url) -> Option<Result<String, HttpError>> {
        let mut tasks = self.tasks.borrow_mut();
        match tasks.get(&task_id) {
            Some(task) => {
                if task.is_finished() {
                    let task = tasks.remove(&task_id).unwrap();
                    Some(
                        task.get()
                            .map_err(|err| HttpError::Other(err.to_string()))
                            .and_then(|res| res),
                    )
                } else {
                    None
                }
            }
            None => {
                let http = self.io.http.clone();
                let task = self
                    .io
                    .rt
                    .spawn(async move { Ok(http.download_text(url).await) });
                tasks.insert(task_id, task);
                None
            }
        }
    }

    fn download_binary(
        &self,
        task_id: u64,
        url: Url,
        hash: Hash,
    ) -> Option<Result<Bytes, HttpError>> {
        let mut tasks = self.bin_tasks.borrow_mut();
        match tasks.get(&task_id) {
            Some(task) => {
                if task.is_finished() {
                    let task = tasks.remove(&task_id).unwrap();
                    Some(
                        task.get()
                            .map_err(|err| HttpError::Other(err.to_string()))
                            .and_then(|res| res),
                    )
                } else {
                    None
                }
            }
            None => {
                let http = self.io.http.clone();
                let task = self
                    .io
                    .rt
                    .spawn(async move { Ok(http.download_binary(url, &hash).await) });
                tasks.insert(task_id, task);
                None
            }
        }
    }

    fn post_json(
        &self,
        task_id: u64,
        url: Url,
        data: Vec<u8>,
    ) -> Option<Result<Vec<u8>, HttpError>> {
        let mut tasks = self.post_tasks.borrow_mut();
        match tasks.get(&task_id) {
            Some(task) => {
                if task.is_finished() {
                    let task = tasks.remove(&task_id).unwrap();
                    Some(
                        task.get()
                            .map_err(|err| HttpError::Other(err.to_string()))
                            .and_then(|res| res),
                    )
                } else {
                    None
                }
            }
            None => {
                let http = self.io.http.clone();
                let task = self
                    .io
                    .rt
                    .spawn(async move { Ok(http.post_json(url, data).await) });
                tasks.insert(task_id, task);
                None
            }
        }
    }
}

pub struct WasmHttpLogic(pub Arc<Mutex<SendOption<WasmHttpLogicImpl>>>);

impl WasmHttpLogic {
    pub fn new(io: Io) -> Self {
        Self(Arc::new(Mutex::new(SendOption::new(Some(
            WasmHttpLogicImpl::new(io),
        )))))
    }

    pub fn get_wasm_logic_imports(
        &self,
        store: &mut Store,
        raw_bytes_env: &FunctionEnv<Arc<RawBytesEnv>>,
    ) -> Imports {
        fn download_text(
            logic_clone: &Arc<Mutex<SendOption<WasmHttpLogicImpl>>>,
            mut env: FunctionEnvMut<Arc<RawBytesEnv>>,
        ) {
            let (data, mut store) = env.data_and_store_mut();
            let (mut param0, instance) = data.param_index_mut();
            let url: Url = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                0,
            );
            let task_id: u64 = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                1,
            );

            let res = logic_clone
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .download_text(task_id, url);
            write_result(instance.as_ref().unwrap(), &mut store, &res);
        }
        fn download_binary(
            logic_clone: &Arc<Mutex<SendOption<WasmHttpLogicImpl>>>,
            mut env: FunctionEnvMut<Arc<RawBytesEnv>>,
        ) {
            let (data, mut store) = env.data_and_store_mut();
            let (mut param0, instance) = data.param_index_mut();
            let url: Url = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                0,
            );
            let hash: Hash = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                1,
            );
            let task_id: u64 = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                2,
            );

            let res = logic_clone
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .download_binary(task_id, url, hash);
            write_result(instance.as_ref().unwrap(), &mut store, &res);
        }
        fn post_json(
            logic_clone: &Arc<Mutex<SendOption<WasmHttpLogicImpl>>>,
            mut env: FunctionEnvMut<Arc<RawBytesEnv>>,
        ) {
            let (data, mut store) = env.data_and_store_mut();
            let (mut param0, instance) = data.param_index_mut();
            let url: Url = read_param(
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
            let task_id: u64 = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut param0,
                2,
            );

            let res = logic_clone
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .post_json(task_id, url, data);
            write_result(instance.as_ref().unwrap(), &mut store, &res);
        }

        let logic = self.0.clone();
        let logic2 = self.0.clone();
        let logic3 = self.0.clone();

        imports! {
            "env" => {
                "api_download_text" => Function::new_typed_with_env(store, raw_bytes_env, move |env: FunctionEnvMut<Arc<RawBytesEnv>>| download_text(&logic, env)),
                "api_download_binary" => Function::new_typed_with_env(store, raw_bytes_env, move |env: FunctionEnvMut<Arc<RawBytesEnv>>| download_binary(&logic2, env)),
                "api_post_json" => Function::new_typed_with_env(store, raw_bytes_env, move |env: FunctionEnvMut<Arc<RawBytesEnv>>| post_json(&logic3, env)),
            }
        }
    }
}
