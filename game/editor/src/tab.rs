use std::time::Duration;

use client_render_base::map::map::RenderMap;
use sound::{sound_listener::SoundListener, sound_play_handle::SoundPlayHandle};

use crate::{
    client::EditorClient,
    event::{ActionDbg, AdminChangeConfig},
    map::EditorMap,
    server::EditorServer,
    tools::auto_saver::AutoSaver,
};

#[derive(Debug, Default, Clone)]
pub struct EditorAdminPanelStateNonAuthed {
    pub password: String,
}

pub type EditorAdminPanelStateAuthed = AdminChangeConfig;

#[derive(Debug, Clone)]
pub enum EditorAdminPanelState {
    NonAuthed(EditorAdminPanelStateNonAuthed),
    Authed(EditorAdminPanelStateAuthed),
}

impl Default for EditorAdminPanelState {
    fn default() -> Self {
        Self::NonAuthed(Default::default())
    }
}

#[derive(Debug, Default)]
pub struct EditorAdminPanel {
    pub open: bool,

    pub state: EditorAdminPanelState,
}

#[derive(Debug, Default)]
pub struct DbgPanel {
    /// Show the btn in the tools
    pub show: bool,
    pub open: bool,

    pub props: ActionDbg,
    pub run: bool,
}

#[derive(Debug, Default)]
pub enum AssetsStoreTab {
    #[default]
    Tileset,
    Image,
    Sound,
}

#[derive(Debug, Default)]
pub struct AssetsStore {
    pub search: String,
    pub selected_entry: String,
    pub tab: AssetsStoreTab,

    pub cur_play: Option<(String, SoundPlayHandle, SoundListener)>,
}

/// a tab, representing a map that is currently edited
pub struct EditorTab {
    pub map: EditorMap,
    pub map_render: RenderMap,
    pub server: Option<EditorServer>,
    pub client: EditorClient,

    pub auto_saver: AutoSaver,

    pub last_info_update: Option<Duration>,

    pub admin_panel: EditorAdminPanel,

    pub dbg_panel: DbgPanel,

    pub assets_store_open: bool,
    pub assets_store: AssetsStore,
}
