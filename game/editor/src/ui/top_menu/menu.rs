use std::{path::PathBuf, time::Duration};

use base::hash::fmt_hash;
use egui::{Align2, Button, DragValue, Grid, TextEdit, Window};
use egui_file_dialog::{DialogMode, DialogState};
use network::network::utils::create_certifified_keys;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    explain::TEXT_ANIM_PANEL_AND_PROPS,
    hotkeys::{
        EditorHotkeyEvent, EditorHotkeyEventEdit, EditorHotkeyEventFile, EditorHotkeyEventPanels,
        EditorHotkeyEventPreferences,
    },
    tab::EditorAdminPanelState,
    ui::user_data::{
        EditorMenuDialogJoinProps, EditorMenuDialogMode, EditorMenuHostDialogMode,
        EditorMenuHostNetworkOptions, EditorUiEvent, EditorUiEventHostMap, UserData,
    },
};

pub fn render(ui: &mut egui::Ui, ui_state: &mut UiState, pipe: &mut UiRenderPipe<UserData>) {
    let style = ui.style();
    // 4.0 is some margin for strokes
    let height = style.spacing.interact_size.y + style.spacing.item_spacing.y + 4.0;
    let res = egui::TopBottomPanel::top("top_menu")
        .resizable(false)
        .default_height(height)
        .height_range(height..=height)
        .show_inside(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                let menu_dialog_mode = &mut *pipe.user_data.menu_dialog_mode;

                ui.horizontal(|ui| {
                    ui.menu_button("File", |ui| {
                        let cur_hotkeys = &mut *pipe.user_data.cur_hotkey_events;
                        let binds = &*pipe.user_data.hotkeys;
                        let per_ev = &mut *pipe.user_data.cached_binds_per_event;
                        let by_hotkey = cur_hotkeys
                            .remove(&EditorHotkeyEvent::File(EditorHotkeyEventFile::New));
                        if ui
                            .add(Button::new("New map").shortcut_text(binds.fmt_ev_bind(
                                per_ev,
                                &EditorHotkeyEvent::File(EditorHotkeyEventFile::New),
                            )))
                            .clicked()
                            || by_hotkey
                        {
                            pipe.user_data.ui_events.push(EditorUiEvent::NewMap);
                        }
                        let by_hotkey = cur_hotkeys
                            .remove(&EditorHotkeyEvent::File(EditorHotkeyEventFile::Open));
                        if ui
                            .add(Button::new("Open map").shortcut_text(binds.fmt_ev_bind(
                                per_ev,
                                &EditorHotkeyEvent::File(EditorHotkeyEventFile::Open),
                            )))
                            .clicked()
                            || by_hotkey
                        {
                            *menu_dialog_mode = EditorMenuDialogMode::open(pipe.user_data.io);
                        }
                        if ui
                            .add(Button::new("Save map").shortcut_text(binds.fmt_ev_bind(
                                per_ev,
                                &EditorHotkeyEvent::File(EditorHotkeyEventFile::Save),
                            )))
                            .clicked()
                        {
                            *menu_dialog_mode = EditorMenuDialogMode::save(pipe.user_data.io);
                        }
                        ui.separator();
                        if ui.button("Host map").clicked() {
                            *menu_dialog_mode = EditorMenuDialogMode::host(pipe.user_data.io);
                        }
                        if ui.button("Join map").clicked() {
                            *menu_dialog_mode = EditorMenuDialogMode::join(pipe.user_data.io);
                        }
                        ui.separator();
                        if ui.button("Minimize").clicked() {
                            pipe.user_data.ui_events.push(EditorUiEvent::Minimize);
                        }
                        if ui.button("Close").clicked() {
                            pipe.user_data.ui_events.push(EditorUiEvent::Close);
                        }
                    });

                    ui.menu_button("Edit", |ui| {
                        let binds = &*pipe.user_data.hotkeys;
                        let per_ev = &mut *pipe.user_data.cached_binds_per_event;
                        ui.set_min_width(250.0);
                        let undo_label = pipe.user_data.editor_tabs.active_tab().and_then(|t| {
                            t.server
                                .as_ref()
                                .map(|s| s.undo_label())
                                .unwrap_or_else(|| t.client.undo_label.clone())
                        });
                        if ui
                            .add_enabled(
                                undo_label.is_some(),
                                Button::new(format!(
                                    "Undo{}",
                                    undo_label.map(|l| format!(": {l}")).unwrap_or_default()
                                ))
                                .shortcut_text(binds.fmt_ev_bind(
                                    per_ev,
                                    &EditorHotkeyEvent::Edit(EditorHotkeyEventEdit::Undo),
                                )),
                            )
                            .clicked()
                        {
                            pipe.user_data.ui_events.push(EditorUiEvent::Undo);
                        }
                        let redo_label = pipe.user_data.editor_tabs.active_tab().and_then(|t| {
                            t.server
                                .as_ref()
                                .map(|s| s.redo_label())
                                .unwrap_or_else(|| t.client.redo_label.clone())
                        });
                        if ui
                            .add_enabled(
                                redo_label.is_some(),
                                Button::new(format!(
                                    "Redo{}",
                                    redo_label.map(|l| format!(": {l}")).unwrap_or_default()
                                ))
                                .shortcut_text(binds.fmt_ev_bind(
                                    per_ev,
                                    &EditorHotkeyEvent::Edit(EditorHotkeyEventEdit::Redo),
                                )),
                            )
                            .clicked()
                        {
                            pipe.user_data.ui_events.push(EditorUiEvent::Redo);
                        }
                    });

                    let hotkeys_open = &mut pipe.user_data.editor_options.hotkeys_open;
                    if ui
                        .add(Button::new("Hotkeys").selected(*hotkeys_open))
                        .clicked()
                    {
                        *hotkeys_open = !*hotkeys_open;
                    }

                    ui.menu_button("Tools", |ui| {
                        if ui
                            .add(
                                Button::new("Automapper-Creator")
                                    .selected(pipe.user_data.auto_mapper.active),
                            )
                            .clicked()
                        {
                            pipe.user_data.auto_mapper.active = !pipe.user_data.auto_mapper.active;
                        }
                        if let Some(tab) = &mut pipe.user_data.editor_tabs.active_tab() {
                            if ui
                                .add(Button::new("Auto-Saver").selected(tab.auto_saver.active))
                                .clicked()
                            {
                                tab.auto_saver.active = !tab.auto_saver.active;
                            }
                        }
                    });

                    if let Some(tab) = &mut pipe.user_data.editor_tabs.active_tab() {
                        ui.menu_button("\u{f013}", |ui| {
                            let btn = Button::new("Disable animations panel + properties")
                                .selected(tab.map.user.options.no_animations_with_properties);
                            if ui
                                .add(btn)
                                .on_hover_ui(|ui| {
                                    let mut cache = egui_commonmark::CommonMarkCache::default();
                                    egui_commonmark::CommonMarkViewer::new().show(
                                        ui,
                                        &mut cache,
                                        TEXT_ANIM_PANEL_AND_PROPS,
                                    );
                                })
                                .clicked()
                            {
                                tab.map.user.options.no_animations_with_properties =
                                    !tab.map.user.options.no_animations_with_properties;
                            }
                            let binds = &*pipe.user_data.hotkeys;
                            let per_ev = &mut *pipe.user_data.cached_binds_per_event;
                            let btn = Button::new("Show tile layer indices")
                                .selected(tab.map.user.options.show_tile_numbers)
                                .shortcut_text(binds.fmt_ev_bind(
                                    per_ev,
                                    &EditorHotkeyEvent::Preferences(
                                        EditorHotkeyEventPreferences::ShowTileLayerIndices,
                                    ),
                                ));
                            let by_hotkey = pipe.user_data.cur_hotkey_events.remove(
                                &EditorHotkeyEvent::Preferences(
                                    EditorHotkeyEventPreferences::ShowTileLayerIndices,
                                ),
                            );
                            if ui.add(btn).clicked() || by_hotkey {
                                tab.map.user.options.show_tile_numbers =
                                    !tab.map.user.options.show_tile_numbers;
                            }
                        });

                        if tab.client.allows_remote_admin
                            && ui
                                .add(Button::new("Server").selected(tab.admin_panel.open))
                                .clicked()
                        {
                            tab.admin_panel.open = !tab.admin_panel.open;
                        }

                        if tab.admin_panel.open {
                            Window::new("Admin panel")
                                .anchor(Align2::CENTER_CENTER, (0.0, 0.0))
                                .show(ui.ctx(), |ui| {
                                    Grid::new("admin-panel-overview").num_columns(2).show(
                                        ui,
                                        |ui| match &mut tab.admin_panel.state {
                                            EditorAdminPanelState::NonAuthed(state) => {
                                                ui.label("Admin password:");
                                                ui.add(
                                                    TextEdit::singleline(&mut state.password)
                                                        .password(true),
                                                );
                                                ui.end_row();

                                                if ui.button("Auth").clicked() {
                                                    pipe.user_data.ui_events.push(
                                                        EditorUiEvent::AdminAuth {
                                                            password: state.password.clone(),
                                                        },
                                                    );
                                                }
                                                ui.end_row();
                                            }
                                            EditorAdminPanelState::Authed(state) => {
                                                ui.label("Do auto saves.");
                                                let mut do_autosaves =
                                                    state.state.auto_save.is_some();
                                                ui.checkbox(&mut do_autosaves, "");
                                                ui.end_row();
                                                if !do_autosaves {
                                                    state.state.auto_save = None;
                                                } else if state.state.auto_save.is_none() {
                                                    state.state.auto_save =
                                                        Some(Duration::from_secs(60));
                                                }
                                                if let Some(auto_save) = &mut state.state.auto_save
                                                {
                                                    ui.label("Save interval:");
                                                    let mut secs = auto_save.as_secs();
                                                    ui.add(
                                                        DragValue::new(&mut secs)
                                                            .update_while_editing(false),
                                                    );
                                                    ui.end_row();
                                                    *auto_save = Duration::from_secs(secs);
                                                }

                                                if ui.button("Apply").clicked() {
                                                    pipe.user_data.ui_events.push(
                                                        EditorUiEvent::AdminChangeConfig {
                                                            state: state.clone(),
                                                        },
                                                    );
                                                }
                                                ui.end_row();
                                            }
                                        },
                                    );
                                });
                        }

                        if tab.dbg_panel.show
                            && ui
                                .add(Button::new("Dbg").selected(tab.dbg_panel.open))
                                .clicked()
                        {
                            tab.dbg_panel.open = !tab.dbg_panel.open;
                        }

                        let by_hotkey =
                            pipe.user_data
                                .cur_hotkey_events
                                .remove(&EditorHotkeyEvent::Panels(
                                    EditorHotkeyEventPanels::ToggleAssetsStore,
                                ));
                        if ui
                            .add(
                                Button::new("\u{f54e} Assets store")
                                    .selected(tab.assets_store_open),
                            )
                            .clicked()
                            || by_hotkey
                        {
                            tab.assets_store_open = !tab.assets_store_open;
                        }
                    }
                });

                if let EditorMenuDialogMode::Open { file_dialog }
                | EditorMenuDialogMode::Save { file_dialog }
                | EditorMenuDialogMode::Host {
                    mode: EditorMenuHostDialogMode::SelectMap { file_dialog },
                } = menu_dialog_mode
                {
                    *pipe.user_data.pointer_is_used = true;
                    if file_dialog.state() == DialogState::Open {
                        let mode = file_dialog.mode();
                        if let Some(selected) = file_dialog.update(ui.ctx()).picked() {
                            let selected: PathBuf = selected.into();
                            if let EditorMenuDialogMode::Open { .. }
                            | EditorMenuDialogMode::Save { .. } = menu_dialog_mode
                            {
                                match mode {
                                    DialogMode::PickFile => {
                                        pipe.user_data
                                            .ui_events
                                            .push(EditorUiEvent::OpenFile { name: selected });
                                    }
                                    DialogMode::PickDirectory | DialogMode::PickMultiple => {
                                        todo!()
                                    }
                                    DialogMode::SaveFile => {
                                        pipe.user_data
                                            .ui_events
                                            .push(EditorUiEvent::SaveFile { name: selected });
                                    }
                                }
                                *menu_dialog_mode = EditorMenuDialogMode::None;
                            } else if let EditorMenuDialogMode::Host { mode } = menu_dialog_mode {
                                let (cert, private_key) = create_certifified_keys();

                                *mode = EditorMenuHostDialogMode::HostNetworkOptions(Box::new(
                                    EditorMenuHostNetworkOptions {
                                        map_path: selected,
                                        port: 0,
                                        password: Default::default(),
                                        cert,
                                        private_key,
                                        mapper_name: "hoster".to_string(),
                                        color: [255, 255, 255],
                                    },
                                ));
                            }
                        }
                    } else {
                        *menu_dialog_mode = EditorMenuDialogMode::None;
                    }
                }

                if let EditorMenuDialogMode::Host {
                    mode: EditorMenuHostDialogMode::HostNetworkOptions(mode),
                } = menu_dialog_mode
                {
                    *pipe.user_data.pointer_is_used = true;
                    let EditorMenuHostNetworkOptions {
                        port,
                        password,
                        cert,
                        mapper_name,
                        color,
                        ..
                    } = mode.as_mut();
                    let window = egui::Window::new("Host map network options")
                        .resizable(false)
                        .collapsible(false);

                    let mut host = false;
                    let mut cancel = false;
                    let window_res = window.show(ui.ctx(), |ui| {
                        ui.label("Port: (0 = random port)");
                        ui.add(DragValue::new(port).update_while_editing(false));

                        ui.label("Certificate hash:");
                        // TODO: cache this
                        let hash = cert
                            .tbs_certificate
                            .subject_public_key_info
                            .fingerprint_bytes()
                            .unwrap();
                        ui.label(fmt_hash(&hash));

                        ui.label("Password:");
                        ui.add(TextEdit::singleline(password).password(true));
                        ui.label("Name:");
                        ui.text_edit_singleline(mapper_name);
                        ui.label("Color:");
                        ui.color_edit_button_srgb(color);
                        if ui.button("Host").clicked() {
                            host = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });

                    if host {
                        let EditorMenuDialogMode::Host {
                            mode: EditorMenuHostDialogMode::HostNetworkOptions(mode),
                        } = std::mem::replace(menu_dialog_mode, EditorMenuDialogMode::None)
                        else {
                            return;
                        };
                        let EditorMenuHostNetworkOptions {
                            port,
                            password,
                            map_path,
                            cert,
                            private_key,
                            mapper_name,
                            color,
                        } = *mode;
                        pipe.user_data
                            .ui_events
                            .push(EditorUiEvent::HostMap(Box::new(EditorUiEventHostMap {
                                map_path,
                                port,
                                password,
                                cert,
                                private_key,
                                mapper_name,
                                color,
                            })));
                    } else if cancel {
                        *menu_dialog_mode = EditorMenuDialogMode::None;
                    }

                    *pipe.user_data.pointer_is_used |= if let Some(window_res) = window_res {
                        let intersected = ui.input(|i| {
                            if i.pointer.primary_down() {
                                Some((
                                    !window_res.response.rect.intersects({
                                        let min = i.pointer.interact_pos().unwrap_or_default();
                                        let max = min;
                                        [min, max].into()
                                    }),
                                    i.pointer.primary_pressed(),
                                ))
                            } else {
                                None
                            }
                        });
                        if intersected.is_some_and(|(outside, clicked)| outside && clicked)
                            && !ui.memory(|i| i.any_popup_open())
                        {
                            *menu_dialog_mode = EditorMenuDialogMode::None;
                        }
                        intersected.is_some_and(|(outside, _)| !outside)
                    } else {
                        false
                    };
                } else if let EditorMenuDialogMode::Join(EditorMenuDialogJoinProps {
                    ip_port,
                    cert_hash,
                    password,
                    mapper_name,
                    color,
                }) = menu_dialog_mode
                {
                    *pipe.user_data.pointer_is_used = true;
                    let window = egui::Window::new("Join map network options")
                        .resizable(false)
                        .collapsible(false);

                    let mut join = false;
                    let mut cancel = false;
                    let window_res = window.show(ui.ctx(), |ui| {
                        ui.label("Address (IP:PORT)");
                        ui.text_edit_singleline(ip_port);
                        ui.label("Certificate hash:");
                        ui.text_edit_singleline(cert_hash);
                        ui.label("Password:");
                        ui.add(TextEdit::singleline(password).password(true));
                        ui.label("Name:");
                        ui.text_edit_singleline(mapper_name);
                        ui.label("Color:");
                        ui.color_edit_button_srgb(color);
                        if ui.button("Join").clicked() {
                            join = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });

                    if join {
                        let EditorMenuDialogMode::Join(props) = &menu_dialog_mode else {
                            return;
                        };

                        // save current props
                        let fs = pipe.user_data.io.fs.clone();
                        let props = props.clone();
                        pipe.user_data.io.rt.spawn_without_lifetime(async move {
                            fs.create_dir("editor".as_ref()).await?;
                            Ok(fs
                                .write_file(
                                    "editor/join_props.json".as_ref(),
                                    serde_json::to_vec_pretty(&props)?,
                                )
                                .await?)
                        });

                        let EditorMenuDialogMode::Join(EditorMenuDialogJoinProps {
                            ip_port,
                            cert_hash,
                            password,
                            mapper_name,
                            color,
                        }) = std::mem::replace(menu_dialog_mode, EditorMenuDialogMode::None)
                        else {
                            return;
                        };
                        pipe.user_data.ui_events.push(EditorUiEvent::Join {
                            ip_port,
                            cert_hash,
                            password,
                            mapper_name,
                            color,
                        });
                    } else if cancel {
                        *menu_dialog_mode = EditorMenuDialogMode::None;
                    }

                    *pipe.user_data.pointer_is_used |= if let Some(window_res) = window_res {
                        let intersected = ui.input(|i| {
                            if i.pointer.primary_down() {
                                Some((
                                    !window_res.response.rect.intersects({
                                        let min = i.pointer.interact_pos().unwrap_or_default();
                                        let max = min;
                                        [min, max].into()
                                    }),
                                    i.pointer.primary_pressed(),
                                ))
                            } else {
                                None
                            }
                        });
                        if intersected.is_some_and(|(outside, clicked)| outside && clicked)
                            && !ui.memory(|i| i.any_popup_open())
                        {
                            *menu_dialog_mode = EditorMenuDialogMode::None;
                        }
                        intersected.is_some_and(|(outside, _)| !outside)
                    } else {
                        false
                    };
                }

                pipe.user_data
                    .auto_mapper
                    .update(pipe.user_data.notifications);
                if pipe.user_data.auto_mapper.active {
                    crate::ui::auto_mapper::auto_mapper::render(pipe, ui, ui_state);
                }

                let cur_hotkeys = &mut *pipe.user_data.cur_hotkey_events;
                if let Some(tab) = pipe.user_data.editor_tabs.active_tab() {
                    if tab.auto_saver.active {
                        crate::ui::auto_saver::render(
                            pipe.cur_time,
                            tab,
                            pipe.user_data.pointer_is_used,
                            ui,
                        );
                    }

                    if tab.server.is_some() && cur_hotkeys.remove(&EditorHotkeyEvent::DbgMode) {
                        tab.dbg_panel.show = true;
                    }
                    if tab.dbg_panel.open {
                        crate::ui::dbg_panel::render(
                            pipe.user_data.ui_events,
                            tab,
                            pipe.user_data.pointer_is_used,
                            ui,
                        );
                    }
                }

                if cur_hotkeys.remove(&EditorHotkeyEvent::File(EditorHotkeyEventFile::Save)) {
                    pipe.user_data.ui_events.push(EditorUiEvent::SaveCurMap);
                }
                if cur_hotkeys.remove(&EditorHotkeyEvent::Edit(EditorHotkeyEventEdit::Redo)) {
                    pipe.user_data.ui_events.push(EditorUiEvent::Redo);
                }
                if cur_hotkeys.remove(&EditorHotkeyEvent::Edit(EditorHotkeyEventEdit::Undo)) {
                    pipe.user_data.ui_events.push(EditorUiEvent::Undo);
                }
            });
        });

    ui_state.add_blur_rect(res.response.rect, 0.0);
}
