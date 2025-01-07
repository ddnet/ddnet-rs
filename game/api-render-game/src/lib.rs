use std::cell::RefCell;
use std::rc::Rc;

use api::read_param_from_host;
use api::GRAPHICS;
use api::GRAPHICS_BACKEND;
use api::SOUND;
use api_wasm_macros::guest_func_call_from_host_auto_dummy;
use api_wasm_macros::{guest_func_call_from_host_auto, impl_guest_functions_render_game};
use client_render_game::render_game::RenderGameCreateOptions;
use client_render_game::render_game::RenderGameInterface;
use config::config::ConfigDebug;
use game_config::config::ConfigMap;

// TODO: remove them
use api::read_param_from_host_ex;
use api::upload_return_val;
use game_interface::chat_commands::ChatCommands;
use graphics_types::types::WindowProps;

extern "Rust" {
    /// returns an instance of the render module
    fn mod_render_game_new(
        map_file: Vec<u8>,
        config: &ConfigDebug,
        props: RenderGameCreateOptions,
    ) -> Result<Box<dyn RenderGameInterface>, String>;
}

pub struct ApiRenderGame {
    state: Rc<RefCell<Option<Box<dyn RenderGameInterface>>>>,
}

thread_local! {
static API_RENDER_GAME: once_cell::unsync::Lazy<ApiRenderGame> =
    once_cell::unsync::Lazy::new(|| ApiRenderGame {
        state: Default::default(),
    });
}

#[no_mangle]
pub fn render_game_new() {
    let map_file: Vec<u8> = read_param_from_host(0);
    let config: ConfigDebug = read_param_from_host(1);
    let window_props: WindowProps = read_param_from_host(2);
    let props: RenderGameCreateOptions = read_param_from_host(3);
    GRAPHICS.with(|g| g.canvas_handle.resized(window_props));
    let res = API_RENDER_GAME.with(|g| g.create(map_file, &config, props));
    upload_return_val(res);
}

impl ApiRenderGame {
    fn create(
        &self,
        map_file: Vec<u8>,
        config: &ConfigDebug,
        props: RenderGameCreateOptions,
    ) -> Result<(), String> {
        let state = unsafe { mod_render_game_new(map_file, config, props)? };
        *self.state.borrow_mut() = Some(state);
        Ok(())
    }
}

#[impl_guest_functions_render_game]
impl ApiRenderGame {
    #[guest_func_call_from_host_auto_dummy]
    fn api_update_window_props(&self) {
        let window_props: WindowProps = read_param_from_host(0);
        GRAPHICS.with(|g| g.canvas_handle.resized(window_props));
    }
}

#[impl_guest_functions_render_game]
impl RenderGameInterface for ApiRenderGame {
    #[guest_func_call_from_host_auto(option)]
    fn render(
        &mut self,
        config_map: &ConfigMap,
        cur_time: &std::time::Duration,
        input: client_render_game::render_game::RenderGameInput,
    ) -> client_render_game::render_game::RenderGameResult {
        GRAPHICS_BACKEND.with(|g| g.actual_run_cmds.set(false));
        GRAPHICS.with(|g| {
            g.backend_handle
                .run_backend_buffer(g.stream_handle.stream_data())
        });
        GRAPHICS_BACKEND.with(|g| g.actual_run_cmds.set(true));
        SOUND.with(|g| g.backend_handle.run_cmds());
    }

    #[guest_func_call_from_host_auto(option)]
    fn continue_loading(&mut self) -> Result<bool, String> {}

    #[guest_func_call_from_host_auto(option)]
    fn set_chat_commands(&mut self, chat_commands: ChatCommands) {}

    #[guest_func_call_from_host_auto(option)]
    fn clear_render_state(&mut self) {}

    #[guest_func_call_from_host_auto(option)]
    fn render_offair_sound(&mut self, samples: u32) {}
}
