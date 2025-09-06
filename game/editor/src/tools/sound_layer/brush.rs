use camera::{Camera, CameraInterface};
use client_render_base::map::map::RenderMap;
use graphics::handles::{
    canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
};
use graphics_types::rendering::State;
use hiarc::Hiarc;
use map::map::groups::layers::design::Sound;
use math::math::vector::{ffixed, ubvec4, vec2};

use crate::{
    actions::actions::{
        ActChangeSoundAttr, ActSoundLayerAddRemSounds, ActSoundLayerAddSounds, EditorAction,
    },
    client::EditorClient,
    map::{EditorLayer, EditorLayerUnionRef, EditorMap, EditorMapInterface},
    tools::{
        shared::{align_pos, in_radius},
        utils::render_rect,
    },
    utils::{UiCanvasSize, ui_pos_to_world_pos, ui_pos_to_world_pos_and_world_height},
};

use super::shared::{SOUND_POINT_RADIUS_FACTOR, SoundPointerDownPoint, render_sound_points};

#[derive(Debug, Hiarc)]
pub struct SoundBrushSounds {
    pub sounds: Vec<Sound>,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Hiarc)]
pub struct SoundSelection {
    pub is_background: bool,
    pub group: usize,
    pub layer: usize,
    pub sound_index: usize,
    pub sound: Sound,
    pub point: SoundPointerDownPoint,
    pub cursor_in_world_pos: vec2,
    pub cursor_corner_offset: vec2,
}

#[derive(Debug, Hiarc)]
pub enum SoundPointerDownState {
    None,
    /// sound corner/center point
    Point(SoundPointerDownPoint),
    /// selection of sounds
    Selection(vec2),
}

impl SoundPointerDownState {
    pub fn is_selection(&self) -> bool {
        matches!(self, Self::Selection(_))
    }
}

/// sound brushes are relative to where the mouse selected them
#[derive(Debug, Hiarc)]
pub struct SoundBrush {
    pub brush: Option<SoundBrushSounds>,

    /// this is the last sound selected (clicked on the corner selectors), this can be used
    /// for the animation to know the current sound
    pub last_popup: Option<SoundSelection>,
    /// Last moving of a sound
    pub last_translation: Option<SoundSelection>,
    /// The sound point last selected, moved etc.
    pub last_selection: Option<SoundSelection>,

    pub pointer_down_state: SoundPointerDownState,

    pub parallax_aware_brush: bool,
}

impl Default for SoundBrush {
    fn default() -> Self {
        Self::new()
    }
}

impl SoundBrush {
    pub fn new() -> Self {
        Self {
            brush: Default::default(),
            last_popup: None,
            last_translation: None,
            last_selection: None,
            pointer_down_state: SoundPointerDownState::None,

            parallax_aware_brush: false,
        }
    }

