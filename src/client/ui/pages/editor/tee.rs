#![allow(unused)]

use std::time::Duration;

use client_render_base::render::{
    animation::{AnimState, TeeAnimation, TeeAnimationFrame},
    tee::RenderTee,
};
use egui::{Color32, ScrollArea, UiBuilder};
use egui_extras::{Size, StripBuilder};
use game_config::config::Config;
use graphics::handles::stream::stream::LinesStreamHandle;
use graphics::handles::stream_types::StreamedLine;
use graphics::{graphics::graphics::Graphics, handles::stream::stream::GraphicsStreamHandle};
use graphics_backend::backend::{GraphicsBackend, GraphicsBackendBase};
use hiarc::hi_closure;
use serde_value::Value;

use graphics_types::rendering::State;

// removed SystemTimeInterface trait; no longer needed

use math::math::vector::{ubvec4, vec2, vec4};
use ui_generic::traits::UiPageInterface;

use ui_base::types::{UiRenderPipe, UiState};

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize)]
pub struct TeeAnimations {
    pub idle: TeeAnimation,
    pub walking: TeeAnimation,
    pub running: TeeAnimation,
    pub jumping: TeeAnimation,
    pub air_jumping: TeeAnimation,
    pub air: TeeAnimation,
    // move with high speed
    pub air_fast: TeeAnimation,
    // fall with high speed
    pub air_falling: TeeAnimation,
}

impl TeeAnimations {
    pub fn get_by_name(&self, name: &str) -> TeeAnimation {
        let mut map = match serde_value::to_value(self) {
            Ok(Value::Map(map)) => map,
            _ => panic!("expected a struct"),
        };

        let key = Value::String(name.to_owned());
        let value = match map.remove(&key) {
            Some(value) => value,
            None => panic!("no such field"),
        };

        match TeeAnimation::deserialize(value) {
            Ok(r) => r,
            Err(_) => panic!("wrong type?"),
        }
    }
}

#[derive(Default)]
pub struct TeeImage {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Default)]
pub struct TeeImages {
    body: TeeImage,
    left_eye: TeeImage,
    right_eye: TeeImage,
    left_foot: TeeImage,
    right_foot: TeeImage,
    left_hand: TeeImage,
    right_hand: TeeImage,

    marking: TeeImage,
    decoration: TeeImage,
}

/// Overwrite the tee's body parts render colors.
/// If the tee uses a custom color, a multiplication between both is performed
#[derive(Default)]
pub struct TeeStateColorOverwrite {
    body: vec4,
    left_eye: vec4,
    right_eye: vec4,
    left_foot: vec4,
    right_foot: vec4,
    left_hand: vec4,
    right_hand: vec4,

    marking: vec4,
    decoration: vec4,
}

#[derive(Default)]
pub struct TeeStatusEffectTriggerEventParticle {
    start_pos: vec2,
    vel: vec2,

    life_time: u64,
    start_size: f32,
    end_size: f32,

    use_alpha_fading: bool,
    start_alpha: f32,
    end_alpha: f32,

    rotation: f32,
    rotation_speed: f32,

    gravity: f32,
    friction: f32,

    color: ubvec4,

    collides: bool,
}

// TODO: stars from ddrace should be designable
#[derive(Default)]
pub struct TeeStatusEffectTriggerEventParticleGroup {
    texture: TeeImage,
    particles: Vec<TeeStatusEffectTriggerEventParticle>,
    // TODO sound effects
}

pub enum TeeStatusEffectTriggerEvent {
    Particles(TeeStatusEffectTriggerEventParticleGroup),
}

/// Other than states, status effects are only an overlay for the
/// tee rendering and thus different status effects can exist
#[derive(Default)]
pub struct TeeStatusEffect {
    // triggered at a certain time stamp
    trigger_events: std::collections::BTreeMap<u64, Vec<TeeStatusEffectTriggerEvent>>,
}

/// States can overwrite tee body parts and thus
/// the Tee can only have one state at a time.
#[derive(Default)]
pub struct TeeState {
    color_overwrites: TeeStateColorOverwrite,
    // if used, uses this image instead of the tee's body part
    texture_overwrites: TeeImages,
    effect: TeeStatusEffect,
}

