use std::{collections::HashSet, path::PathBuf, sync::Arc};

use base::linked_hash_map_view::FxLinkedHashMap;
use base_io::io::Io;
use config::config::ConfigEngine;
use ed25519_dalek::SigningKey;
use egui::{Align2, InputState};
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
use serde::{Deserialize, Serialize};
use sound::scene_object::SceneObject;

use crate::{
    event::ActionDbg,
    hotkeys::{BindsPerEvent, EditorBindsFile, EditorHotkeyEvent},
    image_store_container::ImageStoreContainer,
    notifications::EditorNotifications,
    options::EditorOptions,
    sound_store_container::SoundStoreContainer,
    tab::{EditorAdminPanelStateAuthed, EditorTab},
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
    SaveMapAndClose {
        tab: String,
    },
    SaveAll,
    SaveAllAndClose,
    HostMap(Box<EditorUiEventHostMap>),
    Join {
        ip_port: String,
        cert_hash: String,
        password: String,
        mapper_name: String,
        color: [u8; 3],
    },
    Minimize,
    Close,
    ForceClose,
    Undo,
    Redo,
    CursorWorldPos {
        pos: vec2,
    },
    Chat {
        msg: String,
    },
    AdminAuth {
        password: String,
    },
    AdminChangeConfig {
        state: EditorAdminPanelStateAuthed,
    },
    DbgAction(ActionDbg),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorMenuDialogJoinProps {
    pub ip_port: String,
    pub cert_hash: String,
    pub password: String,
    pub mapper_name: String,
    pub color: [u8; 3],
}

pub enum EditorMenuDialogMode {
    None,
    Open { file_dialog: Box<FileDialog> },
    Save { file_dialog: Box<FileDialog> },
    Host { mode: EditorMenuHostDialogMode },
    Join(EditorMenuDialogJoinProps),
}

impl EditorMenuDialogMode {
    fn icons(dialog: FileDialog) -> FileDialog {
        dialog
            .err_icon("\u{f06a}")
            .device_icon("\u{f390}")
            .default_file_icon("\u{f15b}")
            .default_folder_icon("\u{f07c}")
            .removable_device_icon("\u{f1f8}")
            .labels(egui_file_dialog::FileDialogLabels {
                title_select_directory: "\u{f07c} Select Folder".to_string(),
                title_select_file: "\u{f07c} Open File".to_string(),
                title_select_multiple: "\u{f24d} Select Multiple".to_string(),
                title_save_file: "\u{f0c7} Save File".to_string(),

                cancel: "Cancel".to_string(),
                overwrite: "Overwrite".to_string(),

                reload: "\u{f2f9}  Reload".to_string(),
                show_hidden: " Show hidden".to_string(),
                show_system_files: " Show system files".to_string(),

                heading_pinned: "Pinned".to_string(),
                heading_places: "Places".to_string(),
                heading_devices: "Devices".to_string(),
                heading_removable_devices: "Removable Devices".to_string(),

                home_dir: "\u{f015}  Home".to_string(),
                desktop_dir: "\u{f390}  Desktop".to_string(),
                documents_dir: "\u{f15c}  Documents".to_string(),
                downloads_dir: "\u{f0c7}  Downloads".to_string(),
                audio_dir: "🎵  Audio".to_string(),
                pictures_dir: "\u{f03e}  Pictures".to_string(),
                videos_dir: "\u{f008}  Videos".to_string(),

                pin_folder: "\u{f08d} Pin folder".to_string(),
                unpin_folder: "\u{e68f} Unpin folder".to_string(),

                selected_directory: "Selected directory:".to_string(),
                selected_file: "Selected file:".to_string(),
                selected_items: "Selected items:".to_string(),
                file_name: "File name:".to_string(),
                file_filter_all_files: "All Files".to_string(),

                open_button: "\u{f07b}  Open".to_string(),
                save_button: "\u{f0c7}  Save".to_string(),
                cancel_button: "\u{f05e} Cancel".to_string(),

                overwrite_file_modal_text: "already exists. Do you want to overwrite it?"
                    .to_string(),

                err_empty_folder_name: "Name of the folder cannot be empty".to_string(),
                err_empty_file_name: "The file name cannot be empty".to_string(),
                err_directory_exists: "A directory with the name already exists".to_string(),
                err_file_exists: "A file with the name already exists".to_string(),
            })
    }

    pub fn open(io: &Io) -> Self {
        let mut open_path = io.fs.get_save_path();
        open_path.push("map/maps");

        let mut file_dialog = Box::new(Self::icons(
            FileDialog::new()
                .title("Open Map File")
                .anchor(Align2::CENTER_CENTER, (0.0, 0.0))
                .movable(false)
                .initial_directory(open_path)
                .default_file_name("ctf1.twmap"),
        ));

        file_dialog.pick_file();

        Self::Open { file_dialog }
    }
    pub fn save(io: &Io) -> Self {
        let mut open_path = io.fs.get_save_path();
        open_path.push("map/maps");

        let mut file_dialog = Box::new(Self::icons(
            FileDialog::new()
                .title("Save Map File")
                .anchor(Align2::CENTER_CENTER, (0.0, 0.0))
                .movable(false)
                .initial_directory(open_path)
                .default_file_name("ctf1.twmap"),
        ));

        file_dialog.save_file();

        Self::Save { file_dialog }
    }
    pub fn host(io: &Io) -> Self {
        let mut open_path = io.fs.get_save_path();
        open_path.push("map/maps");

        let mut file_dialog = Box::new(Self::icons(
            FileDialog::new()
                .title("Map File to host")
                .anchor(Align2::CENTER_CENTER, (0.0, 0.0))
                .movable(false)
                .initial_directory(open_path)
                .default_file_name("ctf1.twmap"),
        ));

        file_dialog.pick_file();

        Self::Host {
            mode: EditorMenuHostDialogMode::SelectMap { file_dialog },
        }
    }
    pub fn join(io: &Io) -> Self {
        let fs = io.fs.clone();

        Self::Join(
            io.rt
                .spawn(async move {
                    Ok(serde_json::from_slice(
                        &fs.read_file("editor/join_props.json".as_ref()).await?,
                    )?)
                })
                .get_storage()
                .unwrap_or_else(|_| EditorMenuDialogJoinProps {
                    ip_port: Default::default(),
                    cert_hash: Default::default(),
                    password: Default::default(),
                    mapper_name: "nameless mapper".to_string(),
                    color: [255, 255, 255],
                }),
        )
    }
}

#[derive(Debug)]
pub enum EditorModalDialogMode {
    None,
    CloseTab { tab: String },
    CloseEditor,
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
    pub notifications: &'a EditorNotifications,

    pub canvas_handle: &'a GraphicsCanvasHandle,
    pub stream_handle: &'a GraphicsStreamHandle,
    pub unused_rect: &'a mut Option<egui::Rect>,
    pub input_state: &'a mut Option<InputState>,
    pub canvas_size: &'a mut Option<UiCanvasSize>,
    pub menu_dialog_mode: &'a mut EditorMenuDialogMode,
    pub modal_dialog_mode: &'a mut EditorModalDialogMode,
    pub tools: &'a mut Tools,

    pub editor_options: &'a mut EditorOptions,

    pub auto_mapper: &'a mut TileLayerAutoMapper,

    pub pointer_is_used: &'a mut bool,
    pub io: &'a Io,

    pub tp: &'a Arc<rayon::ThreadPool>,
    pub graphics_mt: &'a GraphicsMultiThreaded,
    pub buffer_object_handle: &'a GraphicsBufferObjectHandle,
    pub backend_handle: &'a GraphicsBackendHandle,

    pub quad_tile_images_container: &'a mut ImageStoreContainer,
    pub sound_images_container: &'a mut SoundStoreContainer,
    pub container_scene: &'a SceneObject,

    pub hotkeys: &'a mut EditorBindsFile,
    pub cur_hotkey_events: &'a mut HashSet<EditorHotkeyEvent>,
    pub cached_binds_per_event: &'a mut Option<BindsPerEvent>,
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

    pub editor_options: &'a mut EditorOptions,

    pub auto_mapper: &'a mut TileLayerAutoMapper,

    pub tp: &'a Arc<rayon::ThreadPool>,
    pub graphics_mt: &'a GraphicsMultiThreaded,
    pub buffer_object_handle: &'a GraphicsBufferObjectHandle,
    pub backend_handle: &'a GraphicsBackendHandle,

    pub quad_tile_images_container: &'a mut ImageStoreContainer,
    pub sound_images_container: &'a mut SoundStoreContainer,
    pub container_scene: &'a SceneObject,

    pub hotkeys: &'a mut EditorBindsFile,
    pub cur_hotkey_events: &'a mut HashSet<EditorHotkeyEvent>,
    pub cached_binds_per_event: &'a mut Option<BindsPerEvent>,
}
