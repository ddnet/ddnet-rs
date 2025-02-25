use std::time::Duration;

use client_render_base::map::render_tools::RenderTools;
use egui::UiBuilder;
use egui_timeline::point::{Point, PointGroup};
use map::{
    map::animations::{AnimPoint, AnimPointColor, AnimPointCurveType, AnimPointPos},
    skeleton::animations::AnimBaseSkeleton,
};
use serde::de::DeserializeOwned;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    map::{EditorAnimationProps, EditorLayer, EditorLayerUnionRef, EditorMapGroupsInterface},
    tools::{
        quad_layer::selection::QuadSelection,
        tool::{ActiveTool, ActiveToolQuads},
    },
    ui::user_data::UserDataWithTab,
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    let map = &mut pipe.user_data.editor_tab.map;
    if !map.user.ui_values.animations_panel_open {
        return;
    }

    let active_layer = map.groups.active_layer();
    let tools = &mut *pipe.user_data.tools;

    let res = {
        let mut panel = egui::TopBottomPanel::bottom("animations_panel")
            .resizable(true)
            .height_range(300.0..=600.0);
        panel = panel.default_height(300.0);

        // if anim panel is open, and quads/sounds are selected
        // they basically automatically select their active animations
        let mut selected_color_anim_selection;
        let mut selected_pos_anim_selection;
        //let mut selected_sound_anim_selection;
        let (selected_color_anim, selected_pos_anim, selected_sound_anim) = {
            let (can_change_pos_anim, can_change_color_anim) = if let (
                Some(EditorLayerUnionRef::Design {
                    layer: EditorLayer::Quad(layer),
                    ..
                }),
                ActiveTool::Quads(ActiveToolQuads::Selection),
                QuadSelection {
                    range: Some(range), ..
                },
                None,
            ) = (
                &active_layer,
                &tools.active_tool,
                &mut tools.quads.selection,
                map.user.options.no_animations_with_properties.then_some(()),
            ) {
                let range = range.indices_checked(layer);
                let range: Vec<_> = range.into_iter().collect();

                (
                    if range
                        .windows(2)
                        .all(|window| window[0].1.pos_anim == window[1].1.pos_anim)
                        && !range.is_empty()
                    {
                        range[0].1.pos_anim
                    } else {
                        None
                    },
                    if range
                        .windows(2)
                        .all(|window| window[0].1.color_anim == window[1].1.color_anim)
                        && !range.is_empty()
                    {
                        range[0].1.color_anim
                    } else {
                        None
                    },
                )
            } else {
                (None, None)
            };
            (
                if let Some(anim) = can_change_color_anim {
                    selected_color_anim_selection = Some(anim);
                    &mut selected_color_anim_selection
                } else {
                    &mut map.animations.user.selected_color_anim
                },
                if let Some(anim) = can_change_pos_anim {
                    selected_pos_anim_selection = Some(anim);
                    &mut selected_pos_anim_selection
                } else {
                    &mut map.animations.user.selected_pos_anim
                },
                &mut map.animations.user.selected_sound_anim,
            )
        };

        Some(panel.show_inside(ui, |ui| {
            fn add_selector<A: Point + DeserializeOwned + PartialOrd + Clone>(
                ui: &mut egui::Ui,
                anims: &[AnimBaseSkeleton<EditorAnimationProps, A>],
                index: &mut Option<usize>,
                name: &str,
            ) {
                ui.label(format!("{}:", name));
                // selection of animation
                if ui.button("\u{f060}").clicked() {
                    *index = index.map(|i| i.checked_sub(1)).flatten();
                }

                fn combobox_name(ty: &str, index: usize, name: &str) -> String {
                    name.is_empty()
                        .then_some(format!("{ty} #{}", index))
                        .unwrap_or_else(|| name.to_owned())
                }
                egui::ComboBox::new(format!("animations-select-anim{name}"), "")
                    .selected_text(
                        anims
                            .get(index.unwrap_or(usize::MAX))
                            .map(|anim| combobox_name(name, index.unwrap(), &anim.def.name.clone()))
                            .unwrap_or_else(|| "None".to_string()),
                    )
                    .show_ui(ui, |ui| {
                        if ui.button("None").clicked() {
                            *index = None;
                        }
                        for (a, anim) in anims.iter().enumerate() {
                            if ui.button(combobox_name(name, a, &anim.def.name)).clicked() {
                                *index = Some(a);
                            }
                        }
                    });

                if ui.button("\u{f061}").clicked() {
                    *index = index.map(|i| (i + 1).clamp(0, anims.len() - 1));
                    if index.is_none() && !anims.is_empty() {
                        *index = Some(0);
                    }
                }
            }
            egui::Grid::new("anim-active-selectors")
                .spacing([2.0, 4.0])
                .num_columns(4)
                .show(ui, |ui| {
                    add_selector(ui, &map.animations.color, selected_color_anim, "color");
                    ui.end_row();
                    add_selector(ui, &map.animations.pos, selected_pos_anim, "pos");

                    ui.end_row();
                    add_selector(ui, &map.animations.sound, selected_sound_anim, "sound");

                    ui.end_row();
                });

            let mut groups: Vec<PointGroup<'_>> = Default::default();

            fn add_group<'a, A: Point + DeserializeOwned + PartialOrd + Clone>(
                groups: &mut Vec<PointGroup<'a>>,
                anims: &'a mut [AnimBaseSkeleton<EditorAnimationProps, A>],
                index: Option<usize>,
                name: &'a str,
            ) {
                if let Some(anim) = anims.get_mut(index.unwrap_or(usize::MAX)) {
                    groups.push(PointGroup {
                        name,
                        points: anim
                            .def
                            .points
                            .iter_mut()
                            .map(|val| val as &mut dyn Point)
                            .collect::<Vec<_>>(),
                        selected_points: &mut anim.user.selected_points,
                        hovered_point: &mut anim.user.hovered_point,
                        selected_point_channels: &mut anim.user.selected_point_channels,
                        hovered_point_channel: &mut anim.user.hovered_point_channels,
                    });
                }
            }

            add_group(
                &mut groups,
                &mut map.animations.color,
                *selected_color_anim,
                "color",
            );
            add_group(
                &mut groups,
                &mut map.animations.pos,
                *selected_pos_anim,
                "pos",
            );
            add_group(
                &mut groups,
                &mut map.animations.sound,
                *selected_sound_anim,
                "sound",
            );

            ui.allocate_new_ui(
                UiBuilder::new().max_rect(ui.available_rect_before_wrap()),
                |ui| map.user.ui_values.timeline.show(ui, &mut groups),
            )
        }))
    };

    if let Some(res) = res {
        ui_state.add_blur_rect(res.response.rect, 0.0);

        if !map.user.options.no_animations_with_properties {
            if res.inner.inner.time_changed {
                // handle time change, e.g. modify the props of selected quads
                handle_anim_time_change(pipe);
            }
            if res.inner.inner.insert_or_replace_point {
                handle_point_insert(pipe);
            }
        }
    }
}

