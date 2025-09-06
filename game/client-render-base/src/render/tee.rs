use client_containers::skins::{Skin, SkinMetrics, SkinTextures};
use game_interface::types::render::character::TeeEye;
use graphics::{
    graphics::graphics::Graphics,
    handles::quad_container::quad_container::QuadContainer,
    quad_container::Quad,
    streaming::{quad_scope_begin, rotate_pos},
};

use graphics_types::rendering::{ColorRgba, State};
use hiarc::{Hiarc, hiarc_safer_rc_refcell};
use math::math::{
    angle,
    vector::{ubvec4, vec2},
};

use super::animation::AnimState;

#[derive(Debug, Hiarc, Clone, Copy)]
pub enum TeeRenderSkinColor {
    Original,
    Colorable(ColorRgba),
}

impl TeeRenderSkinColor {
    pub fn unwrap(self, alpha: f32) -> ColorRgba {
        match self {
            Self::Original => ColorRgba::new(1.0, 1.0, 1.0, alpha),
            Self::Colorable(mut color) => {
                color.a *= alpha;
                color
            }
        }
    }
}

impl From<ColorRgba> for TeeRenderSkinColor {
    fn from(value: ColorRgba) -> Self {
        Self::Colorable(value)
    }
}

impl From<ubvec4> for TeeRenderSkinColor {
    fn from(value: ubvec4) -> Self {
        let value: ColorRgba = value.into();
        Self::Colorable(value)
    }
}

trait RenderSkin {
    fn render_skin(&self, color: &TeeRenderSkinColor) -> &SkinTextures;
}