    fn handle_brush_select(
        &mut self,
        ui_canvas: &UiCanvasSize,
        canvas_handle: &GraphicsCanvasHandle,
        map: &mut EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
        latest_modifiers: &egui::Modifiers,
        client: &mut EditorClient,
    ) {
        let layer = map.active_layer();
        let (offset, parallax) = if let Some(layer) = &layer {
            layer.get_offset_and_parallax()
        } else {
            Default::default()
        };
        let Some(EditorLayerUnionRef::Design {
            layer: EditorLayer::Sound(layer),
            group_index,
            is_background,
            layer_index,
            ..
        }) = layer
        else {
            return;
        };

        let pointer_cur = vec2::new(current_pointer_pos.x, current_pointer_pos.y);

        let vec2 {
            x: mut x1,
            y: mut y1,
        } = ui_pos_to_world_pos(
            canvas_handle,
            ui_canvas,
            map.groups.user.zoom,
            vec2::new(pointer_cur.x, pointer_cur.y),
            map.groups.user.pos.x,
            map.groups.user.pos.y,
            offset.x,
            offset.y,
            parallax.x,
            parallax.y,
            map.groups.user.parallax_aware_zoom,
        );

        // if pointer was already down
        if let SoundPointerDownState::Selection(pointer_down) = &self.pointer_down_state {
            // find current layer
            let &vec2 {
                x: mut x0,
                y: mut y0,
            } = pointer_down;

            if x0 > x1 {
                std::mem::swap(&mut x0, &mut x1);
            }
            if y0 > y1 {
                std::mem::swap(&mut y0, &mut y1);
            }

            // check if any sounds are in the selection
            let mut sounds: Vec<Sound> = Default::default();

            for sound in &layer.layer.sounds {
                if super::super::quad_layer::shared::in_box(&sound.pos, x0, y0, x1, y1) {
                    sounds.push(*sound);
                }
            }

            // if there is an selection, apply that
            if !sounds.is_empty() {
                let pointer_down = vec2::new(x0, y0);

                let x = -pointer_down.x;
                let y = -pointer_down.y;

                for sound in &mut sounds {
                    sound.pos.x += ffixed::from_num(x);
                    sound.pos.y += ffixed::from_num(y);
                }

                self.brush = Some(SoundBrushSounds {
                    sounds,
                    w: x1 - x0,
                    h: y1 - y0,
                });
            } else {
                self.brush = None;
            }

            if !latest_pointer.primary_down() {
                self.pointer_down_state = SoundPointerDownState::None;
            }
        } else {
            let align_pos = |pos: vec2| align_pos(map, latest_modifiers, pos);

            // check if the pointer clicked on one of the sound corner/center points
            let mut clicked_sound_point = false;
            if latest_pointer.primary_pressed() || latest_pointer.secondary_pressed() {
                for (s, sound) in layer.layer.sounds.iter().enumerate() {
                    let pointer_cur = vec2::new(current_pointer_pos.x, current_pointer_pos.y);

                    let (pointer_cur, h) = ui_pos_to_world_pos_and_world_height(
                        canvas_handle,
                        ui_canvas,
                        map.groups.user.zoom,
                        vec2::new(pointer_cur.x, pointer_cur.y),
                        map.groups.user.pos.x,
                        map.groups.user.pos.y,
                        offset.x,
                        offset.y,
                        parallax.x,
                        parallax.y,
                        map.groups.user.parallax_aware_zoom,
                    );

                    let h = h / canvas_handle.canvas_height() as f32;
                    let radius = SOUND_POINT_RADIUS_FACTOR * h;
                    if in_radius(&sound.pos, &pointer_cur, radius) {
                        // pointer is in a drag mode
                        clicked_sound_point = true;
                        let down_point = SoundPointerDownPoint::Center;

                        let sound_pos = vec2::new(sound.pos.x.to_num(), sound.pos.y.to_num());
                        let cursor = vec2::new(x1, y1);

                        self.pointer_down_state = SoundPointerDownState::Point(down_point);
                        *if latest_pointer.primary_pressed() {
                            &mut self.last_translation
                        } else {
                            &mut self.last_popup
                        } = Some(SoundSelection {
                            is_background,
                            group: group_index,
                            layer: layer_index,
                            sound_index: s,
                            sound: *sound,
                            point: down_point,
                            cursor_in_world_pos: cursor,
                            cursor_corner_offset: cursor - sound_pos,
                        });
                        self.last_selection = Some(SoundSelection {
                            is_background,
                            group: group_index,
                            layer: layer_index,
                            sound_index: s,
                            sound: *sound,
                            point: down_point,
                            cursor_in_world_pos: cursor,
                            cursor_corner_offset: cursor - sound_pos,
                        });

                        break;
                    }
                }
            }
            // else check if the pointer is down now
            if !clicked_sound_point
                && latest_pointer.primary_pressed()
                && self.last_translation.is_none()
            {
                let pointer_cur = vec2::new(current_pointer_pos.x, current_pointer_pos.y);
                let pos = ui_pos_to_world_pos(
                    canvas_handle,
                    ui_canvas,
                    map.groups.user.zoom,
                    vec2::new(pointer_cur.x, pointer_cur.y),
                    map.groups.user.pos.x,
                    map.groups.user.pos.y,
                    offset.x,
                    offset.y,
                    parallax.x,
                    parallax.y,
                    map.groups.user.parallax_aware_zoom,
                );
                self.pointer_down_state = SoundPointerDownState::Selection(pos);
            }
            if !clicked_sound_point && latest_pointer.primary_pressed() {
                self.last_translation = None;
                self.last_selection = None;
            }
            if latest_pointer.primary_down() && self.last_translation.is_some() {
                let last_active = self.last_translation.as_mut().unwrap();
                let new_pos = vec2::new(x1, y1);
                let aligned_pos = align_pos(new_pos);
                let new_pos = if let Some(aligned_pos) = aligned_pos {
                    aligned_pos + last_active.cursor_corner_offset
                } else {
                    new_pos
                };
                if let Some(edit_sound) = layer.layer.sounds.get(last_active.sound_index).copied() {
                    let sound = &mut last_active.sound;

                    let pos_anim = edit_sound.pos_anim;
                    let alter_anim_point = map.user.change_animations() && pos_anim.is_some();

                    let cursor_pos = last_active.cursor_in_world_pos;

                    // handle position
                    let diff_x = ffixed::from_num(new_pos.x - cursor_pos.x);
                    let diff_y = ffixed::from_num(new_pos.y - cursor_pos.y);

                    if alter_anim_point {
                        if let Some(pos) = &mut map.animations.user.active_anim_points.pos {
                            pos.value.x += diff_x;
                            pos.value.y += diff_y;
                        }
                    } else {
                        sound.pos.x += diff_x;
                        sound.pos.y += diff_y;
                    }

                    if *sound != edit_sound {
                        client.execute(
                            EditorAction::ChangeSoundAttr(ActChangeSoundAttr {
                                is_background,
                                group_index,
                                layer_index,
                                old_attr: edit_sound,
                                new_attr: *sound,

                                index: last_active.sound_index,
                            }),
                            Some(&format!(
                                "change-sound-attr-{is_background}-{group_index}-{layer_index}"
                            )),
                        );
                    }
                }

                last_active.cursor_in_world_pos = new_pos;
            }
        }
    }

