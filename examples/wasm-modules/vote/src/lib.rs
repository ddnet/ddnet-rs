pub mod vote;

use api::GRAPHICS;
use ui_generic::traits::UiPageInterface;

pub use api_ui::ui_impl::*;
pub use api_ui_game::render::*;

#[unsafe(no_mangle)]
fn mod_ui_new() -> Box<dyn UiPageInterface<()>> {
    Box::new(vote::page::VotePage::new(&GRAPHICS.with(|g| (*g).clone())))
}
