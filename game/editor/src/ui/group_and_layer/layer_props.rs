use std::ops::RangeInclusive;

use base::hash::fmt_hash;
use egui::{Button, Checkbox, Color32, ComboBox, DragValue, InnerResponse};
use map::types::NonZeroU16MinusOne;
use math::math::vector::{nffixed, nfvec4};
use rand::RngCore;
use time::Duration;
use ui_base::{
    types::{UiRenderPipe, UiState},
    utils::toggle_ui,
};

use crate::{
    actions::actions::{
        ActAddRemPhysicsTileLayer, ActAddRemQuadLayer, ActAddRemSoundLayer, ActAddRemTileLayer,
        ActChangeDesignLayerName, ActChangeQuadLayerAttr, ActChangeSoundLayerAttr,
        ActChangeTileLayerDesignAttr, ActMoveLayer, ActRemPhysicsTileLayer, ActRemQuadLayer,
        ActRemSoundLayer, ActRemTileLayer, EditorAction,
    },
    event::EditorEventAutoMap,
    explain::TEXT_LAYER_PROPS_COLOR,
    hotkeys::{EditorHotkeyEvent, EditorHotkeyEventMap},
    map::{
        EditorDesignLayerInterface, EditorGroups, EditorLayer, EditorLayerUnionRef, EditorMap,
        EditorMapInterface, EditorPhysicsLayer, ResourceSelection,
    },
    ui::{
        group_and_layer::{
            resource_selector::ResourceSelectionMode,
            shared::{animations_panel_open_warning, copy_tiles},
        },
        user_data::UserDataWithTab,
    },
};

#[derive(Debug)]
enum MoveLayer {
    IsBackground(bool),
    Group(usize),
    Layer(usize),
}

fn render_layer_move(
    ui: &mut egui::Ui,
    is_background: bool,
    g: usize,
    l: usize,
    can_bg: bool,
    g_range: RangeInclusive<usize>,
    l_range: RangeInclusive<usize>,
) -> Option<MoveLayer> {
    let mut move_layer = None;

    let mut new_is_background = is_background;
    ui.label("In background");
    if ui
        .add_enabled(can_bg, Checkbox::new(&mut new_is_background, ""))
        .changed()
    {
        move_layer = Some(MoveLayer::IsBackground(new_is_background));
    }
    ui.end_row();

    ui.label("Group");
    let mut new_group = g;
    if ui
        .add_enabled(
            *g_range.start() != *g_range.end(),
            DragValue::new(&mut new_group)
                .update_while_editing(false)
                .range(g_range),
        )
        .changed()
    {
        move_layer = Some(MoveLayer::Group(new_group));
    }
    ui.end_row();

    ui.label("Layer");
    let mut new_layer = l;
    if ui
        .add_enabled(
            *l_range.start() != *l_range.end(),
            DragValue::new(&mut new_layer)
                .update_while_editing(false)
                .range(l_range),
        )
        .changed()
    {
        move_layer = Some(MoveLayer::Layer(new_layer));
    }
    ui.end_row();

    move_layer
}

