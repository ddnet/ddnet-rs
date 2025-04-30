use std::collections::HashMap;

use egui::{
    scroll_area::ScrollBarVisibility, text::LayoutJob, Align, Color32, DragValue, FontId, Frame,
    Layout, ScrollArea, TextEdit, TextFormat,
};
use game_base::mapdef_06::DdraceTileNum;
use map::{
    map::{command_value::CommandValue, groups::layers::physics::MapLayerTilePhysicsTuneZone},
    skeleton::groups::layers::physics::MapLayerTunePhysicsSkeleton,
};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use ui_base::{
    components::edit_text::edit_text,
    types::{UiRenderPipe, UiState},
};

use crate::{
    actions::actions::{ActChangeTuneZone, EditorAction, EditorActionGroup},
    client::EditorClient,
    map::{
        EditorLayerUnionRef, EditorLayerUnionRefMut, EditorMapGroupsInterface, EditorPhysicsLayer,
        EditorPhysicsLayerNumberExtra, EditorPhysicsLayerProps, TuneOverviewExtra,
    },
    ui::user_data::UserDataWithTab,
};

fn zone_to_action(zone: &MapLayerTilePhysicsTuneZone) -> ActChangeTuneZone {
    ActChangeTuneZone {
        index: 0,
        old_name: zone.name.clone(),
        new_name: zone.name.clone(),
        old_tunes: zone.tunes.clone(),
        new_tunes: zone.tunes.clone(),
        new_enter_msg: zone.enter_msg.clone(),
        old_enter_msg: zone.enter_msg.clone(),
        new_leave_msg: zone.leave_msg.clone(),
        old_leave_msg: zone.leave_msg.clone(),
    }
}

