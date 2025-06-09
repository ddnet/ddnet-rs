use std::collections::BTreeMap;

use egui::{Button, Color32, InnerResponse};
use map::map::groups::layers::design::{Sound, SoundShape};
use math::math::{
    length,
    vector::{dvec2, ffixed, nffixed, uffixed, ufvec2},
};
use time::Duration;
use ui_base::{
    types::{UiRenderPipe, UiState},
    utils::toggle_ui,
};

use crate::{
    actions::actions::{
        ActChangeSoundAttr, ActSoundLayerAddRemSounds, ActSoundLayerRemSounds, EditorAction,
    },
    hotkeys::{
        BindsPerEvent, EditorBindsFile, EditorHotkeyEvent, EditorHotkeyEventSharedTool,
        EditorHotkeyEventSoundBrush, EditorHotkeyEventSoundTool, EditorHotkeyEventTools,
    },
    map::{EditorAnimations, EditorLayer, EditorLayerUnionRefMut, EditorMapGroupsInterface},
    tools::{
        sound_layer::shared::SoundPointerDownPoint,
        tool::{ActiveTool, ActiveToolSounds},
    },
    ui::{group_and_layer::shared::animations_panel_open_warning, user_data::UserDataWithTab},
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    #[derive(Debug, PartialEq, Eq)]
    enum SoundAttrMode {
        Single,
        /// multiple sounds at once
        Multi,
        None,
    }

    let binds = &*pipe.user_data.hotkeys;
    let per_ev = &mut *pipe.user_data.cached_binds_per_event;

    let map = &mut pipe.user_data.editor_tab.map;
    let animations_panel_open = map.user.change_animations();
    let layer = map.groups.active_layer_mut();
    let mut attr_mode = SoundAttrMode::None;
    if let Some(EditorLayerUnionRefMut::Design {
        layer: EditorLayer::Sound(layer),
        group_index,
        layer_index,
        is_background,
        ..
    }) = layer
    {
        let (mut selected_sounds, point, pos_offset) = match &pipe.user_data.tools.active_tool {
            ActiveTool::Sounds(ActiveToolSounds::Brush) => {
                let brush = &mut pipe.user_data.tools.sounds.brush;
                let point = brush
                    .last_popup
                    .as_ref()
                    .map(|selection| selection.point)
                    .unwrap_or(SoundPointerDownPoint::Center);
                let mut res: BTreeMap<usize, &mut Sound> = Default::default();
                if let Some((selection, sound)) = brush.last_popup.as_mut().and_then(|selection| {
                    if selection.sound_index < layer.layer.sounds.len() {
                        Some((selection.sound_index, &mut selection.sound))
                    } else {
                        None
                    }
                }) {
                    res.insert(selection, sound);
                }
                (res, Some(point), None)
            }
            ActiveTool::Quads(_) | ActiveTool::Tiles(_) => {
                // ignore
                (Default::default(), None, None)
            }
        };

        if point.is_none() {
            return;
        }
        let point = point.unwrap();

        let sounds_count = selected_sounds.len();
        if sounds_count > 0 {
            attr_mode = if sounds_count == 1 {
                SoundAttrMode::Single
            } else {
                SoundAttrMode::Multi
            };
        }

        fn to_circle(sound: &mut Sound) {
            if let SoundShape::Rect { size } = sound.shape {
                sound.shape = SoundShape::Circle {
                    radius: uffixed::from_num(
                        length(&dvec2::new(size.x.to_num(), size.y.to_num())) / 2_f64.sqrt(),
                    ),
                };
            }
        }

        fn to_rect(sound: &mut Sound) {
            if let SoundShape::Circle { radius } = sound.shape {
                sound.shape = SoundShape::Rect {
                    size: ufvec2::new(radius.to_num(), radius.to_num()),
                };
            }
        }

        fn sound_attr_ui(
            ui: &mut egui::Ui,
            binds: &EditorBindsFile,
            per_ev: &mut Option<BindsPerEvent>,
            sounds_count: usize,
            point: SoundPointerDownPoint,
            sound: &mut Sound,
            // make a "move pos" instead of x, y directly
            pos_offset: Option<&mut dvec2>,
            can_change_pos_anim: bool,
            can_change_sound_anim: bool,
            animations_panel_open: bool,
            animations: &mut EditorAnimations,
            pointer_is_used: &mut bool,
        ) -> InnerResponse<bool> {
            let anim_pos = can_change_pos_anim
                .then_some(animations.user.active_anim_points.pos.as_mut())
                .flatten();
            let anim_sound = can_change_sound_anim
                .then_some(animations.user.active_anim_points.sound.as_mut())
                .flatten();

            let mut delete = false;
            egui::Grid::new("design group attr grid")
                .num_columns(2)
                .spacing([20.0, 4.0])
                .show(ui, |ui| {
                    if sounds_count > 1 {
                        ui.label(format!("Selected {sounds_count} sounds"));
                        ui.end_row();
                    }

                    if animations_panel_open
                        && ((can_change_pos_anim && sound.pos_anim.is_some())
                            || (can_change_sound_anim && sound.sound_anim.is_some()))
                    {
                        ui.colored_label(
                            Color32::RED,
                            "The animation panel is open,\n\
                            there are animation properties and sound properites now!",
                        )
                        .on_hover_ui(animations_panel_open_warning);
                        ui.end_row();

                        ui.heading("Animation propterties");
                        ui.end_row();

                        if let Some(pos) = anim_pos {
                            // x
                            ui.label("X");
                            let mut x = pos.value.x.to_num::<f64>();
                            ui.add(egui::DragValue::new(&mut x).update_while_editing(false));
                            pos.value.x = ffixed::from_num(x);
                            ui.end_row();
                            // y
                            ui.label("Y");
                            let mut y = pos.value.y.to_num::<f64>();
                            ui.add(egui::DragValue::new(&mut y).update_while_editing(false));
                            pos.value.y = ffixed::from_num(y);
                            ui.end_row();
                        }
                        if let Some(snd) = anim_sound {
                            // Volume
                            ui.label("Volume");
                            let mut v = snd.value.x.to_num::<f64>();
                            ui.add(
                                egui::DragValue::new(&mut v)
                                    .range(0.0..=1.0)
                                    .speed(0.1)
                                    .update_while_editing(false),
                            );
                            snd.value.x = nffixed::from_num(v);
                            ui.end_row();
                        }

                        ui.separator();
                        ui.separator();
                        ui.end_row();

                        ui.heading("Properties");
                        ui.end_row();
                    }

                    if let Some(pos_offset) = pos_offset {
                        // x
                        ui.label("Move x by");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut pos_offset.x).update_while_editing(false),
                            );
                            if ui.button("Move").clicked() {
                                sound.pos.x =
                                    ffixed::from_num(sound.pos.x.to_num::<f64>() + pos_offset.x);
                            }
                        });
                        ui.end_row();
                        // y
                        ui.label("Move y by");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut pos_offset.y).update_while_editing(false),
                            );
                            if ui.button("Move").clicked() {
                                sound.pos.y =
                                    ffixed::from_num(sound.pos.y.to_num::<f64>() + pos_offset.y);
                            }
                        });
                        ui.end_row();
                    } else {
                        // x
                        ui.label("X");
                        let mut x = sound.pos.x.to_num::<f64>();
                        ui.add(egui::DragValue::new(&mut x).update_while_editing(false));
                        sound.pos.x = ffixed::from_num(x);
                        ui.end_row();
                        // y
                        ui.label("Y");
                        let mut y = sound.pos.y.to_num::<f64>();
                        ui.add(egui::DragValue::new(&mut y).update_while_editing(false));
                        sound.pos.y = ffixed::from_num(y);
                        ui.end_row();
                    }

                    if matches!(point, SoundPointerDownPoint::Center) {
                        fn combobox_name(ty: &str, index: usize, name: &str) -> String {
                            name.is_empty()
                                .then_some(format!("{ty} #{index}"))
                                .unwrap_or_else(|| name.to_owned())
                        }
                        if can_change_pos_anim {
                            // pos anim
                            ui.label("Pos anim");
                            let res = egui::ComboBox::new("sound-select-pos-anim".to_string(), "")
                                .selected_text(
                                    animations
                                        .pos
                                        .get(sound.pos_anim.unwrap_or(usize::MAX))
                                        .map(|anim| {
                                            combobox_name(
                                                "pos",
                                                sound.pos_anim.unwrap(),
                                                &anim.def.name.clone(),
                                            )
                                        })
                                        .unwrap_or_else(|| "None".to_string()),
                                )
                                .show_ui(ui, |ui| {
                                    if ui.button("None").clicked() {
                                        sound.pos_anim = None;
                                    }
                                    for (a, anim) in animations.pos.iter().enumerate() {
                                        if ui
                                            .button(combobox_name("pos", a, &anim.def.name))
                                            .clicked()
                                        {
                                            sound.pos_anim = Some(a);
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
                            ui.label("Pos anim time offset");
                            let mut millis = sound.pos_anim_offset.whole_milliseconds() as i64;
                            if ui
                                .add(egui::DragValue::new(&mut millis).update_while_editing(false))
                                .changed()
                            {
                                sound.pos_anim_offset = Duration::milliseconds(millis);
                            }
                            ui.end_row();
                        }
                        if can_change_sound_anim {
                            // sound anim
                            ui.label("Sound anim");
                            let res =
                                egui::ComboBox::new("sound-select-sound-anim".to_string(), "")
                                    .selected_text(
                                        animations
                                            .sound
                                            .get(sound.sound_anim.unwrap_or(usize::MAX))
                                            .map(|anim| {
                                                combobox_name(
                                                    "sound",
                                                    sound.sound_anim.unwrap(),
                                                    &anim.def.name.clone(),
                                                )
                                            })
                                            .unwrap_or_else(|| "None".to_string()),
                                    )
                                    .show_ui(ui, |ui| {
                                        if ui.button("None").clicked() {
                                            sound.sound_anim = None;
                                        }
                                        for (a, anim) in animations.sound.iter().enumerate() {
                                            if ui
                                                .button(combobox_name("sound", a, &anim.def.name))
                                                .clicked()
                                            {
                                                sound.sound_anim = Some(a);
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

                            // sound time offset
                            ui.label("Sound anim time offset");
                            let mut millis = sound.sound_anim_offset.whole_milliseconds() as i64;
                            if ui
                                .add(egui::DragValue::new(&mut millis).update_while_editing(false))
                                .changed()
                            {
                                sound.sound_anim_offset = Duration::milliseconds(millis);
                            }
                            ui.end_row();
                        }

                        ui.end_row();

                        // sound shape
                        ui.label("Shape");
                        let res = egui::ComboBox::new("sound-select-shape".to_string(), "")
                            .selected_text(if matches!(sound.shape, SoundShape::Circle { .. }) {
                                "circle"
                            } else {
                                "rect"
                            })
                            .show_ui(ui, |ui| {
                                if ui.button("circle").clicked() {
                                    to_circle(sound);
                                }
                                if ui.button("rect").clicked() {
                                    to_rect(sound);
                                }
                            })
                            .response
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
                                                EditorHotkeyEventTools::Sound(
                                                    EditorHotkeyEventSoundTool::Brush(
                                                        EditorHotkeyEventSoundBrush::ToggleShape
                                                    )
                                                )
                                            ),
                                        )
                                    ),
                                );
                            });
                        ui.end_row();

                        *pointer_is_used |= {
                            let intersected = ui.input(|i| {
                                if i.pointer.primary_down() {
                                    Some((
                                        !res.rect.intersects({
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
                            intersected.is_some_and(|(outside, _)| !outside)
                        };

                        // loop
                        ui.label("Loop");
                        toggle_ui(ui, &mut sound.looped);
                        ui.end_row();

                        // panning
                        ui.label("Panning");
                        toggle_ui(ui, &mut sound.panning);
                        ui.end_row();

                        // starting delay
                        ui.label("Start delay (ms)");
                        let mut millis = sound.time_delay.as_millis() as u64;
                        if ui
                            .add(egui::DragValue::new(&mut millis).update_while_editing(false))
                            .changed()
                        {
                            sound.time_delay = std::time::Duration::from_millis(millis);
                        }
                        ui.end_row();

                        // sound falloff
                        ui.label("Falloff");
                        let mut falloff = sound.falloff.to_num::<f64>();
                        if ui
                            .add(
                                egui::DragValue::new(&mut falloff)
                                    .update_while_editing(false)
                                    .speed(0.05),
                            )
                            .changed()
                        {
                            sound.falloff = nffixed::from_num(falloff.clamp(0.0, 1.0));
                        }
                        ui.end_row();

                        // sound size
                        match &mut sound.shape {
                            SoundShape::Rect { size } => {
                                ui.label("Width");
                                let mut x = size.x.to_num::<f64>();
                                if ui
                                    .add(egui::DragValue::new(&mut x).update_while_editing(false))
                                    .changed()
                                {
                                    size.x = uffixed::from_num(x.clamp(0.0, f64::MAX));
                                }
                                ui.end_row();
                                ui.label("Height");
                                let mut y = size.y.to_num::<f64>();
                                if ui
                                    .add(egui::DragValue::new(&mut y).update_while_editing(false))
                                    .changed()
                                {
                                    size.y = uffixed::from_num(y.clamp(0.0, f64::MAX));
                                }
                                ui.end_row();
                            }
                            SoundShape::Circle { radius } => {
                                ui.label("Radius");
                                let mut r = radius.to_num::<f64>();
                                if ui
                                    .add(egui::DragValue::new(&mut r).update_while_editing(false))
                                    .changed()
                                {
                                    *radius = uffixed::from_num(r.clamp(0.0, f64::MAX));
                                }
                                ui.end_row();
                            }
                        }
                    }

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
                                        &EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Shared(
                                            EditorHotkeyEventSharedTool::DeleteQuadOrSound,
                                        )),
                                    )
                                ),
                            );
                        })
                        .clicked()
                    {
                        delete = true;
                    }

                    delete
                })
        }

        let window_res = match attr_mode {
            SoundAttrMode::Single => {
                let (index, sound) = selected_sounds.pop_first().unwrap();
                let sound_cmp = *sound;

                let window = egui::Window::new("Design Sound Attributes")
                    .resizable(false)
                    .collapsible(false);

                let window_res = window.show(ui.ctx(), |ui| {
                    sound_attr_ui(
                        ui,
                        binds,
                        per_ev,
                        sounds_count,
                        point,
                        sound,
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

                if *sound != sound_cmp {
                    let layer_sound = &layer.layer.sounds[index];
                    pipe.user_data.editor_tab.client.execute(
                        EditorAction::ChangeSoundAttr(ActChangeSoundAttr {
                            is_background,
                            group_index,
                            layer_index,
                            old_attr: *layer_sound,
                            new_attr: *sound,

                            index,
                        }),
                        Some(&format!(
                            "change-sound-attr-{is_background}-{group_index}-{layer_index}-{index}"
                        )),
                    );
                } else if delete {
                    pipe.user_data.editor_tab.client.execute(
                        EditorAction::SoundLayerRemSounds(ActSoundLayerRemSounds {
                            base: ActSoundLayerAddRemSounds {
                                is_background,
                                group_index,
                                layer_index,
                                index,
                                sounds: vec![*sound],
                            },
                        }),
                        Some(&format!(
                            "sound-rem-design-{is_background}-{group_index}-{layer_index}-{index}"
                        )),
                    );
                }

                window_res
            }
            SoundAttrMode::Multi => {
                let (_, mut sound) = selected_sounds
                    .iter_mut()
                    .peekable()
                    .next()
                    .map(|(i, q)| (*i, **q))
                    .unwrap();
                let sound_cmp = sound;

                let mut selected_sounds: Vec<_> = selected_sounds.into_iter().collect();
                let can_change_pos_anim = selected_sounds
                    .windows(2)
                    .all(|window| window[0].1.pos_anim == window[1].1.pos_anim);
                let can_change_sound_anim = selected_sounds
                    .windows(2)
                    .all(|window| window[0].1.sound_anim == window[1].1.sound_anim);

                let window = egui::Window::new("Design Sounds Attributes")
                    .resizable(false)
                    .collapsible(false);

                let window_res = window.show(ui.ctx(), |ui| {
                    sound_attr_ui(
                        ui,
                        binds,
                        per_ev,
                        sounds_count,
                        point,
                        &mut sound,
                        pos_offset,
                        can_change_pos_anim,
                        can_change_sound_anim,
                        animations_panel_open,
                        &mut map.animations,
                        pipe.user_data.pointer_is_used,
                    )
                });

                let delete = window_res
                    .as_ref()
                    .is_some_and(|r| r.inner.as_ref().is_some_and(|r| r.inner));

                if sound != sound_cmp {
                    let prop_sound = sound;
                    // copy the changed data into all selected sounds
                    selected_sounds.iter_mut().for_each(|(index, sound)| {
                        let index = *index;
                        let layer_sound = &layer.layer.sounds[index];
                        // move points by diff
                            let diff = prop_sound.pos - sound_cmp.pos;

                            sound.pos += diff;

                        // apply new anims if changed, for the time offset do a difference instead
                        if can_change_pos_anim {
                            let diff = prop_sound.pos_anim != sound_cmp.pos_anim;

                            if diff {
                                sound.pos_anim = prop_sound.pos_anim;
                            }
                            let diff = prop_sound.pos_anim_offset - sound_cmp.pos_anim_offset;

                            sound.pos_anim_offset += diff;
                        }
                        if can_change_sound_anim {
                            let diff = prop_sound.sound_anim != sound_cmp.sound_anim;

                            if diff {
                                sound.sound_anim = prop_sound.sound_anim;
                            }
                            let diff = prop_sound.sound_anim_offset - sound_cmp.sound_anim_offset;

                            sound.sound_anim_offset += diff;
                        }

                        // generate events for all selected sounds                        
                            pipe.user_data.editor_tab.client.execute(
                                EditorAction::ChangeSoundAttr(ActChangeSoundAttr {
                                    is_background,
                                    group_index,
                                    layer_index,
                                    old_attr: *layer_sound,
                                    new_attr: **sound,

                                    index,
                                }),
                                Some(&format!(
                                    "change-sound-attr-{is_background}-{group_index}-{layer_index}-{index}"
                                )),
                            );
                    });
                } else if delete {
                    // rewrite the sound indices, since they get invalid every time a sound is deleted.
                    for i in 0..selected_sounds.len() {
                        let (delete_index, _) = selected_sounds[i];
                        for (index, _) in selected_sounds.iter_mut().skip(i + 1) {
                            if *index > delete_index {
                                *index = index.saturating_sub(1);
                            }
                        }
                    }

                    for (index, sound) in selected_sounds {
                        pipe.user_data.editor_tab.client.execute(
                            EditorAction::SoundLayerRemSounds(ActSoundLayerRemSounds {
                                base: ActSoundLayerAddRemSounds {
                                    is_background,
                                    group_index,
                                    layer_index,
                                    index,
                                    sounds: vec![*sound],
                                },
                            }),
                            Some(&format!(
                                "sound-rem-design-{is_background}-\
                                {group_index}-{layer_index}-{index}"
                            )),
                        );
                    }
                }

                window_res
            }
            SoundAttrMode::None => {
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
            if intersected.is_some_and(|(outside, clicked)| outside && clicked) {
                match &pipe.user_data.tools.active_tool {
                    ActiveTool::Sounds(ActiveToolSounds::Brush) => {
                        pipe.user_data.tools.sounds.brush.last_popup = None;
                    }
                    ActiveTool::Quads(_) | ActiveTool::Tiles(_) => {
                        // ignore
                    }
                }
            }
            intersected.is_some_and(|(outside, _)| !outside)
        } else {
            false
        };

        // additional to the visible ui there is also some handling for hotkeys
        let mut selected_sounds = match &pipe.user_data.tools.active_tool {
            ActiveTool::Sounds(ActiveToolSounds::Brush) => {
                let brush = &mut pipe.user_data.tools.sounds.brush;
                let mut res: BTreeMap<usize, &mut Sound> = Default::default();
                if let Some((selection, sound)) =
                    brush.last_selection.as_mut().and_then(|selection| {
                        if selection.sound_index < layer.layer.sounds.len() {
                            Some((selection.sound_index, &mut selection.sound))
                        } else {
                            None
                        }
                    })
                {
                    res.insert(selection, sound);
                }
                res
            }
            ActiveTool::Quads(_) | ActiveTool::Tiles(_) => {
                // ignore
                Default::default()
            }
        };
        let change_shape_sound =
            pipe.user_data
                .cur_hotkey_events
                .remove(&EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Sound(
                    EditorHotkeyEventSoundTool::Brush(EditorHotkeyEventSoundBrush::ToggleShape),
                )));
        if change_shape_sound {
            for (&index, s) in selected_sounds.iter_mut() {
                let mut new_snd = **s;
                match &new_snd.shape {
                    SoundShape::Rect { .. } => to_circle(&mut new_snd),
                    SoundShape::Circle { .. } => to_rect(&mut new_snd),
                }
                pipe.user_data.editor_tab.client.execute(
                    EditorAction::ChangeSoundAttr(ActChangeSoundAttr {
                        is_background,
                        group_index,
                        layer_index,
                        old_attr: **s,
                        new_attr: new_snd,

                        index,
                    }),
                    Some(&format!(
                        "change-sound-attr-{is_background}-{group_index}-{layer_index}-{index}"
                    )),
                );
            }
        }
        if !selected_sounds.is_empty() {
            let delete_sounds = pipe
                .user_data
                .cur_hotkey_events
                .remove(&EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Shared(
                    EditorHotkeyEventSharedTool::DeleteQuadOrSound,
                )));
            if delete_sounds {
                for (&index, s) in selected_sounds.iter_mut() {
                    pipe.user_data.editor_tab.client.execute(
                        EditorAction::SoundLayerRemSounds(ActSoundLayerRemSounds {
                            base: ActSoundLayerAddRemSounds {
                                is_background,
                                group_index,
                                layer_index,
                                index,
                                sounds: vec![**s],
                            },
                        }),
                        Some(&format!(
                            "delete-sound-{is_background}-{group_index}-{layer_index}-{index}"
                        )),
                    );
                }
            }
        }
    }
}
