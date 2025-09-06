use crate::actions::actions::{
    ActAddGroup, ActAddPhysicsTileLayer, ActAddQuadLayer, ActAddRemGroup,
    ActAddRemPhysicsTileLayer, ActAddRemQuadLayer, ActAddRemSoundLayer, ActAddRemTileLayer,
    ActAddSoundLayer, ActAddTileLayer, EditorAction,
};
use crate::client::EditorClient;
use crate::map::{EditorLayer, EditorLayerUnionRef, EditorMap, EditorPhysicsLayer};
use crate::ui::user_data::UserDataWithTab;
use crate::utils::ui_pos_to_world_pos;
use crate::{
    map::{
        EditorCommonLayerOrGroupAttrInterface, EditorDesignLayerInterface, EditorGroup,
        EditorMapInterface, EditorMapSetGroup, EditorMapSetLayer, EditorResources,
    },
    ui::utils::{group_name, layer_name, layer_name_phy},
};

use egui::{Button, Color32, Layout, collapsing_header::CollapsingState};
use egui_extras::{Size, StripBuilder};
use map::map::groups::MapGroup;
use map::map::groups::layers::design::{
    MapLayerQuad, MapLayerQuadsAttrs, MapLayerSound, MapLayerSoundAttrs, MapLayerTile,
};
use map::map::groups::layers::physics::{
    MapLayerPhysics, MapLayerTilePhysicsBase, MapLayerTilePhysicsSwitch, MapLayerTilePhysicsTele,
    MapLayerTilePhysicsTune,
};
use map::map::groups::layers::tiles::MapTileLayerAttr;
use map::types::NonZeroU16MinusOne;
use math::math::vector::{ivec2, nffixed, nfvec4, vec2};
use ui_base::types::UiRenderPipe;

fn button_selected_style() -> egui::Stroke {
    egui::Stroke::new(2.0, Color32::LIGHT_GREEN)
}

