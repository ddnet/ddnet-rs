use std::{cell::RefCell, num::NonZeroUsize, path::PathBuf, sync::Arc};

use base_io::io::Io;
use config::config::ConfigEngine;
use editor::editor::{Editor, EditorInterface, EditorResult};
use egui::FontDefinitions;
use graphics::graphics::graphics::Graphics;
use sound::sound::SoundManager;

pub struct ApiEditor {
    state: RefCell<Option<Box<dyn EditorInterface>>>,
}

impl ApiEditor {
    fn create(
        &self,
        sound: &SoundManager,
        graphics: &Graphics,
        io: &Io,
        tp: &Arc<rayon::ThreadPool>,
        font_data: &FontDefinitions,
    ) {
        let state = Editor::new(sound, graphics, io, tp, font_data);
        *self.state.borrow_mut() = Some(Box::new(state));
    }
}

thread_local! {
static API_EDITOR: once_cell::unsync::Lazy<ApiEditor> =
    once_cell::unsync::Lazy::new(|| ApiEditor { state: Default::default() });
}

#[no_mangle]
pub fn editor_new(sound: &SoundManager, graphics: &Graphics, io: &Io, font_data: &FontDefinitions) {
    let thread_pool = Arc::new(
        rayon::ThreadPoolBuilder::new()
            .thread_name(|index| format!("editor-rayon {index}"))
            .num_threads(
                std::thread::available_parallelism()
                    .unwrap_or(NonZeroUsize::new(2).unwrap())
                    .get()
                    .max(4)
                    - 2,
            )
            .build()
            .unwrap(),
    );
    API_EDITOR.with(|g| g.create(sound, graphics, io, &thread_pool, font_data));
}

#[no_mangle]
pub fn editor_render(input: egui::RawInput, config: &ConfigEngine) -> EditorResult {
    API_EDITOR.with(|g| g.state.borrow_mut().as_mut().unwrap().render(input, config))
}

#[no_mangle]
pub fn editor_file_dropped(file: PathBuf) {
    API_EDITOR.with(|g| g.state.borrow_mut().as_mut().unwrap().file_dropped(file))
}

#[no_mangle]
pub fn editor_file_hovered(file: Option<PathBuf>) {
    API_EDITOR.with(|g| g.state.borrow_mut().as_mut().unwrap().file_hovered(file))
}

#[no_mangle]
pub fn editor_destroy() {
    API_EDITOR.with(|g| *g.state.borrow_mut() = None);
}
