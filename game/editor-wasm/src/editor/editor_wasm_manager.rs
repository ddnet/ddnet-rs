use std::{path::PathBuf, rc::Rc, sync::Arc};

use base_io::io::Io;
use base_io_traits::fs_traits::{FileSystemPath, FileSystemType, FileSystemWatcherItemInterface};
use cache::Cache;
use config::config::ConfigEngine;
use editor::editor::{Editor, EditorInterface, EditorResult};
use egui::FontDefinitions;
use graphics::graphics::graphics::Graphics;
use graphics_backend::backend::GraphicsBackend;
use rayon::ThreadPool;
use sound::sound::SoundManager;
use wasm_runtime::WasmManager;

use super::{editor_lib::editor_lib::EditorLib, editor_wasm::editor_wasm::EditorWasm};

pub enum EditorWrapper {
    Native(Editor),
    NativeLib(EditorLib),
    Wasm(EditorWasm),
}

impl AsRef<dyn EditorInterface + 'static> for EditorWrapper {
    fn as_ref(&self) -> &(dyn EditorInterface + 'static) {
        match self {
            Self::Native(state) => state,
            Self::NativeLib(state) => state,
            Self::Wasm(state) => state,
        }
    }
}

impl AsMut<dyn EditorInterface + 'static> for EditorWrapper {
    fn as_mut(&mut self) -> &mut (dyn EditorInterface + 'static) {
        match self {
            Self::Native(state) => state,
            Self::NativeLib(state) => state,
            Self::Wasm(state) => state,
        }
    }
}

pub struct EditorWasmManager {
    state: EditorWrapper,
    fs_change_watcher: Box<dyn FileSystemWatcherItemInterface>,
    fs_change_watcher_lib: Box<dyn FileSystemWatcherItemInterface>,
}

const MODS_PATH: &str = "mods/editor";

impl EditorWasmManager {
    pub fn new(
        sound: &SoundManager,
        graphics: &Graphics,
        backend: &Rc<GraphicsBackend>,
        io: &Io,
        thread_pool: &Arc<ThreadPool>,
        font_data: &FontDefinitions,
    ) -> Self {
        let cache = Arc::new(Cache::<0>::new(MODS_PATH, io));
        // check if loading was finished
        let path_str = MODS_PATH.to_string() + "/editor.wasm";
        let fs_change_watcher = io
            .fs
            .watch_for_change(MODS_PATH.as_ref(), Some("editor.wasm".as_ref())); // TODO: even tho watching individual files makes more sense, it should still make sure it's the same the server watches
        let fs_change_watcher_lib = io
            .fs
            .watch_for_change(MODS_PATH.as_ref(), Some("libeditor.so".as_ref())); // TODO: even tho watching individual files makes more sense, it should still make sure it's the same the server watches

        let cache_task = cache.clone();
        let task = io.rt.spawn(async move {
            cache_task
                .load(path_str.as_ref(), |wasm_bytes| {
                    Box::pin(async move {
                        Ok(WasmManager::compile_module(&wasm_bytes)?
                            .serialize()?
                            .to_vec())
                    })
                })
                .await
        });
        let state = if let Ok(wasm_module) = task.get_storage() {
            let state = EditorWasm::new(sound, graphics, backend, io, font_data, &wasm_module);
            EditorWrapper::Wasm(state)
        } else {
            let path_str = MODS_PATH.to_string() + "/libeditor.so";
            let save_path: PathBuf = path_str.into();
            let name_task = io.rt.spawn(async move {
                cache
                    .archieve(
                        &save_path,
                        FileSystemPath::OfType(FileSystemType::ReadWrite),
                    )
                    .await
            });
            let name = name_task.get_storage();
            if let Ok(name) = name {
                let lib_path = io.fs.get_cache_path().join(name);
                if let Ok(lib) = unsafe { libloading::Library::new(&lib_path) } {
                    EditorWrapper::NativeLib(EditorLib::new(sound, graphics, io, font_data, lib))
                } else {
                    let state = Editor::new(sound, graphics, io, thread_pool, font_data);
                    EditorWrapper::Native(state)
                }
            } else {
                let state = Editor::new(sound, graphics, io, thread_pool, font_data);
                EditorWrapper::Native(state)
            }
        };
        Self {
            state,
            fs_change_watcher,
            fs_change_watcher_lib,
        }
    }

    pub fn should_reload(&self) -> bool {
        self.fs_change_watcher.has_file_change() || self.fs_change_watcher_lib.has_file_change()
    }
}

impl EditorInterface for EditorWasmManager {
    fn render(&mut self, input: egui::RawInput, config: &ConfigEngine) -> EditorResult {
        self.state.as_mut().render(input, config)
    }

    fn file_dropped(&mut self, file: PathBuf) {
        self.state.as_mut().file_dropped(file)
    }

    fn file_hovered(&mut self, file: Option<PathBuf>) {
        self.state.as_mut().file_hovered(file)
    }
}

#[derive(Default)]
pub enum EditorState {
    #[default]
    None,
    Open(EditorWasmManager),
    Minimized(EditorWasmManager),
}

impl EditorState {
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Open(_))
    }

    pub fn should_reload(&self) -> bool {
        match self {
            EditorState::Open(editor) | EditorState::Minimized(editor) => editor.should_reload(),
            EditorState::None => false,
        }
    }
}