#[derive(Default)]
pub struct TeeStates {
    none: TeeState,
    ninja: TeeState,
    frozen: TeeState,
    // god/super mode
    super_mode: TeeState,
    ghost: TeeState,
    // e.g. the dots in ddrace when using /spec
    meta: TeeState,
    invisible: TeeState,
}

/// Semi-States can e.g. what eyes are used and the body part's color and thus
/// the Tee can only have one semi-state at a time.
///
/// Other than states they cannot overwrite the body parts directly.
/// Semi states are always evaluated before states.
#[derive(Default)]
pub struct TeeSemiState {
    color_overwrites: TeeStateColorOverwrite,
    effect: TeeStatusEffect,
    eye_emote: u64, // TODO
}

#[derive(Default)]
pub struct TeeSemiStates {
    none: TeeState,
    poisoned: TeeState,
    buffed: TeeState,
    sleeping: TeeState,
    wet: TeeState,
    sweeting: TeeState,
    burning: TeeState,
}

#[derive(Default)]
pub struct TeeServerStatusEffects {
    none: TeeState,
    rampage: TeeState,
    killingspree: TeeState,
    unstoppable: TeeState,
    dominating: TeeState,
    whickedsick: TeeState,
    godlike: TeeState,
    afk: TeeState,
}

#[derive(Default)]
pub struct TeeStatusEffects {
    none: TeeStatusEffect,
    spawning: TeeStatusEffect,
    dieing: TeeStatusEffect,
    fainting: TeeStatusEffect,
    freeze: TeeStatusEffect,
    unfreeze: TeeStatusEffect,
    damage: TeeStatusEffect,
    catching_fire: TeeStatusEffect,
    extinguish_fire: TeeStatusEffect,
}

#[derive(Default)]
pub struct TeePermanentStatusEffects {
    none: TeeStatusEffect,
    tournament_winner: TeeStatusEffect,
    tournament_second: TeeStatusEffect,
    tournament_third: TeeStatusEffect,
    donator: TeeStatusEffect,
    game_moderator: TeeStatusEffect,
}

#[derive(Default)]
pub struct EditorAtom<V> {
    value: V,
}

impl<V> EditorAtom<V>
where
    V: Default + Clone,
{
    pub fn get(&self) -> V {
        self.value.clone()
    }

    pub fn set(&mut self, new_val: V) {
        self.value = new_val;
    }

    pub fn reset(&mut self) {
        self.value = V::default();
    }
}

#[derive(Default)]
pub struct TeeEditorAtoms {
    // current selected body part
    selected_body_part: EditorAtom<String>,
    // current animation data(key frame) of the current frame(frame of the editor)
    anim_frame_data: EditorAtom<TeeAnimationFrame>,
    // current key frame point that was selected in the bottom panel
    selected_key_frame: EditorAtom<Option<usize>>,
}

#[derive(Default)]
pub struct TeeEditorItem {
    animations: TeeAnimations,
    images: TeeImages,
    states: TeeStates,
    semi_states: TeeSemiStates,
    server_status_effects: TeeServerStatusEffects,
    status_effects: TeeStatusEffects,
    permanent_status_effects: TeePermanentStatusEffects,

    // project specific stuff
    // what is currently being edited (animations, images etc.)
    cur_edit: String,
    cur_anim: String,
    atoms: TeeEditorAtoms,
}

pub struct TeeEditorData {
    // current animation time
    anim_time: Duration,

    tee_renderer: RenderTee,
}

impl TeeEditorData {
    pub fn new(graphics: &mut Graphics) -> Self {
        Self {
            anim_time: Duration::ZERO,

            tee_renderer: RenderTee::new(graphics),
        }
    }
}

pub struct TeeEditor {
    // key = name of the editing tee item
    items: std::collections::HashMap<String, TeeEditorItem>,
    cur_item: String,
    item_list: Vec<String>,

    // current activity in the acitivity bar
    activity: String,

    data: TeeEditorData,

    stream_handle: GraphicsStreamHandle,
}

const KEY_RADIUS: f32 = 5.0;

impl TeeEditor {
    pub fn new(graphics: &mut Graphics) -> Self {
        Self {
            items: std::collections::HashMap::new(),
            cur_item: "".to_string(),
            item_list: Vec::new(),
            activity: "File".to_string(),

            data: TeeEditorData::new(graphics),

            stream_handle: graphics.stream_handle.clone(),
        }
    }

