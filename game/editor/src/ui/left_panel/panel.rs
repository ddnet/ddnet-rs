use egui::Button;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    explain::{
        TEXT_2D_IMAGE_ARRAY, TEXT_IMAGES, TEXT_LAYERS_AND_GROUPS_OVERVIEW, TEXT_SOUND_SOURCES,
    },
    map::EditorGroupPanelTab,
    ui::user_data::UserDataWithTab,
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    let res = {
        let mut panel = egui::SidePanel::left("left_panel")
            .resizable(true)
            .width_range(120.0..=260.0);
        panel = panel.default_width(200.0);

        Some(panel.show_inside(ui, |ui| {
            let map = &mut pipe.user_data.editor_tab.map;
            let panel_tab = &mut map.user.ui_values.group_panel_active_tab;
            ui.vertical_centered_justified(|ui| {
                ui.with_layout(
                    egui::Layout::from_main_dir_and_cross_align(
                        egui::Direction::LeftToRight,
                        egui::Align::Min,
                    )
                    .with_main_align(egui::Align::Center),
                    |ui| {
                        if ui
                            .add(Button::new("\u{f5fd}").selected(matches!(
                                panel_tab,
                                EditorGroupPanelTab::GroupsAndLayers
                            )))
                            .on_hover_ui(|ui| {
                                let mut cache = egui_commonmark::CommonMarkCache::default();
                                egui_commonmark::CommonMarkViewer::new().show(
                                    ui,
                                    &mut cache,
                                    TEXT_LAYERS_AND_GROUPS_OVERVIEW,
                                );
                            })
                            .clicked()
                        {
                            *panel_tab = EditorGroupPanelTab::GroupsAndLayers;
                        }
                        if ui
                            .add(
                                Button::new("\u{f03e}")
                                    .selected(matches!(panel_tab, EditorGroupPanelTab::Images(_))),
                            )
                            .on_hover_ui(|ui| {
                                let mut cache = egui_commonmark::CommonMarkCache::default();
                                egui_commonmark::CommonMarkViewer::new().show(
                                    ui,
                                    &mut cache,
                                    TEXT_IMAGES,
                                );
                            })
                            .clicked()
                        {
                            *panel_tab = EditorGroupPanelTab::Images(Default::default());
                        }
                        if ui
                            .add(
                                Button::new("\u{f302}").selected(matches!(
                                    panel_tab,
                                    EditorGroupPanelTab::ArrayImages(_)
                                )),
                            )
                            .on_hover_ui(|ui| {
                                let mut cache = egui_commonmark::CommonMarkCache::default();
                                egui_commonmark::CommonMarkViewer::new().show(
                                    ui,
                                    &mut cache,
                                    TEXT_2D_IMAGE_ARRAY,
                                );
                            })
                            .clicked()
                        {
                            *panel_tab = EditorGroupPanelTab::ArrayImages(Default::default());
                        }
                        if ui
                            .add(
                                Button::new("\u{1f3b5}")
                                    .selected(matches!(panel_tab, EditorGroupPanelTab::Sounds(_))),
                            )
                            .on_hover_ui(|ui| {
                                let mut cache = egui_commonmark::CommonMarkCache::default();
                                egui_commonmark::CommonMarkViewer::new().show(
                                    ui,
                                    &mut cache,
                                    TEXT_SOUND_SOURCES,
                                );
                            })
                            .clicked()
                        {
                            *panel_tab = EditorGroupPanelTab::Sounds(Default::default());
                        }
                    },
                );
            });

            match panel_tab {
                EditorGroupPanelTab::GroupsAndLayers => {
                    super::groups_and_layers::render(ui, pipe);
                }
                EditorGroupPanelTab::Images(panel_data) => {
                    super::images::render(
                        ui,
                        &pipe.user_data.editor_tab.client,
                        &map.groups,
                        &mut map.resources,
                        panel_data,
                        pipe.user_data.io,
                    );
                }
                EditorGroupPanelTab::ArrayImages(panel_data) => {
                    super::image_arrays::render(
                        ui,
                        &pipe.user_data.editor_tab.client,
                        &map.groups,
                        &mut map.resources,
                        panel_data,
                        pipe.user_data.io,
                    );
                }
                EditorGroupPanelTab::Sounds(panel_data) => {
                    super::sounds::render(
                        ui,
                        &pipe.user_data.editor_tab.client,
                        &map.groups,
                        &mut map.resources,
                        panel_data,
                        pipe.user_data.io,
                    );
                }
            }
        }))
    };

    if let Some(res) = res {
        ui_state.add_blur_rect(res.response.rect, 0.0);
    }
}
