use egui::{text::LayoutJob, Button, Color32, FontId, Layout, ScrollArea, TextFormat, UiBuilder};
use egui_extras::{Size, StripBuilder};
use map::map::command_value::CommandValue;
use ui_base::{
    components::clearable_edit_field::clearable_edit_field,
    types::{UiRenderPipe, UiState},
};

use crate::{
    actions::actions::{ActSetCommands, EditorAction},
    map::EditorPhysicsLayer,
    tab::EditorTab,
    ui::{top_toolbar::tune::render_tune_overview, user_data::UserDataWithTab},
};

pub fn render_server_commands(ui: &mut egui::Ui, tab: &mut EditorTab) {
    let map = &mut tab.map;
    ui.allocate_new_ui(
        UiBuilder::new().max_rect(ui.available_rect_before_wrap()),
        |ui| {
            ui.horizontal(|ui| {
                clearable_edit_field(ui, &mut map.config.user.cmd_string, Some(200.0), None);

                if let Some((value, comment)) = ui.button("\u{f0fe}").clicked().then_some(
                    map.config
                        .user
                        .cmd_string
                        .split_once('#')
                        .map(|(s1, s2)| (s1.trim().to_string(), Some(s2.trim().to_string())))
                        .unwrap_or_else(|| (map.config.user.cmd_string.trim().to_string(), None)),
                ) {
                    let old_commands = map.config.def.commands.clone();
                    let mut new_commands = map.config.def.commands.clone();
                    new_commands.push(CommandValue { value, comment });
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
                    for (index, cmd) in map.config.def.commands.iter().enumerate() {
                        ui.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
                            // trash can icon
                            if ui.button("\u{f1f8}").clicked() {
                                let old_commands = map.config.def.commands.clone();
                                let mut new_commands = map.config.def.commands.clone();
                                new_commands.remove(index);
                                tab.client.execute(
                                    EditorAction::SetCommands(ActSetCommands {
                                        old_commands,
                                        new_commands,
                                    }),
                                    Some("server-commands"),
                                );
                            }

                            ui.with_layout(
                                Layout::left_to_right(egui::Align::Min).with_main_justify(true),
                                |ui| {
                                    let mut job = LayoutJob::simple_singleline(
                                        cmd.value.clone(),
                                        FontId::default(),
                                        Color32::WHITE,
                                    );
                                    if let Some(comment) = &cmd.comment {
                                        job.append(
                                            &format!(" # {comment}"),
                                            0.0,
                                            TextFormat {
                                                color: Color32::GRAY,
                                                ..Default::default()
                                            },
                                        );
                                    }
                                    if ui
                                        .add(
                                            Button::new(job).selected(
                                                Some(index) == map.config.user.selected_cmd,
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
    );
}

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    let tab = &mut pipe.user_data.editor_tab;
    let map = &mut tab.map;
    if !map.user.ui_values.server_commands_open {
        return;
    }

    let res = {
        let mut panel = egui::TopBottomPanel::bottom("server_settings_panel")
            .resizable(true)
            .height_range(300.0..=600.0);
        panel = panel.default_height(300.0);

        Some(panel.show_inside(ui, |ui| {
            StripBuilder::new(ui)
                .size(Size::remainder())
                .size(Size::exact(600.0))
                .horizontal(|mut strip| {
                    strip.cell(|ui| {
                        render_server_commands(ui, tab);
                    });
                    strip.cell(|ui| {
                        if let Some(EditorPhysicsLayer::Tune(layer)) = tab
                            .map
                            .groups
                            .physics
                            .layers
                            .iter_mut()
                            .find(|l| matches!(l, EditorPhysicsLayer::Tune(_)))
                        {
                            render_tune_overview(ui, layer, &tab.client);
                        }
                    });
                })
        }))
    };

    if let Some(res) = res {
        ui_state.add_blur_rect(res.response.rect, 0.0);
    }
}
