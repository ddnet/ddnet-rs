use std::{path::PathBuf, sync::Arc};

use base::linked_hash_map_view::FxLinkedHashMap;
use base_io::io::Io;
use config::config::ConfigEngine;
use ed25519_dalek::SigningKey;
use egui::InputState;
use egui_file_dialog::FileDialog;
use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::{
        backend::backend::GraphicsBackendHandle,
        buffer_object::buffer_object::GraphicsBufferObjectHandle,
        canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
    },
};
use math::math::vector::vec2;

use crate::{
    tab::EditorTab,
    tools::{tile_layer::auto_mapper::TileLayerAutoMapper, tool::Tools},
    utils::UiCanvasSize,
};

#[derive(Debug)]
pub struct EditorUiEventHostMap {
    pub map_path: PathBuf,
    pub port: u16,
    pub password: String,
    pub cert: x509_cert::Certificate,
    pub private_key: SigningKey,

    pub mapper_name: String,
    pub color: [u8; 3],
}

#[derive(Debug)]
pub enum EditorUiEvent {
    NewMap,
    OpenFile {
        name: PathBuf,
    },
    SaveFile {
        name: PathBuf,
    },
    SaveCurMap,
    HostMap(Box<EditorUiEventHostMap>),
    Join {
        ip_port: String,
        cert_hash: String,
        password: String,
        mapper_name: String,
        color: [u8; 3],
    },
    Close,
    Undo,
    Redo,
    CursorWorldPos {
        pos: vec2,
    },
}

pub struct EditorMenuHostNetworkOptions {
    pub map_path: PathBuf,
    pub port: u16,
    pub password: String,
    pub cert: x509_cert::Certificate,
    pub private_key: SigningKey,
    pub mapper_name: String,
    pub color: [u8; 3],
}

pub enum EditorMenuHostDialogMode {
    SelectMap { file_dialog: Box<FileDialog> },
    HostNetworkOptions(Box<EditorMenuHostNetworkOptions>),
}

pub enum EditorMenuDialogMode {
    None,
    Open {
        file_dialog: Box<FileDialog>,
    },
    Save {
        file_dialog: Box<FileDialog>,
    },
    Host {
        mode: EditorMenuHostDialogMode,
    },
    Join {
        ip_port: String,
        cert_hash: String,
        password: String,
        mapper_name: String,
        color: [u8; 3],
    },
}

impl EditorMenuDialogMode {
    pub fn open(io: &Io) -> Self {
        let mut open_path = io.fs.get_save_path();
        open_path.push("map/maps");

        let mut file_dialog = Box::new(
            FileDialog::new()
                .title("Open Map File")
                .movable(false)
                .initial_directory(open_path)
                .default_file_name("ctf1.twmap"),
        );

        file_dialog.pick_file();

        Self::Open { file_dialog }
    }
    pub fn save(io: &Io) -> Self {
        let mut open_path = io.fs.get_save_path();
        open_path.push("map/maps");

        let mut file_dialog = Box::new(
            FileDialog::new()
                .title("Save Map File")
                .movable(false)
                .initial_directory(open_path)
                .default_file_name("ctf1.twmap"),
        );

        file_dialog.save_file();

        Self::Save { file_dialog }
    }
    pub fn host(io: &Io) -> Self {
        let mut open_path = io.fs.get_save_path();
        open_path.push("map/maps");

        let mut file_dialog = Box::new(
            FileDialog::new()
                .title("Map File to host")
                .movable(false)
                .initial_directory(open_path)
                .default_file_name("ctf1.twmap"),
        );

        file_dialog.pick_file();

        Self::Host {
            mode: EditorMenuHostDialogMode::SelectMap { file_dialog },
        }
    }
    pub fn join() -> Self {
        Self::Join {
            ip_port: Default::default(),
            cert_hash: Default::default(),
            password: Default::default(),
            mapper_name: "nameless mapper".to_string(),
            color: [255, 255, 255],
        }
    }
}

pub struct EditorTabsRefMut<'a> {
    pub tabs: &'a mut FxLinkedHashMap<String, EditorTab>,
    pub active_tab: &'a mut String,
}

impl EditorTabsRefMut<'_> {
    pub fn active_tab(&mut self) -> Option<&mut EditorTab> {
        self.tabs.get_mut(self.active_tab)
    }
}

pub struct UserData<'a> {
    pub ui_events: &'a mut Vec<EditorUiEvent>,
    pub config: &'a ConfigEngine,
    pub editor_tabs: EditorTabsRefMut<'a>,
    pub canvas_handle: &'a GraphicsCanvasHandle,
    pub stream_handle: &'a GraphicsStreamHandle,
    pub unused_rect: &'a mut Option<egui::Rect>,
    pub input_state: &'a mut Option<InputState>,
    pub canvas_size: &'a mut Option<UiCanvasSize>,
    pub menu_dialog_mode: &'a mut EditorMenuDialogMode,
    pub tools: &'a mut Tools,
    pub auto_mapper: &'a mut TileLayerAutoMapper,
    pub pointer_is_used: &'a mut bool,
    pub io: &'a Io,

    pub tp: &'a Arc<rayon::ThreadPool>,
    pub graphics_mt: &'a GraphicsMultiThreaded,
    pub buffer_object_handle: &'a GraphicsBufferObjectHandle,
    pub backend_handle: &'a GraphicsBackendHandle,
}

pub struct UserDataWithTab<'a> {
    pub ui_events: &'a mut Vec<EditorUiEvent>,
    pub config: &'a ConfigEngine,
    pub canvas_handle: &'a GraphicsCanvasHandle,
    pub stream_handle: &'a GraphicsStreamHandle,
    pub editor_tab: &'a mut EditorTab,
    pub tools: &'a mut Tools,
    pub pointer_is_used: &'a mut bool,
    pub io: &'a Io,

    pub tp: &'a Arc<rayon::ThreadPool>,
    pub graphics_mt: &'a GraphicsMultiThreaded,
    pub buffer_object_handle: &'a GraphicsBufferObjectHandle,
    pub backend_handle: &'a GraphicsBackendHandle,
}
