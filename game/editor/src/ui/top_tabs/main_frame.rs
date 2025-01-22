use egui::{text::LayoutJob, Button, Color32, FontId, Grid, Modal, WidgetText};
use ui_base::types::UiRenderPipe;

use crate::ui::user_data::{EditorModalDialogMode, EditorUiEvent, UserData};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>) {
    let style = ui.style();
    let height = style.spacing.interact_size.y + style.spacing.item_spacing.y;
    egui::TopBottomPanel::top("top_tabs")
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
                    let mut btn = ui.add(
                        Button::new(tab_display_name)
                            .selected(pipe.user_data.editor_tabs.active_tab == tab_name),
                    );
                    if tab.client.clients.len() > 1 {
                        btn = btn.on_hover_ui(|ui| {
                            ui.vertical(|ui| {
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
                            });
                        })
                    }
                    if btn.clicked() {
                        *pipe.user_data.editor_tabs.active_tab = tab_name.clone();
                    }
                    if ui.add(Button::new("\u{f00d}")).clicked() {
                        remove_tab = Some((tab_name.clone(), tab.client.should_save));
                    }
                    ui.add_space(10.0);
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
}
