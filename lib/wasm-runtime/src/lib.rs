use std::{cell::RefCell, sync::Arc};

use arrayvec::ArrayVec;
use serde::{de::DeserializeOwned, Serialize};
pub use wasm_runtime_types::MemoryLimit;
use wasm_runtime_types::{
    read_global, read_global_location, read_param, write_global, InstanceData, RawBytesEnv,
};
use wasmer::{
    imports,
    sys::{Cranelift, CraneliftOptLevel, EngineBuilder, Features},
    AsStoreMut, AsStoreRef, Function, FunctionEnv, FunctionEnvMut, Imports, Instance, Module,
    Store, TypedFunction,
};

/// Creates a WASM instance, automatically uses and fills the cache.
///
/// TODO: should be allowed if no graphics modules etc. were used <= __Note: Please never provide multi-threading support, it doesn't fit our design__
pub struct WasmManager {
    store: RefCell<Store>,
    instance: Instance,
    raw_bytes: Arc<RawBytesEnv>,
    guest_raw_bytes: [RefCell<Vec<u8>>; 10],

    instance_data: InstanceData,
    prepare_param_func: TypedFunction<(u32, u32), ()>,
}

pub enum WasmManagerModuleType<'a, F: FnOnce(&Store) -> anyhow::Result<Module>> {
    FromBytes(&'a [u8]),
    FromClosure(F),
}

impl WasmManager {
    fn get_store() -> Store {
        let mut compiler = Cranelift::new();
        compiler.opt_level(CraneliftOptLevel::Speed);
        Store::new(EngineBuilder::new(compiler).set_features(Some(Features {
            simd: true,
            threads: false,
            ..Default::default()
        })))
    }

    pub fn compile_module(wasm_bytes: &[u8]) -> anyhow::Result<Module> {
        Ok(Module::new(&Self::get_store(), wasm_bytes)?)
    }

    pub fn new<F, FM>(
        wasm_module: WasmManagerModuleType<FM>,
        create_imports: F,
        memory_read_limit: MemoryLimit,
    ) -> anyhow::Result<Self>
    where
        F: FnOnce(&mut Store, &FunctionEnv<Arc<RawBytesEnv>>) -> Option<Imports>,
        FM: FnOnce(&Store) -> anyhow::Result<Module>,
    {
        let mut store = Self::get_store();
        // We then use our store and Wasm bytes to compile a `Module`.
        // A `Module` is a compiled WebAssembly module that isn't ready to execute yet.
        let module = match wasm_module {
            WasmManagerModuleType::FromBytes(wasm_bytes) => Module::new(&store, wasm_bytes)?,
            WasmManagerModuleType::FromClosure(module_gen) => module_gen(&store)?,
        };

        let raw_bytes = Arc::new(RawBytesEnv::default());

        let raw_bytes_env = FunctionEnv::new(&mut store, raw_bytes.clone());

        fn println(mut env: FunctionEnvMut<Arc<RawBytesEnv>>) {
            let (data, store) = env.data_and_store_mut();
            let (mut byte_buffer, instance) = data.param_index_mut();
            let text: String = read_param(
                instance.as_ref().unwrap(),
                &store.as_store_ref(),
                &mut byte_buffer,
                0,
            );

            println!("{}", text);
        }

        // We then create an import object so that the `Module`'s imports can be satisfied.
        let mut import_object = imports! {
            "env" => {
                "host_println" => Function::new_typed_with_env(&mut store, &raw_bytes_env, println),
            }
        };

        let additional_imports = create_imports(&mut store, &raw_bytes_env);
        if let Some(additional_imports) = additional_imports {
            import_object.extend(&additional_imports);
        }

        // We then use the `Module` and the import object to create an `Instance`.
        //
        // An `Instance` is a compiled WebAssembly module that has been set up
        // and is ready to execute.
        let instance = Instance::new(&mut store, &module, &import_object)?;

        let prepare_result_func = instance
            .exports
            .get_typed_function(&store, "prepare_result")
            .unwrap();

        let instance_data = InstanceData {
            result_ptr_ptr: read_global_location(
                &instance,
                &mut store.as_store_mut(),
                "RESULT_PTR",
            ),
            result_size_ptr: read_global_location(
                &instance,
                &mut store.as_store_mut(),
                "RESULT_SIZE",
            ),
            param_ptr_ptrs: (0..10)
                .map(|i| {
                    read_global_location(
                        &instance,
                        &mut store.as_store_mut(),
                        &format!("PARAM{i}_PTR"),
                    )
                })
                .collect::<ArrayVec<_, 10>>()
                .into_inner()
                .unwrap(),
            param_size_ptrs: (0..10)
                .map(|i| {
                    read_global_location(
                        &instance,
                        &mut store.as_store_mut(),
                        &format!("PARAM{i}_SIZE"),
                    )
                })
                .collect::<ArrayVec<_, 10>>()
                .into_inner()
                .unwrap(),
            param_alloc_size_ptrs: (0..10)
                .map(|i| {
                    read_global_location(
                        &instance,
                        &mut store.as_store_mut(),
                        &format!("PARAM{i}_ALLOC_SIZE"),
                    )
                })
                .collect::<ArrayVec<_, 10>>()
                .into_inner()
                .unwrap(),
            memory: instance.exports.get_memory("memory").unwrap().clone(),
            prepare_result_func,
            memory_read_limit,
        };
        raw_bytes.set_instance(instance_data.clone());

        let res = Self {
            instance_data,

            prepare_param_func: instance
                .exports
                .get_typed_function(&store, "prepare_param")
                .unwrap(),

            store: RefCell::new(store),
            instance,
            raw_bytes,
            guest_raw_bytes: Default::default(),
        };
        res.run_by_name::<()>("api_setup")?;
        Ok(res)
    }

