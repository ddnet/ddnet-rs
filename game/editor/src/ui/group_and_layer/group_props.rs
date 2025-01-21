use std::ops::RangeInclusive;

use egui::{Checkbox, DragValue};
use map::{map::groups::layers::tiles::MapTileLayerPhysicsTiles, types::NonZeroU16MinusOne};
use math::math::vector::{ffixed, uffixed};
use ui_base::{
    types::{UiRenderPipe, UiState},
    utils::toggle_ui,
};

use crate::{
    actions::actions::{
        ActAddRemGroup, ActChangeGroupAttr, ActChangeGroupName, ActChangePhysicsGroupAttr,
        ActMoveGroup, ActRemGroup, EditorAction,
    },
    map::{EditorGroups, EditorMap, EditorMapInterface, EditorPhysicsLayer},
    ui::{group_and_layer::shared::copy_tiles, user_data::UserDataWithTab},
};

#[derive(Debug)]
enum MoveGroup {
    IsBackground(bool),
    Group(usize),
}

fn render_group_move(
    ui: &mut egui::Ui,
    is_background: bool,
    g: usize,

    can_bg: bool,
    g_range: RangeInclusive<usize>,
) -> Option<MoveGroup> {
    let mut move_group = None;

    let mut new_is_background = is_background;
    ui.label("In background");
    if ui
        .add_enabled(can_bg, Checkbox::new(&mut new_is_background, ""))
        .changed()
    {
        move_group = Some(MoveGroup::IsBackground(new_is_background));
    }
    ui.end_row();

    ui.label("Group");
    let mut new_group = g;
    if ui
        .add_enabled(
            *g_range.start() != *g_range.end(),
            DragValue::new(&mut new_group).update_while_editing(false),
        )
        .changed()
    {
        move_group = Some(MoveGroup::Group(new_group));
    }
    ui.end_row();

    move_group
}

