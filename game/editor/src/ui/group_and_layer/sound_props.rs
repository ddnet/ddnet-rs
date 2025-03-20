use std::collections::BTreeMap;

use egui::{Color32, InnerResponse};
use map::map::{
    animations::AnimPointPos,
    groups::layers::design::{Sound, SoundShape},
};
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
    actions::actions::{ActChangeSoundAttr, EditorAction},
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

    let map = &mut pipe.user_data.editor_tab.map;
    let animations_panel_open =
        map.user.ui_values.animations_panel_open && !map.user.options.no_animations_with_properties;
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
        let (mut selected_sounds, point, pos_offset, pos_anim) =
            match &pipe.user_data.tools.active_tool {
                ActiveTool::Sounds(ActiveToolSounds::Brush) => {
                    let brush = &mut pipe.user_data.tools.sounds.brush;
                    let point = brush
                        .last_popup
                        .as_ref()
                        .map(|selection| selection.point)
                        .unwrap_or(SoundPointerDownPoint::Center);
                    let mut res: BTreeMap<usize, &mut Sound> = Default::default();
                    if let Some((selection, sound)) =
                        brush.last_popup.as_mut().and_then(|selection| {
                            if selection.sound_index < layer.layer.sounds.len() {
                                Some((selection.sound_index, &mut selection.sound))
                            } else {
                                None
                            }
                        })
                    {
                        res.insert(selection, sound);
                    }
                    (res, Some(point), None, None)
                }
                ActiveTool::Quads(_) | ActiveTool::Tiles(_) => {
                    // ignore
                    (Default::default(), None, None, None)
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

        fn sound_attr_ui(
            ui: &mut egui::Ui,
            sounds_count: usize,
            point: SoundPointerDownPoint,
            sound: &mut Sound,
            // make a "move pos" instead of x, y directly
            pos_offset: Option<&mut dvec2>,
            mut anim_pos: Option<&mut AnimPointPos>,
            can_change_pos_anim: bool,
            can_change_sound_anim: bool,
            animations_panel_open: bool,
            animations: &EditorAnimations,
            pointer_is_used: &mut bool,
        ) -> InnerResponse<()> {
            egui::Grid::new("design group attr grid")
                .num_columns(2)
                .spacing([20.0, 4.0])
                .show(ui, |ui| {
                    if sounds_count > 1 {
                        ui.label(format!("selected {sounds_count} sounds"));
                        ui.end_row();
                    }
                    if !animations_panel_open || (can_change_pos_anim && sound.pos_anim.is_some()) {
                        if let Some(pos_offset) = pos_offset {
                            // x
                            ui.label("move x by");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::DragValue::new(&mut pos_offset.x)
                                        .update_while_editing(false),
                                );
                                if ui.button("move").clicked() {
                                    if let Some(pos_anim) = anim_pos.as_deref_mut() {
                                        pos_anim.value.x = ffixed::from_num(pos_offset.x);
                                    } else {
                                        sound.pos.x = ffixed::from_num(
                                            sound.pos.x.to_num::<f64>() + pos_offset.x,
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
                                        sound.pos.y = ffixed::from_num(
                                            sound.pos.y.to_num::<f64>() + pos_offset.y,
                                        );
                                    }
                                }
                            });
                            ui.end_row();
                        } else {
                            // x
                            ui.label("x");
                            let mut x = sound.pos.x.to_num::<f64>();
                            ui.add(egui::DragValue::new(&mut x).update_while_editing(false));
                            sound.pos.x = ffixed::from_num(x);
                            ui.end_row();
                            // y
                            ui.label("y");
                            let mut y = sound.pos.y.to_num::<f64>();
                            ui.add(egui::DragValue::new(&mut y).update_while_editing(false));
                            sound.pos.y = ffixed::from_num(y);
                            ui.end_row();
                        }
                    }

                    if matches!(point, SoundPointerDownPoint::Center) && !animations_panel_open {
                        fn combobox_name(ty: &str, index: usize, name: &str) -> String {
                            name.is_empty()
                                .then_some(format!("{ty} #{}", index))
                                .unwrap_or_else(|| name.to_owned())
                        }
                        if can_change_pos_anim {
                            // pos anim
                            ui.label("pos anim");
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
                            ui.label("pos anim time offset");
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
                            ui.label("sound anim");
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
                            ui.label("sound anim time offset");
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
                        ui.label("shape");
                        let res = egui::ComboBox::new("sound-select-shape".to_string(), "")
                            .selected_text(if matches!(sound.shape, SoundShape::Circle { .. }) {
                                "circle"
                            } else {
                                "rect"
                            })
                            .show_ui(ui, |ui| {
                                if ui.button("circle").clicked() {
                                    if let SoundShape::Rect { size } = sound.shape {
                                        sound.shape = SoundShape::Circle {
                                            radius: uffixed::from_num(
                                                length(&dvec2::new(
                                                    size.x.to_num(),
                                                    size.y.to_num(),
                                                )) / 2_f64.sqrt(),
                                            ),
                                        };
                                    }
                                }
                                if ui.button("rect").clicked() {
                                    if let SoundShape::Circle { radius } = sound.shape {
                                        sound.shape = SoundShape::Rect {
                                            size: ufvec2::new(radius.to_num(), radius.to_num()),
                                        };
                                    }
                                }
                            });
                        ui.end_row();

                        *pointer_is_used |= {
                            let intersected = ui.input(|i| {
                                if i.pointer.primary_down() {
                                    Some((
                                        !res.response.rect.intersects({
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
                        ui.label("loop");
                        toggle_ui(ui, &mut sound.looped);
                        ui.end_row();

                        // panning
                        ui.label("panning");
                        toggle_ui(ui, &mut sound.panning);
                        ui.end_row();

                        // starting delay
                        ui.label("start delay (ms)");
                        let mut millis = sound.time_delay.as_millis() as u64;
                        if ui
                            .add(egui::DragValue::new(&mut millis).update_while_editing(false))
                            .changed()
                        {
                            sound.time_delay = std::time::Duration::from_millis(millis);
                        }
                        ui.end_row();

                        // sound falloff
                        ui.label("falloff");
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
                                ui.label("width");
                                let mut x = size.x.to_num::<f64>();
                                if ui
                                    .add(egui::DragValue::new(&mut x).update_while_editing(false))
                                    .changed()
                                {
                                    size.x = uffixed::from_num(x.clamp(0.0, f64::MAX));
                                }
                                ui.end_row();
                                ui.label("height");
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
                                ui.label("radius");
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

                    if animations_panel_open {
                        ui.colored_label(
                            Color32::RED,
                            "The animation panel is open,\n\
                                changing attributes will not apply them\n\
                                to the sound permanently!",
                        )
                        .on_hover_ui(animations_panel_open_warning);
                        ui.end_row();
                    }
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
                        sounds_count,
                        point,
                        sound,
                        None,
                        None,
                        true,
                        true,
                        animations_panel_open,
                        &map.animations,
                        pipe.user_data.pointer_is_used,
                    )
                });

                if *sound != sound_cmp && !animations_panel_open {
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
                        sounds_count,
                        point,
                        &mut sound,
                        pos_offset,
                        can_change_pos_anim.then_some(pos_anim).flatten(),
                        can_change_pos_anim,
                        can_change_sound_anim,
                        animations_panel_open,
                        &map.animations,
                        pipe.user_data.pointer_is_used,
                    )
                });

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
                        if !animations_panel_open {
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
                        }
                    });
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
        }
    }
}
