use std::time::Duration;

use math::math::{vector::vec2, PI};

use super::animation::{TeeAnimation, TeeAnimationFrame, TeeAnimationFrames};

pub fn base_anim() -> TeeAnimation {
    let mut anim = TeeAnimation::default();
    anim.body.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(0.0, -2.0 / 32.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.left_foot.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(0.0, 5.0 / 32.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.right_foot.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(0.0, 5.0 / 32.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.left_eye.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.right_eye.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim
}

pub fn idle_anim() -> TeeAnimation {
    let mut anim = TeeAnimation::default();
    anim.left_foot.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(-7.0 / 64.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.right_foot.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(7.0 / 64.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim
}

pub fn inair_anim() -> TeeAnimation {
    let mut anim = TeeAnimation::default();
    anim.left_foot.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(-3.0 / 64.0, 0.0),
            rotation: -0.1 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.right_foot.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(3.0 / 64.0, 0.0),
            rotation: -0.1 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim
}

pub fn sit_left_anim() -> TeeAnimation {
    let mut anim = TeeAnimation::default();
    anim.body.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(0.0, 3.0 / 64.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.left_foot.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(-6.0 / 32.0, 0.0),
            rotation: -0.1 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.right_foot.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(-4.0 / 32.0, 0.0),
            rotation: -0.1 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim
}

pub fn sit_right_anim() -> TeeAnimation {
    let mut anim = TeeAnimation::default();
    anim.body.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(0.0, 3.0 / 64.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.left_foot.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(6.0 / 32.0, 0.0),
            rotation: -0.1 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.right_foot.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(4.0 / 32.0, 0.0),
            rotation: -0.1 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim
}

pub fn walk_anim() -> TeeAnimation {
    let mut anim = TeeAnimation::default();
    // body
    anim.body.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.body.frames.push((
        Duration::from_millis(200),
        TeeAnimationFrame {
            pos: vec2::new(0.0, -1.0 / 64.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.body.frames.push((
        Duration::from_millis(400),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.body.frames.push((
        Duration::from_millis(600),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.body.frames.push((
        Duration::from_millis(800),
        TeeAnimationFrame {
            pos: vec2::new(0.0, -1.0 / 64.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.body.frames.push((
        Duration::from_millis(1000),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));

    // left foot
    anim.left_foot.frames.push((
        Duration::from_millis(0),
        TeeAnimationFrame {
            pos: vec2::new(4.0 / 32.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.left_foot.frames.push((
        Duration::from_millis(200),
        TeeAnimationFrame {
            pos: vec2::new(-4.0 / 32.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.left_foot.frames.push((
        Duration::from_millis(400),
        TeeAnimationFrame {
            pos: vec2::new(-5.0 / 32.0, -2.0 / 32.0),
            rotation: 0.2 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.left_foot.frames.push((
        Duration::from_millis(600),
        TeeAnimationFrame {
            pos: vec2::new(-4.0 / 32.0, -4.0 / 32.0),
            rotation: 0.3 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.left_foot.frames.push((
        Duration::from_millis(800),
        TeeAnimationFrame {
            pos: vec2::new(2.0 / 32.0, -2.0 / 32.0),
            rotation: -0.2 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.left_foot.frames.push((
        Duration::from_millis(1000),
        TeeAnimationFrame {
            pos: vec2::new(4.0 / 32.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));

    // right foot
    anim.right_foot.frames.push((
        Duration::from_millis(0),
        TeeAnimationFrame {
            pos: vec2::new(-5.0 / 32.0, -2.0 / 32.0),
            rotation: 0.2 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.right_foot.frames.push((
        Duration::from_millis(200),
        TeeAnimationFrame {
            pos: vec2::new(-4.0 / 32.0, -4.0 / 32.0),
            rotation: 0.3 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.right_foot.frames.push((
        Duration::from_millis(400),
        TeeAnimationFrame {
            pos: vec2::new(2.0 / 32.0, -2.0 / 32.0),
            rotation: -0.2 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.right_foot.frames.push((
        Duration::from_millis(600),
        TeeAnimationFrame {
            pos: vec2::new(4.0 / 32.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.right_foot.frames.push((
        Duration::from_millis(800),
        TeeAnimationFrame {
            pos: vec2::new(4.0 / 32.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.right_foot.frames.push((
        Duration::from_millis(1000),
        TeeAnimationFrame {
            pos: vec2::new(-5.0 / 32.0, -2.0 / 32.0),
            rotation: 0.2 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim
}

pub fn run_left_anim() -> TeeAnimation {
    let mut anim = TeeAnimation::default();
    // body
    anim.body.frames.push((
        Duration::ZERO,
        TeeAnimationFrame {
            pos: vec2::new(0.0, -1.0 / 64.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.body.frames.push((
        Duration::from_millis(200),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.body.frames.push((
        Duration::from_millis(400),
        TeeAnimationFrame {
            pos: vec2::new(0.0, -1.0 / 64.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.body.frames.push((
        Duration::from_millis(600),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.body.frames.push((
        Duration::from_millis(800),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.body.frames.push((
        Duration::from_millis(1000),
        TeeAnimationFrame {
            pos: vec2::new(0.0, -1.0 / 64.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));

    // left foot
    anim.left_foot.frames.push((
        Duration::from_millis(0),
        TeeAnimationFrame {
            pos: vec2::new(9.0 / 32.0, -4.0 / 32.0),
            rotation: -0.27 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.left_foot.frames.push((
        Duration::from_millis(200),
        TeeAnimationFrame {
            pos: vec2::new(3.0 / 32.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.left_foot.frames.push((
        Duration::from_millis(400),
        TeeAnimationFrame {
            pos: vec2::new(-7.0 / 64.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.left_foot.frames.push((
        Duration::from_millis(600),
        TeeAnimationFrame {
            pos: vec2::new(-13.0 / 64.0, -4.5 / 64.0),
            rotation: 0.05 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.left_foot.frames.push((
        Duration::from_millis(800),
        TeeAnimationFrame {
            pos: vec2::new(0.0, -4.0 / 32.0),
            rotation: -0.2 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.left_foot.frames.push((
        Duration::from_millis(1000),
        TeeAnimationFrame {
            pos: vec2::new(9.0 / 32.0, -4.0 / 32.0),
            rotation: -0.27 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));

    // right foot
    anim.right_foot.frames.push((
        Duration::from_millis(0),
        TeeAnimationFrame {
            pos: vec2::new(-11.0 / 64.0, -2.5 / 64.0),
            rotation: 0.05 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.right_foot.frames.push((
        Duration::from_millis(200),
        TeeAnimationFrame {
            pos: vec2::new(-7.0 / 32.0, -5.0 / 64.0),
            rotation: 0.1 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.right_foot.frames.push((
        Duration::from_millis(400),
        TeeAnimationFrame {
            pos: vec2::new(11.0 / 64.0, -4.0 / 32.0),
            rotation: -0.3 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.right_foot.frames.push((
        Duration::from_millis(600),
        TeeAnimationFrame {
            pos: vec2::new(9.0 / 32.0, -4.0 / 32.0),
            rotation: -0.27 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.right_foot.frames.push((
        Duration::from_millis(800),
        TeeAnimationFrame {
            pos: vec2::new(3.0 / 64.0, 0.0),
            rotation: 0.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.right_foot.frames.push((
        Duration::from_millis(1000),
        TeeAnimationFrame {
            pos: vec2::new(-11.0 / 64.0, -2.5 / 64.0),
            rotation: 0.05 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim
}

pub fn run_right_anim() -> TeeAnimation {
    let mut anim = run_left_anim();
    let (duration, anim_points): (Vec<_>, Vec<_>) = anim.body.frames.into_iter().unzip();
    anim.body.frames = duration
        .into_iter()
        .zip(anim_points.into_iter().rev())
        .map(|mut f| {
            f.1.pos.x *= -1.0;
            f.1.rotation *= -1.0;
            f
        })
        .collect();
    let (duration, anim_points): (Vec<_>, Vec<_>) = anim.left_foot.frames.into_iter().unzip();
    anim.left_foot.frames = duration
        .into_iter()
        .zip(anim_points.into_iter().rev())
        .map(|mut f| {
            f.1.pos.x *= -1.0;
            f.1.rotation *= -1.0;
            f
        })
        .collect();
    let (duration, anim_points): (Vec<_>, Vec<_>) = anim.right_foot.frames.into_iter().unzip();
    anim.right_foot.frames = duration
        .into_iter()
        .zip(anim_points.into_iter().rev())
        .map(|mut f| {
            f.1.pos.x *= -1.0;
            f.1.rotation *= -1.0;
            f
        })
        .collect();
    anim
}

pub fn hammer_swing_anim() -> TeeAnimationFrames {
    let mut anim = TeeAnimationFrames::default();
    anim.frames.push((
        Duration::from_millis(0),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: -0.10 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.frames.push((
        Duration::from_millis(300),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: 0.25 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.frames.push((
        Duration::from_millis(400),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: 0.30 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.frames.push((
        Duration::from_millis(500),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: 0.25 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.frames.push((
        Duration::from_millis(1000),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: -0.10 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim
}

pub fn ninja_swing_anim() -> TeeAnimationFrames {
    let mut anim = TeeAnimationFrames::default();
    anim.frames.push((
        Duration::from_millis(0),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: -0.25 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.frames.push((
        Duration::from_millis(100),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: -0.05 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.frames.push((
        Duration::from_millis(150),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: 0.35 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.frames.push((
        Duration::from_millis(420),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: 0.40 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.frames.push((
        Duration::from_millis(500),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: 0.35 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim.frames.push((
        Duration::from_millis(1000),
        TeeAnimationFrame {
            pos: vec2::new(0.0, 0.0),
            rotation: -0.25 * PI * 2.0,
            scale: vec2::new(1.0, 1.0),
        },
    ));
    anim
}
