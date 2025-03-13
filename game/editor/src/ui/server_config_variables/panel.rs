use std::collections::BTreeMap;

use egui::{text::LayoutJob, Button, Color32, FontId, Layout, ScrollArea, TextFormat, UiBuilder};
use map::map::command_value::CommandValue;
use ui_base::{
    components::clearable_edit_field::clearable_edit_field,
    types::{UiRenderPipe, UiState},
};

use crate::{
    actions::actions::{ActSetConfigVariables, EditorAction},
    ui::user_data::UserDataWithTab,
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    let tab = &mut pipe.user_data.editor_tab;
    let map = &mut tab.map;
    if !map.user.ui_values.server_config_variables_open {
        return;
    }

    let res = {
        let mut panel = egui::TopBottomPanel::bottom("server_config_variables_panel")
            .resizable(true)
            .height_range(300.0..=600.0);
        panel = panel.default_height(300.0);

        Some(panel.show_inside(ui, |ui| {
            ui.allocate_new_ui(
                UiBuilder::new().max_rect(ui.available_rect_before_wrap()),
                |ui| {
                    ui.horizontal(|ui| {
                        clearable_edit_field(
                            ui,
                            &mut map.config.user.conf_var_string,
                            Some(200.0),
                            None,
                        );

                        if let Some(((var_name, args), comment)) = ui
                            .button("\u{f0fe}")
                            .clicked()
                            .then(|| {
                                let (cmd, comment) = map
                                    .config
                                    .user
                                    .conf_var_string
                                    .split_once('#')
                                    .map(|(s1, s2)| {
                                        (s1.trim().to_string(), Some(s2.trim().to_string()))
                                    })
                                    .unwrap_or_else(|| {
                                        (map.config.user.conf_var_string.trim().to_string(), None)
                                    });
                                cmd.trim()
                                    .split_once(char::is_whitespace)
                                    .map(|(s1, s2)| (s1.trim().to_string(), s2.trim().to_string()))
                                    .zip(Some(comment))
                            })
                            .flatten()
                        {
                            let old_config_variables = map.config.def.config_variables.clone();
                            let mut new_config_variables = map.config.def.config_variables.clone();
                            new_config_variables.insert(
                                var_name.to_string(),
                                CommandValue {
                                    value: args.to_string(),
                                    comment,
                                },
                            );
                            tab.client.execute(
                                EditorAction::SetConfigVariables(ActSetConfigVariables {
                                    old_config_variables,
                                    new_config_variables,
                                }),
                                Some("server-config-variables"),
                            );

                            map.config.user.conf_var_string.clear();
                        }
                    });
                    ScrollArea::vertical().show(ui, |ui| {
                        ui.vertical(|ui| {
                            let vars: BTreeMap<_, _> = map
                                .config
                                .def
                                .config_variables
                                .clone()
                                .into_iter()
                                .collect();
                            for (index, (var_name, args)) in vars.iter().enumerate() {
                                ui.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
                                    // trash can icon
                                    if ui.button("\u{f1f8}").clicked() {
                                        let old_config_variables =
                                            map.config.def.config_variables.clone();
                                        let mut new_config_variables =
                                            map.config.def.config_variables.clone();
                                        new_config_variables.remove(var_name);
                                        tab.client.execute(
                                            EditorAction::SetConfigVariables(
                                                ActSetConfigVariables {
                                                    old_config_variables,
                                                    new_config_variables,
                                                },
                                            ),
                                            Some("server-config-variables"),
                                        );
                                    }

                                    ui.with_layout(
                                        Layout::left_to_right(egui::Align::Min)
                                            .with_main_justify(true),
                                        |ui| {
                                            let mut job = LayoutJob::simple_singleline(
                                                format!("{} {}", var_name, args.value),
                                                FontId::default(),
                                                Color32::WHITE,
                                            );
                                            if let Some(comment) = &args.comment {
                                                job.append(
                                                    &format!(" # {}", comment),
                                                    0.0,
                                                    TextFormat {
                                                        color: Color32::GRAY,
                                                        ..Default::default()
                                                    },
                                                );
                                            }
                                            if ui
                                                .add(Button::new(job).selected(
                                                    Some(index)
                                                        == map.config.user.selected_conf_var,
                                                ))
                                                .clicked()
                                            {
                                                map.config.user.selected_conf_var = Some(index);
                                            }
                                        },
                                    );
                                });
                            }
                        });
                    });
                },
            )
        }))
    };

    if let Some(res) = res {
        ui_state.add_blur_rect(res.response.rect, 0.0);
    }
}
