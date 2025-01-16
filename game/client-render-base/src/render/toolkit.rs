use std::time::Duration;

use client_containers::{
    container::ContainerKey,
    hooks::{Hook, HookContainer},
    ninja::Ninja,
    weapons::Weapons,
};
use game_base::game_types::intra_tick_time_to_ratio;
use game_interface::types::{
    emoticons::{EnumCount, IntoEnumIterator},
    game::NonZeroGameTickType,
    id_types::CharacterId,
    render::character::{CharacterRenderInfo, HookCollisionLine, HookCollisionLineColor},
    weapons::WeaponType,
};
use graphics::{
    graphics::graphics::Graphics,
    handles::{
        quad_container::quad_container::QuadContainer,
        stream::stream::{GraphicsStreamHandle, LinesStreamHandle, StreamedSprites},
        stream_types::StreamedLine,
    },
    quad_container::Quad,
    streaming::{quad_scope_begin, DrawScope},
};
use graphics_types::{
    commands::RenderSpriteInfo,
    rendering::{ColorRgba, State},
};
use hiarc::{hi_closure, hiarc_safer_rc_refcell, Hiarc};
use math::math::{
    angle, distance, mix, normalize,
    vector::{dvec2, ubvec4, vec2},
    Rng, RngSlice, PI, PI_F64,
};
use vanilla::weapons::definitions::weapon_def::{
    get_ninja_sprite_scale, get_scale, get_weapon_sprite_scale, get_weapon_visual_scale,
    NINJA_PICKUP_VISUAL_SIZE, NINJA_WEAPON_VISUAL_SIZE,
};

use crate::map::render_pipe::GameTimeInfo;

use super::{
    animation::{AnimState, TeeAnimationFrame},
    default_anim::{hammer_swing_anim, ninja_swing_anim},
    effects::Effects,
    particle_manager::ParticleManager,
    tee::TeeRenderHand,
    weapons::{
        WeaponGrenadeSpec, WeaponGunSpec, WeaponHammerSpec, WeaponLaserSpec, WeaponNinjaSpec,
        WeaponShotgunSpec,
    },
};

/// Render weapons, hook, ninja or similar stuff
#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc)]
pub struct ToolkitRender {
    stream_handle: GraphicsStreamHandle,

    quad_container: QuadContainer,

    hook_chain_quad_offset: usize,
    hook_head_quad_offset: usize,

    weapon_quad_offsets: [(usize, usize); WeaponType::COUNT],
    weapon_muzzle_quad_offsets: [(usize, usize); WeaponType::COUNT],

    ninja_quad_offsets: (usize, usize),
    ninja_muzzle_quad_offset: usize,

    rng: Rng,
}

pub fn get_weapon_as_quad(weapon: &WeaponType) -> Quad {
    let size = get_weapon_visual_scale(weapon);
    let scale = get_weapon_sprite_scale(weapon);
    Quad::new().from_width_and_height_centered(size * scale.0, size * scale.1)
}

pub fn get_ninja_as_quad(as_pickup: bool) -> Quad {
    let size = if as_pickup {
        NINJA_PICKUP_VISUAL_SIZE
    } else {
        NINJA_WEAPON_VISUAL_SIZE
    };
    let scale = get_ninja_sprite_scale();
    Quad::new().from_width_and_height_centered(size * scale.0, size * scale.1)
}

pub fn get_sprite_scale_impl(w: u32, h: u32) -> (f32, f32) {
    let f = ((w * w + h * h) as f32).sqrt();
    (w as f32 / f, h as f32 / f)
}

pub fn pickup_scale() -> (f32, f32) {
    let grid_size = (2, 2);
    get_sprite_scale_impl(grid_size.0, grid_size.1)
}

