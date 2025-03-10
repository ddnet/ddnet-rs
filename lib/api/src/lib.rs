#![allow(clippy::module_inception)]

use std::{
    cell::{Cell, RefCell},
    ptr::addr_of_mut,
    rc::Rc,
    sync::Arc,
};

use ::graphics::graphics::graphics::Graphics;
use ::sound::sound::SoundManager;
use anyhow::anyhow;
use base_fs::filesys::FileSystem;
use base_http::http::HttpClient;
use base_io::{
    io::{create_runtime, Io},
    runtime::IoRuntime,
};
use database::GameDbBackend;
use game_database::traits::DbInterface;
use graphics::graphics::GraphicsBackend;
use graphics_base_traits::traits::{
    GraphicsStreamVertices, GraphicsStreamedData, GraphicsStreamedUniformData,
    GraphicsStreamedUniformRawData,
};
use graphics_types::{
    commands::{
        StreamDataMax, GRAPHICS_DEFAULT_UNIFORM_SIZE, GRAPHICS_MAX_UNIFORM_RENDER_COUNT,
        GRAPHICS_UNIFORM_INSTANCE_COUNT,
    },
    types::WindowProps,
};
use pool::mt_datatypes::PoolVec;
use serde::{de::DeserializeOwned, Serialize};
use sound::sound_backend::SoundBackend;

pub mod base_fs;
pub mod base_http;
pub mod database;
pub mod graphics;
pub mod sound;

extern "C" {
    fn host_println();
}

thread_local! {
pub static GRAPHICS_BACKEND: std::cell::LazyCell<Rc<GraphicsBackend>> =
    std::cell::LazyCell::new(|| {
        Rc::new(GraphicsBackend {
            actual_run_cmds: Cell::new(true),
            sync_points: Default::default(),
        })
    });

pub static GRAPHICS: std::cell::LazyCell<Graphics> = std::cell::LazyCell::new(|| {
    let mut uniform_buffers: pool::mt_recycle::Recycle<Vec<GraphicsStreamedUniformData>> =
        PoolVec::new_without_pool();
    uniform_buffers.resize_with(GRAPHICS_UNIFORM_INSTANCE_COUNT, || {
        GraphicsStreamedUniformData::new(GraphicsStreamedUniformRawData::Vector(
            vec![0; GRAPHICS_MAX_UNIFORM_RENDER_COUNT * GRAPHICS_DEFAULT_UNIFORM_SIZE],
        ))
    });
    Graphics::new(
        GRAPHICS_BACKEND.with(|d| (*d).clone()),
        GraphicsStreamedData::new(
            GraphicsStreamVertices::Vec(vec![
                Default::default();
                StreamDataMax::MaxVertices as usize
            ]),
            uniform_buffers,
        ),
        WindowProps {
            canvas_width: 800,
            canvas_height: 600,
            window_width: 800.0,
            window_height: 600.0,
        },
    )
});

pub static SOUND: once_cell::unsync::Lazy<SoundManager> =
    once_cell::unsync::Lazy::new(|| {
        let mut uniform_buffers = PoolVec::new_without_pool();
        uniform_buffers.resize_with(GRAPHICS_UNIFORM_INSTANCE_COUNT, || {
            GraphicsStreamedUniformData::new(GraphicsStreamedUniformRawData::Vector(
                vec![0; GRAPHICS_MAX_UNIFORM_RENDER_COUNT * GRAPHICS_DEFAULT_UNIFORM_SIZE],
            ))
        });
        SoundManager::new(Rc::new(SoundBackend {})).unwrap()
    });

pub static IO: once_cell::unsync::Lazy<Io> = once_cell::unsync::Lazy::new(|| {
    Io::new(
        |_| Arc::new(FileSystem::new()),
        Arc::new(HttpClient::new()),
    )
});

pub static IO_RUNTIME: once_cell::unsync::Lazy<IoRuntime> =
    once_cell::unsync::Lazy::new(|| {
        let rt = create_runtime();
        IoRuntime::new(rt)
    });

pub static DB: once_cell::unsync::Lazy<Arc<dyn DbInterface>> =
    once_cell::unsync::Lazy::new(|| Arc::new(GameDbBackend::default()));
}

pub static RUNTIME_THREAD_POOL: once_cell::sync::Lazy<Arc<rayon::ThreadPool>> =
    once_cell::sync::Lazy::new(|| {
        Arc::new(
            rayon::ThreadPoolBuilder::default()
                .num_threads(1)
                .use_current_thread()
                .build()
                .unwrap(),
        )
    });

