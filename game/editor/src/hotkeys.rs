use std::collections::{HashMap, HashSet};

use base_io::{io::Io, runtime::IoRuntimeTask};
use base_io_traits::fs_traits::FileSystemInterface;
use egui::{Key, KeyboardShortcut, ModifierNames, Modifiers};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum EditorHotkeyEventFile {
    New,
    Open,
    Save,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum EditorHotkeyEventEdit {
    Undo,
    Redo,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum EditorHotkeyEventTimeline {
    InsertPoint,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum EditorHotkeyEventTileBrush {
    FlipX,
    FlipY,
    RotPlus90,
    RotMinus90,
    RotIndividualTilePlus90,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum EditorHotkeyEventTileTool {
    Brush(EditorHotkeyEventTileBrush),
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum EditorHotkeyEventQuadBrush {
    Square,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum EditorHotkeyEventQuadTool {
    Brush(EditorHotkeyEventQuadBrush),
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum EditorHotkeyEventSoundBrush {
    ToggleShape,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum EditorHotkeyEventSoundTool {
    Brush(EditorHotkeyEventSoundBrush),
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum EditorHotkeyEventSharedTool {
    AddQuadOrSound,
    DeleteQuadOrSound,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum EditorHotkeyEventTools {
    Tile(EditorHotkeyEventTileTool),
    Quad(EditorHotkeyEventQuadTool),
    Sound(EditorHotkeyEventSoundTool),
    Shared(EditorHotkeyEventSharedTool),
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum EditorHotkeyEventTabs {
    Previous,
    Next,
    Close,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum EditorHotkeyEventPreferences {
    ShowTileLayerIndices,
    ToggleParallaxZoom,
    IncreaseMapTimeSpeed,
    DecreaseMapTimeSpeed,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum EditorHotkeyEventPanels {
    ToggleAnimation,
    ToggleServerCommands,
    ToggleServerConfigVars,
    ToggleAssetsStore,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum EditorHotkeyEventMap {
    MoveLayerUp,
    MoveLayerDown,
    DeleteLayer,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum EditorHotkeyEvent {
    /// Tool related events.
    Tools(EditorHotkeyEventTools),
    /// Timeline related stuff, e.g. adding new anim points.
    Timeline(EditorHotkeyEventTimeline),
    /// Switching, e.g. closing tabs etc.
    Tabs(EditorHotkeyEventTabs),
    /// Most options, e.g. tile index rendering.
    Preferences(EditorHotkeyEventPreferences),
    /// Open panels, e.g. animation panel, server settings, assets store.
    Panels(EditorHotkeyEventPanels),
    /// Map related events, e.g. delte current layer, changing order of groups etc.
    Map(EditorHotkeyEventMap),
    /// File operations, e.g. save, open.
    File(EditorHotkeyEventFile),
    /// Edit operations, e.g. undo, redo.
    Edit(EditorHotkeyEventEdit),
    /// Wants to chat
    Chat,
    /// Switch to a debug mode
    DbgMode,
}

pub type EditorBinds = HashMap<KeyboardShortcut, EditorHotkeyEvent>;

#[derive(Debug, Default, Clone)]
pub struct EditorBindsFile {
    pub binds: EditorBinds,
    /// Shortcuts for these hotkey events were changed at least once
    /// indicating that default values should not be loaded.
    pub changed_at_least_once: HashSet<EditorHotkeyEvent>,
}

#[serde_as]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct EditorBindsSer {
    #[serde_as(as = "serde_with::VecSkipError<(_, _)>")]
    pub binds: Vec<(KeyboardShortcut, EditorHotkeyEvent)>,
    pub changed_at_least_once: HashSet<EditorHotkeyEvent>,
}

const BINDS_FILE_PATH: &str = "editor/hotkeys.json";
pub type BindsPerEvent = HashMap<EditorHotkeyEvent, Vec<KeyboardShortcut>>;

impl EditorBindsFile {
    pub fn load_file(io: &Io) -> IoRuntimeTask<EditorBindsFile> {
        let fs = io.fs.clone();
        io.rt.spawn(async move {
            let file = fs.read_file(BINDS_FILE_PATH.as_ref()).await?;
            let file: EditorBindsSer = serde_json::from_slice(&file)?;
            Ok(EditorBindsFile {
                binds: file.binds.into_iter().collect(),
                changed_at_least_once: file.changed_at_least_once,
            })
        })
    }

    pub fn apply_defaults(&mut self) {
        let needs_default = |ev: EditorHotkeyEvent| !self.changed_at_least_once.contains(&ev);

        let mut hotkey = |ev: EditorHotkeyEvent, default_shotcut: KeyboardShortcut| {
            if needs_default(ev) {
                self.binds.insert(default_shotcut, ev);
            }
        };
        hotkey(
            EditorHotkeyEvent::Chat,
            KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter),
        );
        hotkey(
            EditorHotkeyEvent::DbgMode,
            KeyboardShortcut::new(Modifiers::ALT, Key::F12),
        );
        hotkey(
            EditorHotkeyEvent::File(EditorHotkeyEventFile::New),
            KeyboardShortcut::new(Modifiers::CTRL.plus(Modifiers::SHIFT), Key::N),
        );
        hotkey(
            EditorHotkeyEvent::File(EditorHotkeyEventFile::Open),
            KeyboardShortcut::new(Modifiers::CTRL.plus(Modifiers::SHIFT), Key::O),
        );
        hotkey(
            EditorHotkeyEvent::File(EditorHotkeyEventFile::Save),
            KeyboardShortcut::new(Modifiers::CTRL, Key::S),
        );
        hotkey(
            EditorHotkeyEvent::Edit(EditorHotkeyEventEdit::Redo),
            KeyboardShortcut::new(Modifiers::CTRL.plus(Modifiers::SHIFT), Key::Z),
        );
        hotkey(
            EditorHotkeyEvent::Edit(EditorHotkeyEventEdit::Redo),
            KeyboardShortcut::new(Modifiers::CTRL.plus(Modifiers::SHIFT), Key::Y),
        );
        hotkey(
            EditorHotkeyEvent::Edit(EditorHotkeyEventEdit::Undo),
            KeyboardShortcut::new(Modifiers::CTRL, Key::Z),
        );
        hotkey(
            EditorHotkeyEvent::Edit(EditorHotkeyEventEdit::Undo),
            KeyboardShortcut::new(Modifiers::CTRL, Key::Y),
        );
        hotkey(
            EditorHotkeyEvent::Timeline(EditorHotkeyEventTimeline::InsertPoint),
            KeyboardShortcut::new(Default::default(), Key::I),
        );
        hotkey(
            EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                EditorHotkeyEventTileTool::Brush(EditorHotkeyEventTileBrush::FlipX),
            )),
            KeyboardShortcut::new(Default::default(), Key::N),
        );
        hotkey(
            EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                EditorHotkeyEventTileTool::Brush(EditorHotkeyEventTileBrush::FlipY),
            )),
            KeyboardShortcut::new(Default::default(), Key::M),
        );
        hotkey(
            EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                EditorHotkeyEventTileTool::Brush(EditorHotkeyEventTileBrush::RotMinus90),
            )),
            KeyboardShortcut::new(Default::default(), Key::R),
        );
        hotkey(
            EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                EditorHotkeyEventTileTool::Brush(EditorHotkeyEventTileBrush::RotPlus90),
            )),
            KeyboardShortcut::new(Default::default(), Key::T),
        );
        hotkey(
            EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                EditorHotkeyEventTileTool::Brush(
                    EditorHotkeyEventTileBrush::RotIndividualTilePlus90,
                ),
            )),
            KeyboardShortcut::new(Default::default(), Key::G),
        );
        hotkey(
            EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Shared(
                EditorHotkeyEventSharedTool::AddQuadOrSound,
            )),
            KeyboardShortcut::new(Modifiers::CTRL, Key::Q),
        );
        hotkey(
            EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Shared(
                EditorHotkeyEventSharedTool::DeleteQuadOrSound,
            )),
            KeyboardShortcut::new(Modifiers::CTRL, Key::Delete),
        );
        hotkey(
            EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Quad(
                EditorHotkeyEventQuadTool::Brush(EditorHotkeyEventQuadBrush::Square),
            )),
            KeyboardShortcut::new(Modifiers::CTRL.plus(Modifiers::SHIFT), Key::Q),
        );
        hotkey(
            EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Sound(
                EditorHotkeyEventSoundTool::Brush(EditorHotkeyEventSoundBrush::ToggleShape),
            )),
            KeyboardShortcut::new(Modifiers::CTRL.plus(Modifiers::SHIFT), Key::T),
        );
        hotkey(
            EditorHotkeyEvent::Tabs(EditorHotkeyEventTabs::Previous),
            KeyboardShortcut::new(Modifiers::CTRL, Key::ArrowRight),
        );
        hotkey(
            EditorHotkeyEvent::Tabs(EditorHotkeyEventTabs::Next),
            KeyboardShortcut::new(Modifiers::CTRL, Key::ArrowLeft),
        );
        hotkey(
            EditorHotkeyEvent::Tabs(EditorHotkeyEventTabs::Close),
            KeyboardShortcut::new(Modifiers::CTRL, Key::W),
        );
        hotkey(
            EditorHotkeyEvent::Preferences(EditorHotkeyEventPreferences::ShowTileLayerIndices),
            KeyboardShortcut::new(Modifiers::CTRL, Key::I),
        );
        hotkey(
            EditorHotkeyEvent::Preferences(EditorHotkeyEventPreferences::ToggleParallaxZoom),
            KeyboardShortcut::new(Modifiers::CTRL, Key::P),
        );
        hotkey(
            EditorHotkeyEvent::Preferences(EditorHotkeyEventPreferences::IncreaseMapTimeSpeed),
            KeyboardShortcut::new(Modifiers::CTRL, Key::Plus),
        );
        hotkey(
            EditorHotkeyEvent::Preferences(EditorHotkeyEventPreferences::DecreaseMapTimeSpeed),
            KeyboardShortcut::new(Modifiers::CTRL, Key::Minus),
        );
        hotkey(
            EditorHotkeyEvent::Panels(EditorHotkeyEventPanels::ToggleAnimation),
            KeyboardShortcut::new(Modifiers::CTRL, Key::A),
        );
        hotkey(
            EditorHotkeyEvent::Panels(EditorHotkeyEventPanels::ToggleServerCommands),
            KeyboardShortcut::new(Modifiers::CTRL, Key::N),
        );
        hotkey(
            EditorHotkeyEvent::Panels(EditorHotkeyEventPanels::ToggleServerConfigVars),
            KeyboardShortcut::new(Modifiers::CTRL, Key::M),
        );
        hotkey(
            EditorHotkeyEvent::Panels(EditorHotkeyEventPanels::ToggleAssetsStore),
            KeyboardShortcut::new(Modifiers::CTRL, Key::O),
        );
        hotkey(
            EditorHotkeyEvent::Map(EditorHotkeyEventMap::MoveLayerUp),
            KeyboardShortcut::new(Modifiers::SHIFT, Key::ArrowUp),
        );
        hotkey(
            EditorHotkeyEvent::Map(EditorHotkeyEventMap::MoveLayerDown),
            KeyboardShortcut::new(Modifiers::SHIFT, Key::ArrowDown),
        );
        hotkey(
            EditorHotkeyEvent::Map(EditorHotkeyEventMap::DeleteLayer),
            KeyboardShortcut::new(Modifiers::default(), Key::Delete),
        );
    }

    pub async fn save(&self, fs: &dyn FileSystemInterface) -> anyhow::Result<()> {
        let _ = fs.create_dir("editor".as_ref()).await;
        fs.write_file(
            BINDS_FILE_PATH.as_ref(),
            serde_json::to_vec_pretty(&EditorBindsSer {
                binds: self.binds.clone().into_iter().collect(),
                changed_at_least_once: self.changed_at_least_once.clone(),
            })?,
        )
        .await?;
        Ok(())
    }

    pub fn binds_per_event(&self) -> BindsPerEvent {
        let mut binds_per_event: BindsPerEvent = Default::default();
        for (bind, event) in self.binds.iter() {
            binds_per_event.entry(*event).or_default().push(*bind);
        }
        // sort for consistent order
        for (_, keys) in binds_per_event.iter_mut() {
            #[derive(Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
            pub struct CmpModifiers {
                pub alt: bool,
                pub ctrl: bool,
                pub shift: bool,
                pub mac_cmd: bool,
                pub command: bool,
            }
            impl From<Modifiers> for CmpModifiers {
                fn from(value: Modifiers) -> Self {
                    Self {
                        alt: value.alt,
                        ctrl: value.ctrl,
                        shift: value.shift,
                        mac_cmd: value.mac_cmd,
                        command: value.command,
                    }
                }
            }
            keys.sort_by(|b1, b2| {
                (CmpModifiers::from(b1.modifiers), b1.logical_key)
                    .cmp(&(CmpModifiers::from(b2.modifiers), b2.logical_key))
            });
        }
        binds_per_event
    }

    pub fn fmt_ev_bind(
        &self,
        binds_per_event: &mut Option<BindsPerEvent>,
        ev: &EditorHotkeyEvent,
    ) -> String {
        let binds_per_event = binds_per_event.get_or_insert_with(|| self.binds_per_event());

        binds_per_event
            .get(ev)
            .and_then(|b| b.first())
            .map(|b| b.format(&ModifierNames::NAMES, false))
            .unwrap_or_else(|| "None".to_string())
    }
}