    pub fn run_by_name<R>(&self, name: &str) -> anyhow::Result<R>
    where
        R: wasmer::WasmTypeList,
    {
        // get the named function, it can take no args or returns anything
        let run_func: TypedFunction<(), R> = self
            .instance
            .exports
            .get_typed_function(&self.store.borrow(), name)?;
        Ok(run_func.call(&mut self.store.borrow_mut())?)
    }

    pub fn run_by_ref(&self, func: &TypedFunction<(), ()>) -> anyhow::Result<()> {
        func.call(&mut self.store.borrow_mut())?;
        Ok(())
    }

    pub fn run_func_by_name(&self, name: &str) -> TypedFunction<(), ()> {
        self.instance
            .exports
            .get_typed_function(&self.store.borrow(), name)
            .unwrap()
    }

    pub fn get_result_as<F: DeserializeOwned>(&self) -> F {
        let ptr = read_global(
            &self.instance_data.memory,
            &self.store.borrow().as_store_ref(),
            self.instance_data.result_ptr_ptr,
        ) as u32;
        let size = read_global(
            &self.instance_data.memory,
            &self.store.borrow().as_store_ref(),
            self.instance_data.result_size_ptr,
        ) as usize;

        let mut result = self.raw_bytes.result_mut();
        if size > self.instance_data.memory_read_limit.limit() {
            panic!(
                "Currently the memory limit is {} bytes",
                self.instance_data.memory_read_limit.limit()
            );
        }
        result.resize(size, Default::default());

        let memory = &self.instance_data.memory;
        let store = self.store.borrow();
        let mem_view = memory.view(&store);
        mem_view.read(ptr as u64, &mut result).unwrap();

        let config = bincode::config::standard().with_fixed_int_encoding();
        match self.instance_data.memory_read_limit {
            MemoryLimit::OneMebiByte => bincode::serde::decode_from_slice::<F, _>(
                result.as_slice(),
                config.with_limit::<{ 1024 * 1024 }>(),
            ),
            MemoryLimit::TenMebiBytes => bincode::serde::decode_from_slice::<F, _>(
                result.as_slice(),
                config.with_limit::<{ 1024 * 1024 * 10 }>(),
            ),
            MemoryLimit::OneGibiByte => bincode::serde::decode_from_slice::<F, _>(
                result.as_slice(),
                config.with_limit::<{ 1024 * 1024 * 1024 }>(),
            ),
        }
        .unwrap()
        .0
    }

    pub fn add_param<F: Serialize>(&self, param_index: usize, param: &F) {
        let mut raw_bytes = self.guest_raw_bytes[param_index].borrow_mut();

        // clear here and on guest
        raw_bytes.clear();

        // encode and upload
        bincode::serde::encode_into_std_write::<&F, _, _>(
            param,
            &mut *raw_bytes,
            bincode::config::standard().with_fixed_int_encoding(),
        )
        .unwrap();

        let alloc_size = read_global(
            &self.instance_data.memory,
            &self.store.borrow().as_store_ref(),
            self.instance_data.param_alloc_size_ptrs[param_index],
        ) as u32;

        if alloc_size as usize >= raw_bytes.len() {
            write_global(
                &self.instance_data.memory,
                &self.store.borrow().as_store_ref(),
                self.instance_data.param_size_ptrs[param_index],
                raw_bytes.len() as i32,
            );
        } else {
            self.prepare_param_func
                .call(
                    &mut self.store.borrow_mut(),
                    param_index as u32,
                    raw_bytes.len() as u32,
                )
                .unwrap();
        }

        let ptr = read_global(
            &self.instance_data.memory,
            &self.store.borrow().as_store_ref(),
            self.instance_data.param_ptr_ptrs[param_index],
        ) as u32;

        let memory = &self.instance_data.memory;
        let store = self.store.borrow();
        let mem_view = memory.view(&store);
        mem_view.write(ptr as u64, raw_bytes.as_slice()).unwrap();
    }
}
