use egui::{Button, Color32, InnerResponse};
use map::map::groups::layers::design::Quad;
use math::math::vector::{dvec2, ffixed, nffixed, nfvec4, vec2_base};
use time::Duration;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    actions::actions::{
        ActChangeQuadAttr, ActQuadLayerAddRemQuads, ActQuadLayerRemQuads, EditorAction,
    },
    explain::TEXT_QUAD_PROP_COLOR,
    hotkeys::{
        BindsPerEvent, EditorBindsFile, EditorHotkeyEvent, EditorHotkeyEventQuadBrush,
        EditorHotkeyEventQuadTool, EditorHotkeyEventSharedTool, EditorHotkeyEventTools,
    },
    map::{EditorAnimations, EditorLayer, EditorLayerUnionRefMut, EditorMapGroupsInterface},
    tools::{
        quad_layer::shared::QuadPointerDownPoint,
        tool::{ActiveTool, ActiveToolQuads},
    },
    ui::{group_and_layer::shared::animations_panel_open_warning, user_data::UserDataWithTab},
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    #[derive(Debug, PartialEq, Eq)]
    enum QuadAttrMode {
        Single,
        /// multiple quads at once
        Multi,
        None,
    }

    let binds = &*pipe.user_data.hotkeys;
    let per_ev = &mut *pipe.user_data.cached_binds_per_event;

    let map = &mut pipe.user_data.editor_tab.map;
    let animations_panel_open =
        map.user.ui_values.animations_panel_open && !map.user.options.no_animations_with_properties;
    let layer = map.groups.active_layer_mut();
    let mut attr_mode = QuadAttrMode::None;
    if let Some(EditorLayerUnionRefMut::Design {
        layer: EditorLayer::Quad(layer),
        group_index,
        layer_index,
        is_background,
        ..
    }) = layer
    {
        let (mut selected_quads, point, pos_offset) = match &pipe.user_data.tools.active_tool {
            ActiveTool::Quads(ActiveToolQuads::Brush) => {
                let brush = &mut pipe.user_data.tools.quads.brush;
                let point = brush
                    .last_popup
                    .as_ref()
                    .and_then(|selection| selection.point)
                    .unwrap_or(QuadPointerDownPoint::Center);
                (
                    brush
                        .last_popup
                        .as_mut()
                        .map(|selection| selection.indices_checked(layer))
                        .unwrap_or_default(),
                    Some(point),
                    Some(&mut brush.pos_offset),
                )
            }
            ActiveTool::Quads(ActiveToolQuads::Selection) => {
                let selection = &mut pipe.user_data.tools.quads.selection;
                let point = selection.range.as_ref().and_then(|range| range.point);
                (
                    selection
                        .range
                        .as_mut()
                        .map(|range| range.indices_checked(layer))
                        .unwrap_or_default(),
                    point,
                    Some(&mut selection.pos_offset),
                )
            }
            ActiveTool::Sounds(_) | ActiveTool::Tiles(_) => {
                // ignore
                (Default::default(), None, None)
            }
        };

        if point.is_none() {
            return;
        }
        let point = point.unwrap();

        let quads_count = selected_quads.len();
        if quads_count > 0 {
            attr_mode = if quads_count == 1 {
                QuadAttrMode::Single
            } else {
                QuadAttrMode::Multi
            };
        }

        fn square(quad: &mut Quad) {
            let mut min = quad.points[0];
            let mut max = quad.points[0];

            for i in 0..4 {
                min.x = quad.points[i].x.min(min.x);
                min.y = quad.points[i].y.min(min.y);
                max.x = quad.points[i].x.max(max.x);
                max.y = quad.points[i].y.max(max.y);
            }

            quad.points[0] = min;
            quad.points[1] = vec2_base::new(max.x, min.y);
            quad.points[2] = vec2_base::new(min.x, max.y);
            quad.points[3] = max;
        }

        fn quad_attr_ui(
            ui: &mut egui::Ui,
            binds: &EditorBindsFile,
            per_ev: &mut Option<BindsPerEvent>,
            quads_count: usize,
            point: QuadPointerDownPoint,
            quad: &mut Quad,
            // make a "move pos" instead of x, y directly
            pos_offset: Option<&mut dvec2>,
            can_change_pos_anim: bool,
            can_change_color_anim: bool,
            animations_panel_open: bool,
            animations: &mut EditorAnimations,
            pointer_is_used: &mut bool,
        ) -> InnerResponse<bool> {
            let mut anim_pos = can_change_pos_anim
                .then_some(animations.user.active_anim_points.pos.as_mut())
                .flatten();
            let anim_color = can_change_color_anim
                .then_some(animations.user.active_anim_points.color.as_mut())
                .flatten();

            let mut delete = false;
            egui::Grid::new("design group attr grid")
                .num_columns(2)
                .spacing([20.0, 4.0])
                .show(ui, |ui| {
                    if quads_count > 1 {
                        ui.label(format!("selected {quads_count} quads"));
                        ui.end_row();
                    }
                    let p = match point {
                        QuadPointerDownPoint::Center => 4,
                        QuadPointerDownPoint::Corner(index) => index,
                    };
                    if !animations_panel_open || (can_change_pos_anim && quad.pos_anim.is_some()) {
                        if let Some(pos_offset) = pos_offset {
                            // x
                            ui.label("move x by");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::DragValue::new(&mut pos_offset.x)
                                        .update_while_editing(false),
                                );
                                if ui.button("move").clicked() {
                                    if let Some(pos_anim) = &mut anim_pos {
                                        pos_anim.value.x = ffixed::from_num(pos_offset.x);
                                    } else {
                                        quad.points[p].x = ffixed::from_num(
                                            quad.points[p].x.to_num::<f64>() + pos_offset.x,
                                        );
                                    }
                                }
                            });
                            ui.end_row();
                            // y
                            ui.label("move y by");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::DragValue::new(&mut pos_offset.y)
                                        .update_while_editing(false),
                                );
                                if ui.button("move").clicked() {
                                    if let Some(pos_anim) = anim_pos {
                                        pos_anim.value.y = ffixed::from_num(pos_offset.y);
                                    } else {
                                        quad.points[p].y = ffixed::from_num(
                                            quad.points[p].y.to_num::<f64>() + pos_offset.y,
                                        );
                                    }
                                }
                            });
                            ui.end_row();
                        } else {
                            // x
                            ui.label("x");
                            let mut x = quad.points[p].x.to_num::<f64>();
                            ui.add(egui::DragValue::new(&mut x).update_while_editing(false));
                            quad.points[p].x = ffixed::from_num(x);
                            ui.end_row();
                            // y
                            ui.label("y");
                            let mut y = quad.points[p].y.to_num::<f64>();
                            ui.add(egui::DragValue::new(&mut y).update_while_editing(false));
                            quad.points[p].y = ffixed::from_num(y);
                            ui.end_row();
                        }
                    }

                    if matches!(point, QuadPointerDownPoint::Center) && !animations_panel_open {
                        fn combobox_name(ty: &str, index: usize, name: &str) -> String {
                            name.is_empty()
                                .then_some(format!("{ty} #{}", index))
                                .unwrap_or_else(|| name.to_owned())
                        }
                        if can_change_pos_anim {
                            // pos anim
                            ui.label("pos anim");
                            let res = egui::ComboBox::new("quad-select-pos-anim".to_string(), "")
                                .selected_text(
                                    animations
                                        .pos
                                        .get(quad.pos_anim.unwrap_or(usize::MAX))
                                        .map(|anim| {
                                            combobox_name(
                                                "pos",
                                                quad.pos_anim.unwrap(),
                                                &anim.def.name.clone(),
                                            )
                                        })
                                        .unwrap_or_else(|| "None".to_string()),
                                )
                                .show_ui(ui, |ui| {
                                    if ui.button("None").clicked() {
                                        quad.pos_anim = None;
                                    }
                                    for (a, anim) in animations.pos.iter().enumerate() {
                                        if ui
                                            .button(combobox_name("pos", a, &anim.def.name))
                                            .clicked()
                                        {
                                            quad.pos_anim = Some(a);
                                        }
                                    }
                                });
                            ui.end_row();

                            *pointer_is_used |= {
                                let intersected = ui.input(|i| {
                                    if i.pointer.primary_down() {
                                        Some((
                                            !res.response.rect.intersects({
                                                let min =
                                                    i.pointer.interact_pos().unwrap_or_default();
                                                let max = min;
                                                [min, max].into()
                                            }),
                                            i.pointer.primary_pressed(),
                                        ))
                                    } else {
                                        None
                                    }
                                });
                                intersected.is_some_and(|(outside, _)| !outside)
                            };

                            // pos time offset
                            ui.label("pos anim time offset");
                            let mut millis = quad.pos_anim_offset.whole_milliseconds() as i64;
                            if ui
                                .add(egui::DragValue::new(&mut millis).update_while_editing(false))
                                .changed()
                            {
                                quad.pos_anim_offset = Duration::milliseconds(millis);
                            }
                            ui.end_row();
                        }
                        if can_change_color_anim {
                            // color anim
                            ui.label("color anim");
                            let res = egui::ComboBox::new("quad-select-color-anim".to_string(), "")
                                .selected_text(
                                    animations
                                        .color
                                        .get(quad.color_anim.unwrap_or(usize::MAX))
                                        .map(|anim| {
                                            combobox_name(
                                                "color",
                                                quad.color_anim.unwrap(),
                                                &anim.def.name.clone(),
                                            )
                                        })
                                        .unwrap_or_else(|| "None".to_string()),
                                )
                                .show_ui(ui, |ui| {
                                    if ui.button("None").clicked() {
                                        quad.color_anim = None;
                                    }
                                    for (a, anim) in animations.color.iter().enumerate() {
                                        if ui
                                            .button(combobox_name("color", a, &anim.def.name))
                                            .clicked()
                                        {
                                            quad.color_anim = Some(a);
                                        }
                                    }
                                });
                            ui.end_row();

                            *pointer_is_used |= {
                                let intersected = ui.input(|i| {
                                    if i.pointer.primary_down() {
                                        Some((
                                            !res.response.rect.intersects({
                                                let min =
                                                    i.pointer.interact_pos().unwrap_or_default();
                                                let max = min;
                                                [min, max].into()
                                            }),
                                            i.pointer.primary_pressed(),
                                        ))
                                    } else {
                                        None
                                    }
                                });
                                intersected.is_some_and(|(outside, _)| !outside)
                            };

                            // color time offset
                            ui.label("color anim time offset");
                            let mut millis = quad.color_anim_offset.whole_milliseconds() as i64;
                            if ui
                                .add(egui::DragValue::new(&mut millis).update_while_editing(false))
                                .changed()
                            {
                                quad.color_anim_offset = Duration::milliseconds(millis);
                            }
                            ui.end_row();
                        }

                        // square
                        if ui
                            .button("Square")
                            .on_hover_ui(|ui| {
                                let mut cache = egui_commonmark::CommonMarkCache::default();
                                egui_commonmark::CommonMarkViewer::new().show(
                                    ui,
                                    &mut cache,
                                    &format!(
                                        "Hotkey: `{}`",
                                        binds.fmt_ev_bind(
                                            per_ev,
                                            &EditorHotkeyEvent::Tools(
                                                EditorHotkeyEventTools::Quad(
                                                    EditorHotkeyEventQuadTool::Brush(
                                                        EditorHotkeyEventQuadBrush::Square
                                                    )
                                                )
                                            ),
                                        )
                                    ),
                                );
                            })
                            .clicked()
                        {
                            square(quad);
                        }
                        ui.end_row();

                        if ui
                            .add(Button::new("Delete"))
                            .on_hover_ui(|ui| {
                                let mut cache = egui_commonmark::CommonMarkCache::default();
                                egui_commonmark::CommonMarkViewer::new().show(
                                    ui,
                                    &mut cache,
                                    &format!(
                                        "Hotkey: `{}`",
                                        binds.fmt_ev_bind(
                                            per_ev,
                                            &EditorHotkeyEvent::Tools(
                                                EditorHotkeyEventTools::Shared(
                                                    EditorHotkeyEventSharedTool::DeleteQuadOrSound,
                                                )
                                            ),
                                        )
                                    ),
                                );
                            })
                            .clicked()
                        {
                            delete = true;
                        }
                        ui.end_row();
                    } else if let QuadPointerDownPoint::Corner(c) = point {
                        // corner:
                        // color
                        if !animations_panel_open
                            || (can_change_color_anim && quad.color_anim.is_some())
                        {
                            ui.label("Color \u{f05a}").on_hover_ui(|ui| {
                                let mut cache = egui_commonmark::CommonMarkCache::default();
                                egui_commonmark::CommonMarkViewer::new().show(
                                    ui,
                                    &mut cache,
                                    TEXT_QUAD_PROP_COLOR,
                                );
                            });
                            if let Some(color_anim) = anim_color {
                                let mut color = [
                                    (color_anim.value.r().to_num::<f32>() * 255.0) as u8,
                                    (color_anim.value.g().to_num::<f32>() * 255.0) as u8,
                                    (color_anim.value.b().to_num::<f32>() * 255.0) as u8,
                                    (color_anim.value.a().to_num::<f32>() * 255.0) as u8,
                                ];
                                ui.color_edit_button_srgba_unmultiplied(&mut color);
                                color_anim.value = nfvec4::new(
                                    nffixed::from_num(color[0] as f32 / 255.0),
                                    nffixed::from_num(color[1] as f32 / 255.0),
                                    nffixed::from_num(color[2] as f32 / 255.0),
                                    nffixed::from_num(color[3] as f32 / 255.0),
                                );
                            } else {
                                let mut color = [
                                    (quad.colors[c].r().to_num::<f32>() * 255.0) as u8,
                                    (quad.colors[c].g().to_num::<f32>() * 255.0) as u8,
                                    (quad.colors[c].b().to_num::<f32>() * 255.0) as u8,
                                    (quad.colors[c].a().to_num::<f32>() * 255.0) as u8,
                                ];
                                ui.color_edit_button_srgba_unmultiplied(&mut color);
                                quad.colors[c] = nfvec4::new(
                                    nffixed::from_num(color[0] as f32 / 255.0),
                                    nffixed::from_num(color[1] as f32 / 255.0),
                                    nffixed::from_num(color[2] as f32 / 255.0),
                                    nffixed::from_num(color[3] as f32 / 255.0),
                                );
                            }
                            ui.end_row();
                        }
                        // tex u
                        // tex v
                    }

                    if animations_panel_open {
                        ui.colored_label(
                            Color32::RED,
                            "The animation panel is open,\n\
                                changing attributes will not apply them\n\
                                to the quad permanently!",
                        )
                        .on_hover_ui(animations_panel_open_warning);
                        ui.end_row();
                    }
                    delete
                })
        }

        let window_res = match attr_mode {
            QuadAttrMode::Single => {
                let (index, quad) = selected_quads.pop_first().unwrap();
                let quad_cmp = *quad;

                let window = egui::Window::new("Design Quad Attributes")
                    .resizable(false)
                    .collapsible(false);

                let window_res = window.show(ui.ctx(), |ui| {
                    quad_attr_ui(
                        ui,
                        binds,
                        per_ev,
                        quads_count,
                        point,
                        quad,
                        None,
                        true,
                        true,
                        animations_panel_open,
                        &mut map.animations,
                        pipe.user_data.pointer_is_used,
                    )
                });

                let delete = window_res
                    .as_ref()
                    .is_some_and(|r| r.inner.as_ref().is_some_and(|r| r.inner));

                if *quad != quad_cmp && !animations_panel_open {
                    let layer_quad = &layer.layer.quads[index];
                    pipe.user_data.editor_tab.client.execute(
                        EditorAction::ChangeQuadAttr(Box::new(ActChangeQuadAttr {
                            is_background,
                            group_index,
                            layer_index,
                            old_attr: *layer_quad,
                            new_attr: *quad,

                            index,
                        })),
                        Some(&format!(
                            "change-quad-attr-{is_background}-{group_index}-{layer_index}-{index}"
                        )),
                    );
                } else if delete {
                    pipe.user_data.editor_tab.client.execute(
                        EditorAction::QuadLayerRemQuads(ActQuadLayerRemQuads {
                            base: ActQuadLayerAddRemQuads {
                                is_background,
                                group_index,
                                layer_index,
                                index,
                                quads: vec![*quad],
                            },
                        }),
                        Some(&format!(
                            "quad-rem-design-{is_background}-{group_index}-{layer_index}-{index}"
                        )),
                    );
                }

                window_res
            }
            QuadAttrMode::Multi => {
                let (_, mut quad) = selected_quads
                    .iter_mut()
                    .peekable()
                    .next()
                    .map(|(i, q)| (*i, **q))
                    .unwrap();
                let quad_cmp = quad;

                let mut selected_quads: Vec<_> = selected_quads.into_iter().collect();
                let can_change_pos_anim = selected_quads
                    .windows(2)
                    .all(|window| window[0].1.pos_anim == window[1].1.pos_anim);
                let can_change_color_anim = selected_quads
                    .windows(2)
                    .all(|window| window[0].1.color_anim == window[1].1.color_anim);

                let window = egui::Window::new("Design Quads Attributes")
                    .resizable(false)
                    .collapsible(false);

                let window_res = window.show(ui.ctx(), |ui| {
                    quad_attr_ui(
                        ui,
                        binds,
                        per_ev,
                        quads_count,
                        point,
                        &mut quad,
                        pos_offset,
                        can_change_pos_anim,
                        can_change_color_anim,
                        animations_panel_open,
                        &mut map.animations,
                        pipe.user_data.pointer_is_used,
                    )
                });

                let delete = window_res
                    .as_ref()
                    .is_some_and(|r| r.inner.as_ref().is_some_and(|r| r.inner));

                if quad != quad_cmp {
                    let prop_quad = quad;
                    // copy the changed data into all selected quads
                    selected_quads.iter_mut().for_each(|(index, quad)| {
                        let index = *index;
                        let layer_quad = &layer.layer.quads[index];
                        // move points by diff
                        for (p, point) in quad.points.iter_mut().enumerate() {
                            let diff = prop_quad.points[p] - quad_cmp.points[p];

                            *point += diff;
                        }

                        // apply color if changed
                        for (c, color) in quad.colors.iter_mut().enumerate() {
                            let diff = prop_quad.colors[c] != quad_cmp.colors[c];

                            if diff {
                                *color = prop_quad.colors[c];
                            }
                        }

                        // apply tex coords if changed
                        for (t, tex) in quad.tex_coords.iter_mut().enumerate() {
                            let diff = prop_quad.tex_coords[t] != quad_cmp.tex_coords[t];

                            if diff {
                                *tex = prop_quad.tex_coords[t];
                            }
                        }

                        // apply new anims if changed, for the time offset do a difference instead
                        if can_change_pos_anim {
                            let diff = prop_quad.pos_anim != quad_cmp.pos_anim;

                            if diff {
                                quad.pos_anim = prop_quad.pos_anim;
                            }
                            let diff = prop_quad.pos_anim_offset - quad_cmp.pos_anim_offset;

                            quad.pos_anim_offset += diff;
                        }
                        if can_change_color_anim {
                            let diff = prop_quad.color_anim != quad_cmp.color_anim;

                            if diff {
                                quad.color_anim = prop_quad.color_anim;
                            }
                            let diff = prop_quad.color_anim_offset - quad_cmp.color_anim_offset;

                            quad.color_anim_offset += diff;
                        }

                        // generate events for all selected quads
                        if !animations_panel_open {
                            pipe.user_data.editor_tab.client.execute(
                                EditorAction::ChangeQuadAttr(Box::new(ActChangeQuadAttr {
                                    is_background,
                                    group_index,
                                    layer_index,
                                    old_attr: *layer_quad,
                                    new_attr: **quad,

                                    index,
                                })),
                                Some(&format!(
                                    "change-quad-attr-{is_background}-{group_index}-{layer_index}-{index}"
                                )),
                            );
                        }
                    });
                } else if delete {
                    // rewrite the quad indices, since they get invalid every time a quad is deleted.
                    for i in 0..selected_quads.len() {
                        let (delete_index, _) = selected_quads[i];
                        for (index, _) in selected_quads.iter_mut().skip(i + 1) {
                            if *index > delete_index {
                                *index = index.saturating_sub(1);
                            }
                        }
                    }

                    for (index, quad) in selected_quads {
                        pipe.user_data.editor_tab.client.execute(
                            EditorAction::QuadLayerRemQuads(ActQuadLayerRemQuads {
                                base: ActQuadLayerAddRemQuads {
                                    is_background,
                                    group_index,
                                    layer_index,
                                    index,
                                    quads: vec![*quad],
                                },
                            }),
                            Some(&format!(
                                "quad-rem-design-{is_background}-\
                                {group_index}-{layer_index}-{index}"
                            )),
                        );
                    }
                }

                window_res
            }
            QuadAttrMode::None => {
                // nothing to render
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
            if intersected.is_some_and(|(outside, clicked)| outside && clicked)
                && !ui.memory(|i| i.any_popup_open())
            {
                match &pipe.user_data.tools.active_tool {
                    ActiveTool::Quads(ActiveToolQuads::Brush) => {
                        pipe.user_data.tools.quads.brush.last_popup = None;
                    }
                    ActiveTool::Quads(ActiveToolQuads::Selection) => {
                        pipe.user_data.tools.quads.selection.range = None;
                    }
                    ActiveTool::Sounds(_) | ActiveTool::Tiles(_) => {
                        // ignore
                    }
                }
            }
            intersected.is_some_and(|(outside, _)| !outside)
        } else {
            false
        };

        // additional to the visible ui there is also some handling for hotkeys
        let mut selected_quads = match &pipe.user_data.tools.active_tool {
            ActiveTool::Quads(ActiveToolQuads::Brush) => {
                let brush = &mut pipe.user_data.tools.quads.brush;
                brush
                    .last_selection
                    .as_mut()
                    .map(|selection| selection.indices_checked(layer))
                    .unwrap_or_default()
            }
            ActiveTool::Quads(ActiveToolQuads::Selection) => {
                let selection = &mut pipe.user_data.tools.quads.selection;
                selection
                    .range
                    .as_mut()
                    .map(|range| range.indices_checked(layer))
                    .unwrap_or_default()
            }
            ActiveTool::Sounds(_) | ActiveTool::Tiles(_) => {
                // ignore
                Default::default()
            }
        };
        let square_quads = pipe
            .user_data
            .cur_hotkey_events
            .remove(&EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Quad(
                EditorHotkeyEventQuadTool::Brush(EditorHotkeyEventQuadBrush::Square),
            )));
        if square_quads {
            for (&index, q) in selected_quads.iter_mut() {
                let mut new_quad = **q;
                square(&mut new_quad);
                pipe.user_data.editor_tab.client.execute(
                    EditorAction::ChangeQuadAttr(Box::new(ActChangeQuadAttr {
                        is_background,
                        group_index,
                        layer_index,
                        old_attr: **q,
                        new_attr: new_quad,

                        index,
                    })),
                    Some(&format!(
                        "change-quad-attr-{is_background}-{group_index}-{layer_index}-{index}"
                    )),
                );
            }
        }
        if !selected_quads.is_empty() {
            let delete_quads = pipe
                .user_data
                .cur_hotkey_events
                .remove(&EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Shared(
                    EditorHotkeyEventSharedTool::DeleteQuadOrSound,
                )));
            if delete_quads {
                for (&index, q) in selected_quads.iter_mut() {
                    pipe.user_data.editor_tab.client.execute(
                        EditorAction::QuadLayerRemQuads(ActQuadLayerRemQuads {
                            base: ActQuadLayerAddRemQuads {
                                is_background,
                                group_index,
                                layer_index,
                                index,
                                quads: vec![**q],
                            },
                        }),
                        Some(&format!(
                            "delete-quad-{is_background}-{group_index}-{layer_index}-{index}"
                        )),
                    );
                }
            }
        }
    }
}