fn handle_anim_time_change(pipe: &mut UiRenderPipe<UserDataWithTab>) {
    let map = &mut pipe.user_data.editor_tab.map;

    let active_layer = map.groups.active_layer();
    let tools = &mut *pipe.user_data.tools;

    if let (
        Some(EditorLayerUnionRef::Design {
            layer: EditorLayer::Quad(layer),
            ..
        }),
        ActiveTool::Quads(ActiveToolQuads::Selection),
        QuadSelection {
            range: Some(range),
            anim_point_color,
            anim_point_pos,
            ..
        },
    ) = (
        &active_layer,
        &tools.active_tool,
        &mut tools.quads.selection,
    ) {
        let range = range.indices_checked(layer);
        if let Some((_, quad)) = range.iter().next() {
            if let Some(pos_anim) = quad.pos_anim {
                let anim = &map.animations.pos[pos_anim];
                let anim_pos = RenderTools::render_eval_anim(
                    anim.def.points.as_slice(),
                    time::Duration::try_from(map.user.ui_values.timeline.time()).unwrap(),
                );
                *anim_point_pos = AnimPointPos {
                    time: Duration::ZERO,
                    curve_type: AnimPointCurveType::Linear,
                    value: anim_pos,
                };
            }
            if let Some(color_anim) = quad.color_anim {
                let anim = &map.animations.color[color_anim];
                let anim_color = RenderTools::render_eval_anim(
                    anim.def.points.as_slice(),
                    time::Duration::try_from(map.user.ui_values.timeline.time()).unwrap(),
                );
                *anim_point_color = AnimPointColor {
                    time: Duration::ZERO,
                    curve_type: AnimPointCurveType::Linear,
                    value: anim_color,
                };
            }
        }
    }
}

fn handle_point_insert(pipe: &mut UiRenderPipe<UserDataWithTab>) {
    let map = &mut pipe.user_data.editor_tab.map;

    let active_layer = map.groups.active_layer();
    let tools = &mut *pipe.user_data.tools;

    let cur_time = map.user.ui_values.timeline.time();

    if let (
        Some(EditorLayerUnionRef::Design {
            layer: EditorLayer::Quad(layer),
            ..
        }),
        ActiveTool::Quads(ActiveToolQuads::Selection),
        QuadSelection {
            range: Some(range),
            anim_point_color,
            anim_point_pos,
            ..
        },
    ) = (
        &active_layer,
        &tools.active_tool,
        &mut tools.quads.selection,
    ) {
        fn add_or_insert<P: Clone + DeserializeOwned, const CHANNELS: usize>(
            cur_time: Duration,
            anim: &mut AnimBaseSkeleton<EditorAnimationProps, AnimPoint<P, CHANNELS>>,
            insert_repl_point: &AnimPoint<P, CHANNELS>,
        ) {
            enum ReplOrInsert {
                Repl(usize),
                Insert(usize),
            }

            let index = anim.def.points.iter().enumerate().find_map(|(p, point)| {
                match point.time.cmp(&cur_time) {
                    std::cmp::Ordering::Less => None,
                    std::cmp::Ordering::Equal => Some(ReplOrInsert::Repl(p)),
                    std::cmp::Ordering::Greater => Some(ReplOrInsert::Insert(p)),
                }
            });

            let mut insert_repl_point = insert_repl_point.clone();
            insert_repl_point.time = cur_time;

            match index {
                Some(mode) => match mode {
                    ReplOrInsert::Repl(index) => {
                        anim.def.points[index] = insert_repl_point;
                    }
                    ReplOrInsert::Insert(index) => {
                        anim.def.points.insert(index, insert_repl_point);
                    }
                },
                None => {
                    // nothing to do
                }
            }
        }

        let range = range.indices_checked(layer);
        if let Some((_, quad)) = range.iter().next() {
            if let Some(pos_anim) = quad.pos_anim {
                let anim = &mut map.animations.pos[pos_anim];
                add_or_insert(cur_time, anim, anim_point_pos);
            }
            if let Some(color_anim) = quad.color_anim {
                let anim = &mut map.animations.color[color_anim];
                add_or_insert(cur_time, anim, anim_point_color);
            }
        }
    }
}