#[hiarc_safer_rc_refcell]
impl ToolkitRender {
    pub fn new(graphics: &Graphics) -> Self {
        let mut quads: Vec<Quad> = Default::default();

        let mut weapon_quad_offsets: [(usize, usize); WeaponType::COUNT] = Default::default();

        WeaponType::iter().for_each(|weapon| {
            let mut quad = get_weapon_as_quad(&weapon).with_color(&ubvec4::new(255, 255, 255, 255));

            quad = quad.with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0));
            let offset_normal = quads.len();
            quads.push(quad);
            quad = quad.with_uv_from_points(&vec2::new(0.0, 1.0), &vec2::new(1.0, 0.0));
            let offset_flipped = quads.len();
            quads.push(quad);
            weapon_quad_offsets[weapon as usize] = (offset_normal, offset_flipped);
        });

        let mut weapon_muzzle_quad_offsets: [(usize, usize); WeaponType::COUNT] =
            Default::default();

        WeaponType::iter().for_each(|weapon| {
            let (scale_x, scale_y);
            if weapon == WeaponType::Gun || weapon == WeaponType::Shotgun {
                // TODO: hardcoded for now to get the same particle size as before
                (scale_x, scale_y) = get_sprite_scale_impl(3, 2);
            } else {
                // TODO: RenderTools()->GetSpriteScale(g_pData->m_Weapons.m_aId[i].m_aSpriteMuzzles[n], ScaleX, ScaleY);
                (scale_x, scale_y) = (0.0, 0.0);
            }
            let size = get_weapon_visual_scale(&weapon);
            let width = (size * scale_x) * (4.0 / 3.0);
            let height = size * scale_y;

            let mut quad = Quad::new()
                .from_width_and_height_centered(width, height)
                .with_color(&ubvec4::new(255, 255, 255, 255));
            let offset_normal = quads.len();
            quads.push(quad);

            quad = quad.with_tex(&[
                vec2 { x: 0.0, y: 1.0 },
                vec2 { x: 1.0, y: 1.0 },
                vec2 { x: 1.0, y: 0.0 },
                vec2 { x: 0.0, y: 0.0 },
            ]);
            let offset_flipped = quads.len();
            quads.push(quad);

            weapon_muzzle_quad_offsets[weapon as usize] = (offset_normal, offset_flipped);
        });

        // # ninja
        let mut quad = get_ninja_as_quad(false).with_color(&ubvec4::new(255, 255, 255, 255));

        quad = quad.with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0));
        let offset_normal = quads.len();
        quads.push(quad);
        quad = quad.with_uv_from_points(&vec2::new(0.0, 1.0), &vec2::new(1.0, 0.0));
        let offset_flipped = quads.len();
        quads.push(quad);
        let ninja_quad_offsets = (offset_normal, offset_flipped);

        let (scale_x, scale_y) = get_scale(7.0, 4.0);
        let offset_normal = quads.len();
        quads.push(
            Quad::new()
                .from_width_and_height_centered(5.0 * scale_x, 5.0 * scale_y)
                .with_color(&ubvec4::new(255, 255, 255, 255)),
        );
        let ninja_muzzle_quad_offset = offset_normal;

        let quad = Quad::new()
            .from_rect(-3.0 / 8.0, -0.25, 0.75, 0.5)
            .with_color(&ubvec4::new(255, 255, 255, 255))
            .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0));

        let hook_chain_quad_offset = quads.len();
        quads.push(quad);
        let hook_head_quad_offset = hook_chain_quad_offset;

        let quad_container = graphics.quad_container_handle.create_quad_container(quads);

        Self {
            stream_handle: graphics.stream_handle.clone(),

            quad_container,

            hook_chain_quad_offset,
            hook_head_quad_offset,

            weapon_quad_offsets,
            weapon_muzzle_quad_offsets,

            ninja_quad_offsets,
            ninja_muzzle_quad_offset,

            rng: Rng::new(0),
        }
    }

    fn get_player_target_angle(cursor_vec: &dvec2) -> f64 {
        if cursor_vec.x == 0.0 && cursor_vec.y == 0.0 {
            return 0.0;
        } else if cursor_vec.x == 0.0 {
            return if cursor_vec.y < 0.0 {
                -PI_F64 / 2.0
            } else {
                PI_F64 / 2.0
            };
        }
        let tmp_angle = (cursor_vec.y / cursor_vec.x).atan();
        if cursor_vec.x < 0.0 {
            tmp_angle + PI_F64
        } else {
            tmp_angle
        }
    }

    pub fn render_hook_collision_line(
        &mut self,
        hook_collision: &HookCollisionLine,
        base_state: State,
    ) {
        self.stream_handle.render_lines(
            hi_closure!(
                [hook_collision: &HookCollisionLine],
                    |mut stream_handle: LinesStreamHandle<'_>| -> () {
                    let mut line = StreamedLine::new().with_color(match hook_collision.color {
                        HookCollisionLineColor::Nothing => ubvec4::new(255, 0, 0, 255),
                        HookCollisionLineColor::Player => ubvec4::new(0, 255, 255, 255),
                        HookCollisionLineColor::Hookable => ubvec4::new(0, 128, 0, 255),
                        HookCollisionLineColor::Unhookable => ubvec4::new(255, 0, 0, 255),
                        HookCollisionLineColor::Custom(color) => color,
                    });
                    line = line.from_pos(
                        [hook_collision.start, hook_collision.end]
                    );
                    stream_handle.add_vertices(line.into());
                }
            ),
            base_state,
        );
    }

    pub fn render_hook(
        &mut self,
        hook: &Hook,
        pos: vec2,
        hook_pos: vec2,
        hook_dir: vec2,
        size: f32,
        quad_scope: DrawScope<4>,
    ) {
        let texture = &hook.hook_head;
        let hook_render_pos = hook_pos;
        self.quad_container.render_quad_container_as_sprite(
            self.hook_head_quad_offset,
            hook_render_pos.x,
            hook_render_pos.y,
            size,
            size,
            quad_scope,
            texture.into(),
        );
        let base_state = quad_scope.state;

        // render chain
        let quad_container = &self.quad_container;
        let hook_chain_quad_offset = self.hook_chain_quad_offset;
        self.stream_handle.fill_sprites_uniform_instance(
            hi_closure!(
                [
                    pos: vec2,
                    hook_pos: vec2,
                    hook_render_pos: vec2,
                    hook_dir: vec2,
                    size: f32
                ],
                |mut stream_handle: StreamedSprites<'_>| -> () {
                    let hook_el_size = 0.75 * size;
                    let mut f = hook_el_size;
                    let d = distance(&pos, &hook_pos);
                    while f < d && f < hook_el_size * 512.0 {
                        let p = hook_render_pos + hook_dir * f;
                        stream_handle.add(RenderSpriteInfo {
                            pos: p,
                            scale: size,
                            rotation: angle(&hook_dir) + PI,
                            color: ColorRgba::new(1.0, 1.0, 1.0, 1.0)
                        });
                        f += hook_el_size;
                    }
                }
            ),
            hi_closure!(
                [
                    quad_container: &QuadContainer,
                    hook_chain_quad_offset: usize,
                    hook: &Hook,
                    base_state: State
                ],
                |instance: usize, count: usize| -> () {
                    let mut quad_scope = quad_scope_begin();
                    quad_scope.set_state(&base_state);
                    quad_scope.set_colors_from_single(1.0, 1.0, 1.0, 1.0);
                    quad_container
                        .render_quad_container_as_sprite_multiple(
                            hook_chain_quad_offset,
                            instance,
                            count,
                            quad_scope,
                            (&hook.hook_chain).into(),
                        );
                }
            ),
        );
    }

    pub fn render_hook_for_player(
        &mut self,
        hooks: &mut HookContainer,
        hook_key: Option<&ContainerKey>,
        pos: vec2,
        player_render_info: &CharacterRenderInfo,
        base_state: State,
        phased_alpha: f32,
    ) -> Option<TeeRenderHand> {
        if let Some(hook) = &player_render_info.lerped_hook {
            let hook_dir = normalize(&(pos - hook.pos));

            // current hook
            let cur_hook = hooks.get_or_default_opt(hook_key);

            // render head
            let mut quad_scope = quad_scope_begin();
            quad_scope.set_state(&base_state);
            quad_scope.set_rotation(angle(&hook_dir) + PI);
            quad_scope.set_colors_from_single(1.0, 1.0, 1.0, phased_alpha);

            self.render_hook(cur_hook, pos, hook.pos, hook_dir, 1.0, quad_scope);

            Some(TeeRenderHand {
                pos: vec2::new(0.0, 0.0),
                dir: -hook_dir,
                rot_offset: -PI / 2.0,
                offset_after_rot: vec2::new(20.0 / 32.0, 0.0),
                scale: 1.0,
            })
        } else {
            None
        }
    }

    pub fn render_weapon(
        &self,
        weapons: &Weapons,
        weapon_type: &WeaponType,
        weapon_pos: &vec2,
        size: f32,
        cursor_dir: &dvec2,
        quad_scope: DrawScope<4>,
    ) {
        // normal weapons
        let texture = match weapon_type {
            WeaponType::Hammer => &weapons.hammer.weapon.tex,
            WeaponType::Gun => &weapons.gun.tex,
            WeaponType::Shotgun => &weapons.shotgun.weapon.tex,
            WeaponType::Grenade => &weapons.grenade.weapon.tex,
            WeaponType::Laser => &weapons.laser.weapon.tex,
        };

        let quad_offset = if cursor_dir.x >= 0.0 {
            self.weapon_quad_offsets[*weapon_type as usize].0
        } else {
            self.weapon_quad_offsets[*weapon_type as usize].1
        };

        self.quad_container.render_quad_container_as_sprite(
            quad_offset,
            weapon_pos.x,
            weapon_pos.y,
            size,
            size,
            quad_scope,
            texture.into(),
        );
    }

    pub fn render_weapon_for_player(
        &mut self,
        weapons: &Weapons,
        player_render_info: &CharacterRenderInfo,
        render_pos: vec2,
        ticks_in_a_second: NonZeroGameTickType,
        game_info: &GameTimeInfo,
        base_state: State,
        is_sit: bool,
        inactive: bool,
        phased_alpha: f32,
    ) -> Option<TeeRenderHand> {
        let current_weapon = player_render_info.cur_weapon;
        let cursor_dir = normalize(&player_render_info.lerped_cursor_pos); // TODO?
        let cursor_angle = Self::get_player_target_angle(&normalize(&cursor_dir));
        // now render the weapons
        let mut quad_scope = quad_scope_begin();
        quad_scope.set_state(&base_state);
        quad_scope.set_rotation(cursor_angle as f32);

        quad_scope.set_colors_from_single(1.0, 1.0, 1.0, phased_alpha);

        let dir = vec2::new(cursor_dir.x as f32, cursor_dir.y as f32);

        let attack_time = player_render_info.recoil_ticks_passed.map(|recoil| {
            (intra_tick_time_to_ratio(game_info.intra_tick_time, ticks_in_a_second) + recoil as f64)
                / ticks_in_a_second.get() as f64
        });

        let mut weapon_pos: vec2;
        if current_weapon == WeaponType::Hammer {
            let weapon_anim_state = TeeAnimationFrame::default(); // TODO:

            // static position for hammer
            weapon_pos = render_pos + vec2::new(weapon_anim_state.pos.x, weapon_anim_state.pos.y);
            let hammer_spec = WeaponHammerSpec::get();
            weapon_pos.y += hammer_spec.offset_y;

            if dir.x < 0.0 {
                weapon_pos.x -= hammer_spec.offset_x;
            }
            if is_sit {
                weapon_pos.y += 3.0 / 32.0;
            }

            // if active and attack is under way, bash stuffs
            if !inactive
            /* TODO: needed? ( || LastAttackTime < m_pClient->m_aTuning[g_Config.m_ClDummy].GetWeaponFireDelay(Player.m_Weapon))*/
            {
                let hammer_anim = hammer_swing_anim();
                // let hammer_spec = WeaponHammerSpec::get();
                // TODO: old client randomly uses times 5 here instead of some animation length let anim_time = hammer_spec.fire_delay as f64 / 1000.0;
                let attack_time =
                    attack_time.map(|attack_time| (attack_time * 5.0).clamp(0.0, 1.0));
                let mut frame = TeeAnimationFrame::default();

                AnimState::anim_frame_eval(
                    &hammer_anim,
                    &Duration::from_secs_f64(attack_time.unwrap_or(1.0)),
                    &mut frame,
                );
                if cursor_dir.x < 0.0 {
                    quad_scope.set_rotation(-PI / 2.0 - frame.rotation * PI * 2.0);
                } else {
                    quad_scope.set_rotation(-PI / 2.0 + frame.rotation * PI * 2.0);
                }
            } else {
                quad_scope.set_rotation(if cursor_dir.x < 0.0 { 100.0 } else { 500.0 });
            }
        } else {
            // TODO: should be an animation
            let mut recoil = 0.0;
            let a = attack_time
                .map(|attack_time| attack_time * 10.0)
                .unwrap_or(1.0);
            if a < 1.0 {
                recoil = (a * PI as f64).sin();
            }
            let weapon_spec = match current_weapon {
                WeaponType::Hammer => panic!("this weapon should be handled earlier"),
                WeaponType::Gun => WeaponGunSpec::get(),
                WeaponType::Shotgun => WeaponShotgunSpec::get(),
                WeaponType::Grenade => WeaponGrenadeSpec::get(),
                WeaponType::Laser => WeaponLaserSpec::get(),
            };
            weapon_pos =
                render_pos + dir * weapon_spec.offset_x - dir * recoil as f32 * (10.0 / 32.0);
            weapon_pos.y += weapon_spec.offset_y;
            if is_sit {
                weapon_pos.y += 3.0 / 32.0;
            }
        }
        self.render_weapon(
            weapons,
            &current_weapon,
            &weapon_pos,
            1.0,
            &cursor_dir,
            quad_scope,
        );

        // muzzle if weapon is firing
        if current_weapon == WeaponType::Gun || current_weapon == WeaponType::Shotgun {
            // check if we're firing stuff
            let (weapon, spec) = if current_weapon == WeaponType::Gun {
                (&weapons.gun, WeaponGunSpec::get())
            } else {
                (&weapons.shotgun.weapon, WeaponShotgunSpec::get())
            };
            if !weapon.muzzles.is_empty() {
                let mut alpha_muzzle = 0.0;
                let muzzle_duration = 8.0 / 5.0; // TODO: move this into the weapon spec
                let attack_time = attack_time
                    .map(|attack_time| attack_time * 10.0)
                    .unwrap_or(f64::MAX);
                if attack_time < muzzle_duration {
                    alpha_muzzle = mix(&2.0, &0.0, 1.0_f64.min(0.0_f64.max(attack_time)));
                }

                if alpha_muzzle > 0.0 {
                    let mut pos_offset_y = -spec.muzzle_offset_y;
                    let quad_offset = if cursor_dir.x < 0.0 {
                        pos_offset_y = -pos_offset_y;
                        self.weapon_muzzle_quad_offsets[current_weapon as usize].1
                    } else {
                        self.weapon_muzzle_quad_offsets[current_weapon as usize].0
                    };

                    let muzzle_dir_y = vec2::new(-dir.y, dir.x);
                    let muzzle_pos =
                        weapon_pos + dir * spec.muzzle_offset_x + muzzle_dir_y * pos_offset_y;

                    let texture = weapon.muzzles.random_entry(&mut self.rng);

                    self.quad_container.render_quad_container_as_sprite(
                        quad_offset,
                        muzzle_pos.x,
                        muzzle_pos.y,
                        1.0,
                        1.0,
                        quad_scope,
                        texture.into(),
                    );
                }
            }
        }

        let dir_normalized = normalize(&dir);
        match current_weapon {
            WeaponType::Hammer => None,
            WeaponType::Gun => Some(TeeRenderHand {
                pos: weapon_pos - render_pos,
                dir: dir_normalized,
                rot_offset: -3.0 * PI / 4.0,
                offset_after_rot: vec2::new(-15.0, 4.0) / 32.0,
                scale: 1.0,
            }),
            WeaponType::Shotgun => Some(TeeRenderHand {
                pos: weapon_pos - render_pos,
                dir: dir_normalized,
                rot_offset: -PI / 2.0,
                offset_after_rot: vec2::new(-5.0, 4.0) / 32.0,
                scale: 1.0,
            }),
            WeaponType::Grenade => Some(TeeRenderHand {
                pos: weapon_pos - render_pos,
                dir: dir_normalized,
                rot_offset: -PI / 2.0,
                offset_after_rot: vec2::new(-4.0, 7.0) / 32.0,
                scale: 1.0,
            }),
            WeaponType::Laser => None,
        }
    }

    pub fn render_ninja_weapon(
        &mut self,
        ninja: &Ninja,
        particle_manager: &mut ParticleManager,
        character_id: &CharacterId,
        character_render_info: &CharacterRenderInfo,
        game_info: &GameTimeInfo,
        ticks_in_a_second: NonZeroGameTickType,
        cur_time: Duration,
        pos: vec2,
        is_sit: bool,
        base_state: State,
        phased_alpha: f32,
    ) -> Option<TeeRenderHand> {
        let mut weapon_pos = pos;
        let spec = WeaponNinjaSpec::get();
        weapon_pos.y += spec.offset_y;
        if is_sit {
            weapon_pos.y += 3.0;
        }

        let attack_time = character_render_info.recoil_ticks_passed.map(|recoil| {
            (intra_tick_time_to_ratio(game_info.intra_tick_time, ticks_in_a_second) + recoil as f64)
                / ticks_in_a_second.get() as f64
        });

        let ninja_anim = ninja_swing_anim();
        // TODO: old client randomly uses times 2 here instead of some animation length let anim_time = ninja_spec.fire_delay as f64 / 1000.0;
        let attack_time = attack_time
            .map(|attack_time| (attack_time * 2.0).clamp(0.0, 1.0))
            .unwrap_or(1.0);
        let mut frame = TeeAnimationFrame::default();

        AnimState::anim_frame_eval(
            &ninja_anim,
            &Duration::from_secs_f64(attack_time),
            &mut frame,
        );

        let mut quad_scope = quad_scope_begin();
        quad_scope.set_state(&base_state);
        quad_scope.set_colors_from_single(1.0, 1.0, 1.0, phased_alpha);

        let quad_offset = if character_render_info.lerped_cursor_pos.x < 0.0 {
            quad_scope.set_rotation(-PI / 2.0 - frame.rotation * PI * 2.0);
            weapon_pos.x -= spec.offset_x;
            Effects::new(particle_manager, cur_time).powerup_shine(
                &(weapon_pos + vec2::new(1.0, 0.0)),
                &vec2::new(1.0, 12.0 / 32.0),
                Some(*character_id),
            );
            self.ninja_quad_offsets.0
        } else {
            quad_scope.set_rotation(-PI / 2.0 + frame.rotation * PI * 2.0);
            Effects::new(particle_manager, cur_time).powerup_shine(
                &(weapon_pos - vec2::new(1.0, 0.0)),
                &vec2::new(1.0, 12.0 / 32.0),
                Some(*character_id),
            );
            self.ninja_quad_offsets.1
        };

        self.quad_container.render_quad_container_as_sprite(
            quad_offset,
            weapon_pos.x,
            weapon_pos.y,
            1.0,
            1.0,
            quad_scope,
            (&ninja.weapon).into(),
        );

        // HADOKEN
        if attack_time <= 1.0 / 6.0 && !ninja.muzzles.is_empty() {
            let mut hadoken_angle = 0.0;
            let cursor = character_render_info.lerped_cursor_pos;
            let mut dir = vec2::new(cursor.x as f32, cursor.y as f32);
            if dir.x.abs() > 0.0001 || dir.y.abs() > 0.0001 {
                dir = normalize(&dir);
                hadoken_angle = angle(&dir);
            } else {
                dir = vec2::new(1.0, 0.0);
            }
            quad_scope.set_rotation(hadoken_angle);
            weapon_pos = pos;
            let offset_x = spec.muzzle_offset_x;
            weapon_pos -= dir * offset_x;

            self.quad_container.render_quad_container_as_sprite(
                self.ninja_muzzle_quad_offset,
                weapon_pos.x,
                weapon_pos.y,
                1.0,
                1.0,
                quad_scope,
                (ninja.muzzles.random_entry(&mut self.rng)).into(),
            );
        }

        None
    }
}
