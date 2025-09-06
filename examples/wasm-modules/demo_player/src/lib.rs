pub mod demo_player;

use api::GRAPHICS;
use ui_generic::traits::UiPageInterface;

pub use api_ui::ui_impl::*;
pub use api_ui_game::render::*;

#[unsafe(no_mangle)]
fn mod_ui_new() -> Box<dyn UiPageInterface<()>> {
    Box::new(demo_player::page::DemoPlayerPage::new(
        &GRAPHICS.with(|g| (*g).clone()),
    ))
}
