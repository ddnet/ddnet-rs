use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use api::read_param_from_host;
use api::upload_return_val;
use api::GRAPHICS;
use api::GRAPHICS_BACKEND;
use api::SOUND;
use api_wasm_macros::{guest_func_call_from_host_auto, impl_guest_functions_editor};

// TODO: remove them
use api::read_param_from_host_ex;
use config::config::ConfigEngine;
use editor::editor::EditorInterface;
use editor::editor::EditorResult;
use egui::FontDefinitions;
use graphics_types::types::WindowProps;

extern "Rust" {
    /// returns an instance of the game state and the game tick speed
    fn mod_editor_new(font_data: &FontDefinitions) -> Box<dyn EditorInterface>;
}

pub struct ApiEditor {
    state: Rc<RefCell<Option<Box<dyn EditorInterface>>>>,
}

impl ApiEditor {
    fn create(&self, font_data: &FontDefinitions) {
        let state = unsafe { mod_editor_new(font_data) };
        *self.state.borrow_mut() = Some(state);
    }
}

thread_local! {
static API_EDITOR: once_cell::unsync::Lazy<ApiEditor> =
    once_cell::unsync::Lazy::new(|| ApiEditor {
        state: Default::default(),
    });
}

#[no_mangle]
pub fn editor_new() {
    let window_props: WindowProps = read_param_from_host(0);
    let font_data: FontDefinitions = read_param_from_host(1);
    GRAPHICS.with(|g| g.canvas_handle.resized(window_props));

    API_EDITOR.with(|g| g.create(&font_data));
}

#[impl_guest_functions_editor]
impl EditorInterface for ApiEditor {
    #[guest_func_call_from_host_auto(option)]
    fn render(&mut self, input: egui::RawInput, config: &ConfigEngine) -> EditorResult {
        GRAPHICS_BACKEND.with(|g| g.actual_run_cmds.set(false));
        GRAPHICS.with(|g| {
            g.backend_handle
                .run_backend_buffer(g.stream_handle.stream_data())
        });
        GRAPHICS_BACKEND.with(|g| g.actual_run_cmds.set(true));
        SOUND.with(|g| g.backend_handle.run_cmds())
    }

    #[guest_func_call_from_host_auto(option)]
    fn file_dropped(&mut self, file: PathBuf) {}

    #[guest_func_call_from_host_auto(option)]
    fn file_hovered(&mut self, file: Option<PathBuf>) {}
}
