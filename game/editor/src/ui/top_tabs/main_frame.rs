use egui::{
    text::LayoutJob, Button, Color32, CornerRadius, FontId, Grid, Modal, Stroke, WidgetText,
};
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    hotkeys::{EditorHotkeyEvent, EditorHotkeyEventTabs},
    ui::user_data::{EditorModalDialogMode, EditorUiEvent, UserData},
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    let style = ui.style();
    // 4.0 is some margin for strokes
    let height = style.spacing.interact_size.y + style.spacing.item_spacing.y + 4.0;
    let res = egui::TopBottomPanel::top("top_tabs")
        .resizable(false)
        .default_height(height)
        .height_range(height..=height)
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.style_mut().spacing.item_spacing.x = 0.0;
                let mut remove_tab = None;
                for (tab_name, tab) in pipe.user_data.editor_tabs.tabs.iter() {
                    let tab_display_name = if tab.client.clients.len() > 1 {
                        format!("\u{f0c0} {tab_name}")
                    } else {
                        tab_name.clone()
                    };
                    let tab_display_name: WidgetText = if tab.client.should_save {
                        let mut job = LayoutJob::default();
                        job.append(
                            "\u{f192}",
                            0.0,
                            egui::TextFormat {
                                font_id: FontId::proportional(7.0),
                                valign: egui::Align::Center,
                                color: Color32::LIGHT_GRAY,
                                ..Default::default()
                            },
                        );
                        job.append(
                            &tab_display_name,
                            8.0,
                            egui::TextFormat {
                                color: Color32::LIGHT_GRAY,
                                ..Default::default()
                            },
                        );
                        job.into()
                    } else {
                        tab_display_name.into()
                    };
                    let style = ui.style_mut();
                    style.visuals.selection.stroke = Stroke::NONE;
                    style.visuals.widgets.active.bg_stroke = Stroke::NONE;
                    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
                    style.visuals.widgets.hovered.expansion = 0.0;
                    let old_rouding = style.visuals.widgets.inactive.corner_radius.nw;

                    let r = CornerRadius {
                        nw: old_rouding,
                        sw: old_rouding,
                        ..Default::default()
                    };
                    style.visuals.widgets.inactive.corner_radius = r;
                    style.visuals.widgets.active.corner_radius = r;
                    style.visuals.widgets.hovered.corner_radius = r;

                    let mut btn = ui.add(
                        Button::new(tab_display_name)
                            .selected(pipe.user_data.editor_tabs.active_tab == tab_name),
                    );
                    btn = btn.on_hover_ui(|ui| {
                        ui.vertical(|ui| {
                            if tab.client.clients.len() > 1 {
                                Grid::new("overview-mappers-network-tooltip")
                                    .num_columns(2)
                                    .show(ui, |ui| {
                                        for client in tab.client.clients.iter() {
                                            ui.label(&client.mapper_name);
                                            if let Some(stats) = &client.stats {
                                                ui.label(format!(
                                                    "Ping: {}ms",
                                                    stats.ping.as_millis()
                                                ));
                                            }
                                            ui.end_row();
                                        }
                                    });

                                ui.add_space(20.0);
                                ui.separator();
                                ui.add_space(20.0);
                            }
                            let binds = &*pipe.user_data.hotkeys;
                            let per_ev = &mut *pipe.user_data.cached_binds_per_event;

                            let mut cache = egui_commonmark::CommonMarkCache::default();
                            egui_commonmark::CommonMarkViewer::new().show(
                                ui,
                                &mut cache,
                                &format!(
                                    "Next tab hotkey: `{}`  \n\
                                    Prev tab hotkey: `{}`  \n\
                                    Close tab hotkey: `{}`",
                                    binds.fmt_ev_bind(
                                        per_ev,
                                        &EditorHotkeyEvent::Tabs(EditorHotkeyEventTabs::Next),
                                    ),
                                    binds.fmt_ev_bind(
                                        per_ev,
                                        &EditorHotkeyEvent::Tabs(EditorHotkeyEventTabs::Previous),
                                    ),
                                    binds.fmt_ev_bind(
                                        per_ev,
                                        &EditorHotkeyEvent::Tabs(EditorHotkeyEventTabs::Close),
                                    ),
                                ),
                            );
                        });
                    });
                    if btn.clicked() {
                        *pipe.user_data.editor_tabs.active_tab = tab_name.clone();
                    }

                    let style = ui.style_mut();
                    let r = CornerRadius {
                        ne: old_rouding,
                        se: old_rouding,
                        ..Default::default()
                    };
                    style.visuals.widgets.inactive.corner_radius = r;
                    style.visuals.widgets.active.corner_radius = r;
                    style.visuals.widgets.hovered.corner_radius = r;

                    if ui.add(Button::new("\u{f00d}")).clicked() {
                        remove_tab = Some((tab_name.clone(), tab.client.should_save));
                    }
                    ui.add_space(10.0);
                }

                let next_by_hotkey = pipe
                    .user_data
                    .cur_hotkey_events
                    .remove(&EditorHotkeyEvent::Tabs(EditorHotkeyEventTabs::Next));
                if next_by_hotkey {
                    let mut it = pipe
                        .user_data
                        .editor_tabs
                        .tabs
                        .keys()
                        .chain(pipe.user_data.editor_tabs.tabs.keys())
                        .skip_while(|name| name.as_str() != pipe.user_data.editor_tabs.active_tab);
                    // skips the match
                    it.next();
                    if let Some(name) = it
                        .next()
                        .or_else(|| pipe.user_data.editor_tabs.tabs.keys().next())
                    {
                        *pipe.user_data.editor_tabs.active_tab = name.clone();
                    }
                }
                let prev_by_hotkey = pipe
                    .user_data
                    .cur_hotkey_events
                    .remove(&EditorHotkeyEvent::Tabs(EditorHotkeyEventTabs::Previous));
                if prev_by_hotkey {
                    let mut it = pipe
                        .user_data
                        .editor_tabs
                        .tabs
                        .keys()
                        .rev()
                        .chain(pipe.user_data.editor_tabs.tabs.keys().rev())
                        .skip_while(|name| name.as_str() != pipe.user_data.editor_tabs.active_tab);
                    // skips the match
                    it.next();
                    if let Some(name) = it
                        .next()
                        .or_else(|| pipe.user_data.editor_tabs.tabs.keys().next_back())
                    {
                        *pipe.user_data.editor_tabs.active_tab = name.clone();
                    }
                }

                let by_hotkey = pipe
                    .user_data
                    .cur_hotkey_events
                    .remove(&EditorHotkeyEvent::Tabs(EditorHotkeyEventTabs::Close));
                if let Some(tab) = by_hotkey
                    .then(|| {
                        pipe.user_data
                            .editor_tabs
                            .tabs
                            .get(pipe.user_data.editor_tabs.active_tab)
                    })
                    .flatten()
                {
                    remove_tab = Some((
                        pipe.user_data.editor_tabs.active_tab.clone(),
                        tab.client.should_save,
                    ));
                }

                if let Some((tab, should_save)) = remove_tab {
                    if !should_save {
                        pipe.user_data.editor_tabs.tabs.remove(&tab);
                    } else {
                        *pipe.user_data.modal_dialog_mode = EditorModalDialogMode::CloseTab { tab };
                    }
                }
                if let EditorModalDialogMode::CloseTab { tab } = pipe.user_data.modal_dialog_mode {
                    let tab = tab.clone();
                    Modal::new("close-tab-confirm".into()).show(ui.ctx(), |ui| {
                        ui.label(
                            "You are about to close this editor tab, while the map is not saved.",
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Save & close").clicked() {
                                pipe.user_data
                                    .ui_events
                                    .push(EditorUiEvent::SaveMapAndClose { tab: tab.clone() });
                                *pipe.user_data.modal_dialog_mode = EditorModalDialogMode::None;
                            }
                            if ui.button("Close without saving").clicked() {
                                pipe.user_data.editor_tabs.tabs.remove(&tab);
                                *pipe.user_data.modal_dialog_mode = EditorModalDialogMode::None;
                            }
                            if ui.button("Cancel").clicked() {
                                *pipe.user_data.modal_dialog_mode = EditorModalDialogMode::None;
                            }
                        });
                    });
                    *pipe.user_data.pointer_is_used = true;
                }
            })
        });
    ui_state.add_blur_rect(res.response.rect, 0.0);
}