// for system
#[no_mangle]
fn sys_print(log_str: &str) {
    println(log_str);
}

pub struct Logger {}

impl log::Log for Logger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        println(format!(
            "wasm log msg: {}-{}: {} in {}:{}",
            record.level(),
            record.target(),
            record.args(),
            record.module_path().unwrap_or(""),
            record.line().unwrap_or_default()
        ));
    }

    fn flush(&self) {}
}

#[no_mangle]
pub fn api_setup() {
    std::panic::set_hook(Box::new(|panic_info| {
        let panic_text = format!("wasm module {}", panic_info);
        println(panic_text);
    }));
    log::set_boxed_logger(Box::new(Logger {})).unwrap();
    log::set_max_level(log::LevelFilter::Info);
}

// shared memory
thread_local! {
static RES: RefCell<Vec<u8>> = Default::default();
}
#[no_mangle]
pub static mut RESULT_PTR: i32 = 0;
#[no_mangle]
pub static mut RESULT_SIZE: i32 = 0;

static mut PARAMS: once_cell::unsync::Lazy<[Vec<u8>; 10]> =
    once_cell::unsync::Lazy::new(Default::default);

#[no_mangle]
pub static mut PARAM0_PTR: i32 = 0;
#[no_mangle]
pub static mut PARAM0_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM0_ALLOC_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM1_PTR: i32 = 0;
#[no_mangle]
pub static mut PARAM1_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM1_ALLOC_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM2_PTR: i32 = 0;
#[no_mangle]
pub static mut PARAM2_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM2_ALLOC_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM3_PTR: i32 = 0;
#[no_mangle]
pub static mut PARAM3_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM3_ALLOC_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM4_PTR: i32 = 0;
#[no_mangle]
pub static mut PARAM4_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM4_ALLOC_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM5_PTR: i32 = 0;
#[no_mangle]
pub static mut PARAM5_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM5_ALLOC_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM6_PTR: i32 = 0;
#[no_mangle]
pub static mut PARAM6_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM6_ALLOC_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM7_PTR: i32 = 0;
#[no_mangle]
pub static mut PARAM7_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM7_ALLOC_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM8_PTR: i32 = 0;
#[no_mangle]
pub static mut PARAM8_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM8_ALLOC_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM9_PTR: i32 = 0;
#[no_mangle]
pub static mut PARAM9_SIZE: i32 = 0;
#[no_mangle]
pub static mut PARAM9_ALLOC_SIZE: i32 = 0;

unsafe fn param_ptrs(index: usize) -> (*mut i32, *mut i32, *mut i32) {
    match index {
        0 => (
            addr_of_mut!(PARAM0_PTR),
            addr_of_mut!(PARAM0_SIZE),
            addr_of_mut!(PARAM0_ALLOC_SIZE),
        ),
        1 => (
            addr_of_mut!(PARAM1_PTR),
            addr_of_mut!(PARAM1_SIZE),
            addr_of_mut!(PARAM1_ALLOC_SIZE),
        ),
        2 => (
            addr_of_mut!(PARAM2_PTR),
            addr_of_mut!(PARAM2_SIZE),
            addr_of_mut!(PARAM2_ALLOC_SIZE),
        ),
        3 => (
            addr_of_mut!(PARAM3_PTR),
            addr_of_mut!(PARAM3_SIZE),
            addr_of_mut!(PARAM3_ALLOC_SIZE),
        ),
        4 => (
            addr_of_mut!(PARAM4_PTR),
            addr_of_mut!(PARAM4_SIZE),
            addr_of_mut!(PARAM4_ALLOC_SIZE),
        ),
        5 => (
            addr_of_mut!(PARAM5_PTR),
            addr_of_mut!(PARAM5_SIZE),
            addr_of_mut!(PARAM5_ALLOC_SIZE),
        ),
        6 => (
            addr_of_mut!(PARAM6_PTR),
            addr_of_mut!(PARAM6_SIZE),
            addr_of_mut!(PARAM6_ALLOC_SIZE),
        ),
        7 => (
            addr_of_mut!(PARAM7_PTR),
            addr_of_mut!(PARAM7_SIZE),
            addr_of_mut!(PARAM7_ALLOC_SIZE),
        ),
        8 => (
            addr_of_mut!(PARAM8_PTR),
            addr_of_mut!(PARAM8_SIZE),
            addr_of_mut!(PARAM8_ALLOC_SIZE),
        ),
        9 => (
            addr_of_mut!(PARAM9_PTR),
            addr_of_mut!(PARAM9_SIZE),
            addr_of_mut!(PARAM9_ALLOC_SIZE),
        ),
        _ => panic!("unsupported param index"),
    }
}

