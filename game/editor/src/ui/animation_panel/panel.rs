use std::{fmt::Debug, ops::IndexMut, time::Duration};

use client_render_base::map::render_tools::RenderTools;
use egui::UiBuilder;
use egui_timeline::point::{Point, PointGroup};
use fixed::traits::{FromFixed, ToFixed};
use map::{
    map::animations::{
        AnimBase, AnimPoint, AnimPointColor, AnimPointCurveType, AnimPointPos, AnimPointSound,
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
    hotkeys::{EditorHotkeyEvent, EditorHotkeyEventTimeline},
    map::{
        EditorActiveAnimationProps, EditorAnimationProps, EditorGroups, EditorLayer,
        EditorLayerUnionRef, EditorMapGroupsInterface,
    },
    tools::tool::{ActiveTool, ActiveToolQuads, ActiveToolSounds},
    ui::user_data::UserDataWithTab,
};

const COLOR_GROUP_NAME: &str = "color";
const POS_GROUP_NAME: &str = "pos";
const SOUND_GROUP_NAME: &str = "sound";

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    let map = &mut pipe.user_data.editor_tab.map;
    if !map.user.ui_values.animations_panel_open {
        // make sure to clear unused anims
        map.animations.user.active_anims = Default::default();
        map.animations.user.active_anim_points = Default::default();

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
        let mut selected_sound_anim_selection;
        //let mut selected_sound_anim_selection;
        let (selected_color_anim, selected_pos_anim, selected_sound_anim) = {
            let (can_change_pos_anim, can_change_color_anim, can_change_sound_anim) = if let (
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
                    tools.quads.selection.range.as_mut()
                } else if matches!(tools.active_tool, ActiveTool::Quads(ActiveToolQuads::Brush)) {
                    tools.quads.brush.last_selection.as_mut()
                } else {
                    None
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
                    None,
                )
            } else if let (
                Some(EditorLayerUnionRef::Design {
                    layer: EditorLayer::Sound(_),
                    ..
                }),
                ActiveTool::Sounds(ActiveToolSounds::Brush),
                Some(range),
                None,
            ) = (
                &active_layer,
                &tools.active_tool,
                if matches!(
                    tools.active_tool,
                    ActiveTool::Sounds(ActiveToolSounds::Brush)
                ) {
                    tools.sounds.brush.last_selection.as_mut()
                } else {
                    None
                },
                map.user.options.no_animations_with_properties.then_some(()),
            ) {
                let range: Vec<_> = vec![(range.sound_index, &mut range.sound)];

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
                    None,
                    if range
                        .windows(2)
                        .all(|window| window[0].1.sound_anim == window[1].1.sound_anim)
                        && !range.is_empty()
                    {
                        range[0].1.sound_anim
                    } else {
                        None
                    },
                )
            } else {
                (None, None, None)
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
                if let Some(anim) = can_change_sound_anim {
                    selected_sound_anim_selection = Some(anim);
                    &mut selected_sound_anim_selection
                } else {
                    &mut map.animations.user.selected_sound_anim
                },
            )
        };

        Some(panel.show_inside(ui, |ui| {
            fn add_selector<A: Point + DeserializeOwned + PartialOrd + Clone, S>(
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
                sync: S,
                active_anim: &mut Option<(usize, AnimBase<A>, EditorActiveAnimationProps)>,
                active_anim_point: &mut Option<A>,
            ) where
                S: FnOnce(usize, &AnimBaseSkeleton<EditorAnimationProps, A>) -> EditorAction,
            {
                ui.label(format!("{}:", name));
                // selection of animation
                if ui.button("\u{f060}").clicked() {
                    *index = index.map(|i| i.checked_sub(1)).flatten();
                    *active_anim = None;
                    *active_anim_point = None;
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
                    *active_anim = None;
                    *active_anim_point = None;
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

                        *active_anim = None;
                        *active_anim_point = None;
                    }

                    // Whether to sync the current animation to server time
                    let mut is_sync = anims[*index].def.synchronized;
                    if ui.checkbox(&mut is_sync, "Synchronize").changed() {
                        client.execute(sync(*index, &anims[*index]), None);
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
                                base: ActAddRemColorAnim { index, anim },
                            })
                        },
                        &mut map.animations.user.active_anims.color,
                        &mut map.animations.user.active_anim_points.color,
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
                                base: ActAddRemPosAnim { index, anim },
                            })
                        },
                        &mut map.animations.user.active_anims.pos,
                        &mut map.animations.user.active_anim_points.pos,
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
                                base: ActAddRemSoundAnim { index, anim },
                            })
                        },
                        &mut map.animations.user.active_anims.sound,
                        &mut map.animations.user.active_anim_points.sound,
                    );

                    ui.end_row();
                });

            // init animations if not done yet
            fn try_init_group<'a, F, T, const CHANNELS: usize>(
                anims: &'a [AnimBaseSkeleton<EditorAnimationProps, AnimPoint<T, CHANNELS>>],
                anim: &'a mut Option<(
                    usize,
                    AnimBase<AnimPoint<T, CHANNELS>>,
                    EditorActiveAnimationProps,
                )>,
                anim_point: &'a mut Option<AnimPoint<T, CHANNELS>>,
                index: Option<usize>,
                time: &Duration,
            ) where
                AnimBase<AnimPoint<T, CHANNELS>>:
                    From<AnimBaseSkeleton<EditorAnimationProps, AnimPoint<T, CHANNELS>>>,
                AnimPoint<T, CHANNELS>: Point + DeserializeOwned + PartialOrd + Clone,
                F: Copy + FromFixed + ToFixed,
                T: Debug + Copy + Default + IndexMut<usize, Output = F>,
            {
                if let Some((index, copy_anim)) = index.and_then(|i| anims.get(i).map(|a| (i, a))) {
                    if anim
                        .as_ref()
                        .is_none_or(|(cur_index, _, _)| *cur_index != index)
                    {
                        *anim = Some((index, copy_anim.clone().into(), Default::default()));
                    }

                    let Some((_, anim, _)) = anim else {
                        panic!("Should have been initialized directly before this check");
                    };
                    if anim_point.is_none() {
                        let value = RenderTools::render_eval_anim(
                            anim.points.as_slice(),
                            time::Duration::try_from(*time).unwrap(),
                        );
                        *anim_point = Some(AnimPoint {
                            time: Duration::ZERO,
                            curve_type: AnimPointCurveType::Linear,
                            value,
                        });
                    }
                }
            }
            try_init_group(
                &map.animations.pos,
                &mut map.animations.user.active_anims.pos,
                &mut map.animations.user.active_anim_points.pos,
                *selected_pos_anim,
                &map.user.ui_values.timeline.time(),
            );
            try_init_group(
                &map.animations.color,
                &mut map.animations.user.active_anims.color,
                &mut map.animations.user.active_anim_points.color,
                *selected_color_anim,
                &map.user.ui_values.timeline.time(),
            );
            try_init_group(
                &map.animations.sound,
                &mut map.animations.user.active_anims.sound,
                &mut map.animations.user.active_anim_points.sound,
                *selected_sound_anim,
                &map.user.ui_values.timeline.time(),
            );

            let mut groups: Vec<PointGroup<'_>> = Default::default();

            fn add_group<'a, A: Point + DeserializeOwned + PartialOrd + Clone>(
                groups: &mut Vec<PointGroup<'a>>,
                anim: &'a mut Option<(usize, AnimBase<A>, EditorActiveAnimationProps)>,
                index: Option<usize>,
                name: &'a str,
            ) {
                if let Some((anim, props)) = anim
                    .as_mut()
                    .and_then(|(i, a, props)| (Some(*i) == index).then_some((a, props)))
                {
                    let points = anim
                        .points
                        .iter_mut()
                        .map(|val| val as &mut dyn Point)
                        .collect::<Vec<_>>();
                    groups.push(PointGroup {
                        name,
                        points,
                        selected_points: &mut props.selected_points,
                        hovered_point: &mut props.hovered_point,
                        selected_point_channels: &mut props.selected_point_channels,
                        hovered_point_channel: &mut props.hovered_point_channels,
                        selected_point_channel_beziers: &mut props.selected_point_channel_beziers,
                        hovered_point_channel_beziers: &mut props.hovered_point_channel_beziers,
                    });
                }
            }
            add_group(
                &mut groups,
                &mut map.animations.user.active_anims.color,
                *selected_color_anim,
                COLOR_GROUP_NAME,
            );
            add_group(
                &mut groups,
                &mut map.animations.user.active_anims.pos,
                *selected_pos_anim,
                POS_GROUP_NAME,
            );
            add_group(
                &mut groups,
                &mut map.animations.user.active_anims.sound,
                *selected_sound_anim,
                SOUND_GROUP_NAME,
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

            // the upper implementation should insert a new animation point
            // (or replace an existing one) at the current position,
            // if the implementation supports adding frame point data outside of this panel
            if pipe
                .user_data
                .cur_hotkey_events
                .remove(&EditorHotkeyEvent::Timeline(
                    EditorHotkeyEventTimeline::InsertPoint,
                ))
            {
                handle_point_insert(pipe);
            }
            if res.inner.inner.points_changed {
                // generate actions for all changed points
                handle_points_changed(pipe);
            }
            if let Some((group_name, point_index)) = res.inner.inner.point_deleted {
                handle_point_delete(pipe, &group_name, point_index);
            }
        }
    }
}

