use egui::{Color32, Id, Stroke};
use game_interface::types::{
    emoticons::{EmoticonType, EnumCount},
    render::character::{IntoEnumIterator, TeeEye},
};
use geo::Contains;
use math::math::{
    PI, length, normalize_pre_length,
    vector::{dvec2, vec2},
};
use ui_base::types::{UiRenderPipe, UiState};

use crate::utils::{render_emoticon_for_ui, render_tee_for_ui, rotate};

use super::user_data::{EmoteWheelEvent, UserData};

/// not required
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    let rect = ui.ctx().screen_rect();

    let width_scale = rect.width() / pipe.user_data.canvas_handle.canvas_width() as f32;

    let radius = |percentage: f32| {
        (percentage / 100.0 * pipe.user_data.canvas_handle.canvas_height() as f32) * width_scale
    };

    let color = Color32::from_black_alpha(100);

    let inner_stroke_size = radius(15.0);
    let inner_start = radius(5.0);

    let outer_radius = radius(35.0);
    let outer_stroke_size = outer_radius - (inner_stroke_size + inner_start);
    let outer_start = inner_start + inner_stroke_size;
    let outer_center = outer_start + outer_stroke_size / 2.0;
    let outer_end = outer_start + outer_stroke_size;

    ui.painter()
        .circle_filled(rect.center(), outer_radius, color);

    let inner_center = inner_stroke_size / 2.0 + inner_start;
    let inner_end = inner_stroke_size + inner_start;

    ui.painter().circle_stroke(
        rect.center(),
        radius(5.0),
        Stroke::new(inner_stroke_size, color),
    );

    ui_state.add_blur_circle(rect.center(), outer_radius);

    let mouse = &mut *pipe.user_data.mouse;

    // render emoticons in a radius around the outer circle
    let mut start_pos = vec2::new(0.0, outer_start);
    let mut pos = vec2::new(0.0, outer_center);
    let mut end_pos = vec2::new(0.0, outer_end);
    let center = rect.center();
    let center = vec2::new(center.x, center.y);

    let mouse_dir = dvec2::new(mouse.x, mouse.y) - dvec2::new(center.x as f64, center.y as f64);
    let mouse_len = length(&mouse_dir);
    if mouse_len > outer_radius as f64 {
        let center = dvec2::new(center.x as f64, center.y as f64);
        let mouse_dir = normalize_pre_length(&mouse_dir, mouse_len);
        mouse.x = center.x + mouse_dir.x * outer_radius as f64;
        mouse.y = center.y + mouse_dir.y * outer_radius as f64;
    }

    // rotate a bit so oop emote is on the very right
    let start_rot = |pos: &mut vec2| {
        rotate(
            &vec2::default(),
            -2.0 * 5.0 / EmoticonType::COUNT as f32 * PI,
            std::slice::from_mut(pos),
        )
    };
    start_rot(&mut start_pos);
    start_rot(&mut pos);
    start_rot(&mut end_pos);
    for emote in EmoticonType::iter() {
        let rot = |pos: &mut vec2, scale: f32| {
            rotate(
                &vec2::default(),
                scale * 2.0 / EmoticonType::COUNT as f32 * PI,
                std::slice::from_mut(pos),
            )
        };

        rot(&mut pos, 1.0);

        rot(&mut start_pos, 1.0);
        let mut start_p0 = start_pos;
        let mut start_p1 = start_pos;
        rot(&mut start_p0, 0.5);
        rot(&mut start_p1, -0.5);
        start_p0 += center;
        start_p1 += center;

        rot(&mut end_pos, 1.0);
        let mut end_p0 = end_pos;
        let mut end_p1 = end_pos;
        rot(&mut end_p0, -0.5);
        rot(&mut end_p1, 0.5);
        end_p0 += center;
        end_p1 += center;

        let center = center + pos;
        let size = radius(10.0);
        let selected = {
            let trapez = geo::Polygon::new(
                vec![
                    (start_p0.x, start_p0.y),
                    (start_p1.x, start_p1.y),
                    (end_p0.x, end_p0.y),
                    (end_p1.x, end_p1.y),
                ]
                .into_iter()
                .map(geo::Point::from)
                .collect(),
                vec![],
            );
            trapez.contains(&geo::Point::new(mouse.x as f32, mouse.y as f32))
        };
        if selected {
            pipe.user_data
                .events
                .push(EmoteWheelEvent::EmoticonSelected(emote));
        }
        let val = if selected {
            ui.ctx().animate_value_with_time(
                Id::new(format!("emote-wheel-anims-emoticons-{}", emote as usize)),
                1.5,
                0.15,
            )
        } else {
            ui.ctx().animate_value_with_time(
                Id::new(format!("emote-wheel-anims-emoticons-{}", emote as usize)),
                1.0,
                0.15,
            )
        };
        render_emoticon_for_ui(
            pipe.user_data.stream_handle,
            pipe.user_data.canvas_handle,
            pipe.user_data.emoticons_container,
            ui,
            ui_state,
            rect,
            None,
            pipe.user_data.emoticon,
            center,
            size * val,
            emote,
        );
    }

    // render tees in a radius around the inner circle
    let mut start_pos = vec2::new(0.0, inner_start);
    let mut pos = vec2::new(0.0, inner_center);
    let mut end_pos = vec2::new(0.0, inner_end);
    let center = rect.center();
    let center = vec2::new(center.x, center.y);

    // rotate a bit so normal eyes are on the very right
    let start_rot = |pos: &mut vec2| {
        rotate(
            &vec2::default(),
            -3.0 / TeeEye::COUNT as f32 * PI,
            std::slice::from_mut(pos),
        )
    };
    start_rot(&mut start_pos);
    start_rot(&mut pos);
    start_rot(&mut end_pos);
    for eye in TeeEye::iter().rev() {
        let rot = |pos: &mut vec2, scale: f32| {
            rotate(
                &vec2::default(),
                -scale * 2.0 / TeeEye::COUNT as f32 * PI,
                std::slice::from_mut(pos),
            )
        };
        rot(&mut pos, 1.0);

        rot(&mut start_pos, 1.0);
        let mut start_p0 = start_pos;
        let mut start_p1 = start_pos;
        rot(&mut start_p0, 0.5);
        rot(&mut start_p1, -0.5);
        start_p0 += center;
        start_p1 += center;

        rot(&mut end_pos, 1.0);
        let mut end_p0 = end_pos;
        let mut end_p1 = end_pos;
        rot(&mut end_p0, -0.5);
        rot(&mut end_p1, 0.5);
        end_p0 += center;
        end_p1 += center;

        let center = center + pos;
        let size = radius(10.0);
        let selected = {
            let trapez = geo::Polygon::new(
                vec![
                    (start_p0.x, start_p0.y),
                    (start_p1.x, start_p1.y),
                    (end_p0.x, end_p0.y),
                    (end_p1.x, end_p1.y),
                ]
                .into_iter()
                .map(geo::Point::from)
                .collect(),
                vec![],
            );
            trapez.contains(&geo::Point::new(mouse.x as f32, mouse.y as f32))
        };
        if selected {
            pipe.user_data
                .events
                .push(EmoteWheelEvent::EyeSelected(eye));
        }
        let val = if selected {
            ui.ctx().animate_value_with_time(
                Id::new(format!("emote-wheel-anims-eyes-{}", eye as usize)),
                1.5,
                0.15,
            )
        } else {
            ui.ctx().animate_value_with_time(
                Id::new(format!("emote-wheel-anims-eyes-{}", eye as usize)),
                1.0,
                0.15,
            )
        };
        render_tee_for_ui(
            pipe.user_data.canvas_handle,
            pipe.user_data.skin_container,
            pipe.user_data.render_tee,
            ui,
            ui_state,
            rect,
            None,
            pipe.user_data.skin,
            pipe.user_data.skin_info.as_ref(),
            center,
            size * val,
            eye,
        );
    }

    ui.painter().circle(
        egui::pos2(mouse.x as f32, mouse.y as f32),
        10.0,
        Color32::from_white_alpha(100),
        Stroke::NONE,
    );
}