    fn central_panel_frame() -> egui::containers::Frame {
        egui::containers::Frame {
            inner_margin: egui::Margin {
                left: 0,
                right: 0,
                top: 0,
                bottom: 0,
            },
            outer_margin: egui::Margin {
                left: 0,
                right: 0,
                top: 0,
                bottom: 0,
            },
            corner_radius: egui::CornerRadius {
                nw: 0,
                ne: 0,
                sw: 0,
                se: 0,
            },
            shadow: egui::epaint::Shadow::NONE,
            fill: egui::Color32::TRANSPARENT,
            stroke: egui::Stroke::NONE,
        }
    }

    fn render_anim_preview(
        editor_data: &TeeEditorData,
        ui: &mut egui::Ui,
        item: &mut TeeEditorItem,
        _pipe: &mut UiRenderPipe<Config>,
        ui_state: &mut UiState,
        stream_handle: &GraphicsStreamHandle,
    ) {
        let mut anim_state = AnimState::default();
        anim_state.set(
            &item.animations.get_by_name(&item.cur_anim),
            &editor_data.anim_time,
        );

        let rect_unscaled = ui.available_rect_before_wrap();
        let _response = ui.allocate_response(rect_unscaled.size(), egui::Sense::click_and_drag());
        let zoom_level = ui.ctx().pixels_per_point();
        let ui_rect = egui::Rect::from_min_max(
            egui::Pos2 {
                x: rect_unscaled.min.x * zoom_level,
                y: rect_unscaled.min.y * zoom_level,
            },
            egui::Pos2 {
                x: rect_unscaled.max.x * zoom_level,
                y: rect_unscaled.max.y * zoom_level,
            },
        );
        let mut state = State::new();
        state.map_canvas(
            -ui_rect.min.x,
            ui_rect.min.y,
            ui_rect.max.x - ui_rect.min.x,
            ui_rect.max.y,
        );

        // render tee at the current animation frame
        /*
        TODO: some default skins are required
        let tee_render_info = TeeRenderInfo {
            render_skin: TeeRenderSkinTextures::Original(Default::default()),
            color_body: ColorRGBA {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            color_feet: ColorRGBA {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            got_air_jump: false,
            feet_flipped: false,
            size: 64.0,
        };
        let tee_dir = vec2 { x: 1.0, y: 0.0 };
        let tee_emote = crate::render::tee::TeeEyeEmote::Normal;
        let render_math = RenderTee::render_tee_math(
            &anim_state,
            &tee_render_info,
            tee_emote,
            &tee_dir,
            &vec2 {
                x: ui_rect.width() / 2.0,
                y: ui_rect.height() / 2.0,
            },
        );

        if response.clicked() {
            let mouse_pos = response.interact_pointer_pos();
            if let Some(mouse_pos_unscaled) = mouse_pos {
                let mouse_pos = egui::Pos2 {
                    x: (mouse_pos_unscaled.x - rect_unscaled.min.x) * ui_state.zoom_level,
                    y: (mouse_pos_unscaled.y - rect_unscaled.min.y) * ui_state.zoom_level,
                };
                // find a part that was clicked, unselect else
                // first try body
                if mouse_pos.x
                    >= render_math.body_pos.x
                        - (render_math.body_scale.x * RENDER_TEE_BODY_SIZE_BASE) / 2.0
                    && mouse_pos.x
                        < render_math.body_pos.x
                            + (render_math.body_scale.x * RENDER_TEE_BODY_SIZE_BASE) / 2.0
                {
                    item.atoms.selected_body_part.set("body".to_string());
                } else {
                    // nothing found, reset the atom
                    item.atoms.selected_body_part.reset();
                }
            }
        }

        editor_data.tee_renderer.render_tee_from_math(
            &render_math,
            pipe.graphics,
            &tee_render_info,
            tee_emote,
            &tee_dir,
            1.0,
            &state,
        );*/

        // render all animation frames as plot lines to visualize their animation path
        stream_handle.render_lines(&[StreamedLine::new(), StreamedLine::new()], State::new());

        if item.atoms.selected_body_part.get() != "" {
            // add an overlay to show the current frames interpolated data(pos, scale, rotation)
            egui::Window::new("Vector data")
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(0.0, 0.0))
                .default_width(32.0)
                .show(ui.ctx(), |ui| {
                    // position, scale & rotation
                    let anim_pos = item.atoms.anim_frame_data.get().pos;
                    let anim_scale = item.atoms.anim_frame_data.get().scale;
                    let anim_rot = item.atoms.anim_frame_data.get().rotation;
                    let mut pos: [String; 2] = [anim_pos.x.to_string(), anim_pos.y.to_string()];
                    let mut scale: [String; 2] =
                        [anim_scale.x.to_string(), anim_scale.y.to_string()];
                    let mut rot: String = anim_rot.to_string();
                    ui.label("Position:");
                    ui.text_edit_singleline(&mut pos[0]);
                    ui.text_edit_singleline(&mut pos[1]);
                    ui.label("Scale:");
                    ui.text_edit_singleline(&mut scale[0]);
                    ui.text_edit_singleline(&mut scale[1]);
                    ui.label("Rotation:");
                    ui.text_edit_singleline(&mut rot);

                    item.atoms.anim_frame_data.set(TeeAnimationFrame {
                        pos: vec2::new(
                            pos[0].parse().unwrap_or(anim_pos.x),
                            pos[1].parse().unwrap_or(anim_pos.y),
                        ),
                        scale: vec2::new(
                            scale[0].parse().unwrap_or(anim_scale.x),
                            scale[1].parse().unwrap_or(anim_scale.y),
                        ),
                        rotation: rot.parse().unwrap_or(anim_rot),
                    });
                });
        }
    }

    pub fn render_central_panel(
        editor_data: &TeeEditorData,
        ui: &mut egui::Ui,
        item: &mut TeeEditorItem,
        pipe: &mut UiRenderPipe<Config>,
        ui_state: &mut UiState,
        stream_handle: &GraphicsStreamHandle,
    ) {
        if item.cur_edit.as_str() == "anim" {
            Self::render_anim_preview(editor_data, ui, item, pipe, ui_state, stream_handle);
        }
    }

    fn render_anim_key_frame_key_paint(
        response: &egui::Response,
        painter: &egui::Painter,
        item: &mut TeeEditorItem,
        _pipe: &mut UiRenderPipe<Config>,
        item_size: f32,
    ) {
        let mut shapes: Vec<egui::Shape> = Vec::new();

        let rect = response.rect;
        let rect = emath::Rect::from_min_max(
            emath::pos2(rect.min.x, rect.min.y),
            emath::pos2(rect.max.x, rect.max.y),
        );
        let to_screen = emath::RectTransform::from_to(
            emath::Rect::from_center_size(emath::Pos2::ZERO, rect.square_proportions() / 1.0),
            rect,
        );

        let rect = emath::Rect::from_min_max(rect.min, rect.max);

        let _paint_line = |points: [emath::Pos2; 2], color: Color32, width: f32| {
            let line_p0 = to_screen * points[0];
            let line_p1 = to_screen * points[1];
            let line = [
                egui::pos2(line_p0.x, line_p0.y),
                egui::pos2(line_p1.x, line_p1.y),
            ];

            // culling
            if rect.intersects(emath::Rect::from_two_pos(line_p0, line_p1)) {
                shapes.push(egui::Shape::line_segment(line, (width, color)));
            }
        };

        let mut paint_rect = |corners: &[emath::Pos2; 2], color: Color32| {
            let rect_points = [corners[0], corners[1]];

            shapes.push(egui::Shape::rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(rect_points[0].x, rect_points[0].y),
                    egui::pos2(rect_points[1].x, rect_points[1].y),
                ),
                0.0,
                color,
            ));
        };

        // draw background rect
        paint_rect(
            &[rect.left_top(), rect.right_bottom()],
            Color32::from_rgb(128, 128, 128),
        );

        let mut found_key_frame = false;

        let mut paint_key_frame_circle =
            |index: usize,
             center: &egui::Pos2,
             radius: f32,
             color: Color32,
             color_if_selected: Color32| {
                let is_selected = item.atoms.selected_key_frame.get().is_some()
                    && index == item.atoms.selected_key_frame.get().unwrap();
                let circle = egui::Shape::circle_filled(
                    *center,
                    radius,
                    if is_selected {
                        color_if_selected
                    } else {
                        color
                    },
                );
                if response.clicked()
                    && circle
                        .visual_bounding_rect()
                        .contains(response.interact_pointer_pos().unwrap_or_default())
                {
                    item.atoms.selected_key_frame.set(Some(index));
                    found_key_frame |= true;
                }
                shapes.push(circle);
            };

        let cur_zoom = 1.0;
        let _cur_scroll = 0.0;

        let width = rect.width() * cur_zoom;
        let _height = rect.height();

        let mut draw_key_frame =
            |index: usize,
             y_render_off,
             (timestamp, _key_frame): &(Duration, TeeAnimationFrame)| {
                let x_off = (((timestamp.as_nanos() as f64
                    / Duration::from_secs(1).as_nanos() as f64)
                    * (width as f64 - KEY_RADIUS as f64 * 2.0))
                    + rect.left_top().x as f64)
                    + KEY_RADIUS as f64;
                let y_off = y_render_off + rect.left_top().y + KEY_RADIUS;
                paint_key_frame_circle(
                    index,
                    &egui::pos2(x_off as f32, y_off),
                    KEY_RADIUS,
                    Color32::from_rgb(200, 128, 128),
                    Color32::from_rgb(128, 200, 128),
                );
            };

        let mut y = 0.0;
        item.animations
            .get_by_name(&item.cur_anim)
            .body
            .frames
            .iter()
            .enumerate()
            .for_each(|(index, v)| draw_key_frame(index, y, v));

        y += item_size;
        item.animations
            .get_by_name(&item.cur_anim)
            .left_eye
            .frames
            .iter()
            .enumerate()
            .for_each(|(index, v)| draw_key_frame(index, y, v));

        y += item_size;
        item.animations
            .get_by_name(&item.cur_anim)
            .right_eye
            .frames
            .iter()
            .enumerate()
            .for_each(|(index, v)| draw_key_frame(index, y, v));

        y += item_size;
        item.animations
            .get_by_name(&item.cur_anim)
            .left_foot
            .frames
            .iter()
            .enumerate()
            .for_each(|(index, v)| draw_key_frame(index, y, v));

        y += item_size;
        item.animations
            .get_by_name(&item.cur_anim)
            .right_foot
            .frames
            .iter()
            .enumerate()
            .for_each(|(index, v)| draw_key_frame(index, y, v));

        y += item_size;
        item.animations
            .get_by_name(&item.cur_anim)
            .left_hand
            .frames
            .iter()
            .enumerate()
            .for_each(|(index, v)| draw_key_frame(index, y, v));

        y += item_size;
        item.animations
            .get_by_name(&item.cur_anim)
            .right_hand
            .frames
            .iter()
            .enumerate()
            .for_each(|(index, v)| draw_key_frame(index, y, v));

        if !found_key_frame && response.clicked() {
            item.atoms.selected_key_frame.set(None);
        }

        painter.extend(shapes);
    }

    fn render_anim_key_frame_key(
        ui: &mut egui::Ui,
        item: &mut TeeEditorItem,
        pipe: &mut UiRenderPipe<Config>,
    ) {
        ScrollArea::both().show(ui, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            let mut painter_rect = ui.available_rect_before_wrap();
            painter_rect.max.x = painter_rect.min.x + 120.0 * (KEY_RADIUS * 2.0); // assume 120 points per second
            painter_rect.max.y = painter_rect.min.y + 7.0 * (KEY_RADIUS * 2.0);
            let _rect = ui.scope_builder(UiBuilder::new().max_rect(painter_rect), |ui| {
                let (response, painter) =
                    ui.allocate_painter(painter_rect.size(), egui::Sense::click());
                Self::render_anim_key_frame_key_paint(&response, &painter, item, pipe, 20.0);
            });
        });
    }

    fn render_anim_keyframe_overview(
        ui: &mut egui::Ui,
        item: &mut TeeEditorItem,
        pipe: &mut UiRenderPipe<Config>,
    ) {
        // render all keyframe points
        Self::render_anim_key_frame_key(ui, item, pipe);
    }

    pub fn render_bottom_panel(
        ui: &mut egui::Ui,
        item: &mut TeeEditorItem,
        pipe: &mut UiRenderPipe<Config>,
    ) {
        if item.cur_edit.as_str() == "anim" {
            Self::render_anim_keyframe_overview(ui, item, pipe);
        }
    }

    pub fn render_left_panel(&mut self, ui: &mut egui::Ui) {
        match self.activity.as_str() {
            // all projects
            "List" => {
                for item in &self.item_list {
                    let _collapse = ui.collapsing(item.as_str(), |ui| {
                        if ui.button("select").clicked() {
                            let editor_item = TeeEditorItem::default();
                            self.items.insert(item.clone(), editor_item);
                        }
                    });
                }
            }
            // current project
            "Project" => {
                if !self.cur_item.is_empty() {
                    let item_res = self.items.get(&self.cur_item);
                    if let Some(_item) = item_res {
                        // TODO!
                    }
                }
            }
            _ => {}
        }
    }

    pub fn drag_source(ui: &mut egui::Ui, id: egui::Id, body: impl FnOnce(&mut egui::Ui)) {
        let is_being_dragged = ui.ctx().is_being_dragged(id);

        if !is_being_dragged {
            let response = ui.scope(body).response;

            // Check for drags:
            let response = ui.interact(response.rect, id, egui::Sense::drag());
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
        } else {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);

            // Paint the body to a new layer:
            let layer_id = egui::LayerId::new(egui::Order::Tooltip, id);
            let response = ui
                .scope_builder(UiBuilder::new().layer_id(layer_id), body)
                .response;

            // Now we move the visuals of the body to where the mouse is.
            // Normally you need to decide a location for a widget first,
            // because otherwise that widget cannot interact with the mouse.
            // However, a dragged component cannot be interacted with anyway
            // (anything with `Order::Tooltip` always gets an empty [`Response`])
            // So this is fine!

            if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
                let delta = pointer_pos - response.rect.center();
                ui.ctx().transform_layer_shapes(
                    layer_id,
                    egui::emath::TSTransform {
                        scaling: 1.0,
                        translation: delta,
                    },
                );
            }
        }
    }

    pub fn drop_target<R>(
        ui: &mut egui::Ui,
        can_accept_what_is_being_dragged: bool,
        body: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        let is_being_dragged = ui.ctx().dragged_id().is_some();

        let margin = egui::Vec2::splat(4.0);

        let outer_rect_bounds = ui.available_rect_before_wrap();
        let inner_rect = outer_rect_bounds.shrink2(margin);
        let where_to_put_background = ui.painter().add(egui::Shape::Noop);
        let mut content_ui =
            ui.new_child(UiBuilder::new().max_rect(inner_rect).layout(*ui.layout()));
        let ret = body(&mut content_ui);
        let outer_rect =
            egui::Rect::from_min_max(outer_rect_bounds.min, content_ui.min_rect().max + margin);
        let (rect, response) = ui.allocate_at_least(outer_rect.size(), egui::Sense::hover());

        let style = if is_being_dragged && can_accept_what_is_being_dragged && response.hovered() {
            ui.visuals().widgets.active
        } else {
            ui.visuals().widgets.inactive
        };

        let mut fill = style.bg_fill;
        let mut stroke = style.bg_stroke;
        if is_being_dragged && !can_accept_what_is_being_dragged {
            fill = ui.visuals().gray_out(fill);
            stroke.color = ui.visuals().gray_out(stroke.color);
        }

        ui.painter().set(
            where_to_put_background,
            egui::epaint::RectShape::new(
                rect,
                style.corner_radius,
                fill,
                stroke,
                egui::StrokeKind::Inside,
            ),
        );

        egui::InnerResponse::new(ret, response)
    }

    fn tee_editor_acitivity_bar(&mut self, ui: &mut egui::Ui) {
        let id_source = "my_drag_and_drop_demo";
        let mut source_col_row = None;

        let mut items = vec!["File", "Search", "Impl"];

        let can_accept_what_is_being_dragged = true; // We accept anything being dragged (for now) ¯\_(ツ)_/¯
        let response = Self::drop_target(ui, can_accept_what_is_being_dragged, |ui| {
            ui.set_min_size(egui::vec2(64.0, 100.0));
            for (row_idx, item) in items.clone().iter().enumerate() {
                let item_id = egui::Id::new(id_source).with(row_idx);
                Self::drag_source(ui, item_id, |ui| {
                    let response =
                        ui.add(egui::Button::new(item.to_string()).sense(egui::Sense::click()));
                    if response.clicked() {
                        self.activity = item.to_string();
                    }
                    response.context_menu(|ui| {
                        if ui.button("Remove").clicked() {
                            items.remove(row_idx);
                            ui.close_kind(egui::UiKind::Menu);
                        }
                    });
                });

                if ui.ctx().is_being_dragged(item_id) {
                    source_col_row = Some(row_idx);
                }
            }
        })
        .response;

        let _response = response.context_menu(|ui| {
            if ui.button("New Item").clicked() {
                items.push("New Item");
                ui.close_kind(egui::UiKind::Menu);
            }
        });
    }
}