fn handle_anim_time_change(pipe: &mut UiRenderPipe<UserDataWithTab>) {
    let map = &mut pipe.user_data.editor_tab.map;
    // reset active points
    map.animations.user.active_anim_points = Default::default();
}

fn handle_point_insert(pipe: &mut UiRenderPipe<UserDataWithTab>) {
    let map = &mut pipe.user_data.editor_tab.map;
    let anims = &mut map.animations.user.active_anims;
    let anim_points = &map.animations.user.active_anim_points;

    let cur_time = map.user.ui_values.timeline.time();

    fn add_or_insert<P: Clone + DeserializeOwned, const CHANNELS: usize>(
        cur_time: Duration,
        anim: &mut AnimBase<AnimPoint<P, CHANNELS>>,
        insert_repl_point: &AnimPoint<P, CHANNELS>,
        props: &mut EditorActiveAnimationProps,
    ) {
        enum ReplOrInsert {
            Repl(usize),
            Insert(usize),
        }

        let index =
            anim.points
                .iter()
                .enumerate()
                .find_map(|(p, point)| match point.time.cmp(&cur_time) {
                    std::cmp::Ordering::Less => None,
                    std::cmp::Ordering::Equal => Some(ReplOrInsert::Repl(p)),
                    std::cmp::Ordering::Greater => Some(ReplOrInsert::Insert(p)),
                });

        let mut insert_repl_point = insert_repl_point.clone();
        insert_repl_point.time = cur_time;

        match index {
            Some(mode) => match mode {
                ReplOrInsert::Repl(index) => {
                    anim.points[index] = insert_repl_point;
                }
                ReplOrInsert::Insert(index) => {
                    anim.points.insert(index, insert_repl_point);

                    fn new_point(p: usize, point_index: usize) -> usize {
                        match p.cmp(&point_index) {
                            std::cmp::Ordering::Less => p,
                            std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => p + 1,
                        }
                    }
                    props.hovered_point = props.hovered_point.map(|p| new_point(p, index));
                    props.hovered_point_channels = props
                        .hovered_point_channels
                        .drain()
                        .map(|(p, rest)| (new_point(p, index), rest))
                        .collect();
                    props.hovered_point_channel_beziers = props
                        .hovered_point_channel_beziers
                        .drain()
                        .map(|(p, rest)| (new_point(p, index), rest))
                        .collect();
                    props.selected_points = props
                        .selected_points
                        .drain()
                        .map(|p| new_point(p, index))
                        .collect();
                    props.selected_point_channels = props
                        .selected_point_channels
                        .drain()
                        .map(|(p, rest)| (new_point(p, index), rest))
                        .collect();
                    props.selected_point_channel_beziers = props
                        .selected_point_channel_beziers
                        .drain()
                        .map(|(p, rest)| (new_point(p, index), rest))
                        .collect();
                }
            },
            None => {
                // push new point
                anim.points.push(insert_repl_point);
            }
        }
    }

    if let Some(((index, anim, props), anim_point)) =
        anims.pos.as_mut().zip(anim_points.pos.as_ref())
    {
        add_or_insert(cur_time, anim, anim_point, props);
        pipe.user_data.editor_tab.client.execute(
            EditorAction::ReplPosAnim(ActReplPosAnim {
                base: ActAddRemPosAnim {
                    index: *index,
                    anim: anim.clone(),
                },
            }),
            Some(&format!("pos-anim-repl-anim-{}", index)),
        );
    }
    if let Some(((index, anim, props), anim_point)) =
        anims.color.as_mut().zip(anim_points.color.as_ref())
    {
        add_or_insert(cur_time, anim, anim_point, props);
        pipe.user_data.editor_tab.client.execute(
            EditorAction::ReplColorAnim(ActReplColorAnim {
                base: ActAddRemColorAnim {
                    index: *index,
                    anim: anim.clone(),
                },
            }),
            Some(&format!("color-anim-repl-anim-{}", index)),
        );
    }
    if let Some(((index, anim, props), anim_point)) =
        anims.sound.as_mut().zip(anim_points.sound.as_ref())
    {
        add_or_insert(cur_time, anim, anim_point, props);
        pipe.user_data.editor_tab.client.execute(
            EditorAction::ReplSoundAnim(ActReplSoundAnim {
                base: ActAddRemSoundAnim {
                    index: *index,
                    anim: anim.clone(),
                },
            }),
            Some(&format!("sound-anim-repl-anim-{}", index)),
        );
    }
}