fn group_move_to_act(
    mv: MoveGroup,
    is_background: bool,
    g: usize,
    map: &EditorMap,
) -> Option<ActMoveGroup> {
    match mv {
        MoveGroup::IsBackground(new_is_background) => {
            if new_is_background == is_background {
                return None;
            }
            let groups = if new_is_background {
                &map.groups.background
            } else {
                &map.groups.foreground
            };
            Some(ActMoveGroup {
                old_is_background: is_background,
                old_group: g,
                new_is_background,
                new_group: groups.len(),
            })
        }
        MoveGroup::Group(new_group) => {
            if new_group == g {
                return None;
            }
            let groups = if is_background {
                &map.groups.background
            } else {
                &map.groups.foreground
            };

            if new_group < g || new_group < groups.len() {
                Some(ActMoveGroup {
                    old_is_background: is_background,
                    old_group: g,
                    new_is_background: is_background,
                    new_group,
                })
            } else {
                None
            }
        }
    }
}

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    #[derive(Debug, PartialEq, Eq)]
    enum GroupAttrMode {
        Design,
        Physics,
        /// only design groups selected
        DesignMulti,
        /// design & physics groups mixed
        DesignAndPhysicsMulti,
        None,
    }

    // check which groups are `selected`
    let tab = &mut *pipe.user_data.editor_tab;
    let map = &mut tab.map;

    let bg_selection = map
        .groups
        .background
        .iter()
        .filter(|bg| bg.user.selected.is_some());
    let fg_selection = map
        .groups
        .foreground
        .iter()
        .filter(|fg| fg.user.selected.is_some());
    let bg_selected = bg_selection.count();
    let phy_selected = map.groups.physics.user.selected.is_some();
    let fg_selected = fg_selection.count();

    let mut attr_mode = GroupAttrMode::None;
    if bg_selected > 0 {
        attr_mode = if bg_selected == 1 {
            GroupAttrMode::Design
        } else {
            GroupAttrMode::DesignMulti
        };
    }
    if phy_selected {
        if attr_mode == GroupAttrMode::None {
            attr_mode = GroupAttrMode::Physics;
        } else {
            attr_mode = GroupAttrMode::DesignAndPhysicsMulti;
        }
    }
    if fg_selected > 0 {
        if attr_mode == GroupAttrMode::None {
            attr_mode = if fg_selected == 1 {
                GroupAttrMode::Design
            } else {
                GroupAttrMode::DesignMulti
            };
        } else if attr_mode == GroupAttrMode::Design {
            attr_mode = GroupAttrMode::DesignMulti;
        } else if attr_mode == GroupAttrMode::Physics {
            attr_mode = GroupAttrMode::DesignAndPhysicsMulti;
        }
    }
    fn move_limits(groups: &EditorGroups, is_background: bool) -> (bool, RangeInclusive<usize>) {
        (
            {
                let groups = if !is_background {
                    &groups.background
                } else {
                    &groups.foreground
                };
                !groups.is_empty()
            },
            {
                let groups = if is_background {
                    &groups.background
                } else {
                    &groups.foreground
                };
                0..=groups.len().saturating_sub(1)
            },
        )
    }

    let mut bg_selection = map
        .groups
        .background
        .iter()
        .enumerate()
        .filter(|(_, bg)| bg.user.selected.is_some())
        .map(|(g, _)| (true, g));
    let mut fg_selection = map
        .groups
        .foreground
        .iter()
        .enumerate()
        .filter(|(_, fg)| fg.user.selected.is_some())
        .map(|(g, _)| (false, g));
    let window_res = match attr_mode {
        GroupAttrMode::Design => {
            let (is_background, g) = bg_selection
                .next()
                .unwrap_or_else(|| fg_selection.next().unwrap());
            let (bg_move_limit, g_limit) = move_limits(&map.groups, is_background);
            let group = if is_background {
                &mut map.groups.background[g]
            } else {
                &mut map.groups.foreground[g]
            };

            let window = egui::Window::new("Design Group Attributes")
                .resizable(false)
                .collapsible(false);

            // render group attributes
            let group_editor = group.user.selected.as_mut().unwrap();
            let attr = &mut group_editor.attr;
            let attr_cmp = *attr;
            let name_cmp = group_editor.name.clone();

            let mut delete_group = false;
            let mut move_group = None;

            let res = window.show(ui.ctx(), |ui| {
                egui::Grid::new("design group attr grid")
                    .num_columns(2)
                    .spacing([20.0, 4.0])
                    .show(ui, |ui| {
                        // pos x
                        ui.label("Pos x");
                        let mut x = attr.offset.x.to_num::<f64>();
                        ui.add(egui::DragValue::new(&mut x).update_while_editing(false));
                        attr.offset.x = ffixed::from_num(x);
                        ui.end_row();
                        // pos y
                        ui.label("Pos y");
                        let mut y = attr.offset.y.to_num::<f64>();
                        ui.add(egui::DragValue::new(&mut y).update_while_editing(false));
                        attr.offset.y = ffixed::from_num(y);
                        ui.end_row();
                        // para x
                        ui.label("Parallax x");
                        let mut x = attr.parallax.x.to_num::<f64>();
                        ui.add(egui::DragValue::new(&mut x).update_while_editing(false));
                        attr.parallax.x = ffixed::from_num(x);
                        ui.end_row();
                        // para y
                        ui.label("Parallax y");
                        let mut y = attr.parallax.y.to_num::<f64>();
                        ui.add(egui::DragValue::new(&mut y).update_while_editing(false));
                        attr.parallax.y = ffixed::from_num(y);
                        ui.end_row();
                        // clipping on/off
                        ui.label("Clipping");
                        let mut clip_on_off = attr.clipping.is_some();
                        toggle_ui(ui, &mut clip_on_off);
                        ui.end_row();
                        if attr.clipping.is_some() != clip_on_off {
                            if clip_on_off {
                                attr.clipping = Some(Default::default());
                            } else {
                                attr.clipping = None;
                            }
                        }
                        if let Some(clipping) = &mut attr.clipping {
                            // clipping x
                            ui.label("Clipping - x");
                            let mut x = clipping.pos.x.to_num::<f64>();
                            ui.add(egui::DragValue::new(&mut x).update_while_editing(false));
                            clipping.pos.x = ffixed::from_num(x);
                            ui.end_row();
                            // clipping y
                            ui.label("Clipping - y");
                            let mut y = clipping.pos.y.to_num::<f64>();
                            ui.add(egui::DragValue::new(&mut y).update_while_editing(false));
                            clipping.pos.y = ffixed::from_num(y);
                            ui.end_row();
                            // clipping w
                            ui.label("Clipping - width");
                            let mut x = clipping.size.x.to_num::<f64>();
                            ui.add(
                                egui::DragValue::new(&mut x)
                                    .update_while_editing(false)
                                    .range(0.0..=f64::MAX),
                            );
                            clipping.size.x = uffixed::from_num(x);
                            ui.end_row();
                            // clipping h
                            ui.label("Clipping - height");
                            let mut y = clipping.size.y.to_num::<f64>();
                            ui.add(
                                egui::DragValue::new(&mut y)
                                    .update_while_editing(false)
                                    .range(0.0..=f64::MAX),
                            );
                            clipping.size.y = uffixed::from_num(y);
                            ui.end_row();
                        }
                        // name
                        ui.label("Group name");
                        ui.text_edit_singleline(&mut group_editor.name);
                        ui.end_row();
                        // delete
                        if ui.button("Delete group").clicked() {
                            delete_group = true;
                        }
                        ui.end_row();

                        ui.label("Move group");
                        ui.end_row();

                        // group moving
                        move_group =
                            render_group_move(ui, is_background, g, bg_move_limit, g_limit);
                    });
            });

            if *attr != attr_cmp {
                tab.client.execute(
                    EditorAction::ChangeGroupAttr(ActChangeGroupAttr {
                        is_background,
                        group_index: g,
                        old_attr: group.attr,
                        new_attr: *attr,
                    }),
                    Some(&format!("change-design-group-attr-{is_background}-{g}")),
                );
            } else if group_editor.name != name_cmp {
                tab.client.execute(
                    EditorAction::ChangeGroupName(ActChangeGroupName {
                        is_background,
                        group_index: g,
                        old_name: group.name.clone(),
                        new_name: group_editor.name.clone(),
                    }),
                    Some(&format!("change-design-group-name-{is_background}-{g}")),
                );
            } else if delete_group {
                tab.client.execute(
                    EditorAction::RemGroup(ActRemGroup {
                        base: ActAddRemGroup {
                            is_background,
                            index: g,
                            group: group.clone().into(),
                        },
                    }),
                    None,
                );
            } else if let Some(move_act) =
                move_group.and_then(|mv| group_move_to_act(mv, is_background, g, map))
            {
                tab.client.execute(EditorAction::MoveGroup(move_act), None);
            }

            res
        }
        GroupAttrMode::Physics => {
            // width & height, nothing else
            let group = &mut map.groups.physics;
            let window = egui::Window::new("Physics Group Attributes")
                .resizable(false)
                .collapsible(false);
            let res =
                window.show(ui.ctx(), |ui| {
                    egui::Grid::new("design group attr grid")
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            // render physics group attributes
                            let attr = group.user.selected.as_mut().unwrap();
                            let attr_cmp = *attr;
                            // w
                            ui.label("width");
                            let mut w = attr.width.get();
                            ui.add(
                                egui::DragValue::new(&mut w)
                                    .update_while_editing(false)
                                    .update_while_editing(false)
                                    .range(1..=u16::MAX - 1),
                            );
                            attr.width = NonZeroU16MinusOne::new(w).unwrap();
                            ui.end_row();
                            // h
                            ui.label("height");
                            let mut h = attr.height.get();
                            ui.add(
                                egui::DragValue::new(&mut h)
                                    .update_while_editing(false)
                                    .update_while_editing(false)
                                    .range(1..=u16::MAX - 1),
                            );
                            attr.height = NonZeroU16MinusOne::new(h).unwrap();
                            ui.end_row();
                            if *attr != attr_cmp {
                                let old_layer_tiles: Vec<_> = group
                                    .layers
                                    .iter()
                                    .map(|layer| match layer {
                                        EditorPhysicsLayer::Arbitrary(_) => {
                                            panic!("arbitrary tile layers are unsupported")
                                        }
                                        EditorPhysicsLayer::Game(layer) => {
                                            MapTileLayerPhysicsTiles::Game(
                                                layer.layer.tiles.clone(),
                                            )
                                        }
                                        EditorPhysicsLayer::Front(layer) => {
                                            MapTileLayerPhysicsTiles::Front(
                                                layer.layer.tiles.clone(),
                                            )
                                        }
                                        EditorPhysicsLayer::Tele(layer) => {
                                            MapTileLayerPhysicsTiles::Tele(
                                                layer.layer.base.tiles.clone(),
                                            )
                                        }
                                        EditorPhysicsLayer::Speedup(layer) => {
                                            MapTileLayerPhysicsTiles::Speedup(
                                                layer.layer.tiles.clone(),
                                            )
                                        }
                                        EditorPhysicsLayer::Switch(layer) => {
                                            MapTileLayerPhysicsTiles::Switch(
                                                layer.layer.base.tiles.clone(),
                                            )
                                        }
                                        EditorPhysicsLayer::Tune(layer) => {
                                            MapTileLayerPhysicsTiles::Tune(
                                                layer.layer.base.tiles.clone(),
                                            )
                                        }
                                    })
                                    .collect();
                                tab.client.execute(
                                    EditorAction::ChangePhysicsGroupAttr(
                                        ActChangePhysicsGroupAttr {
                                            old_attr: group.attr,
                                            new_attr: *attr,

                                            new_layer_tiles: {
                                                let width_or_height_change = group.attr.width
                                                    != attr.width
                                                    || group.attr.height != attr.height;
                                                if width_or_height_change {
                                                    let width_old = group.attr.width.get() as usize;
                                                    let height_old =
                                                        group.attr.height.get() as usize;
                                                    let width_new = attr.width.get() as usize;
                                                    let height_new = attr.height.get() as usize;
                                                    group
                                            .layers
                                            .iter()
                                            .map(|layer| match layer {
                                                EditorPhysicsLayer::Arbitrary(_) => {
                                                    panic!("arbitrary tile layers are unsupported")
                                                }
                                                EditorPhysicsLayer::Game(layer) => {
                                                    MapTileLayerPhysicsTiles::Game(copy_tiles(
                                                        width_old, height_old, width_new,
                                                        height_new, &layer.layer.tiles,
                                                    ))
                                                }
                                                EditorPhysicsLayer::Front(layer) => {
                                                    MapTileLayerPhysicsTiles::Front(copy_tiles(
                                                        width_old, height_old, width_new,
                                                        height_new, &layer.layer.tiles,
                                                    ))
                                                }
                                                EditorPhysicsLayer::Tele(layer) => {
                                                    MapTileLayerPhysicsTiles::Tele(copy_tiles(
                                                        width_old, height_old, width_new,
                                                        height_new, &layer.layer.base.tiles,
                                                    ))
                                                }
                                                EditorPhysicsLayer::Speedup(layer) => {
                                                    MapTileLayerPhysicsTiles::Speedup(copy_tiles(
                                                        width_old, height_old, width_new,
                                                        height_new, &layer.layer.tiles,
                                                    ))
                                                }
                                                EditorPhysicsLayer::Switch(layer) => {
                                                    MapTileLayerPhysicsTiles::Switch(copy_tiles(
                                                        width_old, height_old, width_new,
                                                        height_new, &layer.layer.base.tiles,
                                                    ))
                                                }
                                                EditorPhysicsLayer::Tune(layer) => {
                                                    MapTileLayerPhysicsTiles::Tune(copy_tiles(
                                                        width_old, height_old, width_new,
                                                        height_new, &layer.layer.base.tiles,
                                                    ))
                                                }
                                            })
                                            .collect()
                                                } else {
                                                    old_layer_tiles.clone()
                                                }
                                            },
                                            old_layer_tiles,
                                        },
                                    ),
                                    Some("change-physics-group-attr"),
                                );
                            }
                        });
                });
            res
        }
        GroupAttrMode::DesignMulti => todo!(),
        GroupAttrMode::DesignAndPhysicsMulti => todo!(),
        GroupAttrMode::None => {
            // render nothing
            None
        }
    };

    if window_res.is_some() {
        let window_res = window_res.as_ref().unwrap();
        ui_state.add_blur_rect(window_res.response.rect, 0.0);
    }

    *pipe.user_data.pointer_is_used |= if let Some(window_res) = &window_res {
        let intersected = ui.input(|i| {
            if i.pointer.primary_down() {
                Some((
                    !window_res.response.rect.intersects({
                        let min = i.pointer.interact_pos().unwrap_or_default();
                        let max = min;
                        [min, max].into()
                    }),
                    i.pointer.primary_pressed(),
                ))
            } else {
                None
            }
        });
        if intersected.is_some_and(|(outside, clicked)| outside && clicked) {
            map.unselect_all(true, true);
        }
        intersected.is_some_and(|(outside, _)| !outside)
    } else {
        false
    };
}