pub fn render_tune_overview(
    ui: &mut egui::Ui,
    layer: &mut MapLayerTunePhysicsSkeleton<EditorPhysicsLayerProps>,
    client: &EditorClient,
) {
    ui.horizontal(|ui| {
        ui.label("Tunes of all zones");

        let btn = ui.button("\u{f884}").on_hover_text(
            "Sorts tune zones by index.\n\
            Note that this also changes the \
            order inside the map file permanently.",
        );
        if btn.clicked() {
            let mut acts = vec![];
            let mut acts_new: HashMap<_, _> = Default::default();

            for (tune_index, tune_zone) in layer.layer.tune_zones.iter() {
                let mut act = zone_to_action(tune_zone);
                act.index = *tune_index;
                let new_act = act.clone();
                acts_new.insert(*tune_index, new_act);
                act.new_enter_msg = None;
                act.new_leave_msg = None;
                act.new_name = Default::default();
                act.new_tunes = Default::default();
                acts.push(EditorAction::ChangeTuneZone(act));
            }
            let mut zones_sorted: Vec<_> = layer.layer.tune_zones.keys().collect();
            zones_sorted.sort();
            for tune_index in zones_sorted {
                let act = acts_new.remove(tune_index).expect("Bug in previous code.");
                acts.push(EditorAction::ChangeTuneZone(act));
            }
            client.execute_group(EditorActionGroup {
                actions: acts,
                identifier: None,
            });
        }
    });

    ui.separator();

    let val = &mut layer.user.number_extra_text;
    let zone_val = &mut layer.user.number_extra_zone;
    ui.label("Add commands for specified tune zone");
    ui.horizontal(|ui| {
        ui.label("Tune zone:");
        ui.add(DragValue::new(zone_val));
        ui.label("Tune command:");
        ui.add(TextEdit::singleline(val).hint_text("gravtiy 0.25"));
        if ui.button("\u{f0fe}").clicked() && !val.is_empty() {
            let (val, comment) = val
                .trim()
                .split_once('#')
                .map(|(s1, s2)| (s1.trim().to_string(), Some(s2.trim().to_string())))
                .unwrap_or_else(|| (val.trim().to_string(), None));
            if let Some((name, val)) = val.split_once(char::is_whitespace) {
                let tune_zone = layer
                    .layer
                    .tune_zones
                    .get(zone_val)
                    .cloned()
                    .unwrap_or_else(|| MapLayerTilePhysicsTuneZone {
                        name: Default::default(),
                        tunes: Default::default(),
                        enter_msg: Default::default(),
                        leave_msg: Default::default(),
                    });
                let mut new_tunes = tune_zone.tunes.clone();

                new_tunes.insert(
                    name.to_string(),
                    CommandValue {
                        value: val.to_string(),
                        comment,
                    },
                );

                let mut act = zone_to_action(&tune_zone);
                act.index = *zone_val;
                act.new_tunes = new_tunes;
                client.execute(
                    EditorAction::ChangeTuneZone(act),
                    Some(&format!("tune_zone_change_zones-{}", zone_val)),
                );
            }
        }
    });

    ui.separator();
    ui.add_space(10.0);

    ui.style_mut().spacing.item_spacing.y = 15.0;
    ui.style_mut().spacing.scroll.floating = false;
    ScrollArea::vertical()
        .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| {
            let overview_extra = &mut layer.user.tune_overview_extra;
            for (tune_index, tune_zone) in layer.layer.tune_zones.iter() {
                Frame::new()
                    .fill(Color32::from_black_alpha(100))
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

                        ui.label("Enter message:");
                        let cancel = |overview_extra: &mut TuneOverviewExtra| {
                            if let Some(edit_tune_zone) = overview_extra.get_mut(tune_index) {
                                edit_tune_zone.enter_msg = None;
                                if !edit_tune_zone.in_use() {
                                    overview_extra.remove(tune_index);
                                }
                            }
                        };
                        edit_text(
                            ui,
                            overview_extra,
                            |overview_extra| {
                                overview_extra
                                    .get_mut(tune_index)
                                    .and_then(|e| e.enter_msg.as_mut())
                                    .map(|m| ("".into(), m))
                            },
                            |overview_extra| {
                                let zone = overview_extra
                                    .entry(*tune_index)
                                    .or_insert_with_keep_order(Default::default);
                                zone.enter_msg =
                                    Some(tune_zone.enter_msg.clone().unwrap_or_default());
                            },
                            cancel,
                            |overview_extra| {
                                if let Some(edit_tune_zone) = overview_extra.get_mut(tune_index) {
                                    if let Some(edit_text) = edit_tune_zone.enter_msg.as_mut() {
                                        let new_enter_msg =
                                            (!edit_text.is_empty()).then(|| edit_text.clone());
                                        let mut act = zone_to_action(tune_zone);
                                        act.index = *tune_index;
                                        act.new_enter_msg = new_enter_msg;
                                        client.execute(
                                            EditorAction::ChangeTuneZone(act),
                                            Some(&format!("tune_zone_change_zones-{}", tune_index)),
                                        );
                                    }
                                    cancel(overview_extra);
                                }
                            },
                            || tune_zone.enter_msg.clone().unwrap_or_default().into(),
                        );
                        ui.separator();
                        ui.label("Leave message:");
                        let cancel = |overview_extra: &mut TuneOverviewExtra| {
                            if let Some(edit_tune_zone) = overview_extra.get_mut(tune_index) {
                                edit_tune_zone.leave_msg = None;
                                if !edit_tune_zone.in_use() {
                                    overview_extra.remove(tune_index);
                                }
                            }
                        };
                        edit_text(
                            ui,
                            overview_extra,
                            |overview_extra| {
                                overview_extra
                                    .get_mut(tune_index)
                                    .and_then(|e| e.leave_msg.as_mut())
                                    .map(|m| ("".into(), m))
                            },
                            |overview_extra| {
                                let zone = overview_extra
                                    .entry(*tune_index)
                                    .or_insert_with_keep_order(Default::default);
                                zone.leave_msg =
                                    Some(tune_zone.leave_msg.clone().unwrap_or_default());
                            },
                            cancel,
                            |overview_extra| {
                                if let Some(edit_tune_zone) = overview_extra.get_mut(tune_index) {
                                    if let Some(edit_text) = edit_tune_zone.leave_msg.as_mut() {
                                        let new_leave_msg =
                                            (!edit_text.is_empty()).then(|| edit_text.clone());
                                        let mut act = zone_to_action(tune_zone);
                                        act.index = *tune_index;
                                        act.new_leave_msg = new_leave_msg;
                                        client.execute(
                                            EditorAction::ChangeTuneZone(act),
                                            Some(&format!("tune_zone_change_zones-{}", tune_index)),
                                        );
                                    }
                                    cancel(overview_extra);
                                }
                            },
                            || tune_zone.leave_msg.clone().unwrap_or_default().into(),
                        );
                        ui.separator();

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
                                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                                        if ui.button("\u{f1f8}").clicked() {
                                            let mut act = zone_to_action(tune_zone);
                                            act.index = *tune_index;
                                            act.new_tunes.remove(cmd_name);
                                            client.execute(
                                                EditorAction::ChangeTuneZone(act),
                                                Some(&format!(
                                                    "tune_zone_change_zones-{}",
                                                    tune_index
                                                )),
                                            );
                                        }
                                        let cancel = |overview_extra: &mut TuneOverviewExtra| {
                                            if let Some(edit_tune_zone) =
                                                overview_extra.get_mut(tune_index)
                                            {
                                                edit_tune_zone.tunes.remove(cmd_name);
                                                if !edit_tune_zone.in_use() {
                                                    overview_extra.remove(tune_index);
                                                }
                                            }
                                        };
                                        edit_text(
                                            ui,
                                            overview_extra,
                                            |overview_extra| {
                                                overview_extra
                                                    .get_mut(tune_index)
                                                    .and_then(|e| e.tunes.get_mut(cmd_name))
                                                    .map(|c| &mut c.value)
                                                    .map(|m| (cmd_name.as_str().into(), m))
                                            },
                                            |overview_extra| {
                                                let zone = overview_extra
                                                    .entry(*tune_index)
                                                    .or_insert_with_keep_order(Default::default);
                                                let tune = zone
                                                    .tunes
                                                    .entry(cmd_name.clone())
                                                    .or_insert_keep_order(val.clone());
                                                *tune = val.clone();
                                            },
                                            cancel,
                                            |overview_extra| {
                                                if let Some(edit_tune_zone) =
                                                    overview_extra.get_mut(tune_index)
                                                {
                                                    if let Some(edit_text) = edit_tune_zone
                                                        .tunes
                                                        .get_mut(cmd_name)
                                                        .map(|c| &mut c.value)
                                                    {
                                                        let mut new_tunes = tune_zone.tunes.clone();

                                                        let entry = new_tunes
                                                            .entry(cmd_name.clone())
                                                            .or_insert_keep_order(CommandValue {
                                                                value: Default::default(),
                                                                comment: Default::default(),
                                                            });
                                                        entry.value = edit_text.clone();
                                                        let mut act = zone_to_action(tune_zone);
                                                        act.index = *tune_index;
                                                        act.new_tunes = new_tunes;
                                                        client.execute(
                                                            EditorAction::ChangeTuneZone(act),
                                                            Some(&format!(
                                                                "tune_zone_change_zones-{}",
                                                                tune_index
                                                            )),
                                                        );
                                                    }
                                                    cancel(overview_extra);
                                                }
                                            },
                                            || job.into(),
                                        );
                                    });
                                });
                            }
                        });
                    });
            }
        });

    if layer.layer.tune_zones.is_empty() {
        ui.label("No tune zones configuration found.");
    }
}

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
                                        .or_insert_with_keep_order(Default::default);

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
                        .or_insert_with_keep_order(Default::default);

                    let mut context_menu_extra_open = false;

                    let cur_tune_name = if tune.name.is_empty() {
                        active_tune.to_string()
                    } else {
                        tune.name.clone()
                    };
                    ui.menu_button(format!("Tunes of {}", cur_tune_name,), |ui| {
                        context_menu_extra_open = true;

                        ui.label("Enter message:");
                        let old_enter_msg = tune.enter_extra.clone();
                        let mut enter_msg = old_enter_msg.clone().unwrap_or_default();
                        let mut is_changed = ui
                            .add(
                                TextEdit::singleline(&mut enter_msg)
                                    .hint_text("Enabled magic tune"),
                            )
                            .changed();
                        tune.enter_extra = (!enter_msg.is_empty()).then_some(enter_msg);
                        ui.label("Leave message:");
                        let old_leave_msg = tune.leave_extra.clone();
                        let mut leave_msg = tune.leave_extra.clone().unwrap_or_default();
                        is_changed |= ui
                            .add(
                                TextEdit::singleline(&mut leave_msg)
                                    .hint_text("Disabled magic tune"),
                            )
                            .changed();
                        tune.leave_extra = (!leave_msg.is_empty()).then_some(leave_msg);

                        if is_changed {
                            pipe.user_data.editor_tab.client.execute(
                                EditorAction::ChangeTuneZone(ActChangeTuneZone {
                                    index: active_tune,
                                    old_name: tune.name.clone(),
                                    new_name: tune.name.clone(),
                                    old_tunes: tune.extra.clone(),
                                    new_tunes: tune.extra.clone(),
                                    new_enter_msg: tune.enter_extra.clone(),
                                    old_enter_msg,
                                    new_leave_msg: tune.leave_extra.clone(),
                                    old_leave_msg,
                                }),
                                Some(&format!("tune_zone_change_zones-{}", active_tune)),
                            );
                        }

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
                        ui.label(format!("Add commands for tune zone {}.", cur_tune_name));
                        ui.horizontal(|ui| {
                            ui.label("Tune command:");
                            ui.add(TextEdit::singleline(val).hint_text("gravtiy 0.25"));
                            if ui.button("\u{f0fe}").clicked() && !val.is_empty() {
                                let (val, comment) = val
                                    .trim()
                                    .split_once('#')
                                    .map(|(s1, s2)| {
                                        (s1.trim().to_string(), Some(s2.trim().to_string()))
                                    })
                                    .unwrap_or_else(|| (val.trim().to_string(), None));
                                if let Some((name, val)) = val.split_once(char::is_whitespace) {
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
                                        Some(&format!("tune_zone_change_zones-{}", active_tune)),
                                    );
                                }
                            }
                        });
                    });

                    let mut pointer_used = false;
                    ui.menu_button("Tunes overview", |ui| {
                        ui.set_min_width(600.0);
                        ui.set_min_height(
                            (100.0 * layer.layer.tune_zones.len() as f32).clamp(100.0, 600.0),
                        );

                        pointer_used = true;

                        render_tune_overview(ui, layer, &pipe.user_data.editor_tab.client);
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