fn handle_points_changed(pipe: &mut UiRenderPipe<UserDataWithTab>) {
    let tab = &*pipe.user_data.editor_tab;

    fn check_anim<AP: DeserializeOwned + PartialOrd + Clone>(
        client: &EditorClient,
        anim: &Option<(usize, AnimBase<AP>, EditorActiveAnimationProps)>,
        prefix: &str,
        gen_action: &dyn Fn(usize, &AnimBase<AP>) -> EditorAction,
    ) {
        if let Some((index, anim, props)) = anim {
            if !props.selected_point_channels.is_empty() || !props.selected_points.is_empty() {
                client.execute(
                    gen_action(*index, anim),
                    Some(&format!("{}-anim-repl-anim-{}", prefix, index)),
                );
            }
        }
    }
    check_anim(
        &tab.client,
        &tab.map.animations.user.active_anims.pos,
        "pos",
        &|index, anim| {
            EditorAction::ReplPosAnim(ActReplPosAnim {
                base: ActAddRemPosAnim {
                    index,
                    anim: anim.clone(),
                },
            })
        },
    );
    check_anim(
        &tab.client,
        &tab.map.animations.user.active_anims.color,
        "color",
        &|index, anim| {
            EditorAction::ReplColorAnim(ActReplColorAnim {
                base: ActAddRemColorAnim {
                    index,
                    anim: anim.clone(),
                },
            })
        },
    );
    check_anim(
        &tab.client,
        &tab.map.animations.user.active_anims.sound,
        "sound",
        &|index, anim| {
            EditorAction::ReplSoundAnim(ActReplSoundAnim {
                base: ActAddRemSoundAnim {
                    index,
                    anim: anim.clone(),
                },
            })
        },
    );
}

