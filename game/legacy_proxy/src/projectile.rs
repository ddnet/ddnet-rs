use math::math::vector::vec2;

pub fn get_vel(now: i32, start_tick: i32, dir: vec2, speed: f32, curvature: f32) -> vec2 {
    let mut time = now.abs_diff(start_tick) as f32 / 50.0;
    time *= speed;
    let x = dir.x;
    let y = dir.y + curvature / 10000.0 * 2.0 * time;
    vec2::new(x, y)
}

fn get_dir(now: i32, start_tick: i32, dir: vec2, speed: f32, curvature: f32) -> vec2 {
    let mut time = now.abs_diff(start_tick) as f32 / 50.0;
    time *= speed;
    let x = dir.x * time;
    let y = dir.y * time + curvature / 10000.0 * (time * time);
    vec2::new(x, y)
}

pub fn get_pos(
    pos: vec2,
    dir: vec2,
    speed: f32,
    curvature: f32,
    now: i32,
    start_tick: i32,
) -> vec2 {
    let dir = get_dir(now, start_tick, dir, speed, curvature);
    let x = pos.x + dir.x;
    let y = pos.y + dir.y;
    vec2::new(x, y)
}