fn check_layer_clicked_tile(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>) {
    if ui.input(|i| i.modifiers.ctrl && i.pointer.secondary_pressed()) {
        let pointer_pos = ui.input(|i| {
            i.pointer
                .latest_pos()
                .or(i.pointer.hover_pos())
                .or(i.pointer.interact_pos())
                .unwrap_or_default()
        });
        // search for the next layer for whos tile is potentially clicked
        let map = &mut pipe.user_data.editor_tab.map;
        fn group_iter(
            groups: &[EditorGroup],
            is_background: bool,
        ) -> impl Iterator<Item = (EditorLayerUnionRef<'_>, EditorMapSetLayer)> {
            groups
                .iter()
                .enumerate()
                .flat_map(move |(group_index, group)| {
                    group
                        .layers
                        .iter()
                        .enumerate()
                        .map(move |(layer_index, layer)| {
                            (
                                EditorLayerUnionRef::Design {
                                    layer,
                                    group,
                                    group_index,
                                    layer_index,
                                    is_background,
                                },
                                if is_background {
                                    EditorMapSetLayer::Background {
                                        group: group_index,
                                        layer: layer_index,
                                    }
                                } else {
                                    EditorMapSetLayer::Foreground {
                                        group: group_index,
                                        layer: layer_index,
                                    }
                                },
                            )
                        })
                })
        }
        fn phy_group_iter(
            map: &EditorMap,
        ) -> impl Iterator<Item = (EditorLayerUnionRef<'_>, EditorMapSetLayer)> {
            map.groups
                .physics
                .layers
                .iter()
                .enumerate()
                .map(move |(layer_index, layer)| {
                    (
                        EditorLayerUnionRef::Physics {
                            layer_index,
                            layer,
                            group_attr: &map.groups.physics.attr,
                        },
                        EditorMapSetLayer::Physics { layer: layer_index },
                    )
                })
        }
        let bg1 = group_iter(&map.groups.background, true);
        let fg1 = group_iter(&map.groups.foreground, false);
        let bg2 = group_iter(&map.groups.background, true);
        let fg2 = group_iter(&map.groups.foreground, false);
        let phy1 = phy_group_iter(map);
        let phy2 = phy_group_iter(map);

        let layers: Vec<_> = bg1
            .chain(phy1)
            .chain(fg1)
            .chain(bg2.chain(phy2).chain(fg2))
            .collect();

        let mut first_found = None;
        let mut first_after_active_found = None;
        let mut active_found = false;
        for (layer, set_layer) in layers.iter() {
            let (offset, parallax) = layer.get_offset_and_parallax();
            let pos = ui_pos_to_world_pos(
                pipe.user_data.canvas_handle,
                &ui.ctx().screen_rect(),
                map.groups.user.zoom,
                vec2::new(pointer_pos.x, pointer_pos.y),
                map.groups.user.pos.x,
                map.groups.user.pos.y,
                offset.x,
                offset.y,
                parallax.x,
                parallax.y,
                map.groups.user.parallax_aware_zoom,
            );

            if layer.is_tile_layer() {
                let (w, h) = match layer {
                    EditorLayerUnionRef::Physics { group_attr, .. } => {
                        (group_attr.width, group_attr.height)
                    }
                    EditorLayerUnionRef::Design {
                        layer: EditorLayer::Tile(layer),
                        ..
                    } => (layer.layer.attr.width, layer.layer.attr.height),
                    _ => panic!("not a tile layer, code bug."),
                };

                let posi = ivec2::new(pos.x.floor() as i32, pos.y.floor() as i32);
                if posi.x >= 0 && posi.y >= 0 && posi.x < w.get() as i32 && posi.y < h.get() as i32
                {
                    let tile_index = posi.y as usize * w.get() as usize + posi.x as usize;
                    let is_non_air_tile = match layer {
                        EditorLayerUnionRef::Physics { layer, .. } => match layer {
                            EditorPhysicsLayer::Arbitrary(_) => false,
                            EditorPhysicsLayer::Game(layer) | EditorPhysicsLayer::Front(layer) => {
                                layer.layer.tiles[tile_index].index != 0
                            }
                            EditorPhysicsLayer::Tele(layer) => {
                                layer.layer.base.tiles[tile_index].base.index != 0
                            }
                            EditorPhysicsLayer::Speedup(layer) => {
                                layer.layer.tiles[tile_index].base.index != 0
                            }
                            EditorPhysicsLayer::Switch(layer) => {
                                layer.layer.base.tiles[tile_index].base.index != 0
                            }
                            EditorPhysicsLayer::Tune(layer) => {
                                layer.layer.base.tiles[tile_index].base.index != 0
                            }
                        },
                        EditorLayerUnionRef::Design {
                            layer: EditorLayer::Tile(layer),
                            ..
                        } => layer.layer.tiles[tile_index].index != 0,
                        _ => panic!("not a tile layer, code bug."),
                    };

                    if is_non_air_tile {
                        if first_found.is_none() {
                            first_found = Some(set_layer);
                        }
                        if active_found && first_after_active_found.is_none() {
                            first_after_active_found = Some(set_layer);
                        }
                    }
                }
            }

            if layer.is_active() {
                active_found = true;
            }
        }

        if let Some(set_layer) = first_after_active_found.or(first_found) {
            map.set_active_layer(*set_layer);
        }
    }
}

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>) {
    let tab = &mut *pipe.user_data.editor_tab;
    let map = &mut tab.map;

    let mut activated_layer = None;
    let mut selected_layers = Vec::new();
    let mut selected_groups = Vec::new();
    let group_ui = |id: &str,
                    ui: &mut egui::Ui,
                    resources: &EditorResources,
                    groups: &mut Vec<EditorGroup>,
                    is_background: bool,
                    client: &mut EditorClient| {
        let mut activated_layer = None;
        let mut selected_layers = Vec::new();
        let mut selected_groups = Vec::new();
        for (g, group) in groups.iter_mut().enumerate() {
            CollapsingState::load_with_default_open(ui.ctx(), format!("{id}-{g}").into(), true)
                .show_header(ui, |ui| {
                    ui.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
                        let hidden = group.editor_attr_mut().hidden;
                        let hide_btn = Button::new(if hidden { "\u{f070}" } else { "\u{f06e}" })
                            .selected(group.editor_attr().hidden)
                            .fill(ui.style().visuals.window_fill);
                        if ui.add(hide_btn).clicked() {
                            group.editor_attr_mut().hidden = !hidden;
                        }
                        ui.vertical_centered_justified(|ui| {
                            let btn = Button::new(group_name(group, g)).frame(false);
                            if ui.add(btn).secondary_clicked() {
                                selected_groups.push(g);
                            }
                        })
                    })
                })
                .body(|ui| {
                    for (l, layer) in group.layers.iter_mut().enumerate() {
                        let (icon, layer_btn) = {
                            let (icon, name) = layer_name(ui, resources, layer, l);

                            let mut btn = egui::Button::new(name);
                            if layer.editor_attr().active {
                                btn = btn.selected(true);
                            }
                            if layer.is_selected() {
                                btn = btn.stroke(button_selected_style());
                            }
                            (icon, btn)
                        };

                        ui.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
                            let hidden = layer.editor_attr_mut().hidden;
                            let hide_btn =
                                Button::new(if hidden { "\u{f070}" } else { "\u{f06e}" })
                                    .selected(layer.editor_attr().hidden);
                            if ui.add(hide_btn).clicked() {
                                layer.editor_attr_mut().hidden = !hidden;
                            }

                            ui.vertical_centered_justified(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(icon);

                                    ui.with_layout(
                                        Layout::left_to_right(egui::Align::Center)
                                            .with_main_justify(true),
                                        |ui| {
                                            let btn = ui.add(layer_btn);

                                            if btn.clicked() {
                                                activated_layer = Some((g, l));
                                            }
                                            if btn.secondary_clicked() {
                                                selected_layers.push((g, l));
                                            }
                                        },
                                    );
                                });
                            });
                        });
                    }

                    ui.menu_button("\u{f0fe} Add design layer", |ui| {
                        if ui.button("Tile").clicked() {
                            let add_layer = MapLayerTile {
                                attr: MapTileLayerAttr {
                                    width: NonZeroU16MinusOne::new(50).unwrap(),
                                    height: NonZeroU16MinusOne::new(50).unwrap(),
                                    color: nfvec4::new(
                                        nffixed::const_from_int(1),
                                        nffixed::const_from_int(1),
                                        nffixed::const_from_int(1),
                                        nffixed::const_from_int(1),
                                    ),
                                    high_detail: false,
                                    color_anim: None,
                                    color_anim_offset: time::Duration::ZERO,
                                    image_array: None,
                                },
                                tiles: vec![Default::default(); 50 * 50],
                                name: "".into(),
                            };
                            client.execute(
                                EditorAction::AddTileLayer(ActAddTileLayer {
                                    base: ActAddRemTileLayer {
                                        is_background,
                                        group_index: g,
                                        index: group.layers.len(),
                                        layer: add_layer,
                                    },
                                }),
                                None,
                            );
                        }
                        if ui.button("Quad").clicked() {
                            let add_layer = MapLayerQuad {
                                attr: MapLayerQuadsAttrs {
                                    image: None,
                                    high_detail: false,
                                },
                                quads: vec![],
                                name: "".into(),
                            };
                            client.execute(
                                EditorAction::AddQuadLayer(ActAddQuadLayer {
                                    base: ActAddRemQuadLayer {
                                        is_background,
                                        group_index: g,
                                        index: group.layers.len(),
                                        layer: add_layer,
                                    },
                                }),
                                None,
                            );
                        }
                        if ui.button("Sound").clicked() {
                            let add_layer = MapLayerSound {
                                attr: MapLayerSoundAttrs {
                                    sound: None,
                                    high_detail: false,
                                },
                                sounds: vec![],
                                name: "".into(),
                            };
                            client.execute(
                                EditorAction::AddSoundLayer(ActAddSoundLayer {
                                    base: ActAddRemSoundLayer {
                                        is_background,
                                        group_index: g,
                                        index: group.layers.len(),
                                        layer: add_layer,
                                    },
                                }),
                                None,
                            );
                        }
                    });
                });
        }
        (activated_layer, selected_layers, selected_groups)
    };

    let scroll_color = Color32::from_rgba_unmultiplied(0, 0, 0, 50);
    let height = ui.available_height() - (ui.style().spacing.item_spacing.y * 3.0); // spacing between the elements
    let calc_paint_rect = |ui: &egui::Ui| -> egui::Rect {
        let available_rect = ui.available_rect_before_wrap();

        egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(available_rect.width(), available_rect.height()),
        )
    };

    let height_physics = (height * 0.3).min(230.0);
    let height_design = (height - height_physics) / 2.0;

    egui_extras::StripBuilder::new(ui)
        .size(Size::exact(height_design))
        .size(Size::exact(height_physics))
        .size(Size::exact(height_design))
        .vertical(|mut strip| {
            // background
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                ui.add(egui::Separator::default().spacing(15.0));
                ui.vertical_centered(|ui| {
                    ui.heading("Background");
                });
                ui.add(egui::Separator::default().spacing(15.0));

                ui.vertical_centered_justified(|ui| {
                    ui.painter()
                        .rect_filled(calc_paint_rect(ui), 0.0, scroll_color);
                    StripBuilder::new(ui)
                        .size(Size::remainder())
                        .size(Size::exact(20.0))
                        .vertical(|mut strip| {
                            strip.cell(|ui| {
                                ui.style_mut().wrap_mode = None;
                                egui::ScrollArea::vertical()
                                    .id_salt("scroll-bg".to_string())
                                    .show(ui, |ui| {
                                        let groups_res = group_ui(
                                            "bg-groups",
                                            ui,
                                            &map.resources,
                                            &mut map.groups.background,
                                            true,
                                            &mut tab.client,
                                        );
                                        if let (Some((g, l)), _, _) = groups_res {
                                            activated_layer = Some(EditorMapSetLayer::Background {
                                                group: g,
                                                layer: l,
                                            });
                                        }
                                        selected_layers.extend(&mut groups_res.1.into_iter().map(
                                            |(g, l)| EditorMapSetLayer::Background {
                                                group: g,
                                                layer: l,
                                            },
                                        ));
                                        selected_groups.extend(
                                            &mut groups_res.2.into_iter().map(|g| {
                                                EditorMapSetGroup::Background { group: g }
                                            }),
                                        );
                                    });
                            });
                            strip.cell(|ui| {
                                ui.style_mut().wrap_mode = None;
                                if ui.button("\u{f0fe} Add design group").clicked() {
                                    tab.client.execute(
                                        EditorAction::AddGroup(ActAddGroup {
                                            base: ActAddRemGroup {
                                                is_background: true,
                                                index: map.groups.background.len(),
                                                group: MapGroup {
                                                    attr: Default::default(),
                                                    layers: Default::default(),
                                                    name: "".into(),
                                                },
                                            },
                                        }),
                                        None,
                                    );
                                }
                            });
                        });
                });
            });

            // physics
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                ui.add(egui::Separator::default().spacing(15.0));
                ui.vertical_centered(|ui| {
                    ui.heading("Physics");
                });
                ui.add(egui::Separator::default().spacing(15.0));

                ui.vertical_centered_justified(|ui| {
                    ui.painter()
                        .rect_filled(calc_paint_rect(ui), 0.0, scroll_color);
                    StripBuilder::new(ui)
                        .size(Size::remainder())
                        .size(Size::exact(25.0))
                        .vertical(|mut strip| {
                            strip.cell(|ui| {
                                ui.style_mut().wrap_mode = None;
                                egui::ScrollArea::vertical()
                                    .id_salt("scroll-phy".to_string())
                                    .show(ui, |ui| {
                                        let group = &mut map.groups.physics;
                                        CollapsingState::load_with_default_open(
                                            ui.ctx(),
                                            "physics-group".into(),
                                            true,
                                        )
                                        .show_header(ui, |ui| {
                                            ui.with_layout(
                                                Layout::right_to_left(egui::Align::Min),
                                                |ui| {
                                                    let hidden = group.editor_attr_mut().hidden;
                                                    let hide_btn = Button::new(if hidden {
                                                        "\u{f070}"
                                                    } else {
                                                        "\u{f06e}"
                                                    })
                                                    .selected(group.editor_attr().hidden)
                                                    .fill(ui.style().visuals.window_fill);
                                                    if ui.add(hide_btn).clicked() {
                                                        group.editor_attr_mut().hidden = !hidden;
                                                    }
                                                    ui.vertical_centered_justified(|ui| {
                                                        let btn =
                                                            Button::new("Physics").frame(false);
                                                        if ui.add(btn).secondary_clicked() {
                                                            selected_groups
                                                                .push(EditorMapSetGroup::Physics);
                                                        }
                                                    })
                                                },
                                            )
                                        })
                                        .body(|ui| {
                                            for (l, layer) in
                                                map.groups.physics.layers.iter_mut().enumerate()
                                            {
                                                let layer_btn = {
                                                    let mut btn =
                                                        egui::Button::new(layer_name_phy(layer, l));
                                                    if layer.editor_attr().active {
                                                        btn = btn.selected(true);
                                                    }
                                                    if layer.user().selected.is_some() {
                                                        btn = btn.stroke(button_selected_style());
                                                    }
                                                    btn
                                                };

                                                ui.with_layout(
                                                    Layout::right_to_left(egui::Align::Min),
                                                    |ui| {
                                                        let hidden = layer.editor_attr_mut().hidden;
                                                        let hide_btn = Button::new(if hidden {
                                                            "\u{f070}"
                                                        } else {
                                                            "\u{f06e}"
                                                        })
                                                        .selected(layer.editor_attr().hidden);
                                                        if ui.add(hide_btn).clicked() {
                                                            layer.editor_attr_mut().hidden =
                                                                !hidden;
                                                        }

                                                        ui.vertical_centered_justified(|ui| {
                                                            let btn = ui.add(layer_btn);
                                                            if btn.clicked() {
                                                                activated_layer = Some(
                                                                    EditorMapSetLayer::Physics {
                                                                        layer: l,
                                                                    },
                                                                );
                                                            }
                                                            if btn.secondary_clicked() {
                                                                selected_layers.push(
                                                                    EditorMapSetLayer::Physics {
                                                                        layer: l,
                                                                    },
                                                                );
                                                            }
                                                        });
                                                    },
                                                );
                                            }
                                        });
                                    });
                            });

                            strip.cell(|ui| {
                                ui.style_mut().wrap_mode = None;
                                #[derive(Debug, Default)]
                                struct FoundPhyLayers {
                                    front: bool,
                                    tele: bool,
                                    speedup: bool,
                                    switch: bool,
                                    tune: bool,
                                }
                                let mut phy_layers = FoundPhyLayers::default();

                                let physics = &map.groups.physics;
                                physics.layers.iter().for_each(|layer| {
                                    match layer {
                                        EditorPhysicsLayer::Arbitrary(_)
                                        | EditorPhysicsLayer::Game(_) => {
                                            // ignore
                                        }
                                        EditorPhysicsLayer::Front(_) => phy_layers.front = true,
                                        EditorPhysicsLayer::Tele(_) => phy_layers.tele = true,
                                        EditorPhysicsLayer::Speedup(_) => phy_layers.speedup = true,
                                        EditorPhysicsLayer::Switch(_) => phy_layers.switch = true,
                                        EditorPhysicsLayer::Tune(_) => phy_layers.tune = true,
                                    }
                                });

                                if !phy_layers.front
                                    || !phy_layers.tele
                                    || !phy_layers.speedup
                                    || !phy_layers.switch
                                    || !phy_layers.tune
                                {
                                    ui.menu_button("\u{f0fe} Add physics layer", |ui| {
                                        let mut add_layer = None;
                                        if !phy_layers.front && ui.button("Front").clicked() {
                                            add_layer = Some(MapLayerPhysics::Front(
                                                MapLayerTilePhysicsBase {
                                                    tiles: vec![
                                                        Default::default();
                                                        physics.attr.width.get() as usize
                                                            * physics.attr.height.get()
                                                                as usize
                                                    ],
                                                },
                                            ));
                                        }
                                        if !phy_layers.tele && ui.button("Tele").clicked() {
                                            add_layer = Some(MapLayerPhysics::Tele(
                                                MapLayerTilePhysicsTele {
                                                    base: MapLayerTilePhysicsBase {
                                                        tiles: vec![
                                                            Default::default();
                                                            physics.attr.width.get()
                                                                as usize
                                                                * physics.attr.height.get()
                                                                    as usize
                                                        ],
                                                    },
                                                    tele_names: Default::default(),
                                                },
                                            ));
                                        }
                                        if !phy_layers.switch && ui.button("Switch").clicked() {
                                            add_layer = Some(MapLayerPhysics::Switch(
                                                MapLayerTilePhysicsSwitch {
                                                    base: MapLayerTilePhysicsBase {
                                                        tiles: vec![
                                                            Default::default();
                                                            physics.attr.width.get()
                                                                as usize
                                                                * physics.attr.height.get()
                                                                    as usize
                                                        ],
                                                    },
                                                    switch_names: Default::default(),
                                                },
                                            ));
                                        }
                                        if !phy_layers.speedup && ui.button("Speedup").clicked() {
                                            add_layer = Some(MapLayerPhysics::Speedup(
                                                MapLayerTilePhysicsBase {
                                                    tiles: vec![
                                                        Default::default();
                                                        physics.attr.width.get() as usize
                                                            * physics.attr.height.get()
                                                                as usize
                                                    ],
                                                },
                                            ));
                                        }
                                        if !phy_layers.tune && ui.button("Tune").clicked() {
                                            add_layer = Some(MapLayerPhysics::Tune(
                                                MapLayerTilePhysicsTune {
                                                    base: MapLayerTilePhysicsBase {
                                                        tiles: vec![
                                                            Default::default();
                                                            physics.attr.width.get()
                                                                as usize
                                                                * physics.attr.height.get()
                                                                    as usize
                                                        ],
                                                    },
                                                    tune_zones: Default::default(),
                                                },
                                            ));
                                        }

                                        if let Some(add_layer) = add_layer {
                                            tab.client.execute(
                                                EditorAction::AddPhysicsTileLayer(
                                                    ActAddPhysicsTileLayer {
                                                        base: ActAddRemPhysicsTileLayer {
                                                            index: physics.layers.len(),
                                                            layer: add_layer,
                                                        },
                                                    },
                                                ),
                                                None,
                                            );
                                        }
                                    });
                                }
                            });
                        });
                });
            });

            // foreground
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                ui.add(egui::Separator::default().spacing(15.0));
                ui.vertical_centered(|ui| {
                    ui.heading("Foreground");
                });
                ui.add(egui::Separator::default().spacing(15.0));

                ui.vertical_centered_justified(|ui| {
                    ui.painter()
                        .rect_filled(calc_paint_rect(ui), 0.0, scroll_color);
                    StripBuilder::new(ui)
                        .size(Size::remainder())
                        .size(Size::exact(20.0))
                        .vertical(|mut strip| {
                            strip.cell(|ui| {
                                ui.style_mut().wrap_mode = None;
                                egui::ScrollArea::vertical()
                                    .id_salt("scroll-fg".to_string())
                                    .show(ui, |ui| {
                                        let groups_res = group_ui(
                                            "fg-groups",
                                            ui,
                                            &map.resources,
                                            &mut map.groups.foreground,
                                            false,
                                            &mut tab.client,
                                        );
                                        if let (Some((g, l)), _, _) = groups_res {
                                            activated_layer = Some(EditorMapSetLayer::Foreground {
                                                group: g,
                                                layer: l,
                                            });
                                        }
                                        selected_layers.extend(&mut groups_res.1.into_iter().map(
                                            |(g, l)| EditorMapSetLayer::Foreground {
                                                group: g,
                                                layer: l,
                                            },
                                        ));
                                        selected_groups.extend(
                                            &mut groups_res.2.into_iter().map(|g| {
                                                EditorMapSetGroup::Foreground { group: g }
                                            }),
                                        );

                                        if let Some(activated_layer) = activated_layer {
                                            map.set_active_layer(activated_layer);
                                        }
                                        for selected_layer in selected_layers {
                                            map.toggle_selected_layer(selected_layer, false);
                                        }
                                        for selected_group in selected_groups {
                                            map.toggle_selected_group(selected_group, false);
                                        }
                                    });
                            });
                            strip.cell(|ui| {
                                ui.style_mut().wrap_mode = None;
                                if ui.button("\u{f0fe} Add design group").clicked() {
                                    tab.client.execute(
                                        EditorAction::AddGroup(ActAddGroup {
                                            base: ActAddRemGroup {
                                                is_background: false,
                                                index: map.groups.foreground.len(),
                                                group: MapGroup {
                                                    attr: Default::default(),
                                                    layers: Default::default(),
                                                    name: "".into(),
                                                },
                                            },
                                        }),
                                        None,
                                    );
                                }
                            });
                        });
                });
            });
        });

    check_layer_clicked_tile(ui, pipe);
}