fn handle_point_delete(
    pipe: &mut UiRenderPipe<UserDataWithTab>,
    group_name: &str,
    point_index: usize,
) {
    let anims = &mut pipe.user_data.editor_tab.map.animations.user.active_anims;

    fn delete_point_anim<AP: DeserializeOwned + PartialOrd + Clone>(
        client: &EditorClient,
        anim: &mut Option<(usize, AnimBase<AP>, EditorActiveAnimationProps)>,
        gen_action: &dyn Fn(usize, &AnimBase<AP>) -> EditorAction,
        group_name: &str,
        point_index: usize,
        expected_group_name: &str,
    ) {
        if let Some((index, anim, props)) = (group_name == expected_group_name)
            .then_some(anim.as_mut())
            .flatten()
        {
            if point_index < anim.points.len() {
                anim.points.remove(point_index);
                client.execute(
                    gen_action(*index, anim),
                    Some(&format!("{}-anim-repl-anim-{}", group_name, index)),
                );

                fn new_point(p: usize, point_index: usize) -> Option<usize> {
                    match p.cmp(&point_index) {
                        std::cmp::Ordering::Less => Some(p),
                        std::cmp::Ordering::Equal => None,
                        std::cmp::Ordering::Greater => p.checked_sub(1),
                    }
                }
                props.hovered_point = props.hovered_point.and_then(|p| new_point(p, point_index));
                props.hovered_point_channels = props
                    .hovered_point_channels
                    .drain()
                    .filter_map(|(p, rest)| new_point(p, point_index).map(|p| (p, rest)))
                    .collect();
                props.hovered_point_channel_beziers = props
                    .hovered_point_channel_beziers
                    .drain()
                    .filter_map(|(p, rest)| new_point(p, point_index).map(|p| (p, rest)))
                    .collect();
                props.selected_points = props
                    .selected_points
                    .drain()
                    .filter_map(|p| new_point(p, point_index))
                    .collect();
                props.selected_point_channels = props
                    .selected_point_channels
                    .drain()
                    .filter_map(|(p, rest)| new_point(p, point_index).map(|p| (p, rest)))
                    .collect();
                props.selected_point_channel_beziers = props
                    .selected_point_channel_beziers
                    .drain()
                    .filter_map(|(p, rest)| new_point(p, point_index).map(|p| (p, rest)))
                    .collect();
            }
        }
    }
    delete_point_anim(
        &pipe.user_data.editor_tab.client,
        &mut anims.color,
        &|index, anim| {
            EditorAction::ReplColorAnim(ActReplColorAnim {
                base: ActAddRemColorAnim {
                    index,
                    anim: anim.clone(),
                },
            })
        },
        group_name,
        point_index,
        COLOR_GROUP_NAME,
    );
    delete_point_anim(
        &pipe.user_data.editor_tab.client,
        &mut anims.pos,
        &|index, anim| {
            EditorAction::ReplPosAnim(ActReplPosAnim {
                base: ActAddRemPosAnim {
                    index,
                    anim: anim.clone(),
                },
            })
        },
        group_name,
        point_index,
        POS_GROUP_NAME,
    );
    delete_point_anim(
        &pipe.user_data.editor_tab.client,
        &mut anims.sound,
        &|index, anim| {
            EditorAction::ReplSoundAnim(ActReplSoundAnim {
                base: ActAddRemSoundAnim {
                    index,
                    anim: anim.clone(),
                },
            })
        },
        group_name,
        point_index,
        SOUND_GROUP_NAME,
    );
}
