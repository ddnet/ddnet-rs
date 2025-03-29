use std::{collections::VecDeque, sync::Arc};

use egui::{Key, Modifiers};

use crate::hotkeys::{EditorBindsFile, EditorHotkeyEvent};

#[derive(Debug)]
pub struct EditorHotkeyEdit {
    pub modifiers: Modifiers,
    pub key: Option<Key>,

    /// Edit for this event
    pub ev: EditorHotkeyEvent,
}

#[derive(Debug, Default)]
pub struct EditorOptions {
    pub hotkeys_open: bool,
    pub hotkeys_edit: Option<EditorHotkeyEdit>,
    pub hotkeys_write_in_order: Arc<tokio::sync::Mutex<VecDeque<EditorBindsFile>>>,
}
