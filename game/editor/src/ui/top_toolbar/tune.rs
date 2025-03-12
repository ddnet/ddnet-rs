use egui::{
    text::LayoutJob, Color32, DragValue, FontId, Frame, Layout, ScrollArea, TextEdit, TextFormat,
};
use game_base::mapdef_06::DdraceTileNum;
use map::map::command_value::CommandValue;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    actions::actions::{ActChangeTuneZone, EditorAction},
    map::{
        EditorLayerUnionRef, EditorLayerUnionRefMut, EditorMapGroupsInterface, EditorPhysicsLayer,
        EditorPhysicsLayerNumberExtra,
    },
    ui::user_data::UserDataWithTab,
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    let map = &mut pipe.user_data.editor_tab.map;
    let Some(EditorLayerUnionRef::Physics {
        layer: EditorPhysicsLayer::Tune(layer),
        ..
    }) = map.groups.active_layer()
    else {
        return;
    };
    let style = ui.style();
    let height = style.spacing.interact_size.y + style.spacing.item_spacing.y;

    // TODO: maybe recheck in an interval?
    if map.groups.physics.user.active_tune_zone_in_use.is_none() {
        let active_tune_zone = map.groups.physics.user.active_tune_zone;
        let tiles = &layer.layer.base.tiles;
        map.groups.physics.user.active_tune_zone_in_use = Some(pipe.user_data.tp.install(|| {
            tiles
                .par_iter()
                .find_any(|tile| {
                    DdraceTileNum::Tune as u8 == tile.base.index && tile.number == active_tune_zone
                })
                .is_some()
        }));
    }

    let res = egui::TopBottomPanel::top("top_toolbar_tune_extra")
        .resizable(false)
        .default_height(height)
        .height_range(height..=height)
        .show_inside(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    let bg_color =
                        if let Some(in_use) = map.groups.physics.user.active_tune_zone_in_use {
                            if in_use {
                                Color32::GREEN
                            } else {
                                Color32::RED
                            }
                        } else {
                            Color32::GRAY
                        };
                    let mut rect = ui.available_rect_before_wrap();
                    rect.set_width(5.0);
                    ui.painter().rect_filled(rect, 5.0, bg_color);
                    ui.add_space(5.0);
                    let prev_tune = map.groups.physics.user.active_tune_zone;
                    let cur_tune = &mut map.groups.physics.user.active_tune_zone;
                    let response = ui.add(
                        DragValue::new(cur_tune)
                            .range(1..=u8::MAX)
                            .update_while_editing(false)
                            .prefix("Tune zone: "),
                    );
                    let context_menu_open = response.context_menu_opened();

                    let mut active_tune = map.groups.physics.user.active_tune_zone;

                    let Some(EditorLayerUnionRefMut::Physics {
                        layer: EditorPhysicsLayer::Tune(layer),
                        ..
                    }) = map.groups.active_layer_mut()
                    else {
                        return;
                    };
                    response.context_menu(|ui| {
                        ScrollArea::vertical()
                            .id_salt("tune_extra_scroll")
                            .show(ui, |ui| {
                                for i in 1..=u8::MAX {
                                    let mut tune_name = String::new();
                                    if let Some(tune) = layer.user.number_extra.get(&i) {
                                        tune_name.clone_from(&tune.name);
                                    }
                                    ui.with_layout(
                                        Layout::right_to_left(egui::Align::Min)
                                            .with_cross_justify(false)
                                            .with_main_wrap(false),
                                        |ui| {
                                            if ui.button("\u{f25a}").clicked() {
                                                active_tune = i;
                                            }
                                            ui.add(
                                                TextEdit::singleline(&mut tune_name)
                                                    .hint_text(format!("Tune zone #{i}")),
                                            );
                                        },
                                    );
                                    let tune = layer
                                        .user
                                        .number_extra
                                        .entry(i)
                                        .or_insert_with(Default::default);

                                    if tune.name != tune_name {
                                        let (old_name, old_zones, enter_msg, leave_msg) = layer
                                            .layer
                                            .tune_zones
                                            .get(&i)
                                            .map(|zone| {
                                                (
                                                    zone.name.clone(),
                                                    zone.tunes.clone(),
                                                    zone.enter_msg.clone(),
                                                    zone.leave_msg.clone(),
                                                )
                                            })
                                            .unwrap_or_default();
                                        pipe.user_data.editor_tab.client.execute(
                                            EditorAction::ChangeTuneZone(ActChangeTuneZone {
                                                index: i,
                                                old_name,
                                                new_name: tune_name.clone(),
                                                old_tunes: old_zones,
                                                new_tunes: tune.extra.clone(),
                                                new_enter_msg: enter_msg,
                                                old_enter_msg: tune.enter_extra.clone(),
                                                new_leave_msg: leave_msg,
                                                old_leave_msg: tune.leave_extra.clone(),
                                            }),
                                            Some(&format!(
                                                "tune_zone_change_zones-{}",
                                                active_tune
                                            )),
                                        );
                                    }
                                    tune.name = tune_name;
                                }
                            });
                    });

                    let tune = layer
                        .user
                        .number_extra
                        .entry(active_tune)
                        .or_insert_with(Default::default);

                    let mut context_menu_extra_open = false;

                    ui.menu_button(
                        format!(
                            "Tunes of {}",
                            if tune.name.is_empty() {
                                active_tune.to_string()
                            } else {
                                tune.name.clone()
                            },
                        ),
                        |ui| {
                            context_menu_extra_open = true;

                            ui.label("Enter message:");
                            let mut enter_msg = tune.enter_extra.clone().unwrap_or_default();
                            ui.text_edit_singleline(&mut enter_msg);
                            tune.enter_extra = (!enter_msg.is_empty()).then_some(enter_msg);
                            ui.label("Leave message:");
                            let mut leave_msg = tune.leave_extra.clone().unwrap_or_default();
                            ui.text_edit_singleline(&mut leave_msg);
                            tune.leave_extra = (!leave_msg.is_empty()).then_some(leave_msg);

                            ui.separator();

                            let tunes_clone = tune.extra.clone();
                            if !tunes_clone.is_empty() {
                                ui.label("Commands:");
                            }
                            ui.vertical(|ui| {
                                for (cmd_name, val) in tunes_clone.iter() {
                                    ui.horizontal(|ui| {
                                        let mut job = LayoutJob::simple_singleline(
                                            format!("{} {}", cmd_name, val.value),
                                            FontId::default(),
                                            Color32::WHITE,
                                        );
                                        if let Some(comment) = &val.comment {
                                            job.append(
                                                &format!(" # {}", comment),
                                                0.0,
                                                TextFormat {
                                                    color: Color32::GRAY,
                                                    ..Default::default()
                                                },
                                            );
                                        }
                                        ui.label(job);
                                        if ui.button("\u{f1f8}").clicked() {
                                            let old_tunes = tune.extra.clone();
                                            tune.extra.remove(cmd_name);
                                            pipe.user_data.editor_tab.client.execute(
                                                EditorAction::ChangeTuneZone(ActChangeTuneZone {
                                                    index: active_tune,
                                                    old_name: tune.name.clone(),
                                                    new_name: tune.name.clone(),
                                                    old_tunes,
                                                    new_tunes: tune.extra.clone(),
                                                    new_enter_msg: tune.enter_extra.clone(),
                                                    old_enter_msg: tune.enter_extra.clone(),
                                                    new_leave_msg: tune.leave_extra.clone(),
                                                    old_leave_msg: tune.leave_extra.clone(),
                                                }),
                                                Some(&format!(
                                                    "tune_zone_change_zones-{}",
                                                    active_tune
                                                )),
                                            );
                                        }
                                    });
                                }
                            });
                            let val = &mut layer.user.number_extra_text;
                            ui.add_space(10.0);
                            ui.separator();
                            ui.label("Add commands");
                            ui.horizontal(|ui| {
                                ui.label("Tune command:");
                                ui.text_edit_singleline(val);
                                if ui.button("\u{f0fe}").clicked() && !val.is_empty() {
                                    let (val, comment) = val
                                        .trim()
                                        .split_once('#')
                                        .map(|(s1, s2)| {
                                            (s1.trim().to_string(), Some(s2.trim().to_string()))
                                        })
                                        .unwrap_or_else(|| (val.trim().to_string(), None));
                                    if let Some((name, val)) = val.split_once(' ') {
                                        tune.extra.insert(
                                            name.to_string(),
                                            CommandValue {
                                                value: val.to_string(),
                                                comment,
                                            },
                                        );

                                        let (old_name, old_zones, enter_msg, leave_msg) = layer
                                            .layer
                                            .tune_zones
                                            .get(&active_tune)
                                            .map(|zone| {
                                                (
                                                    zone.name.clone(),
                                                    zone.tunes.clone(),
                                                    zone.enter_msg.clone(),
                                                    zone.leave_msg.clone(),
                                                )
                                            })
                                            .unwrap_or_default();
                                        pipe.user_data.editor_tab.client.execute(
                                            EditorAction::ChangeTuneZone(ActChangeTuneZone {
                                                index: active_tune,
                                                old_name,
                                                new_name: tune.name.clone(),
                                                old_tunes: old_zones,
                                                new_tunes: tune.extra.clone(),
                                                new_enter_msg: enter_msg,
                                                old_enter_msg: tune.enter_extra.clone(),
                                                new_leave_msg: leave_msg,
                                                old_leave_msg: tune.leave_extra.clone(),
                                            }),
                                            Some(&format!(
                                                "tune_zone_change_zones-{}",
                                                active_tune
                                            )),
                                        );
                                    }
                                }
                            });
                        },
                    );

                    let mut pointer_used = false;
                    ui.menu_button("Tunes overview", |ui| {
                        pointer_used = true;
                        ui.label("Tunes of all zones");

                        ui.style_mut().spacing.item_spacing.y = 20.0;
                        ScrollArea::vertical().show(ui, |ui| {
                            for (tune_index, tune_zone) in layer.layer.tune_zones.iter() {
                                Frame::new()
                                    .fill(Color32::from_black_alpha(50))
                                    .corner_radius(5)
                                    .inner_margin(5)
                                    .show(ui, |ui| {
                                        ui.style_mut().spacing.item_spacing.y = 4.0;
                                        ui.label(format!(
                                            "Tune zone {}",
                                            if tune_zone.name.is_empty() {
                                                tune_index.to_string()
                                            } else {
                                                tune_zone.name.clone()
                                            },
                                        ));
                                        ui.separator();

                                        if let Some(msg) = &tune_zone.enter_msg {
                                            ui.label("Enter message:");
                                            ui.label(msg);
                                            ui.separator();
                                        }
                                        if let Some(msg) = &tune_zone.leave_msg {
                                            ui.label("Leave message:");
                                            ui.label(msg);
                                            ui.separator();
                                        }

                                        if !tune_zone.tunes.is_empty() {
                                            ui.label("Commands:");
                                        }
                                        ui.vertical(|ui| {
                                            for (cmd_name, val) in tune_zone.tunes.iter() {
                                                ui.horizontal(|ui| {
                                                    let mut job = LayoutJob::simple_singleline(
                                                        format!("{} {}", cmd_name, val.value),
                                                        FontId::default(),
                                                        Color32::WHITE,
                                                    );
                                                    if let Some(comment) = &val.comment {
                                                        job.append(
                                                            &format!(" # {}", comment),
                                                            0.0,
                                                            TextFormat {
                                                                color: Color32::GRAY,
                                                                ..Default::default()
                                                            },
                                                        );
                                                    }
                                                    ui.label(job);
                                                });
                                            }
                                        });
                                    });
                            }
                        });
                    });

                    if (context_menu_open && !layer.user.context_menu_open)
                        || (context_menu_extra_open && !layer.user.context_menu_extra_open)
                    {
                        layer.user.number_extra.clear();
                        layer
                            .user
                            .number_extra
                            .extend(layer.layer.tune_zones.iter().map(|(i, z)| {
                                (
                                    *i,
                                    EditorPhysicsLayerNumberExtra {
                                        name: z.name.clone(),
                                        extra: z.tunes.clone(),
                                        enter_extra: z.enter_msg.clone(),
                                        leave_extra: z.leave_msg.clone(),
                                    },
                                )
                            }));
                    }
                    layer.user.context_menu_open = context_menu_open;
                    layer.user.context_menu_extra_open = context_menu_extra_open;

                    *pipe.user_data.pointer_is_used |= layer.user.context_menu_open
                        || layer.user.context_menu_extra_open
                        || pointer_used;

                    map.groups.physics.user.active_tune_zone = active_tune;
                    if prev_tune != map.groups.physics.user.active_tune_zone {
                        // recheck used
                        map.groups.physics.user.active_tune_zone_in_use = None;
                    }
                });
            });
        });
    ui_state.add_blur_rect(res.response.rect, 0.0);
}
