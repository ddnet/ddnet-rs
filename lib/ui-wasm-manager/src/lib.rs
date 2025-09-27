#![allow(clippy::too_many_arguments)]

use std::{collections::HashMap, rc::Rc, sync::Arc, time::Duration};

use anyhow::anyhow;
use base_io::{io::Io, runtime::IoRuntimeTask};
use base_io_traits::fs_traits::FileSystemWatcherItemInterface;
use cache::Cache;

use config::config::ConfigPath;
use graphics::graphics::graphics::Graphics;
use graphics_backend::backend::GraphicsBackend;

use hiarc::{Hiarc, hiarc_safer_rc_refcell};
use serde::Serialize;
use sound::sound::SoundManager;
use tracing::instrument;
use ui_base::{
    types::{RawInputWrapper, RawOutputWrapper, UiFonts, UiRenderPipe},
    ui::{UiContainer, UiCreator},
};
use ui_generic::{generic_ui_renderer, traits::UiPageInterface};
use wasm_logic_fs::fs::WasmFileSystemLogic;
use wasm_logic_graphics::WasmGraphicsLogic;
use wasm_logic_http::http::WasmHttpLogic;
use wasm_logic_sound::sound::WasmSoundLogic;
use wasm_runtime::{MemoryLimit, WasmManager, WasmManagerModuleType};
use wasmer::Module;

pub struct UiWasmPageEntry {
    wasm_runtime: WasmManager,
}

impl UiWasmPageEntry {
    fn call_new(&mut self, fonts: &UiFonts) -> anyhow::Result<()> {
        self.wasm_runtime.add_param(0, fonts);
        self.wasm_runtime.run_by_name::<()>("ui_new")
    }

    fn wasm_call_mount(&mut self) -> anyhow::Result<()> {
        self.wasm_runtime.run_by_name::<()>("ui_mount")
    }

    fn wasm_call_unmount(&mut self) -> anyhow::Result<()> {
        self.wasm_runtime.run_by_name::<()>("ui_unmount")
    }

    #[instrument(level = "trace", skip_all)]
    fn call(
        &mut self,
        cur_time: &Duration,
        graphics: &Graphics,
        input: RawInputWrapper,
        zoom_level: Option<f32>,
    ) -> anyhow::Result<RawOutputWrapper> {
        self.wasm_runtime.add_param(0, cur_time);
        self.wasm_runtime
            .add_param(1, &graphics.canvas_handle.window_props());
        self.wasm_runtime.add_param(2, &input);
        self.wasm_runtime.add_param(3, &zoom_level);
        self.wasm_runtime.run_by_name::<()>("ui_run")?;
        let res = self.wasm_runtime.get_result_as::<RawOutputWrapper>();
        Ok(res)
    }
}

enum UiPageEntry<U> {
    Wasm(Box<UiWasmPageEntry>),
    Native(Box<dyn UiPageInterface<U>>),
}

#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc, Default)]
pub struct UiWasmManagerErrorPageErr {
    err: String,
}

#[hiarc_safer_rc_refcell]
impl UiWasmManagerErrorPageErr {
    pub fn set(&mut self, err: String) {
        self.err = err;
    }
    pub fn get(&self) -> String {
        self.err.clone()
    }
}

pub struct UiManagerBase<U>
where
    for<'a> U: 'a,
{
    ui_paths: HashMap<String, UiPageEntry<U>>,
    ui_paths_loading: HashMap<String, anyhow::Result<IoRuntimeTask<Vec<u8>>>>,
    cache: Arc<Cache<202306060000>>,
    show_cur_page_during_load: bool,

    pub ui: UiContainer,
    last_path: String,

    fs_change_watcher: Box<dyn FileSystemWatcherItemInterface>,

    /// id offset for textures buffers etc. that come from the wasm module's graphics
    id_offset: u128,

    fonts: UiFonts,

    err: UiWasmManagerErrorPageErr,
}