impl UiPageInterface<Config> for TeeEditor {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut UiRenderPipe<Config>,
        ui_state: &mut UiState,
    ) {
        // TODO: debug
        if self.items.is_empty() {
            self.items
                .insert("test".to_string(), TeeEditorItem::default());
            self.cur_item = "test".to_string();
            let item = self.items.get_mut("test").unwrap();
            item.cur_edit = "anim".to_string();
            item.cur_anim = "idle".to_string();
            item.animations
                .idle
                .body
                .frames
                .push((Duration::ZERO, TeeAnimationFrame::default()));
            item.animations
                .idle
                .body
                .frames
                .push((Duration::from_secs(1), TeeAnimationFrame::default()));

            self.item_list.push("test".to_string());
            self.item_list.push("dont_click_me".to_string());
        }

        self.data.anim_time = pipe.cur_time;

        let dark_mode = ui.visuals().dark_mode;
        let faded_color = ui.visuals().window_fill();
        let _faded_color = |color: Color32| -> Color32 {
            use egui::Rgba;
            let t = if dark_mode { 0.95 } else { 0.8 };
            egui::lerp(Rgba::from(color)..=Rgba::from(faded_color), t).into()
        };

        StripBuilder::new(ui)
            .size(Size::exact(10.0))
            .size(Size::remainder())
            .vertical(|mut strip| {
                strip.cell(|ui| {
                    ui.style_mut().wrap_mode = None;
                    ui.menu_button("Test", |ui| {
                        if ui.button("Close menu").clicked() {
                            ui.close_kind(egui::UiKind::Menu);
                            pipe.user_data.engine.ui.path.route("");
                        };
                    });
                });
                strip.strip(|builder| {
                    builder
                        .size(Size::exact(10.0))
                        .size(Size::remainder())
                        .horizontal(|mut strip| {
                            strip.cell(|ui| {
                                ui.style_mut().wrap_mode = None;
                                self.tee_editor_acitivity_bar(ui);
                            });
                            strip.cell(|ui| {
                                ui.style_mut().wrap_mode = None;
                                egui::SidePanel::left("left_panel")
                                    .resizable(true)
                                    .default_width(150.0)
                                    .width_range(80.0..=200.0)
                                    .show_inside(ui, |ui| {
                                        ui.vertical_centered(|ui| {
                                            ui.heading("Left Panel");
                                            self.render_left_panel(ui);
                                        });
                                    });

                                egui::TopBottomPanel::bottom("bottom_panel")
                                    .resizable(true)
                                    .default_height(50.0)
                                    .height_range(20.0..=100.0)
                                    .show_inside(ui, |ui| {
                                        ui.vertical_centered(|ui| {
                                            ui.heading("Bottom Panel");
                                            let item_res = self.items.get_mut(&self.cur_item);
                                            if let Some(item) = item_res {
                                                Self::render_bottom_panel(ui, item, pipe);
                                            }
                                        });
                                    });

                                egui::CentralPanel::default()
                                    .frame(Self::central_panel_frame())
                                    .show_inside(ui, |ui| {
                                        ui.vertical_centered(|ui| {
                                            ui.heading("Central Panel");
                                            let item_res = self.items.get_mut(&self.cur_item);
                                            if let Some(item) = item_res {
                                                Self::render_central_panel(
                                                    &self.data,
                                                    ui,
                                                    item,
                                                    pipe,
                                                    ui_state,
                                                    &self.stream_handle,
                                                );
                                            }
                                        });
                                    });
                            });
                        });
                });
            });
    }
}
