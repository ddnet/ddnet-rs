use egui::{text::LayoutJob, Button, Color32, DragValue};
use egui_extras::Size;
use math::math::vector::vec2;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    explain::{ANIMATION_PANEL, SERVER_COMMANDS_CONFIG_VAR},
    hotkeys::{
        EditorHotkeyEvent, EditorHotkeyEventPanels, EditorHotkeyEventPreferences,
        EditorHotkeyEventTimeline,
    },
    ui::user_data::{EditorUiEvent, UserDataWithTab},
    utils::ui_pos_to_world_pos,
};

fn render_buttons(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>) {
    let editor_tab = &mut *pipe.user_data.editor_tab;
    let binds = &*pipe.user_data.hotkeys;
    let per_ev = &mut *pipe.user_data.cached_binds_per_event;
    let by_hotkey = pipe
        .user_data
        .cur_hotkey_events
        .remove(&EditorHotkeyEvent::Panels(
            EditorHotkeyEventPanels::ToggleAnimation,
        ));
    if ui
        .add(
            egui::Button::new("Animations")
                .selected(editor_tab.map.user.ui_values.animations_panel_open),
        )
        .on_hover_ui(|ui| {
            let mut cache = egui_commonmark::CommonMarkCache::default();
            egui_commonmark::CommonMarkViewer::new().show(
                ui,
                &mut cache,
                &format!(
                    "{}\n\nHotkey: `{}`",
                    ANIMATION_PANEL.replace(
                        "$ANIM_POINT_INSERT$",
                        &binds.fmt_ev_bind(
                            per_ev,
                            &EditorHotkeyEvent::Timeline(EditorHotkeyEventTimeline::InsertPoint),
                        )
                    ),
                    binds.fmt_ev_bind(
                        per_ev,
                        &EditorHotkeyEvent::Panels(EditorHotkeyEventPanels::ToggleAnimation),
                    )
                ),
            );
        })
        .clicked()
        || by_hotkey
    {
        editor_tab.map.user.ui_values.animations_panel_open =
            !editor_tab.map.user.ui_values.animations_panel_open;
    }
    let by_hotkey = pipe
        .user_data
        .cur_hotkey_events
        .remove(&EditorHotkeyEvent::Panels(
            EditorHotkeyEventPanels::ToggleServerCommands,
        ));
    if ui
        .add(
            Button::new("Server commands")
                .selected(editor_tab.map.user.ui_values.server_commands_open),
        )
        .on_hover_ui(|ui| {
            let mut cache = egui_commonmark::CommonMarkCache::default();
            egui_commonmark::CommonMarkViewer::new().show(
                ui,
                &mut cache,
                &format!(
                    "{}\n\nHotkey: `{}`",
                    SERVER_COMMANDS_CONFIG_VAR,
                    binds.fmt_ev_bind(
                        per_ev,
                        &EditorHotkeyEvent::Panels(EditorHotkeyEventPanels::ToggleServerCommands,),
                    )
                ),
            );
        })
        .clicked()
        || by_hotkey
    {
        editor_tab.map.user.ui_values.server_commands_open =
            !editor_tab.map.user.ui_values.server_commands_open;
    }
    let by_hotkey = pipe
        .user_data
        .cur_hotkey_events
        .remove(&EditorHotkeyEvent::Panels(
            EditorHotkeyEventPanels::ToggleServerConfigVars,
        ));
    if ui
        .add(
            Button::new("Server config variables")
                .selected(editor_tab.map.user.ui_values.server_config_variables_open),
        )
        .on_hover_ui(|ui| {
            let mut cache = egui_commonmark::CommonMarkCache::default();
            egui_commonmark::CommonMarkViewer::new().show(
                ui,
                &mut cache,
                &format!(
                    "{}\n\nHotkey: `{}`",
                    SERVER_COMMANDS_CONFIG_VAR,
                    binds.fmt_ev_bind(
                        per_ev,
                        &EditorHotkeyEvent::Panels(EditorHotkeyEventPanels::ToggleServerConfigVars),
                    )
                ),
            );
        })
        .clicked()
        || by_hotkey
    {
        editor_tab.map.user.ui_values.server_config_variables_open =
            !editor_tab.map.user.ui_values.server_config_variables_open;
    }
    let by_hotkey = pipe
        .user_data
        .cur_hotkey_events
        .remove(&EditorHotkeyEvent::Preferences(
            EditorHotkeyEventPreferences::ToggleParallaxZoom,
        ));
    if ui
        .add(Button::new("Parallax zoom").selected(editor_tab.map.groups.user.parallax_aware_zoom))
        .on_hover_ui(|ui| {
            let mut cache = egui_commonmark::CommonMarkCache::default();
            egui_commonmark::CommonMarkViewer::new().show(
                ui,
                &mut cache,
                &format!(
                    "Hotkey: `{}`",
                    binds.fmt_ev_bind(
                        per_ev,
                        &EditorHotkeyEvent::Preferences(
                            EditorHotkeyEventPreferences::ToggleParallaxZoom,
                        ),
                    )
                ),
            );
        })
        .clicked()
        || by_hotkey
    {
        editor_tab.map.groups.user.parallax_aware_zoom =
            !editor_tab.map.groups.user.parallax_aware_zoom;
    }

    // Editor time
    let increase_by_hotkey =
        pipe.user_data
            .cur_hotkey_events
            .remove(&EditorHotkeyEvent::Preferences(
                EditorHotkeyEventPreferences::IncreaseMapTimeSpeed,
            ));
    if increase_by_hotkey {
        editor_tab.map.user.time_scale = (editor_tab.map.user.time_scale * 2).max(1);
    }
    let decrease_by_hotkey =
        pipe.user_data
            .cur_hotkey_events
            .remove(&EditorHotkeyEvent::Preferences(
                EditorHotkeyEventPreferences::DecreaseMapTimeSpeed,
            ));
    if decrease_by_hotkey {
        editor_tab.map.user.time_scale /= 2;
    }
    ui.menu_button("\u{f017}", |ui| {
        ui.label("Control over how time in the editor advances.");
        ui.label("Affects for example the animations.");
        ui.add_space(10.0);
        ui.label("Time multiplier:");
        ui.add(DragValue::new(&mut editor_tab.map.user.time_scale));
    })
    .response
    .on_hover_ui(|ui| {
        let mut cache = egui_commonmark::CommonMarkCache::default();
        egui_commonmark::CommonMarkViewer::new().show(
            ui,
            &mut cache,
            &format!(
                "Increase time factor hotkey: `{}`  \nDecrease time factor hotkey: `{}`",
                binds.fmt_ev_bind(
                    per_ev,
                    &EditorHotkeyEvent::Preferences(
                        EditorHotkeyEventPreferences::IncreaseMapTimeSpeed,
                    ),
                ),
                binds.fmt_ev_bind(
                    per_ev,
                    &EditorHotkeyEvent::Preferences(
                        EditorHotkeyEventPreferences::DecreaseMapTimeSpeed,
                    ),
                )
            ),
        );
    });
}

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    let style = ui.style();
    let item_height = style.spacing.interact_size.y;
    let row_height = item_height + style.spacing.item_spacing.y;
    let height = row_height * 2.0;
    let res = egui::TopBottomPanel::bottom("bottom_panel")
        .resizable(false)
        .default_height(height)
        .height_range(height..=height)
        .show_inside(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.vertical(|ui| {
                    egui_extras::StripBuilder::new(ui)
                        .size(Size::exact(item_height))
                        .size(Size::exact(row_height))
                        .clip(true)
                        .vertical(|mut strip| {
                            strip.cell(|ui| {
                                ui.style_mut().wrap_mode = None;
                                ui.horizontal(|ui| {
                                    render_buttons(ui, pipe);
                                });
                            });
                            strip.cell(|ui| {
                                ui.style_mut().wrap_mode = None;
                                egui_extras::StripBuilder::new(ui)
                                    .size(Size::exact(180.0))
                                    .size(Size::exact(100.0))
                                    .size(Size::remainder())
                                    .clip(true)
                                    .horizontal(|mut strip| {
                                        let editor_tab = &mut *pipe.user_data.editor_tab;
                                        strip.cell(|ui| {
                                            ui.style_mut().wrap_mode = None;
                                            let mut layout = LayoutJob::default();
                                            let number_format = egui::TextFormat {
                                                color: Color32::from_rgb(100, 100, 255),
                                                ..Default::default()
                                            };
                                            layout.append(
                                                "camera (",
                                                0.0,
                                                egui::TextFormat::default(),
                                            );
                                            layout.append(
                                                    &format!(
                                                        "{:.2}",
                                                        editor_tab.map.groups.user.pos.x,
                                                    ),
                                                    0.0,
                                                    number_format.clone(),
                                                );
                                            layout.append(", ", 0.0, egui::TextFormat::default());
                                            layout.append(
                                                &format!("{:.2}", editor_tab.map.groups.user.pos.y),
                                                0.0,
                                                number_format.clone(),
                                            );
                                            layout.append(")", 0.0, egui::TextFormat::default());
                                            ui.label(layout);
                                        });
                                        strip.cell(|ui| {
                                            ui.style_mut().wrap_mode = None;
                                            let mut layout = LayoutJob::default();
                                            let number_format = egui::TextFormat {
                                                color: Color32::from_rgb(100, 100, 255),
                                                ..Default::default()
                                            };
                                            layout.append(
                                                " zoom (",
                                                0.0,
                                                egui::TextFormat::default(),
                                            );
                                            layout.append(
                                                &format!("{:.2}", editor_tab.map.groups.user.zoom),
                                                0.0,
                                                number_format.clone(),
                                            );
                                            layout.append(")", 0.0, egui::TextFormat::default());
                                            ui.label(layout);
                                        });
                                        strip.cell(|ui| {
                                            ui.style_mut().wrap_mode = None;
                                            if let Some(cursor_pos) =
                                                ui.input(|i| i.pointer.latest_pos())
                                            {
                                                let mut layout = LayoutJob::default();
                                                let number_format = egui::TextFormat {
                                                    color: Color32::from_rgb(100, 100, 255),
                                                    ..Default::default()
                                                };
                                                let pos = ui_pos_to_world_pos(
                                                    pipe.user_data.canvas_handle,
                                                    &ui.ctx().screen_rect(),
                                                    editor_tab.map.groups.user.zoom,
                                                    vec2::new(cursor_pos.x, cursor_pos.y),
                                                    editor_tab.map.groups.user.pos.x,
                                                    editor_tab.map.groups.user.pos.y,
                                                    0.0,
                                                    0.0,
                                                    100.0,
                                                    100.0,
                                                    false,
                                                );
                                                layout.append(
                                                    " mouse (",
                                                    0.0,
                                                    egui::TextFormat::default(),
                                                );
                                                layout.append(
                                                    &format!("{:.2}", pos.x),
                                                    0.0,
                                                    number_format.clone(),
                                                );
                                                layout.append(
                                                    ", ",
                                                    0.0,
                                                    egui::TextFormat::default(),
                                                );
                                                layout.append(
                                                    &format!("{:.2}", pos.y),
                                                    0.0,
                                                    number_format.clone(),
                                                );
                                                layout.append(
                                                    ")",
                                                    0.0,
                                                    egui::TextFormat::default(),
                                                );
                                                ui.label(layout);

                                                pipe.user_data
                                                    .ui_events
                                                    .push(EditorUiEvent::CursorWorldPos { pos });
                                            }
                                        });
                                    });
                            });
                        });
                });
            });
        });
    ui_state.add_blur_rect(res.response.rect, 0.0);
}
