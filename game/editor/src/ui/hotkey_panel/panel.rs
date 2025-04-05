use egui::{Button, Grid, Key, KeyboardShortcut, ModifierNames, Modifiers, ScrollArea, Window};
use ui_base::types::UiRenderPipe;

use crate::{
    hotkeys::{
        BindsPerEvent, EditorBindsFile, EditorHotkeyEvent, EditorHotkeyEventEdit,
        EditorHotkeyEventFile, EditorHotkeyEventMap, EditorHotkeyEventPanels,
        EditorHotkeyEventPreferences, EditorHotkeyEventQuadBrush, EditorHotkeyEventQuadTool,
        EditorHotkeyEventSharedTool, EditorHotkeyEventSoundBrush, EditorHotkeyEventSoundTool,
        EditorHotkeyEventTabs, EditorHotkeyEventTileBrush, EditorHotkeyEventTileTool,
        EditorHotkeyEventTimeline, EditorHotkeyEventTools,
    },
    options::EditorHotkeyEdit,
    ui::user_data::UserDataWithTab,
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>) {
    if !pipe.user_data.editor_options.hotkeys_open {
        return;
    }

    let window_res = Window::new("Configure Hotkeys").show(ui.ctx(), |ui| {
        let editor_options = &mut *pipe.user_data.editor_options;
        let options = &mut editor_options.hotkeys_edit;

        let binds_per_event = pipe
            .user_data
            .cached_binds_per_event
            .get_or_insert_with(|| pipe.user_data.hotkeys.binds_per_event());

        fn hotkey_button(
            ui: &mut egui::Ui,
            heading: &str,
            _explain: &str,
            ev: EditorHotkeyEvent,
            options: &mut Option<EditorHotkeyEdit>,
            binds_per_event: &BindsPerEvent,
            binds: &mut EditorBindsFile,
        ) -> bool {
            ui.label(heading);
            fn format_modifier_key(modifiers: &Modifiers, key: &Option<Key>) -> String {
                let modifier_str = ModifierNames::NAMES.format(modifiers, false);
                format!(
                    "{}{}",
                    modifier_str,
                    key.map(|k| format!(
                        "{}{}",
                        if modifier_str.is_empty() { "" } else { "+" },
                        k.name(),
                    ),)
                        .unwrap_or_default()
                )
            }
            let text = if let Some(text) = options
                .as_ref()
                .and_then(|edit| (edit.ev == ev).then_some(edit))
            {
                format_modifier_key(&text.modifiers, &text.key)
            } else if let Some(bind) = binds_per_event.get(&ev) {
                let bind = bind.first().unwrap();
                format_modifier_key(&bind.modifiers, &Some(bind.logical_key))
            } else {
                "None".into()
            };
            let btn =
                ui.add(Button::new(text).selected(options.as_ref().is_some_and(|e| e.ev == ev)));
            if btn.clicked() {
                let (modifiers, key) = if let Some(bind) = binds_per_event.get(&ev) {
                    let bind = bind.first().unwrap();
                    (bind.modifiers, Some(bind.logical_key))
                } else {
                    (Default::default(), None)
                };
                *options = Some(EditorHotkeyEdit { modifiers, key, ev });
            } else if btn.secondary_clicked() {
                if let Some(bind) = binds_per_event.get(&ev) {
                    let bind = bind.first().unwrap();
                    binds
                        .binds
                        .remove(&KeyboardShortcut::new(bind.modifiers, bind.logical_key));
                }
            }
            ui.end_row();

            ui.input_mut(|i| {
                if let Some(edit) = options
                    .as_mut()
                    .and_then(|edit| (edit.ev == ev).then_some(edit))
                {
                    edit.modifiers = i.modifiers;
                    edit.key = i.keys_down.iter().next().copied();

                    if let Some(key) = edit.key {
                        if let Some(bind) = binds_per_event.get(&ev) {
                            let bind = bind.first().unwrap();
                            binds
                                .binds
                                .remove(&KeyboardShortcut::new(bind.modifiers, bind.logical_key));
                        }
                        binds
                            .binds
                            .insert(KeyboardShortcut::new(edit.modifiers, key), ev);
                        binds.changed_at_least_once.insert(ev);
                        *options = None;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
        }

        ScrollArea::vertical().show(ui, |ui| {
            Grid::new("hotkey-buttons-rebind-grid")
                .num_columns(2)
                .show(ui, |ui| {
                    let mut binds_changed = false;
                    binds_changed |= hotkey_button(
                        ui,
                        "Flip X",
                        "",
                        EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                            EditorHotkeyEventTileTool::Brush(EditorHotkeyEventTileBrush::FlipX),
                        )),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Flip Y",
                        "",
                        EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                            EditorHotkeyEventTileTool::Brush(EditorHotkeyEventTileBrush::FlipY),
                        )),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Rotate +90°",
                        "",
                        EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                            EditorHotkeyEventTileTool::Brush(EditorHotkeyEventTileBrush::RotPlus90),
                        )),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Rotate -90°",
                        "",
                        EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                            EditorHotkeyEventTileTool::Brush(
                                EditorHotkeyEventTileBrush::RotMinus90,
                            ),
                        )),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Rotate individual tiles +90°",
                        "",
                        EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                            EditorHotkeyEventTileTool::Brush(
                                EditorHotkeyEventTileBrush::RotIndividualTilePlus90,
                            ),
                        )),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    ui.separator();
                    ui.separator();
                    ui.end_row();
                    binds_changed |= hotkey_button(
                        ui,
                        "Add sound/quad",
                        "",
                        EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Shared(
                            EditorHotkeyEventSharedTool::AddQuadOrSound,
                        )),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Delete sound/quad",
                        "",
                        EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Shared(
                            EditorHotkeyEventSharedTool::DeleteQuadOrSound,
                        )),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    ui.separator();
                    ui.separator();
                    ui.end_row();
                    binds_changed |= hotkey_button(
                        ui,
                        "Square quad",
                        "",
                        EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Quad(
                            EditorHotkeyEventQuadTool::Brush(EditorHotkeyEventQuadBrush::Square),
                        )),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    ui.separator();
                    ui.separator();
                    ui.end_row();
                    binds_changed |= hotkey_button(
                        ui,
                        "Change sound shape",
                        "",
                        EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Sound(
                            EditorHotkeyEventSoundTool::Brush(
                                EditorHotkeyEventSoundBrush::ToggleShape,
                            ),
                        )),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    ui.separator();
                    ui.separator();
                    ui.end_row();
                    binds_changed |= hotkey_button(
                        ui,
                        "Insert animation point",
                        "",
                        EditorHotkeyEvent::Timeline(EditorHotkeyEventTimeline::InsertPoint),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    ui.separator();
                    ui.separator();
                    ui.end_row();
                    binds_changed |= hotkey_button(
                        ui,
                        "Previous tab",
                        "",
                        EditorHotkeyEvent::Tabs(EditorHotkeyEventTabs::Previous),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Next tab",
                        "",
                        EditorHotkeyEvent::Tabs(EditorHotkeyEventTabs::Next),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Close tab",
                        "",
                        EditorHotkeyEvent::Tabs(EditorHotkeyEventTabs::Close),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    ui.separator();
                    ui.separator();
                    ui.end_row();
                    binds_changed |= hotkey_button(
                        ui,
                        "Show tile layer indices",
                        "",
                        EditorHotkeyEvent::Preferences(
                            EditorHotkeyEventPreferences::ShowTileLayerIndices,
                        ),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Toggle parallax zoom",
                        "",
                        EditorHotkeyEvent::Preferences(
                            EditorHotkeyEventPreferences::ToggleParallaxZoom,
                        ),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Increase map time speed factor",
                        "",
                        EditorHotkeyEvent::Preferences(
                            EditorHotkeyEventPreferences::IncreaseMapTimeSpeed,
                        ),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Decrease map time speed factor",
                        "",
                        EditorHotkeyEvent::Preferences(
                            EditorHotkeyEventPreferences::DecreaseMapTimeSpeed,
                        ),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    ui.separator();
                    ui.separator();
                    ui.end_row();
                    binds_changed |= hotkey_button(
                        ui,
                        "Toggle animation panel",
                        "",
                        EditorHotkeyEvent::Panels(EditorHotkeyEventPanels::ToggleAnimation),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Toggle server commands panel",
                        "",
                        EditorHotkeyEvent::Panels(EditorHotkeyEventPanels::ToggleServerCommands),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Toggle server config variables panel",
                        "",
                        EditorHotkeyEvent::Panels(EditorHotkeyEventPanels::ToggleServerConfigVars),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Toggle assets store panel",
                        "",
                        EditorHotkeyEvent::Panels(EditorHotkeyEventPanels::ToggleAssetsStore),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    ui.separator();
                    ui.separator();
                    ui.end_row();
                    binds_changed |= hotkey_button(
                        ui,
                        "Move active layer up",
                        "",
                        EditorHotkeyEvent::Map(EditorHotkeyEventMap::MoveLayerUp),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Move active layer down",
                        "",
                        EditorHotkeyEvent::Map(EditorHotkeyEventMap::MoveLayerDown),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Delete active layer",
                        "",
                        EditorHotkeyEvent::Map(EditorHotkeyEventMap::DeleteLayer),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    ui.separator();
                    ui.separator();
                    ui.end_row();
                    binds_changed |= hotkey_button(
                        ui,
                        "New map",
                        "",
                        EditorHotkeyEvent::File(EditorHotkeyEventFile::New),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Open map",
                        "",
                        EditorHotkeyEvent::File(EditorHotkeyEventFile::Open),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Save map",
                        "",
                        EditorHotkeyEvent::File(EditorHotkeyEventFile::Save),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    ui.separator();
                    ui.separator();
                    ui.end_row();
                    binds_changed |= hotkey_button(
                        ui,
                        "Undo",
                        "",
                        EditorHotkeyEvent::Edit(EditorHotkeyEventEdit::Undo),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    binds_changed |= hotkey_button(
                        ui,
                        "Redo",
                        "",
                        EditorHotkeyEvent::Edit(EditorHotkeyEventEdit::Redo),
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    ui.separator();
                    ui.separator();
                    ui.end_row();
                    binds_changed |= hotkey_button(
                        ui,
                        "Chat",
                        "",
                        EditorHotkeyEvent::Chat,
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );
                    ui.separator();
                    ui.separator();
                    ui.end_row();
                    binds_changed |= hotkey_button(
                        ui,
                        "Debug mode",
                        "",
                        EditorHotkeyEvent::DbgMode,
                        options,
                        binds_per_event,
                        pipe.user_data.hotkeys,
                    );

                    ui.input_mut(|i| {
                        for key in &i.keys_down.clone() {
                            i.consume_shortcut(&KeyboardShortcut::new(i.modifiers, *key));
                        }
                    });

                    if binds_changed {
                        let binds = pipe.user_data.hotkeys.clone();
                        let fs = pipe.user_data.io.fs.clone();
                        let order_stack = editor_options.hotkeys_write_in_order.clone();
                        order_stack.blocking_lock().push_back(binds);
                        pipe.user_data.io.rt.spawn_without_lifetime(async move {
                            let mut order_stack = order_stack.lock().await;
                            let binds = order_stack.pop_front().unwrap();
                            match binds.save(fs.as_ref()).await {
                                Ok(_) => Ok(()),
                                Err(err) => {
                                    log::error!("failed to write binds file: {err}");
                                    Err(err)
                                }
                            }
                        });
                    }
                });
        });
    });

    *pipe.user_data.pointer_is_used |= if let Some(window_res) = &window_res {
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
        intersected.is_some_and(|(outside, _)| !outside)
    } else {
        false
    };
}
