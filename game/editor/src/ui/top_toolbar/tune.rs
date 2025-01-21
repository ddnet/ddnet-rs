use egui::{Color32, DragValue, Layout, ScrollArea, TextEdit};
use game_base::mapdef_06::DdraceTileNum;
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
                    let response = ui.add(
                        DragValue::new(&mut map.groups.physics.user.active_tune_zone)
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
                                        let (old_name, old_zones) = layer
                                            .layer
                                            .tune_zones
                                            .get(&i)
                                            .map(|zone| (zone.name.clone(), zone.tunes.clone()))
                                            .unwrap_or_default();
                                        pipe.user_data.editor_tab.client.execute(
                                            EditorAction::ChangeTuneZone(ActChangeTuneZone {
                                                index: i,
                                                old_name,
                                                new_name: tune_name.clone(),
                                                old_tunes: old_zones,
                                                new_tunes: tune.extra.clone(),
                                            }),
                                            Some("tune_zone_change_zones"),
                                        );
                                    }
                                    tune.name = tune_name;
                                }
                            });
                    });
                    if context_menu_open && !layer.user.context_menu_open {
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
                                    },
                                )
                            }));
                    }
                    layer.user.context_menu_open = context_menu_open;

                    ui.menu_button("tunes", |ui| {
                        let tune = layer
                            .user
                            .number_extra
                            .entry(active_tune)
                            .or_insert_with(Default::default);

                        for (tune, val) in tune.extra.iter_mut() {
                            ui.horizontal(|ui| {
                                ui.label(tune);
                                ui.text_edit_singleline(val);
                                ui.label("delete_icon");
                            });
                        }
                        let (name, val) = &mut layer.user.number_extra_texts;
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(name);
                            ui.text_edit_singleline(val);
                            if ui.button("add_btn").clicked() && !name.is_empty() {
                                tune.extra.insert(name.clone(), val.clone());

                                let (old_name, old_zones) = layer
                                    .layer
                                    .tune_zones
                                    .get(&active_tune)
                                    .map(|zone| (zone.name.clone(), zone.tunes.clone()))
                                    .unwrap_or_default();
                                pipe.user_data.editor_tab.client.execute(
                                    EditorAction::ChangeTuneZone(ActChangeTuneZone {
                                        index: active_tune,
                                        old_name,
                                        new_name: tune.name.clone(),
                                        old_tunes: old_zones,
                                        new_tunes: tune.extra.clone(),
                                    }),
                                    Some("tune_zone_change_zones"),
                                );
                            }
                        });
                    });

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
