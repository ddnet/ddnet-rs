use std::{rc::Rc, sync::Mutex};

use pool::{mt_datatypes::PoolVec, mt_pool::Pool};
use sendable::SendOption;
use serde::{Serialize, de::DeserializeOwned};
use wasmer::{AsStoreRef, Instance, Memory, StoreMut, StoreRef, TypedFunction};

#[derive(Debug, Clone, Copy)]
pub enum MemoryLimit {
    OneMebiByte,
    TenMebiBytes,
    OneGibiByte,
}

impl MemoryLimit {
    pub const fn limit(&self) -> usize {
        match self {
            Self::OneMebiByte => 1024 * 1024,
            Self::TenMebiBytes => 1024 * 1024 * 10,
            Self::OneGibiByte => 1024 * 1024 * 1024,
        }
    }
}

impl From<MemoryLimit> for usize {
    fn from(val: MemoryLimit) -> Self {
        val.limit()
    }
}

#[derive(Clone)]
pub struct InstanceData {
    pub result_ptr_ptr: i32,
    pub result_size_ptr: i32,
    pub param_ptr_ptrs: [i32; 10],
    pub param_size_ptrs: [i32; 10],
    pub param_alloc_size_ptrs: [i32; 10],
    pub memory: Memory,
    pub prepare_result_func: TypedFunction<u32, ()>,
    pub memory_read_limit: MemoryLimit,
}

pub struct RawBytesEnv {
    raw_bytes: Pool<Vec<u8>>,
    instance: Mutex<SendOption<Rc<InstanceData>>>,
}

impl Default for RawBytesEnv {
    fn default() -> Self {
        Self {
            instance: Default::default(),
            raw_bytes: Pool::with_capacity(10),
        }
    }
}

impl RawBytesEnv {
    pub fn param_index_mut(&self) -> (PoolVec<u8>, Option<Rc<InstanceData>>) {
        let raw_bytes = self.raw_bytes.new();
        (
            raw_bytes,
            self.instance.lock().unwrap().as_ref().map(|s| s.clone()),
        )
    }

    pub fn result_mut(&self) -> PoolVec<u8> {
        self.raw_bytes.new()
    }

    pub fn set_instance(&self, instance: InstanceData) {
        let _ = self.instance.lock().unwrap().insert(Rc::new(instance));
    }
}

pub fn read_global_location(
    instance: &Instance,
    store: &mut StoreMut<'_>,
    global_name: &str,
) -> i32 {
    instance
        .exports
        .get_global(global_name)
        .unwrap()
        .get(store)
        .i32()
        .unwrap()
}

pub fn read_global(memory: &wasmer::Memory, store: &StoreRef<'_>, global_ptr: i32) -> i32 {
    let mem_view = memory.view(store);
    let mut result: [u8; std::mem::size_of::<i32>()] = Default::default();
    mem_view.read(global_ptr as u64, &mut result).unwrap();
    // wasm always uses little-endian
    i32::from_le_bytes(result)
}

pub fn write_global(memory: &wasmer::Memory, store: &StoreRef<'_>, global_ptr: i32, data: i32) {
    let mem_view = memory.view(store);
    // wasm always uses little-endian
    mem_view
        .write(global_ptr as u64, &data.to_le_bytes())
        .unwrap();
}

pub fn read_param<F: DeserializeOwned>(
    instance: &InstanceData,
    store: &StoreRef<'_>,
    byte_buffer: &mut Vec<u8>,
    param_index: usize,
) -> F {
    let raw_bytes = byte_buffer;

    let ptr = read_global(
        &instance.memory,
        store,
        instance.param_ptr_ptrs[param_index],
    );
    let size = read_global(
        &instance.memory,
        store,
        instance.param_size_ptrs[param_index],
    ) as usize;

    let mem_limit: usize = instance.memory_read_limit.into();
    if size > mem_limit {
        panic!("Currently the memory limit is {mem_limit} bytes");
    }
    raw_bytes.resize(size, Default::default());

    let mem_view = instance.memory.view(store);
    mem_view.read(ptr as u64, raw_bytes).unwrap();

    let config = bincode::config::standard().with_fixed_int_encoding();
    match instance.memory_read_limit {
        MemoryLimit::OneMebiByte => bincode::serde::decode_from_slice::<F, _>(
            raw_bytes.as_slice(),
            config.with_limit::<{ 1024 * 1024 }>(),
        ),
        MemoryLimit::TenMebiBytes => bincode::serde::decode_from_slice::<F, _>(
            raw_bytes.as_slice(),
            config.with_limit::<{ 1024 * 1024 * 10 }>(),
        ),
        MemoryLimit::OneGibiByte => bincode::serde::decode_from_slice::<F, _>(
            raw_bytes.as_slice(),
            config.with_limit::<{ 1024 * 1024 * 1024 }>(),
        ),
    }
    .unwrap()
    .0
}

pub fn write_result<F: Serialize>(instance: &InstanceData, store: &mut StoreMut<'_>, param: &F) {
    // encode and upload
    let res = bincode::serde::encode_to_vec::<&F, _>(
        param,
        bincode::config::standard().with_fixed_int_encoding(),
    )
    .unwrap();

    instance
        .prepare_result_func
        .call(store, res.len() as u32)
        .unwrap();

    let ptr = read_global(
        &instance.memory,
        &store.as_store_ref(),
        instance.result_ptr_ptr,
    );

    let memory = &instance.memory;
    let mem_view = memory.view(store);
    mem_view.write(ptr as u64, &res).unwrap();
}
