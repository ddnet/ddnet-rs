pub use api::*;
pub use api_editor::*;
use editor::editor::{Editor, EditorInterface};
use ui_base::font_data::FontDefinitions;

#[unsafe(no_mangle)]
fn mod_editor_new(font_data: &FontDefinitions) -> Box<dyn EditorInterface> {
    let editor = Editor::new(
        &SOUND.with(|g| (*g).clone()),
        &GRAPHICS.with(|g| (*g).clone()),
        &IO.with(|g| (*g).clone()),
        &RUNTIME_THREAD_POOL,
        font_data,
    );
    Box::new(editor)
}
