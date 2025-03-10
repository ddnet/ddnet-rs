use std::{rc::Rc, sync::Arc};

use base::benchmark::Benchmark;
use base_fs::filesys::FileSystem;
use base_http::http::HttpClient;
use base_io::io::{Io, IoFileSys};
use config::{
    config::{ConfigBackend, ConfigWindow},
    types::ConfRgb,
};
use graphics::graphics::graphics::Graphics;
use graphics_backend::{
    backend::{
        GraphicsBackend, GraphicsBackendBase, GraphicsBackendIoLoading, GraphicsBackendLoading,
    },
    window::{BackendRawDisplayHandle, BackendWindow},
};
use graphics_base_traits::traits::GraphicsStreamedData;
use graphics_types::types::WindowProps;
use rayon::ThreadPool;
use sound::sound::SoundManager;
use sound_backend::sound_backend::SoundBackend;

pub struct Options {
    pub width: u32,
    pub height: u32,
}

fn prepare_backend(
    io: &Io,
    tp: &Arc<ThreadPool>,
    config_gl: &ConfigBackend,
    config_wnd: &config::config::ConfigWindow,
    backend_validation: bool,
) -> (Rc<GraphicsBackend>, GraphicsStreamedData) {
    let config_gfx = config::config::ConfigGfx::default();
    let io_loading = GraphicsBackendIoLoading::new(&config_gfx, &io.clone().into());
    let config_dbg = config::config::ConfigDebug {
        bench: true,
        gfx: if backend_validation {
            config::config::GfxDebugModes::All
        } else {
            config::config::GfxDebugModes::None
        },
        ..Default::default()
    };

    let bench = Benchmark::new(true);
    let backend_loading = GraphicsBackendLoading::new(
        &config_gfx,
        &config_dbg,
        config_gl,
        BackendRawDisplayHandle::Headless,
        None,
        io.clone().into(),
    )
    .unwrap();
    bench.bench("backend loading");
    let (backend_base, stream_data) = GraphicsBackendBase::new(
        io_loading,
        backend_loading,
        tp,
        BackendWindow::Headless {
            width: config_wnd.window_width as u32,
            height: config_wnd.window_height as u32,
        },
    )
    .unwrap();
    bench.bench("backend base init");
    let backend = GraphicsBackend::new(backend_base);
    bench.bench("backend init");

    (backend, stream_data)
}

pub fn get_base(
    backend_validation: bool,
    options: Option<Options>,
) -> (
    Io,
    Arc<ThreadPool>,
    Graphics,
    Rc<GraphicsBackend>,
    SoundManager,
) {
    let io = IoFileSys::new(|rt| {
        Arc::new(
            FileSystem::new(rt, "ddnet-test", "ddnet-test", "ddnet-test", "ddnet-test").unwrap(),
        )
    });
    let tp = Arc::new(
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap(),
    );

    let io = Io::from(io, Arc::new(HttpClient::new()));

    let config_gl = ConfigBackend {
        clear_color: ConfRgb::grey(),
        full_pipeline_creation: false,
        ..Default::default()
    };
    let mut config_wnd = ConfigWindow::default();
    if let Some(options) = options {
        config_wnd.window_width = options.width as f64;
        config_wnd.window_height = options.height as f64;
    }
    let (backend, stream_data) =
        prepare_backend(&io, &tp, &config_gl, &config_wnd, backend_validation);

    let sound_backend = SoundBackend::new(&config::config::ConfigSound {
        backend: "None".to_string(),
        limits: Default::default(),
    })
    .unwrap();
    let sound = SoundManager::new(sound_backend.clone()).unwrap();

    (
        io,
        tp,
        Graphics::new(
            backend.clone(),
            stream_data,
            WindowProps {
                canvas_width: config_wnd.window_width as u32,
                canvas_height: config_wnd.window_height as u32,
                window_width: config_wnd.window_width,
                window_height: config_wnd.window_height,
            },
        ),
        backend,
        sound,
    )
}
