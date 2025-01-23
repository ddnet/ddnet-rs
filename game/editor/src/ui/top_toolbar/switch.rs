use egui::{Color32, DragValue, Layout, ScrollArea, TextEdit};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    actions::actions::{ActChangeSwitch, EditorAction},
    map::{
        EditorLayerUnionRef, EditorLayerUnionRefMut, EditorMapGroupsInterface, EditorPhysicsLayer,
        EditorPhysicsLayerNumberExtra,
    },
    ui::user_data::UserDataWithTab,
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    let map = &mut pipe.user_data.editor_tab.map;
    let Some(EditorLayerUnionRef::Physics {
        layer: EditorPhysicsLayer::Switch(layer),
        ..
    }) = map.groups.active_layer()
    else {
        return;
    };
    let style = ui.style();
    let height = style.spacing.interact_size.y + style.spacing.item_spacing.y;

    // TODO: maybe recheck in an interval?
    if map.groups.physics.user.active_switch_in_use.is_none() {
        let active_switch = map.groups.physics.user.active_switch;
        let tiles = &layer.layer.base.tiles;
        map.groups.physics.user.active_switch_in_use = Some(pipe.user_data.tp.install(|| {
            tiles
                .par_iter()
                .find_any(|tile| tile.base.index != 0 && tile.number == active_switch)
                .is_some()
        }));
    }

    let res = egui::TopBottomPanel::top("top_toolbar_switch_extra")
        .resizable(false)
        .default_height(height)
        .height_range(height..=height)
        .show_inside(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    let bg_color =
                        if let Some(in_use) = map.groups.physics.user.active_switch_in_use {
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
                    let prev_switch = map.groups.physics.user.active_switch;
                    let response = ui.add(
                        DragValue::new(&mut map.groups.physics.user.active_switch)
                            .update_while_editing(false)
                            .prefix("Switch: "),
                    );
                    let context_menu_open = response.context_menu_opened();

                    let mut active_switch = map.groups.physics.user.active_switch;

                    let Some(EditorLayerUnionRefMut::Physics {
                        layer: EditorPhysicsLayer::Switch(layer),
                        ..
                    }) = map.groups.active_layer_mut()
                    else {
                        return;
                    };
                    response.context_menu(|ui| {
                        ScrollArea::vertical().show(ui, |ui| {
                            for i in 1..=u8::MAX {
                                let mut switch_name = String::new();
                                if let Some(switch) = layer.user.number_extra.get(&i) {
                                    switch_name.clone_from(&switch.name);
                                }
                                ui.with_layout(
                                    Layout::right_to_left(egui::Align::Min)
                                        .with_cross_justify(false)
                                        .with_main_wrap(false),
                                    |ui| {
                                        if ui.button("\u{f25a}").clicked() {
                                            active_switch = i;
                                        }
                                        ui.add(
                                            TextEdit::singleline(&mut switch_name)
                                                .hint_text(format!("Switch #{i}")),
                                        );
                                    },
                                );
                                let switch = layer
                                    .user
                                    .number_extra
                                    .entry(i)
                                    .or_insert_with(Default::default);

                                if switch.name != switch_name {
                                    let old_name = layer
                                        .layer
                                        .switch_names
                                        .get(&i)
                                        .cloned()
                                        .unwrap_or_default();
                                    pipe.user_data.editor_tab.client.execute(
                                        EditorAction::ChangeSwitch(ActChangeSwitch {
                                            index: i,
                                            old_name,
                                            new_name: switch_name.clone(),
                                        }),
                                        Some("switch_changes"),
                                    );
                                }
                                switch.name = switch_name;
                            }
                        });
                    });
                    if context_menu_open && !layer.user.context_menu_open {
                        layer.user.number_extra.clear();
                        layer
                            .user
                            .number_extra
                            .extend(layer.layer.switch_names.iter().map(|(i, z)| {
                                (
                                    *i,
                                    EditorPhysicsLayerNumberExtra {
                                        name: z.clone(),
                                        extra: Default::default(),
                                    },
                                )
                            }));
                    }
                    layer.user.context_menu_open = context_menu_open;

                    ui.add(
                        DragValue::new(&mut layer.user.switch_delay)
                            .update_while_editing(false)
                            .prefix("Delay: "),
                    );

                    map.groups.physics.user.active_switch = active_switch;
                    if prev_switch != map.groups.physics.user.active_switch {
                        // recheck used
                        map.groups.physics.user.active_switch_in_use = None;
                    }
                });
            });
        });

    ui_state.add_blur_rect(res.response.rect, 0.0);
}
