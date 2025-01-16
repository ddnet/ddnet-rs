use std::{rc::Rc, sync::Arc, time::Duration};

use base_fs::filesys::FileSystem;
use base_http::http::HttpClient;
use base_io::io::{Io, IoFileSys};
use clap::Parser;
use editor::editor::{Editor, EditorInterface};
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

fn prepare_backend(io: &Io, tp: &Arc<ThreadPool>) -> (Rc<GraphicsBackend>, GraphicsStreamedData) {
    let config_gfx = config::config::ConfigGfx {
        backend: "null".into(),
    };
    let io_loading = GraphicsBackendIoLoading::new(&config_gfx, &io.clone().into());

    let backend_loading = GraphicsBackendLoading::new(
        &config_gfx,
        &Default::default(),
        &Default::default(),
        BackendRawDisplayHandle::Headless,
        None,
        io.clone().into(),
    )
    .unwrap();
    let (backend_base, stream_data) = GraphicsBackendBase::new(
        io_loading,
        backend_loading,
        tp,
        BackendWindow::Headless {
            width: 64,
            height: 64,
        },
    )
    .unwrap();
    let backend = GraphicsBackend::new(backend_base);

    (backend, stream_data)
}

pub fn get_base() -> (
    Io,
    Arc<ThreadPool>,
    Graphics,
    Rc<GraphicsBackend>,
    SoundManager,
) {
    let io = IoFileSys::new(|rt| {
        Arc::new(
            FileSystem::new(
                rt,
                "ddnet-editor-server",
                "ddnet-editor-server",
                "ddnet-editor-server",
                "ddnet-editor-server",
            )
            .unwrap(),
        )
    });
    let tp = Arc::new(
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap(),
    );

    let io = Io::from(io, Arc::new(HttpClient::new()));

    let (backend, stream_data) = prepare_backend(&io, &tp);

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
                canvas_width: 1920_f64,
                canvas_height: 1080_f64,
                window_width: 1920,
                window_height: 1080,
            },
        ),
        backend,
        sound,
    )
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the map file. (Legacy maps expect mapres to be in the io path, new maps expect map dir to be there).
    /// Map path must be in config dir.
    file: String,
    /// Password to join the server.
    password: String,
    /// Port of the server
    port: u16,
}

fn main() {
    if std::env::var("RUST_LOG").is_err() {
        unsafe { std::env::set_var("RUST_LOG", "info") };
    }
    env_logger::init();

    let (io, tp, graphics, _, sound) = get_base();

    let args = Args::parse();
    let mut editor = Editor::new(&sound, &graphics, &io, &tp, &Default::default());

    let hash = editor
        .host_map(args.file.as_ref(), args.port, args.password)
        .unwrap();

    log::info!("Cert hash: {}", base::hash::fmt_hash(&hash));

    loop {
        editor.render(Default::default(), &Default::default());
        // 100 ticks per second
        std::thread::sleep(Duration::from_millis(10));
    }
}
