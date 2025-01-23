use std::collections::BTreeMap;

use egui::{Button, Layout, ScrollArea, UiBuilder};
use ui_base::{
    components::clearable_edit_field::clearable_edit_field,
    types::{UiRenderPipe, UiState},
};

use crate::{
    actions::actions::{ActSetCommands, EditorAction},
    ui::user_data::UserDataWithTab,
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    let tab = &mut pipe.user_data.editor_tab;
    let map = &mut tab.map;
    if !map.user.ui_values.server_settings_open {
        return;
    }

    let res = {
        let mut panel = egui::TopBottomPanel::bottom("server_settings_panel")
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
                            &mut map.config.user.cmd_string,
                            Some(200.0),
                            None,
                        );

                        if let Some((cmd, args)) = ui
                            .button("\u{f0fe}")
                            .clicked()
                            .then_some(map.config.user.cmd_string.split_once(" "))
                            .flatten()
                        {
                            let old_commands = map.config.def.commands.clone();
                            let mut new_commands = map.config.def.commands.clone();
                            new_commands.insert(cmd.to_string(), args.to_string());
                            tab.client.execute(
                                EditorAction::SetCommands(ActSetCommands {
                                    old_commands,
                                    new_commands,
                                }),
                                Some("server-commands"),
                            );

                            map.config.user.cmd_string.clear();
                        }
                    });
                    ScrollArea::vertical().show(ui, |ui| {
                        ui.vertical(|ui| {
                            let cmds: BTreeMap<_, _> =
                                map.config.def.commands.clone().into_iter().collect();
                            for (index, (cmd_name, args)) in cmds.iter().enumerate() {
                                ui.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
                                    // trash can icon
                                    if ui.button("\u{f1f8}").clicked() {
                                        let old_commands = map.config.def.commands.clone();
                                        let mut new_commands = map.config.def.commands.clone();
                                        new_commands.remove(cmd_name);
                                        tab.client.execute(
                                            EditorAction::SetCommands(ActSetCommands {
                                                old_commands,
                                                new_commands,
                                            }),
                                            Some("server-commands"),
                                        );
                                    }

                                    ui.with_layout(
                                        Layout::left_to_right(egui::Align::Min)
                                            .with_main_justify(true),
                                        |ui| {
                                            if ui
                                                .add(
                                                    Button::new(format!("{} {}", cmd_name, args))
                                                        .selected(
                                                            Some(index)
                                                                == map.config.user.selected_cmd,
                                                        ),
                                                )
                                                .clicked()
                                            {
                                                map.config.user.selected_cmd = Some(index);
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