impl RenderSkin for Skin {
    fn render_skin(&self, color: &TeeRenderSkinColor) -> &SkinTextures {
        match color {
            TeeRenderSkinColor::Original => &self.textures,
            TeeRenderSkinColor::Colorable(_) => &self.grey_scaled_textures,
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct TeeRenderInfo {
    pub color_body: TeeRenderSkinColor,
    pub color_feet: TeeRenderSkinColor,

    pub got_air_jump: bool,
    pub feet_flipped: bool,

    pub eye_left: TeeEye,
    pub eye_right: TeeEye,

    pub size: f32,
}

#[derive(Debug, Hiarc)]
pub struct TeeRenderHand {
    pub pos: vec2,
    pub dir: vec2,
    pub rot_offset: f32,
    pub offset_after_rot: vec2,
    pub scale: f32,
}

#[derive(Debug, Hiarc)]
pub struct TeeRenderHands {
    /// this is considered the hook hand
    pub left: Option<TeeRenderHand>,
    /// this is considered the weapon hand
    pub right: Option<TeeRenderHand>,
}

fn get_render_tee_body_scale(base_size: f32, anim: &AnimState) -> vec2 {
    let mut body_scale = vec2::new(1.0, 1.0) * base_size; // TODO: g_Config.m_ClFatSkins ? BaseSize * 1.3f : BaseSize;
    body_scale.x *= anim.body.scale.x;
    body_scale.y *= anim.body.scale.y;
    body_scale
}

/// adding the result of this function to a position will move
/// the position so that the tee is centered, note that the position
/// is assumed to be the center of the quad too
pub fn offset_to_mid(metrics: &SkinMetrics, anim: &AnimState, info: &TeeRenderInfo) -> vec2 {
    let scale = info.size;

    let math = RenderTeeMath::new(
        anim,
        info,
        &TeeRenderHands {
            left: None,
            right: None,
        },
        &Default::default(),
        &vec2::default(),
    );

    let body_height = metrics.body.height().to_num::<f32>() * math.body_scale.y;
    let body_offset_y = metrics.body.y().to_num::<f32>() * math.body_scale.y;

    // -0.5 is the assumed min relative position for the quad
    let mut min_y = -0.5 * scale;
    // the body pos shifts the body away from center
    min_y += math.body_pos.y;
    // the actual body is smaller though, because it doesn't use the full skin image in most cases
    min_y += body_offset_y;

    let left_feet_height = metrics.feet.height().to_num::<f32>() * 0.5 * math.foot_left_scale.y;
    let right_feet_height = metrics.feet.height().to_num::<f32>() * 0.5 * math.foot_right_scale.y;
    let left_feet_offset_y = metrics.feet.y().to_num::<f32>() * 0.5 * math.foot_left_scale.y;
    let right_feet_offset_y = metrics.feet.y().to_num::<f32>() * 0.5 * math.foot_right_scale.y;

    // max_y builds up from the min_y
    let max_y = min_y + body_height;
    // if the body is smaller than the total feet offset, use feet
    let max_y = max_y.max(
        -0.25 * scale
            + (math.foot_left_pos.y + left_feet_offset_y + left_feet_height)
                .max(math.foot_right_pos.y + right_feet_offset_y + right_feet_height),
    );

    // now we got the full rendered size
    let full_height = max_y - min_y;

    // next step is to calculate the offset that was created compared to the assumed relative position
    let mid_of_rendered = min_y + full_height / 2.0;

    // negative value, because the calculation that uses this offset should work with addition.
    vec2::new(0.0, -mid_of_rendered)
}

#[derive(Debug, Hiarc)]
pub struct RenderTeeHandMath {
    pub pos: vec2,
    pub scale: vec2,
    pub rotation: f32,
}

impl RenderTeeHandMath {
    pub fn new(pos: &vec2, base_size: f32, hand: &TeeRenderHand) -> RenderTeeHandMath {
        let mut hand_pos = hand.pos + hand.dir / 32.0;
        let mut angle = angle(&hand.dir);
        if hand.dir.x < 0.0 {
            angle -= hand.rot_offset;
        } else {
            angle += hand.rot_offset;
        }

        let dir_x = hand.dir;
        let mut dir_y = vec2::new(-hand.dir.y, hand.dir.x);

        if hand.dir.x < 0.0 {
            dir_y = -dir_y;
        }

        hand_pos += dir_x * hand.offset_after_rot.x;
        hand_pos += dir_y * hand.offset_after_rot.y;

        RenderTeeHandMath {
            pos: *pos + hand_pos,
            scale: vec2::new(base_size, base_size),
            rotation: angle,
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct RenderTeeMath {
    pub body_pos: vec2,
    pub body_scale: vec2,
    pub body_rotation: f32,
    pub eye_left_pos: vec2,
    pub eye_left_scale: vec2,
    pub eye_left_rotation: f32,
    pub eye_right_pos: vec2,
    pub eye_right_scale: vec2,
    pub eye_right_rotation: f32,
    pub foot_left_pos: vec2,
    pub foot_left_scale: vec2,
    pub foot_left_rotation: f32,
    pub foot_right_pos: vec2,
    pub foot_right_scale: vec2,
    pub foot_right_rotation: f32,
    pub hand_left: Option<RenderTeeHandMath>,
    pub hand_right: Option<RenderTeeHandMath>,
}

impl RenderTeeMath {
    pub fn new(
        anim: &AnimState,
        info: &TeeRenderInfo,
        hands: &TeeRenderHands,
        dir: &vec2,
        pos: &vec2,
    ) -> RenderTeeMath {
        let direction = *dir;
        let position = *pos;

        // general
        let render_size = info.size;

        // body
        let body_pos = position
            + vec2 {
                x: anim.body.pos.x,
                y: anim.body.pos.y,
            } * render_size;

        let body_scale = get_render_tee_body_scale(render_size, anim);
        let body_rotation = anim.body.rotation;

        // eye
        let eye_scale = render_size * 0.40;
        let eye_left_height = if info.eye_left == TeeEye::Blink {
            render_size * 0.15
        } else {
            eye_scale
        };
        let eye_right_height = if info.eye_right == TeeEye::Blink {
            render_size * 0.15
        } else {
            eye_scale
        };
        let eye_separation = (0.075 - 0.010 * direction.x.abs()) * render_size;
        let offset = vec2 {
            x: direction.x * 0.125,
            y: -0.05 + direction.y * 0.10,
        } * render_size;

        let eye_left = rotate_pos(
            &Default::default(),
            body_rotation,
            vec2::new(-eye_separation + offset.x, offset.y),
        );
        let eye_pos_left = body_pos + eye_left;

        let eye_right = rotate_pos(
            &Default::default(),
            body_rotation,
            vec2::new(eye_separation + offset.x, offset.y),
        );
        let eye_pos_right = body_pos + eye_right;

        let eye_scale_left = vec2 {
            x: eye_scale / 0.4 * anim.left_eye.scale.x,
            y: eye_left_height / 0.4 * anim.left_eye.scale.y,
        };

        let eye_size_right = vec2 {
            x: -eye_scale / 0.4 * anim.right_eye.scale.x,
            y: eye_right_height / 0.4 * anim.right_eye.scale.y,
        };

        let eye_rotation_left = anim.left_eye.rotation + body_rotation;
        let eye_rotation_right = anim.right_eye.rotation - body_rotation;

        // feet
        let feet_width = render_size;
        let feet_height = render_size / 2.0;

        let foot = &anim.left_foot;
        let feet_pos_left = vec2 {
            x: position.x + foot.pos.x * render_size,
            y: position.y + foot.pos.y * render_size,
        };
        let foot_left_rotation = foot.rotation;

        let foot = &anim.right_foot;
        let feet_pos_right = vec2 {
            x: position.x + foot.pos.x * render_size,
            y: position.y + foot.pos.y * render_size,
        };
        let foot_right_rotation = foot.rotation;

        let hand_left = hands
            .left
            .as_ref()
            .map(|hand| RenderTeeHandMath::new(&position, render_size, hand));
        let hand_right = hands
            .right
            .as_ref()
            .map(|hand| RenderTeeHandMath::new(&position, render_size, hand));

        RenderTeeMath {
            body_pos,
            body_scale: vec2 {
                x: body_scale.x,
                y: body_scale.y,
            },
            body_rotation,
            eye_left_pos: eye_pos_left,
            eye_left_scale: eye_scale_left,
            eye_left_rotation: eye_rotation_left,
            eye_right_pos: eye_pos_right,
            eye_right_scale: eye_size_right,
            eye_right_rotation: eye_rotation_right,
            foot_left_pos: feet_pos_left,
            foot_left_scale: vec2 {
                x: feet_width,
                y: feet_height * 2.0,
            },
            foot_left_rotation,
            foot_right_pos: feet_pos_right,
            foot_right_scale: vec2 {
                x: feet_width,
                y: feet_height * 2.0,
            },
            foot_right_rotation,
            hand_left,
            hand_right,
        }
    }
}

#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc)]
pub struct RenderTee {
    tee_quad_container: QuadContainer,

    body_offset: usize,
    body_outline_offset: usize,
    eye_offset: usize,
    foot_offset: usize,
    foot_outline_offset: usize,
    mirrored_foot_offset: usize,
    mirrored_outline_foot_offset: usize,
    hand_offset: usize,
}

#[hiarc_safer_rc_refcell]
impl RenderTee {
    pub fn new(graphics: &Graphics) -> Self {
        let mut quads: Vec<Quad> = Default::default();

        let body_offset = quads.len();
        quads.push(Quad::new().from_size_centered(1.0));

        let body_outline_offset = body_offset;

        let eye_offset = quads.len();
        quads.push(Quad::new().from_size_centered(0.4));

        let foot_offset = quads.len();
        quads.push(Quad::new().from_rect(-1.0 / 2.0, -1.0 / 2.0 / 2.0, 1.0, 1.0 / 2.0));
        let foot_outline_offset = foot_offset;

        let mirrored_foot_offset = quads.len();
        quads.push(
            Quad::new()
                .from_rect(-1.0 / 2.0, -1.0 / 2.0 / 2.0, 1.0, 1.0 / 2.0)
                .with_tex(&[
                    vec2 { x: 1.0, y: 0.0 },
                    vec2 { x: 0.0, y: 0.0 },
                    vec2 { x: 0.0, y: 1.0 },
                    vec2 { x: 1.0, y: 1.0 },
                ]),
        );
        let mirrored_outline_foot_offset = mirrored_foot_offset;

        let hand_offset = quads.len();
        quads.push(Quad::new().from_size_centered(10.0 / 32.0));

        let tee_quad_container = graphics.quad_container_handle.create_quad_container(quads);

        Self {
            tee_quad_container,
            body_offset,
            body_outline_offset,
            eye_offset,
            foot_offset,
            foot_outline_offset,
            mirrored_foot_offset,
            mirrored_outline_foot_offset,
            hand_offset,
        }
    }

    pub fn render_tee_body(
        &self,
        state: &State,
        body_pos: &vec2,
        body_scale: &vec2,
        body_rotation: f32,
        body_color: &TeeRenderSkinColor,
        alpha: f32,
        skin: &Skin,
        outline: bool,
    ) {
        let mut quad_scope = quad_scope_begin();
        quad_scope.set_state(state);
        quad_scope.set_rotation(body_rotation);

        let render_skin = skin.render_skin(body_color);
        let body_color = body_color.unwrap(alpha);
        // draw body
        quad_scope.set_colors_from_single(body_color.r, body_color.g, body_color.b, body_color.a);
        let texture = if outline {
            &render_skin.body_outline
        } else {
            &render_skin.body
        };
        self.tee_quad_container.render_quad_container_as_sprite(
            if !outline {
                self.body_offset
            } else {
                self.body_outline_offset
            },
            body_pos.x,
            body_pos.y,
            body_scale.x,
            body_scale.y,
            quad_scope,
            texture.into(),
        );
    }

    pub fn render_tee_eyes(
        &self,
        state: &State,
        eye_left: TeeEye,
        eye_right: TeeEye,
        eye_left_pos: &vec2,
        eye_left_scale: &vec2,
        eye_left_rotation: f32,
        eye_right_pos: &vec2,
        eye_right_scale: &vec2,
        eye_right_rotation: f32,
        eye_left_color: &TeeRenderSkinColor,
        eye_right_color: &TeeRenderSkinColor,
        alpha: f32,
        skin: &Skin,
    ) {
        let quad_offset = self.eye_offset;
        let tee_eye_index = eye_left as usize - TeeEye::Normal as usize;

        let mut quad_scope = quad_scope_begin();
        quad_scope.set_state(state);
        quad_scope.set_rotation(eye_left_rotation);
        let render_skin = skin.render_skin(eye_left_color);
        let eye_left_color = eye_left_color.unwrap(alpha);
        let texture = &render_skin.left_eyes[tee_eye_index];
        quad_scope.set_colors_from_single(
            eye_left_color.r,
            eye_left_color.g,
            eye_left_color.b,
            eye_left_color.a,
        );
        self.tee_quad_container.render_quad_container_as_sprite(
            quad_offset,
            eye_left_pos.x,
            eye_left_pos.y,
            eye_left_scale.x,
            eye_left_scale.y,
            quad_scope,
            texture.into(),
        );
        let tee_eye_index = eye_right as usize - TeeEye::Normal as usize;
        let mut quad_scope = quad_scope_begin();
        quad_scope.set_state(state);
        quad_scope.set_rotation(eye_right_rotation);
        let render_skin = skin.render_skin(eye_right_color);
        let eye_right_color = eye_right_color.unwrap(alpha);
        let texture = &render_skin.right_eyes[tee_eye_index];
        quad_scope.set_colors_from_single(
            eye_right_color.r,
            eye_right_color.g,
            eye_right_color.b,
            eye_right_color.a,
        );
        self.tee_quad_container.render_quad_container_as_sprite(
            quad_offset,
            eye_right_pos.x,
            eye_right_pos.y,
            eye_right_scale.x,
            eye_right_scale.y,
            quad_scope,
            texture.into(),
        );
    }

    pub fn render_tee_feet(
        &self,
        state: &State,
        dir: &vec2,
        foot_left_pos: &vec2,
        foot_left_scale: &vec2,
        foot_left_rotation: f32,
        foot_right_pos: &vec2,
        foot_right_scale: &vec2,
        foot_right_rotation: f32,
        foot_left_color: &TeeRenderSkinColor,
        foot_right_color: &TeeRenderSkinColor,
        alpha: f32,
        skin: &Skin,
        outline: bool,
        flipped: bool,
        got_air_jump: bool,
        is_left_foot: bool,
    ) {
        let mut quad_offset = if !outline {
            self.foot_offset
        } else {
            self.foot_outline_offset
        };
        if dir.x < 0.0 && flipped {
            quad_offset = if !outline {
                self.mirrored_foot_offset
            } else {
                self.mirrored_outline_foot_offset
            };
        }

        let foot_rotation = if is_left_foot {
            foot_left_rotation
        } else {
            foot_right_rotation
        };

        let mut quad_scope = quad_scope_begin();
        quad_scope.set_state(state);
        quad_scope.set_rotation(foot_rotation);

        let indicate = !got_air_jump;
        let mut color_scale = 1.0;

        if !outline && indicate {
            color_scale = 0.5;
        }

        let foot_color = if is_left_foot {
            foot_left_color
        } else {
            foot_right_color
        };

        let render_skin = skin.render_skin(foot_color);
        let foot_color = foot_color.unwrap(alpha);
        quad_scope.set_colors_from_single(
            foot_color.r * color_scale,
            foot_color.g * color_scale,
            foot_color.b * color_scale,
            foot_color.a,
        );

        let texture = if outline {
            if !is_left_foot {
                &render_skin.left_foot_outline
            } else {
                &render_skin.right_foot_outline
            }
        } else if !is_left_foot {
            &render_skin.left_foot
        } else {
            &render_skin.right_foot
        };
        let foot_pos = if is_left_foot {
            foot_left_pos
        } else {
            foot_right_pos
        };
        let foot_scale = if is_left_foot {
            foot_left_scale
        } else {
            foot_right_scale
        };
        self.tee_quad_container.render_quad_container_as_sprite(
            quad_offset,
            foot_pos.x,
            foot_pos.y,
            foot_scale.x,
            foot_scale.y,
            quad_scope,
            texture.into(),
        );
    }

    pub fn render_tee_hand(
        &self,
        hand: &RenderTeeHandMath,
        color: &TeeRenderSkinColor,
        skin: &Skin,
        alpha: f32,
        state: &State,
    ) {
        let mut quad_scope = quad_scope_begin();
        quad_scope.set_state(state);
        let render_skin = skin.render_skin(color);
        let color = color.unwrap(alpha);
        quad_scope.set_colors_from_single(color.r, color.g, color.b, color.a);

        // two passes
        for i in 0..2 {
            quad_scope.set_rotation(hand.rotation);
            let texture = if i == 0 {
                &render_skin.left_hand_outline
            } else {
                &render_skin.left_hand
            };
            self.tee_quad_container.render_quad_container_as_sprite(
                self.hand_offset,
                hand.pos.x,
                hand.pos.y,
                hand.scale.x,
                hand.scale.y,
                quad_scope,
                texture.into(),
            );
        }
    }

    pub fn render_tee_from_math(
        &self,
        tee_math: &RenderTeeMath,
        skin: &Skin,
        info: &TeeRenderInfo,
        dir: &vec2,
        alpha: f32,
        state: &State,
    ) {
        let RenderTeeMath {
            body_pos,
            body_scale,
            body_rotation,
            eye_left_pos,
            eye_left_scale,
            eye_left_rotation,
            eye_right_pos,
            eye_right_scale,
            eye_right_rotation,
            foot_left_pos,
            foot_left_scale,
            foot_left_rotation,
            foot_right_pos,
            foot_right_scale,
            foot_right_rotation,
            hand_left,
            hand_right,
        } = tee_math;

        if let Some(left_hand) = &hand_left {
            self.render_tee_hand(left_hand, &info.color_body, skin, alpha, state);
        }
        if let Some(right_hand) = &hand_right {
            self.render_tee_hand(right_hand, &info.color_body, skin, alpha, state);
        }

        // first pass we draw the outline
        // second pass we draw the filling
        for p in 0..2 {
            let outline = if p == 0 { 1 } else { 0 };

            for f in 0..2 {
                if f == 1 {
                    // draw body
                    self.render_tee_body(
                        state,
                        body_pos,
                        body_scale,
                        *body_rotation,
                        &info.color_body,
                        alpha,
                        skin,
                        outline == 1,
                    );

                    // draw eyes
                    if p == 1 {
                        self.render_tee_eyes(
                            state,
                            info.eye_left,
                            info.eye_right,
                            eye_left_pos,
                            eye_left_scale,
                            *eye_left_rotation,
                            eye_right_pos,
                            eye_right_scale,
                            *eye_right_rotation,
                            &info.color_body,
                            &info.color_body,
                            alpha,
                            skin,
                        );
                    }
                }

                // draw feet
                self.render_tee_feet(
                    state,
                    dir,
                    foot_left_pos,
                    foot_left_scale,
                    *foot_left_rotation,
                    foot_right_pos,
                    foot_right_scale,
                    *foot_right_rotation,
                    &info.color_feet,
                    &info.color_feet,
                    alpha,
                    skin,
                    outline == 1,
                    info.feet_flipped,
                    info.got_air_jump,
                    f == 0,
                )
            }
        }
    }

    pub fn render_tee(
        &self,
        anim: &AnimState,
        skin: &Skin,
        info: &TeeRenderInfo,
        hands: &TeeRenderHands,
        dir: &vec2,
        pos: &vec2,
        alpha: f32,
        state: &State,
    ) {
        self.render_tee_from_math(
            &RenderTeeMath::new(anim, info, hands, dir, pos),
            skin,
            info,
            dir,
            alpha,
            state,
        )
    }
}
