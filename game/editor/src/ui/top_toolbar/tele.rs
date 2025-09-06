use egui::{Color32, DragValue, Layout, ScrollArea, TextEdit};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    actions::actions::{ActChangeTeleporter, EditorAction},
    map::{
        EditorLayerUnionRef, EditorLayerUnionRefMut, EditorMapGroupsInterface, EditorPhysicsLayer,
        EditorPhysicsLayerNumberExtra,
    },
    ui::user_data::UserDataWithTab,
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    let map = &mut pipe.user_data.editor_tab.map;
    let Some(EditorLayerUnionRef::Physics {
        layer: EditorPhysicsLayer::Tele(layer),
        ..
    }) = map.groups.active_layer()
    else {
        return;
    };
    let style = ui.style();
    let height = style.spacing.interact_size.y + style.spacing.item_spacing.y;

    // TODO: maybe recheck in an interval?
    if map.groups.physics.user.active_tele_in_use.is_none() {
        let active_tele = map.groups.physics.user.active_tele;
        let tiles = &layer.layer.base.tiles;
        map.groups.physics.user.active_tele_in_use = Some(pipe.user_data.tp.install(|| {
            tiles
                .par_iter()
                .find_any(|tile| tile.base.index != 0 && tile.number == active_tele)
                .is_some()
        }));
    }

    let res = egui::TopBottomPanel::top("top_toolbar_tele_extra")
        .resizable(false)
        .default_height(height)
        .height_range(height..=height)
        .show_inside(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    let bg_color = if let Some(in_use) = map.groups.physics.user.active_tele_in_use
                    {
                        if in_use { Color32::GREEN } else { Color32::RED }
                    } else {
                        Color32::GRAY
                    };
                    let mut rect = ui.available_rect_before_wrap();
                    rect.set_width(5.0);
                    ui.painter().rect_filled(rect, 5.0, bg_color);
                    ui.add_space(5.0);
                    let prev_tele = map.groups.physics.user.active_tele;
                    let response = ui.add(
                        DragValue::new(&mut map.groups.physics.user.active_tele)
                            .update_while_editing(false)
                            .prefix("Tele: "),
                    );
                    let context_menu_open = response.context_menu_opened();

                    let mut active_tele = map.groups.physics.user.active_tele;

                    let Some(EditorLayerUnionRefMut::Physics {
                        layer: EditorPhysicsLayer::Tele(layer),
                        ..
                    }) = map.groups.active_layer_mut()
                    else {
                        return;
                    };
                    response.context_menu(|ui| {
                        ScrollArea::vertical().show(ui, |ui| {
                            for i in 1..=u8::MAX {
                                let mut tele_name = String::new();
                                if let Some(tele) = layer.user.number_extra.get(&i) {
                                    tele_name.clone_from(&tele.name);
                                }
                                ui.with_layout(
                                    Layout::right_to_left(egui::Align::Min)
                                        .with_cross_justify(false)
                                        .with_main_wrap(false),
                                    |ui| {
                                        if ui.button("\u{f25a}").clicked() {
                                            active_tele = i;
                                        }
                                        ui.add(
                                            TextEdit::singleline(&mut tele_name)
                                                .hint_text(format!("Tele #{i}")),
                                        );
                                    },
                                );
                                let tele = layer
                                    .user
                                    .number_extra
                                    .entry(i)
                                    .or_insert_with_keep_order(Default::default);

                                if tele.name != tele_name {
                                    let old_name =
                                        layer.layer.tele_names.get(&i).cloned().unwrap_or_default();
                                    pipe.user_data.editor_tab.client.execute(
                                        EditorAction::ChangeTeleporter(ActChangeTeleporter {
                                            index: i,
                                            old_name,
                                            new_name: tele_name.clone(),
                                        }),
                                        Some("tele_changes"),
                                    );
                                }
                                tele.name = tele_name;
                            }
                        });
                    });
                    if context_menu_open && !layer.user.context_menu_open {
                        layer.user.number_extra.clear();
                        layer
                            .user
                            .number_extra
                            .extend(layer.layer.tele_names.iter().map(|(i, z)| {
                                (
                                    *i,
                                    EditorPhysicsLayerNumberExtra {
                                        name: z.clone(),
                                        extra: Default::default(),
                                        enter_extra: Default::default(),
                                        leave_extra: Default::default(),
                                    },
                                )
                            }));
                    }
                    layer.user.context_menu_open = context_menu_open;

                    *pipe.user_data.pointer_is_used |=
                        layer.user.context_menu_open || layer.user.context_menu_extra_open;

                    map.groups.physics.user.active_tele = active_tele;
                    if prev_tele != map.groups.physics.user.active_tele {
                        // recheck used
                        map.groups.physics.user.active_tele_in_use = None;
                    }
                });
            });
        });
    ui_state.add_blur_rect(res.response.rect, 0.0);
}