fn layer_move_to_act(
    mv: MoveLayer,
    is_background: bool,
    g: usize,
    l: usize,
    map: &EditorMap,
) -> Option<ActMoveLayer> {
    match mv {
        MoveLayer::IsBackground(new_is_background) => {
            if new_is_background == is_background {
                return None;
            }
            let groups = if new_is_background {
                &map.groups.background
            } else {
                &map.groups.foreground
            };
            if let Some((new_group, group)) = groups.iter().enumerate().next_back() {
                Some(ActMoveLayer {
                    old_is_background: is_background,
                    old_group: g,
                    old_layer: l,
                    new_is_background,
                    new_group,
                    new_layer: group.layers.len(),
                })
            } else {
                None
            }
        }
        MoveLayer::Group(new_group) => {
            if new_group == g {
                return None;
            }
            let groups = if is_background {
                &map.groups.background
            } else {
                &map.groups.foreground
            };
            groups.get(new_group).map(|group| ActMoveLayer {
                old_is_background: is_background,
                old_group: g,
                old_layer: l,
                new_is_background: is_background,
                new_group,
                new_layer: group.layers.len(),
            })
        }
        MoveLayer::Layer(new_layer) => {
            if new_layer == l {
                return None;
            }
            let groups = if is_background {
                &map.groups.background
            } else {
                &map.groups.foreground
            };
            if let Some(group) = groups.get(g) {
                if new_layer < l || new_layer < group.layers.len() {
                    Some(ActMoveLayer {
                        old_is_background: is_background,
                        old_group: g,
                        old_layer: l,
                        new_is_background: is_background,
                        new_group: g,
                        new_layer,
                    })
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    #[derive(Debug, PartialEq, Eq)]
    enum LayerAttrMode {
        DesignTile,
        DesignQuad,
        DesignSound,
        /// only tile layers selected
        DesignTileMulti,
        /// only quad layers selected
        DesignQuadMulti,
        /// only sound layers selected
        DesignSoundMulti,
        /// all design layers mixed, only `high detail` is shared across all
        DesignMulti,
        /// empty attr
        Physics,
        /// mixing physics & design always leads to empty attr intersection
        PhysicsDesignMulti,
        None,
    }

    // check which layers are `selected`
    let tab = &mut *pipe.user_data.editor_tab;
    let map = &mut tab.map;
    let animations_panel_open =
        map.user.ui_values.animations_panel_open && !map.user.options.no_animations_with_properties;
    let bg_selection = map
        .groups
        .background
        .iter()
        .flat_map(|bg| bg.layers.iter().filter(|layer| layer.is_selected()));
    let fg_selection = map
        .groups
        .foreground
        .iter()
        .flat_map(|fg| fg.layers.iter().filter(|layer| layer.is_selected()));
    let phy_selection = map
        .groups
        .physics
        .layers
        .iter()
        .filter(|layer| layer.user().selected.is_some());

    let bg_selected = bg_selection.clone().count();
    let phy_selected = phy_selection.clone().count();
    let fg_selected = fg_selection.clone().count();

    let mut attr_mode = LayerAttrMode::None;
    if bg_selected > 0 {
        let tile_count = bg_selection
            .clone()
            .filter(|layer| matches!(layer, EditorLayer::Tile(_)))
            .count();
        let quad_count = bg_selection
            .clone()
            .filter(|layer| matches!(layer, EditorLayer::Quad(_)))
            .count();
        let sound_count = bg_selection
            .clone()
            .filter(|layer| matches!(layer, EditorLayer::Sound(_)))
            .count();
        if tile_count > 0 {
            attr_mode = if tile_count == 1 {
                LayerAttrMode::DesignTile
            } else {
                LayerAttrMode::DesignTileMulti
            };
        }
        if quad_count > 0 {
            if attr_mode == LayerAttrMode::None {
                attr_mode = if quad_count == 1 {
                    LayerAttrMode::DesignQuad
                } else {
                    LayerAttrMode::DesignQuadMulti
                };
            } else {
                attr_mode = LayerAttrMode::DesignMulti;
            }
        }
        if sound_count > 0 {
            if attr_mode == LayerAttrMode::None {
                attr_mode = if sound_count == 1 {
                    LayerAttrMode::DesignSound
                } else {
                    LayerAttrMode::DesignSoundMulti
                };
            } else {
                attr_mode = LayerAttrMode::DesignMulti;
            }
        }
    }
    if phy_selected > 0 {
        if attr_mode == LayerAttrMode::None {
            // ignore multi here, bcs phy attr are always empty
            attr_mode = LayerAttrMode::Physics;
        } else {
            attr_mode = LayerAttrMode::PhysicsDesignMulti;
        }
    }
    if fg_selected > 0 {
        let tile_count = fg_selection
            .clone()
            .filter(|layer| matches!(layer, EditorLayer::Tile(_)))
            .count();
        let quad_count = fg_selection
            .clone()
            .filter(|layer| matches!(layer, EditorLayer::Quad(_)))
            .count();
        let sound_count = fg_selection
            .clone()
            .filter(|layer| matches!(layer, EditorLayer::Sound(_)))
            .count();
        if tile_count > 0 {
            if attr_mode == LayerAttrMode::None {
                attr_mode = if tile_count == 1 {
                    LayerAttrMode::DesignTile
                } else {
                    LayerAttrMode::DesignTileMulti
                };
            } else if let LayerAttrMode::Physics | LayerAttrMode::PhysicsDesignMulti = attr_mode {
                attr_mode = LayerAttrMode::PhysicsDesignMulti;
            } else {
                attr_mode = LayerAttrMode::DesignMulti;
            }
        }
        if quad_count > 0 {
            if attr_mode == LayerAttrMode::None {
                attr_mode = if quad_count == 1 {
                    LayerAttrMode::DesignQuad
                } else {
                    LayerAttrMode::DesignQuadMulti
                };
            } else if let LayerAttrMode::Physics | LayerAttrMode::PhysicsDesignMulti = attr_mode {
                attr_mode = LayerAttrMode::PhysicsDesignMulti;
            } else {
                attr_mode = LayerAttrMode::DesignMulti;
            }
        }
        if sound_count > 0 {
            if attr_mode == LayerAttrMode::None {
                attr_mode = if sound_count == 1 {
                    LayerAttrMode::DesignSound
                } else {
                    LayerAttrMode::DesignSoundMulti
                };
            } else if let LayerAttrMode::Physics | LayerAttrMode::PhysicsDesignMulti = attr_mode {
                attr_mode = LayerAttrMode::PhysicsDesignMulti;
            } else {
                attr_mode = LayerAttrMode::DesignMulti;
            }
        }
    }

    let mut bg_selection = map
        .groups
        .background
        .iter()
        .enumerate()
        .flat_map(|(g, bg)| {
            bg.layers
                .iter()
                .enumerate()
                .filter(|(_, layer)| layer.is_selected())
                .map(move |(l, _)| (true, g, l))
        });
    let mut fg_selection = map
        .groups
        .foreground
        .iter()
        .enumerate()
        .flat_map(|(g, fg)| {
            fg.layers
                .iter()
                .enumerate()
                .filter(|(_, layer)| layer.is_selected())
                .map(move |(l, _)| (false, g, l))
        });
    let mut phy_selection = map
        .groups
        .physics
        .layers
        .iter_mut()
        .enumerate()
        .filter(|(_, layer)| layer.user().selected.is_some());

    fn move_limits(
        groups: &EditorGroups,
        is_background: bool,
        g: usize,
    ) -> (bool, RangeInclusive<usize>, RangeInclusive<usize>) {
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
            {
                let groups = if is_background {
                    &groups.background
                } else {
                    &groups.foreground
                };
                0..=groups
                    .get(g)
                    .map(|g| g.layers.len().saturating_sub(1))
                    .unwrap_or_default()
            },
        )
    }

    fn layer_mut(
        groups: &mut EditorGroups,
        is_background: bool,
        g: usize,
        l: usize,
    ) -> &mut EditorLayer {
        let groups = if is_background {
            &mut groups.background
        } else {
            &mut groups.foreground
        };

        groups
            .get_mut(g)
            .expect("group index out of bounds, logic error.")
            .layers
            .get_mut(l)
            .expect("layer index out of bounds, logic error.")
    }

    let mut resource_selector_was_outside = true;
    let window_res = match attr_mode {
        LayerAttrMode::DesignTile => {
            let (is_background, g, l) = bg_selection
                .next()
                .unwrap_or_else(|| fg_selection.next().unwrap());
            let (bg_move_limit, g_limit, l_limit) = move_limits(&map.groups, is_background, g);
            let EditorLayer::Tile(layer) = layer_mut(&mut map.groups, is_background, g, l) else {
                panic!("not a tile layer, bug in above calculations")
            };
            let layer_editor = layer.user.selected.as_mut().unwrap();
            let layer_attr_cmp = layer_editor.attr;
            let layer_name_cmp = layer_editor.name.clone();

            let window = egui::Window::new("Design Tile Layer Attributes")
                .resizable(false)
                .collapsible(false);

            let image_array = layer.layer.attr.image_array;
            let resource_name = if let Some(image_array) =
                image_array.and_then(|image_array| map.resources.image_arrays.get(image_array))
            {
                let res = format!(
                    "{}_{}",
                    image_array.def.name.as_str(),
                    fmt_hash(&image_array.def.meta.blake3_hash)
                );
                pipe.user_data.auto_mapper.try_load(
                    &res,
                    image_array.def.name.as_str(),
                    &image_array.def.meta.blake3_hash,
                    &image_array.user.file,
                );
                Some(res)
            } else {
                None
            };

            let mut delete_layer = false;
            let mut auto_mapper = None;
            let mut auto_mapper_live = None;
            let mut move_layer = None;

            let res = window.show(ui.ctx(), |ui| {
                egui::Grid::new("design group attr grid")
                    .num_columns(2)
                    .spacing([20.0, 4.0])
                    .show(ui, |ui| {
                        let attr = &mut layer_editor.attr;
                        // detail
                        ui.label("High detail");
                        toggle_ui(ui, &mut attr.high_detail);
                        ui.end_row();
                        // w
                        ui.label("Width");
                        let mut w = attr.width.get();
                        ui.add(
                            egui::DragValue::new(&mut w)
                                .update_while_editing(false)
                                .range(1..=u16::MAX - 1),
                        );
                        attr.width = NonZeroU16MinusOne::new(w).unwrap();
                        ui.end_row();
                        // h
                        ui.label("Height");
                        let mut h = attr.height.get();
                        ui.add(
                            egui::DragValue::new(&mut h)
                                .update_while_editing(false)
                                .range(1..=u16::MAX - 1),
                        );
                        attr.height = NonZeroU16MinusOne::new(h).unwrap();
                        ui.end_row();
                        // image
                        if ui
                            .add(
                                egui::Button::new("Image selection")
                                    .selected(layer_editor.image_2d_array_selection_open.is_some()),
                            )
                            .clicked()
                        {
                            layer_editor.image_2d_array_selection_open = layer_editor
                                .image_2d_array_selection_open
                                .is_none()
                                .then_some(ResourceSelection {
                                    hovered_resource: None,
                                });
                        }
                        ui.end_row();
                        // color
                        ui.label("Color \u{f05a}").on_hover_ui(|ui| {
                            let mut cache = egui_commonmark::CommonMarkCache::default();
                            egui_commonmark::CommonMarkViewer::new().show(
                                ui,
                                &mut cache,
                                TEXT_LAYER_PROPS_COLOR,
                            );
                        });
                        let mut color = [
                            (attr.color.r().to_num::<f32>() * 255.0) as u8,
                            (attr.color.g().to_num::<f32>() * 255.0) as u8,
                            (attr.color.b().to_num::<f32>() * 255.0) as u8,
                            (attr.color.a().to_num::<f32>() * 255.0) as u8,
                        ];
                        ui.color_edit_button_srgba_unmultiplied(&mut color);
                        attr.color = nfvec4::new(
                            nffixed::from_num(color[0] as f32 / 255.0),
                            nffixed::from_num(color[1] as f32 / 255.0),
                            nffixed::from_num(color[2] as f32 / 255.0),
                            nffixed::from_num(color[3] as f32 / 255.0),
                        );
                        ui.end_row();
                        // color anim
                        fn combobox_name(ty: &str, index: usize, name: &str) -> String {
                            name.is_empty()
                                .then_some(format!("{ty} #{}", index))
                                .unwrap_or_else(|| name.to_owned())
                        }
                        ui.label("color anim");
                        egui::ComboBox::new("tile-layer-select-color-anim".to_string(), "")
                            .selected_text(
                                map.animations
                                    .color
                                    .get(attr.color_anim.unwrap_or(usize::MAX))
                                    .map(|anim| {
                                        combobox_name(
                                            "color",
                                            attr.color_anim.unwrap(),
                                            &anim.def.name.clone(),
                                        )
                                    })
                                    .unwrap_or_else(|| "None".to_string()),
                            )
                            .show_ui(ui, |ui| {
                                if ui.button("None").clicked() {
                                    attr.color_anim = None;
                                }
                                for (a, anim) in map.animations.color.iter().enumerate() {
                                    if ui
                                        .button(combobox_name("color", a, &anim.def.name))
                                        .clicked()
                                    {
                                        attr.color_anim = Some(a);
                                    }
                                }
                            });
                        ui.end_row();
                        // color time offset
                        ui.label("color anim time offset");
                        let mut millis = attr.color_anim_offset.whole_milliseconds() as i64;
                        if ui
                            .add(egui::DragValue::new(&mut millis).update_while_editing(false))
                            .changed()
                        {
                            attr.color_anim_offset = Duration::milliseconds(millis);
                        }
                        ui.end_row();
                        // name
                        ui.label("Name");
                        ui.text_edit_singleline(&mut layer_editor.name);
                        ui.end_row();

                        if animations_panel_open {
                            ui.colored_label(
                                Color32::RED,
                                "The animation panel is open,\n\
                                    changing attributes will not apply\n\
                                    them to the quad permanently!",
                            )
                            .on_hover_ui(animations_panel_open_warning);
                        }
                        ui.end_row();
                        // delete
                        if ui.button("Delete layer").clicked() {
                            delete_layer = true;
                        }
                        ui.end_row();

                        // auto mapper
                        if let Some(rule) = resource_name
                            .as_ref()
                            .and_then(|r| pipe.user_data.auto_mapper.resources.get(r))
                        {
                            ui.label("Select auto mapper rule");
                            ComboBox::new("auto-mapper-rule-selector-tile-layer", "")
                                .selected_text(
                                    layer.user.auto_mapper_rule.as_deref().unwrap_or("None"),
                                )
                                .show_ui(ui, |ui| {
                                    for rule in rule.rules.keys() {
                                        if ui.button(rule).clicked() {
                                            layer.user.auto_mapper_rule = Some(rule.clone());
                                        }
                                    }
                                });
                            ui.end_row();
                            ui.label("Auto mapper seed");
                            let mut seed = layer
                                .user
                                .auto_mapper_seed
                                .unwrap_or_else(|| rand::rng().next_u64());
                            ui.add(DragValue::new(&mut seed));
                            layer.user.auto_mapper_seed = Some(seed);
                            ui.end_row();
                            if layer.user.auto_mapper_rule.is_some() {
                                if ui.button("Run once").clicked() {
                                    auto_mapper = Some(seed);
                                }
                                if ui
                                    .add(
                                        Button::new("Live edit")
                                            .selected(layer.user.live_edit.is_some()),
                                    )
                                    .clicked()
                                {
                                    auto_mapper_live =
                                        Some(layer.user.live_edit.is_none().then_some(seed));
                                }
                            }

                            ui.end_row();
                        } else {
                            ui.label("No auto mapper rules found..");
                            ui.end_row();
                        }

                        ui.label("Move layer");
                        ui.end_row();

                        // layer moving
                        move_layer = render_layer_move(
                            ui,
                            is_background,
                            g,
                            l,
                            bg_move_limit,
                            g_limit,
                            l_limit,
                        );
                    })
            });

            if let Some(resource_selection) = &mut layer_editor.image_2d_array_selection_open {
                resource_selection.hovered_resource = None;
                let res = super::resource_selector::render(
                    ui,
                    ui_state,
                    pipe.user_data.pointer_is_used,
                    &map.resources.image_arrays,
                );
                resource_selector_was_outside = res.pointer_was_outside;
                if let Some(resource) = res.mode {
                    match resource {
                        ResourceSelectionMode::Hovered(index) => {
                            resource_selection.hovered_resource = Some(index);
                        }
                        ResourceSelectionMode::Clicked(index) => {
                            layer_editor.attr.image_array = index;
                        }
                    }
                }
                if res.pointer_was_outside {
                    layer_editor.image_2d_array_selection_open = None;
                }
            }

            if layer_editor.attr != layer_attr_cmp && !animations_panel_open {
                tab.client.execute(
                    EditorAction::ChangeTileLayerDesignAttr(ActChangeTileLayerDesignAttr {
                        is_background,
                        group_index: g,
                        layer_index: l,
                        old_attr: layer.layer.attr,
                        new_attr: layer_editor.attr,

                        old_tiles: layer.layer.tiles.clone(),
                        new_tiles: {
                            let width_or_height_change = layer.layer.attr.width
                                != layer_editor.attr.width
                                || layer.layer.attr.height != layer_editor.attr.height;
                            if width_or_height_change {
                                let width_old = layer.layer.attr.width.get() as usize;
                                let height_old = layer.layer.attr.height.get() as usize;
                                let width_new = layer_editor.attr.width.get() as usize;
                                let height_new = layer_editor.attr.height.get() as usize;
                                copy_tiles(
                                    width_old,
                                    height_old,
                                    width_new,
                                    height_new,
                                    &layer.layer.tiles,
                                )
                            } else {
                                layer.layer.tiles.clone()
                            }
                        },
                    }),
                    Some(&format!(
                        "change-design-tile-layer-attr-{is_background}-{g}-{l}"
                    )),
                );
            } else if layer_editor.name != layer_name_cmp {
                tab.client.execute(
                    EditorAction::ChangeDesignLayerName(ActChangeDesignLayerName {
                        is_background,
                        group_index: g,
                        layer_index: l,
                        old_name: layer.layer.name.clone(),
                        new_name: layer_editor.name.clone(),
                    }),
                    Some(&format!(
                        "change-design-tile-layer-name-{is_background}-{g}-{l}"
                    )),
                );
            } else if delete_layer {
                tab.client.execute(
                    EditorAction::RemTileLayer(ActRemTileLayer {
                        base: ActAddRemTileLayer {
                            is_background,
                            group_index: g,
                            index: l,
                            layer: layer.clone().into(),
                        },
                    }),
                    None,
                );
            } else if let Some(seed) = auto_mapper {
                let rule = layer.user.auto_mapper_rule.clone();
                if let Some((resource, rule_name, rule)) = resource_name
                    .as_ref()
                    .and_then(|r| {
                        pipe.user_data
                            .auto_mapper
                            .resources
                            .get_mut(r)
                            .map(|rule| (r, rule))
                    })
                    .and_then(|(res, rules)| {
                        rule.and_then(|r| rules.rules.get_mut(&r).map(|rule| (res, r, rule)))
                    })
                {
                    tab.client.auto_map(EditorEventAutoMap {
                        is_background,
                        group_index: g,
                        layer_index: l,
                        resource_and_hash: resource.to_string(),
                        name: rule_name,
                        hash: rule.hash(),
                        seed,
                    });
                }
            } else if let Some(seed) = auto_mapper_live {
                let rule = layer.user.auto_mapper_rule.clone();
                if let Some((resource, rule_name, rule)) = resource_name
                    .as_ref()
                    .and_then(|r| {
                        pipe.user_data
                            .auto_mapper
                            .resources
                            .get_mut(r)
                            .map(|rule| (r, rule))
                    })
                    .and_then(|(res, rules)| {
                        rule.and_then(|r| rules.rules.get_mut(&r).map(|rule| (res, r, rule)))
                    })
                {
                    tab.client.toggle_auto_map_live(
                        EditorEventAutoMap {
                            is_background,
                            group_index: g,
                            layer_index: l,
                            resource_and_hash: resource.to_string(),
                            name: rule_name,
                            hash: rule.hash(),
                            seed: seed.unwrap_or_default(),
                        },
                        seed.is_some(),
                    );
                }
            } else if let Some(move_act) =
                move_layer.and_then(|mv| layer_move_to_act(mv, is_background, g, l, map))
            {
                tab.client.execute(EditorAction::MoveLayer(move_act), None);
            }

            res
        }
        LayerAttrMode::DesignQuad => {
            let (is_background, g, l) = bg_selection
                .next()
                .unwrap_or_else(|| fg_selection.next().unwrap());
            let (bg_move_limit, g_limit, l_limit) = move_limits(&map.groups, is_background, g);
            let EditorLayer::Quad(layer) = layer_mut(&mut map.groups, is_background, g, l) else {
                panic!("not a quad layer, bug in above calculations")
            };
            let layer_editor = layer.user.selected.as_mut().unwrap();
            let layer_attr_cmp = layer_editor.attr;
            let layer_name_cmp = layer_editor.name.clone();

            let window = egui::Window::new("Design Quad Layer Attributes")
                .resizable(false)
                .collapsible(false);

            let mut delete_layer = false;
            let mut move_layer = None;

            let res = window.show(ui.ctx(), |ui| {
                egui::Grid::new("design group attr grid")
                    .num_columns(2)
                    .spacing([20.0, 4.0])
                    .show(ui, |ui| {
                        let attr = &mut layer_editor.attr;
                        // detail
                        ui.label("High detail");
                        toggle_ui(ui, &mut attr.high_detail);
                        ui.end_row();
                        // image
                        if ui
                            .add(
                                egui::Button::new("Image selection")
                                    .selected(layer_editor.image_selection_open.is_some()),
                            )
                            .clicked()
                        {
                            layer_editor.image_selection_open = layer_editor
                                .image_selection_open
                                .is_none()
                                .then_some(ResourceSelection {
                                    hovered_resource: None,
                                });
                        }
                        ui.end_row();
                        // name
                        ui.label("Name");
                        ui.text_edit_singleline(&mut layer_editor.name);
                        ui.end_row();
                        // delete
                        if ui.button("Delete layer").clicked() {
                            delete_layer = true;
                        }
                        ui.end_row();

                        ui.label("Move layer");
                        ui.end_row();

                        // layer moving
                        move_layer = render_layer_move(
                            ui,
                            is_background,
                            g,
                            l,
                            bg_move_limit,
                            g_limit,
                            l_limit,
                        );
                    })
            });

            if let Some(resource_selection) = &mut layer_editor.image_selection_open {
                resource_selection.hovered_resource = None;
                let res = super::resource_selector::render(
                    ui,
                    ui_state,
                    pipe.user_data.pointer_is_used,
                    &map.resources.images,
                );
                resource_selector_was_outside = res.pointer_was_outside;
                if let Some(resource) = res.mode {
                    match resource {
                        ResourceSelectionMode::Hovered(index) => {
                            resource_selection.hovered_resource = Some(index);
                        }
                        ResourceSelectionMode::Clicked(index) => {
                            layer_editor.attr.image = index;
                        }
                    }
                }
                if res.pointer_was_outside {
                    layer_editor.image_selection_open = None;
                }
            }

            if layer_editor.attr != layer_attr_cmp {
                tab.client.execute(
                    EditorAction::ChangeQuadLayerAttr(ActChangeQuadLayerAttr {
                        is_background,
                        group_index: g,
                        layer_index: l,
                        old_attr: layer.layer.attr,
                        new_attr: layer_editor.attr,
                    }),
                    Some(&format!("change-quad-layer-attr-{is_background}-{g}-{l}")),
                );
            } else if layer_editor.name != layer_name_cmp {
                tab.client.execute(
                    EditorAction::ChangeDesignLayerName(ActChangeDesignLayerName {
                        is_background,
                        group_index: g,
                        layer_index: l,
                        old_name: layer.layer.name.clone(),
                        new_name: layer_editor.name.clone(),
                    }),
                    Some(&format!(
                        "change-design-tile-layer-name-{is_background}-{g}-{l}"
                    )),
                );
            } else if delete_layer {
                tab.client.execute(
                    EditorAction::RemQuadLayer(ActRemQuadLayer {
                        base: ActAddRemQuadLayer {
                            is_background,
                            group_index: g,
                            index: l,
                            layer: layer.clone().into(),
                        },
                    }),
                    None,
                );
            } else if let Some(move_act) =
                move_layer.and_then(|mv| layer_move_to_act(mv, is_background, g, l, map))
            {
                tab.client.execute(EditorAction::MoveLayer(move_act), None);
            }

            res
        }
        LayerAttrMode::DesignSound => {
            let (is_background, g, l) = bg_selection
                .next()
                .unwrap_or_else(|| fg_selection.next().unwrap());
            let (bg_move_limit, g_limit, l_limit) = move_limits(&map.groups, is_background, g);
            let EditorLayer::Sound(layer) = layer_mut(&mut map.groups, is_background, g, l) else {
                panic!("not a sound layer, bug in above calculations")
            };
            let layer_editor = layer.user.selected.as_mut().unwrap();
            let layer_attr_cmp = layer_editor.attr;
            let layer_name_cmp = layer_editor.name.clone();

            let window = egui::Window::new("Design Sound Layer Attributes")
                .resizable(false)
                .collapsible(false);

            let mut delete_layer = false;
            let mut move_layer = None;

            let res = window.show(ui.ctx(), |ui| {
                egui::Grid::new("design group attr grid")
                    .num_columns(2)
                    .spacing([20.0, 4.0])
                    .show(ui, |ui| {
                        let attr = &mut layer_editor.attr;
                        // detail
                        ui.label("High detail");
                        toggle_ui(ui, &mut attr.high_detail);
                        ui.end_row();
                        // sound
                        if ui
                            .add(
                                egui::Button::new("Sound selection")
                                    .selected(layer_editor.sound_selection_open.is_some()),
                            )
                            .clicked()
                        {
                            layer_editor.sound_selection_open = layer_editor
                                .sound_selection_open
                                .is_none()
                                .then_some(ResourceSelection {
                                    hovered_resource: None,
                                });
                        }
                        ui.end_row();
                        // name
                        ui.label("Name");
                        ui.text_edit_singleline(&mut layer_editor.name);
                        ui.end_row();
                        // delete
                        if ui.button("Delete layer").clicked() {
                            delete_layer = true;
                        }
                        ui.end_row();

                        ui.label("Move layer");
                        ui.end_row();

                        // layer moving
                        move_layer = render_layer_move(
                            ui,
                            is_background,
                            g,
                            l,
                            bg_move_limit,
                            g_limit,
                            l_limit,
                        );
                    })
            });

            if let Some(resource_selection) = &mut layer_editor.sound_selection_open {
                resource_selection.hovered_resource = None;
                let res = super::resource_selector::render(
                    ui,
                    ui_state,
                    pipe.user_data.pointer_is_used,
                    &map.resources.sounds,
                );
                resource_selector_was_outside = res.pointer_was_outside;
                if let Some(resource) = res.mode {
                    match resource {
                        ResourceSelectionMode::Hovered(index) => {
                            resource_selection.hovered_resource = Some(index);
                        }
                        ResourceSelectionMode::Clicked(index) => {
                            layer_editor.attr.sound = index;
                        }
                    }
                }
                if res.pointer_was_outside {
                    layer_editor.sound_selection_open = None;
                }
            }

            if layer_editor.attr != layer_attr_cmp {
                tab.client.execute(
                    EditorAction::ChangeSoundLayerAttr(ActChangeSoundLayerAttr {
                        is_background,
                        group_index: g,
                        layer_index: l,
                        old_attr: layer.layer.attr,
                        new_attr: layer_editor.attr,
                    }),
                    Some(&format!("change-sound-layer-attr-{is_background}-{g}-{l}")),
                );
            } else if layer_editor.name != layer_name_cmp {
                tab.client.execute(
                    EditorAction::ChangeDesignLayerName(ActChangeDesignLayerName {
                        is_background,
                        group_index: g,
                        layer_index: l,
                        old_name: layer.layer.name.clone(),
                        new_name: layer_editor.name.clone(),
                    }),
                    Some(&format!(
                        "change-design-tile-layer-name-{is_background}-{g}-{l}"
                    )),
                );
            } else if delete_layer {
                tab.client.execute(
                    EditorAction::RemSoundLayer(ActRemSoundLayer {
                        base: ActAddRemSoundLayer {
                            is_background,
                            group_index: g,
                            index: l,
                            layer: layer.clone().into(),
                        },
                    }),
                    None,
                );
            } else if let Some(move_act) =
                move_layer.and_then(|mv| layer_move_to_act(mv, is_background, g, l, map))
            {
                tab.client.execute(EditorAction::MoveLayer(move_act), None);
            }

            res
        }
        LayerAttrMode::DesignTileMulti => todo!(),
        LayerAttrMode::DesignQuadMulti => todo!(),
        LayerAttrMode::DesignSoundMulti => todo!(),
        LayerAttrMode::DesignMulti => todo!(),
        LayerAttrMode::Physics => {
            let Some((l, layer)) = phy_selection.next() else {
                panic!("not a tile layer, bug in above calculations")
            };

            let window = egui::Window::new("Physics Layer Attributes")
                .resizable(false)
                .collapsible(false);

            let mut delete_layer = false;

            let res = window.show(ui.ctx(), |ui| {
                let res = ui
                    .label("Physics layers have no properties. Look in the physics group instead.");
                if !matches!(layer, EditorPhysicsLayer::Game(_)) {
                    // delete
                    if ui.button("Delete layer").clicked() {
                        delete_layer = true;
                    }
                }
                res
            });

            if delete_layer {
                tab.client.execute(
                    EditorAction::RemPhysicsTileLayer(ActRemPhysicsTileLayer {
                        base: ActAddRemPhysicsTileLayer {
                            index: l,
                            layer: layer.clone().into(),
                        },
                    }),
                    None,
                );
            }

            res.map(|res| {
                InnerResponse::new(
                    res.inner.map(|res| InnerResponse::new((), res)),
                    res.response,
                )
            })
        }
        LayerAttrMode::PhysicsDesignMulti => todo!(),
        LayerAttrMode::None => {
            // render nothing
            None
        }
    };

    if let Some(window_res) = &window_res {
        ui_state.add_blur_rect(window_res.response.rect, 0.0);
    }

    *pipe.user_data.pointer_is_used |= if let Some(window_res) = window_res {
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
        if intersected
            .is_some_and(|(outside, clicked)| outside && clicked && resource_selector_was_outside)
            && !ui.memory(|i| i.any_popup_open())
        {
            map.unselect_all(true, true);
        }
        intersected.is_some_and(|(outside, _)| !outside)
    } else {
        false
    };

    let move_up = pipe
        .user_data
        .cur_hotkey_events
        .remove(&EditorHotkeyEvent::Map(EditorHotkeyEventMap::MoveLayerUp));
    let move_down = pipe
        .user_data
        .cur_hotkey_events
        .remove(&EditorHotkeyEvent::Map(EditorHotkeyEventMap::MoveLayerDown));
    let active_layer = map.active_layer();
    // move
    if let Some(act) = active_layer
        .and_then(|layer| {
            if let EditorLayerUnionRef::Design {
                group_index,
                is_background,
                layer_index,
                group,
                ..
            } = layer
            {
                Some((
                    is_background,
                    group_index,
                    layer_index,
                    if is_background {
                        map.groups.background.len()
                    } else {
                        map.groups.foreground.len()
                    },
                    group.layers.len(),
                ))
            } else {
                None
            }
        })
        .and_then(|(is_background, g, l, group_len, layers_in_group)| {
            if move_up {
                if g == 0 && l == 0 && !is_background {
                    layer_move_to_act(MoveLayer::IsBackground(true), is_background, g, l, map)
                } else if g > 0 && l == 0 {
                    layer_move_to_act(MoveLayer::Group(g - 1), is_background, g, l, map)
                } else if l > 0 {
                    layer_move_to_act(MoveLayer::Layer(l - 1), is_background, g, l, map)
                } else {
                    None
                }
            } else if move_down {
                if g + 1 == group_len && l + 1 == layers_in_group && is_background {
                    layer_move_to_act(MoveLayer::IsBackground(false), is_background, g, l, map)
                } else if g + 1 < group_len && l + 1 == layers_in_group {
                    layer_move_to_act(MoveLayer::Group(g + 1), is_background, g, l, map)
                } else if l + 1 < layers_in_group {
                    layer_move_to_act(MoveLayer::Layer(l + 1), is_background, g, l, map)
                } else {
                    None
                }
            } else {
                None
            }
        })
    {
        tab.client.execute(EditorAction::MoveLayer(act), None);
    }
    let delete_layer = pipe
        .user_data
        .cur_hotkey_events
        .remove(&EditorHotkeyEvent::Map(EditorHotkeyEventMap::DeleteLayer));
    if let Some(layer) = delete_layer.then(|| map.active_layer()).flatten() {
        match layer {
            EditorLayerUnionRef::Design {
                layer,
                group_index,
                layer_index,
                is_background,
                ..
            } => {
                match layer {
                    EditorLayer::Tile(layer) => {
                        tab.client.execute(
                            EditorAction::RemTileLayer(ActRemTileLayer {
                                base: ActAddRemTileLayer {
                                    is_background,
                                    group_index,
                                    index: layer_index,
                                    layer: layer.clone().into(),
                                },
                            }),
                            None,
                        );
                    }
                    EditorLayer::Quad(layer) => {
                        tab.client.execute(
                            EditorAction::RemQuadLayer(ActRemQuadLayer {
                                base: ActAddRemQuadLayer {
                                    is_background,
                                    group_index,
                                    index: layer_index,
                                    layer: layer.clone().into(),
                                },
                            }),
                            None,
                        );
                    }
                    EditorLayer::Sound(layer) => {
                        tab.client.execute(
                            EditorAction::RemSoundLayer(ActRemSoundLayer {
                                base: ActAddRemSoundLayer {
                                    is_background,
                                    group_index,
                                    index: layer_index,
                                    layer: layer.clone().into(),
                                },
                            }),
                            None,
                        );
                    }
                    EditorLayer::Abritrary(_) => {
                        // ignore
                    }
                }
            }
            EditorLayerUnionRef::Physics {
                layer, layer_index, ..
            } => {
                tab.client.execute(
                    EditorAction::RemPhysicsTileLayer(ActRemPhysicsTileLayer {
                        base: ActAddRemPhysicsTileLayer {
                            index: layer_index,
                            layer: layer.clone().into(),
                        },
                    }),
                    None,
                );
            }
        }
    }
}