fn set_param_params(index: usize, len: usize) {
    unsafe {
        let (ptr, size, alloc_size) = param_ptrs(index);
        *ptr = PARAMS[index].as_ptr() as i32;
        *size = len as i32;
        *alloc_size = PARAMS[index].len() as i32;
    }
}

pub fn upload_param<F: Serialize>(index: usize, data: F) {
    let prev_len = unsafe {
        let prev_len = PARAMS[index].len();
        PARAMS[index].clear();
        prev_len
    };

    let res = bincode::serde::encode_into_std_write::<F, _, Vec<_>>(
        data,
        unsafe { &mut PARAMS[index] },
        bincode::config::standard().with_fixed_int_encoding(),
    );

    let data_len = unsafe {
        let data_len = PARAMS[index].len();
        PARAMS[index].set_len(prev_len.max(data_len));
        data_len
    };

    set_param_params(index, data_len);

    res.unwrap();
}

pub fn println<F: Serialize + std::string::ToString>(text: F) {
    upload_param(0, text.to_string());
    unsafe { host_println() };
}

pub fn upload_return_val<F: Serialize>(res: F) {
    RES.with(|g| {
        let mut res_container = g.borrow_mut();

        res_container.clear();

        bincode::serde::encode_into_std_write::<F, _, _>(
            res,
            &mut *res_container,
            bincode::config::standard().with_fixed_int_encoding(),
        )
        .unwrap();

        unsafe {
            RESULT_PTR = res_container.as_ptr() as i32;
            RESULT_SIZE = res_container.len() as i32;
        }
    })
}

fn read_param_from_host_checked<F: DeserializeOwned>(index: u32) -> anyhow::Result<F> {
    unsafe {
        let (_, size, _) = param_ptrs(index as usize);
        bincode::serde::decode_from_slice(
            &PARAMS[index as usize].as_slice()[0..*size as usize],
            bincode::config::standard().with_fixed_int_encoding(),
        )
        .map(|opt| opt.0)
        .map_err(|err| {
            anyhow!(
                "failed to decode the given parameter (size: {}): {err}",
                PARAMS[index as usize].len()
            )
        })
    }
}

pub fn read_param_from_host<F: DeserializeOwned>(index: u32) -> F {
    read_param_from_host_checked(index)
        .map_err(|err| anyhow!("failed to read param {index}: {err}"))
        .unwrap()
}

pub fn read_param_from_host_ex<F: DeserializeOwned>(
    index: u32,
    ty_name: &str,
    caller_name: &str,
) -> F {
    unsafe {
        read_param_from_host_checked::<F>(index).unwrap_or_else(|e| {
            panic!(
                "error decoding type \"{}\" ({:?}): {} called by {}",
                ty_name, PARAMS[index as usize], e, caller_name
            )
        })
    }
}

fn read_result_from_host_checked<F: DeserializeOwned>() -> Result<F, bincode::error::DecodeError> {
    RES.with(|g| {
        bincode::serde::decode_from_slice(
            g.borrow().as_slice(),
            bincode::config::standard().with_fixed_int_encoding(),
        )
        .map(|opt| opt.0)
    })
}

pub fn read_result_from_host<F: DeserializeOwned>() -> F {
    read_result_from_host_checked().unwrap()
}

#[no_mangle]
pub fn prepare_param(index: u32, expected_size: u32) {
    unsafe {
        let cur_size = PARAMS[index as usize].len();
        PARAMS[index as usize].resize(cur_size.max(expected_size as usize), Default::default());
    }
    set_param_params(index as usize, expected_size as usize);
}

#[no_mangle]
pub fn prepare_result(expected_size: u32) {
    RES.with(|g| {
        let mut g = g.borrow_mut();
        g.resize(expected_size as usize, Default::default());
        unsafe {
            RESULT_PTR = g.as_ptr() as i32;
            RESULT_SIZE = g.len() as i32;
        }
    })
}