    pub fn handle_brush_draw(
        &mut self,
        ui_canvas: &UiCanvasSize,
        canvas_handle: &GraphicsCanvasHandle,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
        client: &mut EditorClient,
    ) {
        let layer = map.active_layer().unwrap();
        let (offset, parallax) = layer.get_offset_and_parallax();

        // reset brush
        if latest_pointer.secondary_pressed() {
            self.brush = None;
        }
        // apply brush
        else {
            let brush = self.brush.as_ref().unwrap();

            if latest_pointer.primary_pressed() {
                let pos = current_pointer_pos;

                let pos = vec2::new(pos.x, pos.y);

                let vec2 { x, y } = ui_pos_to_world_pos(
                    canvas_handle,
                    ui_canvas,
                    map.groups.user.zoom,
                    vec2::new(pos.x, pos.y),
                    map.groups.user.pos.x,
                    map.groups.user.pos.y,
                    offset.x,
                    offset.y,
                    parallax.x,
                    parallax.y,
                    map.groups.user.parallax_aware_zoom,
                );

                let mut sounds = brush.sounds.clone();
                for sound in &mut sounds {
                    sound.pos.x += ffixed::from_num(x);
                    sound.pos.y += ffixed::from_num(y);
                }

                if let Some((action, group_indentifier)) = if let EditorLayerUnionRef::Design {
                    layer: EditorLayer::Sound(layer),
                    layer_index,
                    is_background,
                    group_index,
                    ..
                } = layer
                {
                    Some((
                        EditorAction::SoundLayerAddSounds(ActSoundLayerAddSounds {
                            base: ActSoundLayerAddRemSounds {
                                is_background,
                                group_index,
                                layer_index,
                                index: layer.layer.sounds.len(),
                                sounds,
                            },
                        }),
                        format!("sound-brush design {layer_index}"),
                    ))
                } else {
                    None
                } {
                    client.execute(action, Some(&group_indentifier));
                }
            }
        }
    }

    fn render_selection(
        &self,
        ui_canvas: &UiCanvasSize,
        canvas_handle: &GraphicsCanvasHandle,
        stream_handle: &GraphicsStreamHandle,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
    ) {
        let layer = map.active_layer();
        let (offset, parallax) = if let Some(layer) = &layer {
            layer.get_offset_and_parallax()
        } else {
            Default::default()
        };
        // if pointer was already down
        if let SoundPointerDownState::Selection(pointer_down) = &self.pointer_down_state
            && latest_pointer.primary_down()
        {
            let pos = current_pointer_pos;
            let pos = ui_pos_to_world_pos(
                canvas_handle,
                ui_canvas,
                map.groups.user.zoom,
                vec2::new(pos.x, pos.y),
                map.groups.user.pos.x,
                map.groups.user.pos.y,
                offset.x,
                offset.y,
                parallax.x,
                parallax.y,
                map.groups.user.parallax_aware_zoom,
            );
            let pos = egui::pos2(pos.x, pos.y);

            let down_pos = pointer_down;
            let down_pos = egui::pos2(down_pos.x, down_pos.y);

            let rect = egui::Rect::from_min_max(pos, down_pos);

            render_rect(
                canvas_handle,
                stream_handle,
                map,
                rect,
                ubvec4::new(255, 0, 0, 255),
                &parallax,
                &offset,
            );
        }
    }

