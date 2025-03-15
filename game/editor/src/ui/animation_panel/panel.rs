use std::time::Duration;

use client_render_base::map::render_tools::RenderTools;
use egui::UiBuilder;
use egui_timeline::point::{Point, PointGroup};
use map::{
    map::animations::{
        AnimPoint, AnimPointColor, AnimPointCurveType, AnimPointPos, AnimPointSound,
        ColorAnimation, PosAnimation, SoundAnimation,
    },
    skeleton::animations::AnimBaseSkeleton,
};
use serde::de::DeserializeOwned;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    actions::{
        actions::{
            ActAddColorAnim, ActAddPosAnim, ActAddRemColorAnim, ActAddRemPosAnim,
            ActAddRemSoundAnim, ActAddSoundAnim, ActReplColorAnim, ActReplPosAnim,
            ActReplSoundAnim, EditorAction, EditorActionGroup,
        },
        utils::{rem_color_anim, rem_pos_anim, rem_sound_anim},
    },
    client::EditorClient,
    map::{
        EditorAnimationProps, EditorGroups, EditorLayer, EditorLayerUnionRef,
        EditorMapGroupsInterface,
    },
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
                ActiveTool::Quads(ActiveToolQuads::Selection | ActiveToolQuads::Brush),
                Some(range),
                None,
            ) = (
                &active_layer,
                &tools.active_tool,
                if matches!(
                    tools.active_tool,
                    ActiveTool::Quads(ActiveToolQuads::Selection)
                ) {
                    &mut tools.quads.selection.range
                } else if matches!(tools.active_tool, ActiveTool::Quads(ActiveToolQuads::Brush)) {
                    &mut tools.quads.brush.last_selection
                } else {
                    &mut None
                },
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
                groups: &EditorGroups,
                index: &mut Option<usize>,
                name: &str,
                client: &EditorClient,
                add: impl FnOnce(usize) -> EditorAction,
                del: impl FnOnce(
                    usize,
                    &[AnimBaseSkeleton<EditorAnimationProps, A>],
                    &EditorGroups,
                ) -> EditorActionGroup,
                sync: impl FnOnce(usize, &AnimBaseSkeleton<EditorAnimationProps, A>) -> EditorAction,
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

                // add new anim
                if ui.button("\u{f0fe}").clicked() {
                    let index = anims.len();

                    client.execute(
                        add(index),
                        Some(&format!("{name}-anim-insert-anim-at-{}", index)),
                    );
                }

                if let Some(index) = index
                    .as_mut()
                    .and_then(|index| (*index < anims.len()).then_some(index))
                {
                    // delete currently selected anim
                    if ui.button("\u{f1f8}").clicked() {
                        client.execute_group(del(*index, anims, groups));
                        *index = index.saturating_sub(1);
                    }

                    // Whether to sync the current animation to server time
                    let mut is_sync = anims[*index].def.synchronized;
                    if ui.checkbox(&mut is_sync, "Synchronize").changed() {
                        client.execute(sync(
                            *index, &anims[*index]), None);
                    }
                }
            }
            egui::Grid::new("anim-active-selectors")
                .spacing([2.0, 4.0])
                .num_columns(4)
                .show(ui, |ui| {
                    let client = &pipe.user_data.editor_tab.client;
                    add_selector(
                        ui,
                        &map.animations.color,
                        &map.groups,
                        selected_color_anim,
                        "color",
                        client,
                        |index| {
                            EditorAction::AddColorAnim(ActAddColorAnim {
                                base: ActAddRemColorAnim {
                                    anim: ColorAnimation {
                                        name: Default::default(),
                                        points: vec![
                                            AnimPointColor {
                                                time: Duration::ZERO,
                                                curve_type: AnimPointCurveType::Linear,
                                                value: Default::default(),
                                            },
                                            AnimPointColor {
                                                time: Duration::from_secs(1),
                                                curve_type: AnimPointCurveType::Linear,
                                                value: Default::default(),
                                            },
                                        ],
                                        synchronized: false,
                                    },
                                    index,
                                },
                            })
                        },
                        |index, anims, groups| EditorActionGroup {
                            actions: rem_color_anim(anims, groups, index),
                            identifier: Some(format!("color-anim-del-anim-at-{}", index)),
                        },
                        |index, anim| {
                            let mut anim: ColorAnimation = anim.clone().into();
                            anim.synchronized = !anim.synchronized;
                            EditorAction::ReplColorAnim(ActReplColorAnim {
                                base: ActAddRemColorAnim {
                                    index,
                                    anim ,
                                },
                            })
                        },
                    );
                    ui.end_row();
                    add_selector(
                        ui,
                        &map.animations.pos,
                        &map.groups,
                        selected_pos_anim,
                        "pos",
                        client,
                        |index| {
                            EditorAction::AddPosAnim(ActAddPosAnim {
                                base: ActAddRemPosAnim {
                                    anim: PosAnimation {
                                        name: Default::default(),
                                        points: vec![
                                            AnimPointPos {
                                                time: Duration::ZERO,
                                                curve_type: AnimPointCurveType::Linear,
                                                value: Default::default(),
                                            },
                                            AnimPointPos {
                                                time: Duration::from_secs(1),
                                                curve_type: AnimPointCurveType::Linear,
                                                value: Default::default(),
                                            },
                                        ],
                                        synchronized: false,
                                    },
                                    index,
                                },
                            })
                        },
                        |index, anims, groups| EditorActionGroup {
                            actions: rem_pos_anim(anims, groups, index),
                            identifier: Some(format!("pos-anim-del-anim-at-{}", index)),
                        },
                        |index, anim| {
                            let mut anim: PosAnimation = anim.clone().into();
                            anim.synchronized = !anim.synchronized;
                            EditorAction::ReplPosAnim(ActReplPosAnim {
                                base: ActAddRemPosAnim {
                                    index,
                                    anim ,
                                },
                            })
                        },
                    );

                    ui.end_row();
                    add_selector(
                        ui,
                        &map.animations.sound,
                        &map.groups,
                        selected_sound_anim,
                        "sound",
                        client,
                        |index| {
                            EditorAction::AddSoundAnim(ActAddSoundAnim {
                                base: ActAddRemSoundAnim {
                                    anim: SoundAnimation {
                                        name: Default::default(),
                                        points: vec![
                                            AnimPointSound {
                                                time: Duration::ZERO,
                                                curve_type: AnimPointCurveType::Linear,
                                                value: Default::default(),
                                            },
                                            AnimPointSound {
                                                time: Duration::from_secs(1),
                                                curve_type: AnimPointCurveType::Linear,
                                                value: Default::default(),
                                            },
                                        ],
                                        synchronized: false,
                                    },
                                    index,
                                },
                            })
                        },
                        |index, anims, groups| EditorActionGroup {
                            actions: rem_sound_anim(anims, groups, index),
                            identifier: Some(format!("sound-anim-del-anim-at-{}", index)),
                        },
                        |index, anim| {
                            let mut anim: SoundAnimation = anim.clone().into();
                            anim.synchronized = !anim.synchronized;
                            EditorAction::ReplSoundAnim(ActReplSoundAnim {
                                base: ActAddRemSoundAnim {
                                    index,
                                    anim ,
                                },
                            })
                        },
                    );

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
                        selected_point_channel_beziers: &mut anim
                            .user
                            .selected_point_channel_beziers,
                        hovered_point_channel_beziers: &mut anim.user.hovered_point_channel_beziers,
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
