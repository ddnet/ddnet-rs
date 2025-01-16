use std::time::Duration;

use client_render_base::map::map::RenderMap;

use crate::{
    client::EditorClient, map::EditorMap, server::EditorServer, tools::auto_saver::AutoSaver,
};

/// a tab, representing a map that is currently edited
pub struct EditorTab {
    pub map: EditorMap,
    pub map_render: RenderMap,
    pub server: Option<EditorServer>,
    pub client: EditorClient,

    pub auto_saver: AutoSaver,

    pub last_info_update: Option<Duration>,
}