    fn render_brush(
        &self,
        ui_canvas: &UiCanvasSize,
        canvas_handle: &GraphicsCanvasHandle,
        stream_handle: &GraphicsStreamHandle,
        map: &EditorMap,
        current_pointer_pos: &egui::Pos2,
    ) {
        let layer = map.active_layer();
        let (offset, parallax) = if let Some(layer) = &layer {
            layer.get_offset_and_parallax()
        } else {
            Default::default()
        };

        let brush = self.brush.as_ref().unwrap();

        let pos = current_pointer_pos;
        let pos_on_map = ui_pos_to_world_pos(
            canvas_handle,
            ui_canvas,
            map.groups.user.zoom,
            vec2::new(pos.x, pos.y),
            map.groups.user.pos.x,
            map.groups.user.pos.y,
            offset.x,
            offset.y,
            parallax.x,
            parallax.y,
            map.groups.user.parallax_aware_zoom,
        );
        let pos = pos_on_map;
        let pos = egui::pos2(pos.x, pos.y);

        let mut state = State::new();

        let (center, group_attr) = if self.parallax_aware_brush {
            (
                map.groups.user.pos - pos_on_map,
                layer.map(|layer| layer.get_or_fake_group_attr()),
            )
        } else {
            let pos = current_pointer_pos;
            let pos_on_map = ui_pos_to_world_pos(
                canvas_handle,
                ui_canvas,
                map.groups.user.zoom,
                vec2::new(pos.x, pos.y),
                map.groups.user.pos.x,
                map.groups.user.pos.y,
                0.0,
                0.0,
                100.0,
                100.0,
                true,
            );
            (map.groups.user.pos - pos_on_map, None)
        };
        Camera::new(
            center,
            map.groups.user.zoom,
            None,
            map.groups.user.parallax_aware_zoom,
        )
        .project(canvas_handle, &mut state, group_attr.as_ref());

        let time = map.user.render_time();
        RenderMap::render_sounds(
            stream_handle,
            map.active_animations(),
            &time,
            &time,
            map.user.include_last_anim_point(),
            brush.sounds.iter(),
            state,
        );

        let brush_size = vec2::new(brush.w, brush.h);
        let rect =
            egui::Rect::from_min_max(pos, egui::pos2(pos.x + brush_size.x, pos.y + brush_size.y));

        render_rect(
            canvas_handle,
            stream_handle,
            map,
            rect,
            ubvec4::new(255, 0, 0, 255),
            &parallax,
            &offset,
        );
    }

    pub fn update(
        &mut self,
        ui_canvas: &UiCanvasSize,
        canvas_handle: &GraphicsCanvasHandle,
        map: &mut EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
        latest_modifiers: &egui::Modifiers,
        client: &mut EditorClient,
    ) {
        let layer = map.active_layer();
        if !layer.as_ref().is_some_and(|layer| layer.is_sound_layer()) {
            return;
        }

        if self.brush.is_none() || self.pointer_down_state.is_selection() {
            self.handle_brush_select(
                ui_canvas,
                canvas_handle,
                map,
                latest_pointer,
                current_pointer_pos,
                latest_modifiers,
                client,
            );
        } else {
            self.handle_brush_draw(
                ui_canvas,
                canvas_handle,
                map,
                latest_pointer,
                current_pointer_pos,
                client,
            );
        }
    }

    pub fn render(
        &mut self,
        ui_canvas: &UiCanvasSize,
        stream_handle: &GraphicsStreamHandle,
        canvas_handle: &GraphicsCanvasHandle,
        map: &EditorMap,
        latest_pointer: &egui::PointerState,
        current_pointer_pos: &egui::Pos2,
    ) {
        let layer = map.active_layer();
        if !layer.as_ref().is_some_and(|layer| layer.is_sound_layer()) {
            return;
        }

        render_sound_points(
            ui_canvas,
            layer,
            current_pointer_pos,
            stream_handle,
            canvas_handle,
            map,
        );

        if self.brush.is_none() || self.pointer_down_state.is_selection() {
            self.render_selection(
                ui_canvas,
                canvas_handle,
                stream_handle,
                map,
                latest_pointer,
                current_pointer_pos,
            );
        } else {
            self.render_brush(
                ui_canvas,
                canvas_handle,
                stream_handle,
                map,
                current_pointer_pos,
            );
        }
    }
}
