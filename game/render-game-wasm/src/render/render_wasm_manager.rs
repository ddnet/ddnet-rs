use std::{rc::Rc, sync::Arc, time::Duration};

use anyhow::anyhow;
use base::system::{System, SystemTimeInterface};
use base_io::io::Io;
use base_io_traits::fs_traits::{FileSystemInterface, FileSystemWatcherItemInterface};
use cache::Cache;
use client_render_game::render_game::{
    RenderGame, RenderGameCreateOptions, RenderGameInput, RenderGameInterface,
};
use config::config::ConfigDebug;
use game_config::config::ConfigMap;
use game_interface::chat_commands::ChatCommands;
use graphics::{graphics::graphics::Graphics, handles::canvas::canvas::GraphicsCanvasHandle};
use graphics_backend::backend::GraphicsBackend;
use graphics_types::types::WindowProps;
use rayon::ThreadPool;
use sound::sound::SoundManager;
use wasm_runtime::WasmManager;

use super::render_wasm::render_wasm::RenderWasm;

#[derive(Debug, Clone)]
pub enum RenderGameMod {
    Native,
    Wasm { file: Vec<u8> },
}

pub enum RenderGameWrapper {
    Native(RenderGame),
    Wasm(RenderWasm),
}

impl AsRef<dyn RenderGameInterface + 'static> for RenderGameWrapper {
    fn as_ref(&self) -> &(dyn RenderGameInterface + 'static) {
        match self {
            Self::Native(state) => state,
            Self::Wasm(state) => state,
        }
    }
}

impl AsMut<dyn RenderGameInterface + 'static> for RenderGameWrapper {
    fn as_mut(&mut self) -> &mut (dyn RenderGameInterface + 'static) {
        match self {
            Self::Native(state) => state,
            Self::Wasm(state) => state,
        }
    }
}

pub struct RenderGameWasmManager {
    state: RenderGameWrapper,
    fs_change_watcher: Box<dyn FileSystemWatcherItemInterface>,
    canvas_handle: GraphicsCanvasHandle,
    window_props: WindowProps,
}

pub const RENDER_MODS_PATH: &str = "mods/render";

impl RenderGameWasmManager {
    pub async fn load_module(
        fs: &Arc<dyn FileSystemInterface>,
        file: Vec<u8>,
    ) -> anyhow::Result<Vec<u8>> {
        let cache = Arc::new(Cache::<0>::new_async(RENDER_MODS_PATH, fs).await);

        cache
            .load_from_binary(file, |wasm_bytes| {
                Box::pin(async move {
                    Ok(WasmManager::compile_module(&wasm_bytes)?
                        .serialize()?
                        .to_vec())
                })
            })
            .await
    }

    pub fn new(
        sound: &SoundManager,
        graphics: &Graphics,
        backend: &Rc<GraphicsBackend>,
        io: &Io,
        thread_pool: &Arc<ThreadPool>,
        sys: &System,
        map_file: Vec<u8>,
        config: &ConfigDebug,
        render_mod: RenderGameMod,
        props: RenderGameCreateOptions,
    ) -> anyhow::Result<Self> {
        let fs_change_watcher = io
            .fs
            .watch_for_change(RENDER_MODS_PATH.as_ref(), Some("render_game.wasm".as_ref())); // TODO: even tho watching individual files makes more sense, it should still make sure it's the same the server watches

        let state = match render_mod {
            RenderGameMod::Native => {
                let state = RenderGame::new(
                    sound,
                    graphics,
                    io,
                    thread_pool,
                    &sys.time.time_get(),
                    map_file,
                    config,
                    props,
                )
                .map_err(|err| anyhow!(err))?;
                RenderGameWrapper::Native(state)
            }
            RenderGameMod::Wasm { file } => {
                let state =
                    RenderWasm::new(sound, graphics, backend, io, &file, map_file, config, props)?;
                RenderGameWrapper::Wasm(state)
            }
        };
        Ok(Self {
            state,
            fs_change_watcher,
            window_props: graphics.canvas_handle.window_props(),
            canvas_handle: graphics.canvas_handle.clone(),
        })
    }

    pub fn should_reload(&self) -> bool {
        self.fs_change_watcher.has_file_change()
    }
}

impl RenderGameInterface for RenderGameWasmManager {
    fn render(
        &mut self,
        config_map: &ConfigMap,
        cur_time: &Duration,
        input: RenderGameInput,
    ) -> client_render_game::render_game::RenderGameResult {
        if let RenderGameWrapper::Wasm(state) = &self.state {
            let window_props = self.canvas_handle.window_props();
            if window_props != self.window_props {
                state.api_update_window_props(&window_props);
                self.window_props = window_props;
            }
        }
        self.state.as_mut().render(config_map, cur_time, input)
    }

    fn continue_loading(&mut self) -> Result<bool, String> {
        self.state.as_mut().continue_loading()
    }

    fn set_chat_commands(&mut self, chat_commands: ChatCommands) {
        self.state.as_mut().set_chat_commands(chat_commands)
    }

    fn clear_render_state(&mut self) {
        self.state.as_mut().clear_render_state()
    }

    fn render_offair_sound(&mut self, samples: u32) {
        self.state.as_mut().render_offair_sound(samples)
    }
}