pub enum UiPageRunReturn {
    Loading,
    Success(egui::PlatformOutput),
    Error404(String),
    RuntimeError(anyhow::Error),
}

pub enum UiPageLoadingType<U> {
    ShowCurrentPage,
    ShowLoadingPage(Box<dyn UiPageInterface<U>>),
}

const MODS_PATH: &str = "mods/ui";

impl<U: Serialize> UiManagerBase<U>
where
    for<'a> U: 'a,
{
    pub fn new(
        io: &Io,
        error_404_page: (Box<dyn UiPageInterface<U>>, UiWasmManagerErrorPageErr),
        loading_page: UiPageLoadingType<U>,
        creator: &UiCreator,
    ) -> Self {
        let cache = Arc::new(Cache::new(MODS_PATH, io));
        let mut ui_paths = HashMap::<String, UiPageEntry<U>>::default();
        let mut show_cur_page_during_load = false;
        match loading_page {
            UiPageLoadingType::ShowCurrentPage => show_cur_page_during_load = true,
            UiPageLoadingType::ShowLoadingPage(page) => {
                ui_paths.insert("000".to_string(), UiPageEntry::Native(page));
            }
        }
        let (error_404_page, error_404_err) = error_404_page;
        ui_paths.insert("404".to_string(), UiPageEntry::Native(error_404_page));
        let fs_change_watcher = io.fs.watch_for_change(MODS_PATH.as_ref(), None);
        let ui = UiContainer::new(creator);
        let fonts = UiFonts {
            fonts: creator.font_definitions.clone(),
        };

        Self {
            ui_paths,
            ui_paths_loading: Default::default(),
            show_cur_page_during_load,
            cache,
            ui,
            last_path: "404".to_string(),
            fs_change_watcher,

            id_offset: u64::MAX as u128,

            fonts,

            err: error_404_err,
        }
    }

    /// returns Some, if the path was already registered
    /// Re-registers/overwrites the path with the new callback in this case
    pub fn register_path(
        &mut self,
        mod_name: &str,
        path: &str,
        cb: Box<dyn UiPageInterface<U>>,
    ) -> Option<()> {
        ConfigPath::is_route_correct(mod_name, path).unwrap();
        self.ui_paths
            .insert(
                if !mod_name.is_empty() {
                    mod_name.to_string() + "/"
                } else {
                    "".to_string()
                } + path,
                UiPageEntry::Native(cb),
            )
            .map(|_| ())
    }

    #[instrument(level = "trace", skip_all)]
    fn mount_path(ui: &mut UiPageEntry<U>) {
        match ui {
            UiPageEntry::Wasm(cb) => {
                let _ = cb.wasm_call_mount();
            }
            UiPageEntry::Native(cb) => cb.mount(),
        }
    }

    #[instrument(level = "trace", skip_all)]
    fn unmount_path(ui: &mut UiPageEntry<U>) {
        match ui {
            UiPageEntry::Wasm(cb) => {
                let _ = cb.wasm_call_unmount();
            }
            UiPageEntry::Native(cb) => {
                cb.unmount();
            }
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip_all)]
    pub fn run_ui_path(
        &mut self,
        path: &str,
        io: &Io,
        graphics: &Graphics,
        backend: &Rc<GraphicsBackend>,
        sound: &mut SoundManager,
        pipe: &mut UiRenderPipe<U>,
        inp: egui::RawInput,
        blur: bool,
    ) -> UiPageRunReturn {
        match self.ui_paths.get_mut(path) {
            Some(cb) => {
                // check if the ui is freshly mounted
                if self.last_path != path {
                    Self::mount_path(cb);
                }
                match cb {
                    UiPageEntry::Wasm(ui) => {
                        if !blur {
                            self.ui.ui_state.blur_shapes.clear();
                        }
                        // for wasm modules, blurs are 1 frame delayed
                        generic_ui_renderer::render_blur_if_needed(
                            &graphics.backend_handle,
                            &graphics.texture_handle,
                            &graphics.stream_handle,
                            &graphics.canvas_handle,
                            &mut self.ui,
                        );
                        match ui.call(
                            &pipe.cur_time,
                            graphics,
                            RawInputWrapper { input: inp },
                            self.ui.zoom_level.get(),
                        ) {
                            Ok(output) => {
                                self.ui.zoom_level.set(output.zoom_level);
                                self.ui.ui_state.blur_shapes = output.blur_shapes;
                                UiPageRunReturn::Success(output.output)
                            }
                            Err(err) => UiPageRunReturn::RuntimeError(err),
                        }
                    }
                    UiPageEntry::Native(cb) => {
                        let res = generic_ui_renderer::render_ex(
                            &graphics.backend_handle,
                            &graphics.texture_handle,
                            &graphics.stream_handle,
                            &graphics.canvas_handle,
                            &mut self.ui,
                            cb.as_mut(),
                            pipe,
                            inp,
                            blur,
                        );
                        UiPageRunReturn::Success(res)
                    }
                }
            }
            None => {
                // check if the path is loading
                if let Some(loading_entry) = self.ui_paths_loading.remove(path) {
                    // check if loading was finished
                    if loading_entry.as_ref().is_ok_and(|task| !task.is_finished()) {
                        self.ui_paths_loading
                            .insert(path.to_string(), loading_entry);
                        UiPageRunReturn::Loading
                    } else {
                        match loading_entry {
                            Ok(loading_entry) => match loading_entry.get() {
                                Ok(item) => {
                                    let graphics_logic = WasmGraphicsLogic::new(
                                        graphics,
                                        backend.clone(),
                                        self.id_offset,
                                    );
                                    let sound_logic = WasmSoundLogic::new(self.id_offset, sound);
                                    let fs_logic = WasmFileSystemLogic::new(io.clone());
                                    let http_logic = WasmHttpLogic::new(io.clone());
                                    self.id_offset += u64::MAX as u128;
                                    let wasm_runtime: WasmManager = WasmManager::new(
                                        WasmManagerModuleType::FromClosure(|store| {
                                            match unsafe { Module::deserialize(store, &item[..]) } {
                                                Ok(module) => Ok(module),
                                                Err(err) => Err(anyhow!(err)),
                                            }
                                        }),
                                        |store, raw_bytes_env| {
                                            let mut imports = graphics_logic
                                                .get_wasm_logic_imports(store, raw_bytes_env);
                                            imports.extend(
                                                &sound_logic
                                                    .get_wasm_logic_imports(store, raw_bytes_env),
                                            );
                                            imports.extend(
                                                &fs_logic
                                                    .get_wasm_logic_imports(store, raw_bytes_env),
                                            );
                                            imports.extend(
                                                &http_logic
                                                    .get_wasm_logic_imports(store, raw_bytes_env),
                                            );
                                            Some(imports)
                                        },
                                        MemoryLimit::OneGibiByte,
                                    )
                                    .unwrap();
                                    let mut entry = UiWasmPageEntry { wasm_runtime };
                                    entry.call_new(&self.fonts).unwrap();
                                    self.ui_paths.insert(
                                        path.to_string(),
                                        UiPageEntry::Wasm(Box::new(entry)),
                                    );
                                    self.run_ui_path(
                                        path, io, graphics, backend, sound, pipe, inp, blur,
                                    )
                                }
                                Err(err) => {
                                    let err_str = err.to_string();
                                    self.ui_paths_loading.insert(path.to_string(), Err(err));
                                    UiPageRunReturn::Error404(err_str)
                                }
                            },
                            Err(err) => {
                                let err_str = err.to_string();
                                self.ui_paths_loading.insert(path.to_string(), Err(err));
                                UiPageRunReturn::Error404(err_str)
                            }
                        }
                    }
                } else {
                    let path_str = MODS_PATH.to_string() + "/" + path + ".wasm";
                    let cache = self.cache.clone();
                    let task = io.rt.spawn(async move {
                        cache
                            .load(path_str.as_ref(), |wasm_bytes| {
                                Box::pin(async move {
                                    Ok(WasmManager::compile_module(&wasm_bytes)?
                                        .serialize()?
                                        .to_vec())
                                })
                            })
                            .await
                    });
                    self.ui_paths_loading.insert(path.to_string(), Ok(task));

                    UiPageRunReturn::Loading
                }
            }
        }
    }

    #[instrument(level = "trace", skip_all)]
    pub fn render(
        &mut self,
        path: &str,
        io: &Io,
        graphics: &Graphics,
        backend: &Rc<GraphicsBackend>,
        sound: &mut SoundManager,
        pipe: &mut UiRenderPipe<U>,
        inp: egui::RawInput,
        blur: bool,
    ) -> Option<egui::PlatformOutput> {
        // check for changes
        if self.fs_change_watcher.has_file_change() {
            // clear all paths that are dynamically loaded
            let (keep_paths, mut destroy_paths) =
                self.ui_paths.drain().partition(|(_, item)| match item {
                    UiPageEntry::Wasm(_) => false,
                    UiPageEntry::Native(_) => true,
                });
            self.ui_paths = keep_paths;
            destroy_paths.clear();
            self.ui_paths_loading.clear();
        }

        // check if the current path unmounted
        if self.last_path != path
            && let Some(cb) = self.ui_paths.get_mut(&self.last_path)
        {
            Self::unmount_path(cb);
        }

        let (success, platform_output) =
            match self.run_ui_path(path, io, graphics, backend, sound, pipe, inp.clone(), blur) {
                UiPageRunReturn::Loading => {
                    if self.show_cur_page_during_load {
                        match self.run_ui_path(
                            &self.last_path.clone(),
                            io,
                            graphics,
                            backend,
                            sound,
                            pipe,
                            inp.clone(),
                            blur,
                        ) {
                            UiPageRunReturn::Loading => (
                                false,
                                self.render(
                                    "404",
                                    io,
                                    graphics,
                                    backend,
                                    sound,
                                    pipe,
                                    inp.clone(),
                                    blur,
                                ),
                            ),
                            UiPageRunReturn::Success(output) => (false, Some(output)),
                            UiPageRunReturn::Error404(err) => {
                                self.err.set(err);
                                (
                                    false,
                                    self.render(
                                        "404",
                                        io,
                                        graphics,
                                        backend,
                                        sound,
                                        pipe,
                                        inp.clone(),
                                        blur,
                                    ),
                                )
                            }
                            UiPageRunReturn::RuntimeError(_) => (
                                false,
                                self.render("404", io, graphics, backend, sound, pipe, inp, blur),
                            ),
                        }
                    } else {
                        match self.run_ui_path(
                            "000",
                            io,
                            graphics,
                            backend,
                            sound,
                            pipe,
                            inp.clone(),
                            blur,
                        ) {
                            UiPageRunReturn::Loading => {
                                panic!("this should never happen")
                            }
                            UiPageRunReturn::Success(output) => (false, Some(output)),
                            UiPageRunReturn::Error404(_) => {
                                panic!("this should never happen")
                            }
                            UiPageRunReturn::RuntimeError(_) => {
                                panic!("this should never happen")
                            }
                        }
                    }
                }
                UiPageRunReturn::Success(output) => (true, Some(output)),
                UiPageRunReturn::Error404(err) => {
                    self.err.set(err);
                    match self.run_ui_path("404", io, graphics, backend, sound, pipe, inp, blur) {
                        UiPageRunReturn::Loading => {
                            panic!("this should never happen")
                        }
                        UiPageRunReturn::Success(output) => (false, Some(output)),
                        UiPageRunReturn::Error404(_) => {
                            panic!("this should never happen")
                        }
                        UiPageRunReturn::RuntimeError(_) => {
                            panic!("this should never happen")
                        }
                    }
                }
                UiPageRunReturn::RuntimeError(err) => todo!("{err}"),
            };

        if success {
            self.last_path = path.to_string();
        }
        platform_output
    }
}
