use egui::Modifiers;
use math::math::{
    distance,
    vector::{ffixed, fvec2, vec2},
};

use crate::map::EditorMap;

pub fn in_radius(pos1: &fvec2, pos2: &vec2, radius: f32) -> bool {
    distance(&vec2::new(pos1.x.to_num(), pos1.y.to_num()), pos2) < radius
}

pub fn rotate(center: &fvec2, rotation: ffixed, points: &mut [fvec2]) {
    let c = ffixed::from_num(rotation.to_num::<f64>().cos());
    let s = ffixed::from_num(rotation.to_num::<f64>().sin());

    for point in points.iter_mut() {
        let x = point.x - center.x;
        let y = point.y - center.y;
        *point = fvec2 {
            x: x * c - y * s + center.x,
            y: x * s + y * c + center.y,
        };
    }
}

pub fn align_pos(map: &EditorMap, modifiers: &Modifiers, mut pos: vec2) -> Option<vec2> {
    if let Some(grid_size) = modifiers
        .alt
        .then_some(map.user.options.render_grid)
        .flatten()
    {
        let grid_size = grid_size as f32;
        fn round_mod(v: f32, rhs: f32) -> f32 {
            let r = v.rem_euclid(rhs);

            if r <= rhs / 2.0 {
                -r
            } else {
                rhs - r
            }
        }
        pos.x += round_mod(pos.x, grid_size);
        pos.y += round_mod(pos.y, grid_size);
        Some(pos)
    } else {
        None
    }
}
