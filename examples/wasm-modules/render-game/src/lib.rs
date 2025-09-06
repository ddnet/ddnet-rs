use std::time::Duration;

pub use api::*;
pub use api_render_game::*;
use client_render_game::render_game::{RenderGame, RenderGameCreateOptions, RenderGameInterface};
use config::config::ConfigDebug;

#[unsafe(no_mangle)]
fn mod_render_game_new(
    map_file: Vec<u8>,
    config: &ConfigDebug,
    props: RenderGameCreateOptions,
) -> Result<Box<dyn RenderGameInterface>, String> {
    let state = RenderGame::new(
        &SOUND.with(|g| (*g).clone()),
        &GRAPHICS.with(|g| (*g).clone()),
        &IO.with(|g| (*g).clone()),
        &RUNTIME_THREAD_POOL,
        &Duration::ZERO,
        map_file,
        config,
        props,
    )?;
    Ok(Box::new(state))
}
