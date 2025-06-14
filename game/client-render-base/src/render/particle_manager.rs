#![allow(unused)]

use std::{cell::RefCell, collections::VecDeque, sync::Arc, time::Duration};

use crate::render::canvas_mapping::CanvasMappingIngame;
use base::linked_hash_map_view::FxLinkedHashMap;
use camera::CameraInterface;
use client_containers::{
    container::ContainerKey,
    particles::{ParticleType, ParticlesContainer},
};
use game_interface::types::{id_types::CharacterId, render::character::CharacterInfo};
use graphics::{
    graphics::graphics::Graphics,
    handles::{
        quad_container::quad_container::{QuadContainer, QuadContainerRenderCount},
        stream::stream::{GraphicsStreamHandle, StreamedSprites, StreamedUniforms},
        texture::texture::{TextureContainer, TextureType},
    },
    quad_container::Quad,
    streaming::quad_scope_begin,
};
use graphics_types::{
    commands::RenderSpriteInfo,
    rendering::{ColorRgba, State},
};
use hashlink::LinkedHashMap;
use hiarc::{hi_closure, Hiarc};
use math::math::{
    mix,
    vector::{ubvec4, vec2},
    Rng,
};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use vanilla::collision::collision::Collision;

use super::particle::Particle;

const MAX_PARTICLES: usize = 1024 * 8;

#[derive(Copy, Hiarc, Clone, PartialEq, FromPrimitive)]
pub enum ParticleGroup {
    ProjectileTrail = 0,
    Explosions,
    Extra,
    General,

    // must stay last
    Count,
}

#[derive(Debug, Hiarc)]
pub struct ParticleManager {
    particle_quad_container: QuadContainer,
    canvas_mapping: CanvasMappingIngame,
    stream_handle: GraphicsStreamHandle,

    particle_groups: [VecDeque<Particle>; ParticleGroup::Count as usize],

    // TODO: wtf is this?
    friction_fraction: f32,

    last_time: Duration,
    /// 5 times per second
    pub last_5_time: Duration,
    /// 10 times per second
    pub last_10_time: Duration,
    /// 50 times per second
    pub last_50_time: Duration,
    /// 100 times per second
    pub last_100_time: Duration,

    pub rng: Rng,
}

impl ParticleManager {
    pub fn new(graphics: &Graphics, cur_time: &Duration) -> Self {
        let particle_quad_container = graphics
            .quad_container_handle
            .create_quad_container([Quad::new().from_size_centered(1.0)].into());

        Self {
            particle_quad_container,
            canvas_mapping: CanvasMappingIngame::new(graphics),
            stream_handle: graphics.stream_handle.clone(),

            particle_groups: Default::default(),
            friction_fraction: 0.0,

            last_time: *cur_time,
            last_5_time: Duration::from_nanos(
                ((cur_time.as_nanos() / Duration::from_millis(1000 / 5).as_nanos())
                    * Duration::from_millis(1000 / 5).as_nanos()) as u64,
            ),
            last_10_time: Duration::from_nanos(
                ((cur_time.as_nanos() / Duration::from_millis(1000 / 10).as_nanos())
                    * Duration::from_millis(1000 / 10).as_nanos()) as u64,
            ),
            last_50_time: Duration::from_nanos(
                ((cur_time.as_nanos() / Duration::from_millis(1000 / 50).as_nanos())
                    * Duration::from_millis(1000 / 50).as_nanos()) as u64,
            ),
            last_100_time: Duration::from_nanos(
                ((cur_time.as_nanos() / Duration::from_millis(1000 / 100).as_nanos())
                    * Duration::from_millis(1000 / 100).as_nanos()) as u64,
            ),

            rng: Rng::new(0),
        }
    }

    pub fn reset(&mut self) {
        // reset particles
        self.particle_groups.iter_mut().for_each(|p| p.clear());
    }

    pub fn add(&mut self, group: ParticleGroup, mut part: Particle, time_passed: f32) {
        part.life = time_passed;
        self.particle_groups[group as usize].push_back(part);
    }

    pub fn update_rates(&mut self) {
        let next_5 = Duration::from_nanos(
            ((self.last_time.as_nanos() / Duration::from_millis(1000 / 5).as_nanos())
                * Duration::from_millis(1000 / 5).as_nanos()) as u64,
        );
        let offset_5 = Duration::from_millis(1000 / 5);
        if next_5 >= self.last_5_time {
            self.last_5_time = next_5 + offset_5;
        }

        let next_10 = Duration::from_nanos(
            ((self.last_time.as_nanos() / Duration::from_millis(1000 / 10).as_nanos())
                * Duration::from_millis(1000 / 10).as_nanos()) as u64,
        );
        let offset_10 = Duration::from_millis(1000 / 10);
        if next_10 >= self.last_10_time {
            self.last_10_time = next_10 + offset_10;
        }

        let next_50 = Duration::from_nanos(
            ((self.last_time.as_nanos() / Duration::from_millis(1000 / 50).as_nanos())
                * Duration::from_millis(1000 / 50).as_nanos()) as u64,
        );
        let offset_50 = Duration::from_millis(1000 / 50);
        if next_50 >= self.last_50_time {
            self.last_50_time = next_50 + offset_50;
        }

        let next_100 = Duration::from_nanos(
            ((self.last_time.as_nanos() / Duration::from_millis(1000 / 100).as_nanos())
                * Duration::from_millis(1000 / 100).as_nanos()) as u64,
        );
        let offset_100 = Duration::from_millis(1000 / 100);
        if next_100 >= self.last_100_time {
            self.last_100_time = next_100 + offset_100;
        }
    }

    pub fn update(&mut self, cur_time: &Duration, collision: &Collision) {
        let time_passed_dur = cur_time.saturating_sub(self.last_time);
        self.last_time = *cur_time;
        if time_passed_dur.is_zero() {
            return;
        }

        let time_passed = time_passed_dur.as_secs_f32();

        self.friction_fraction += time_passed;

        if self.friction_fraction > 2.0 {
            // safety measure
            self.friction_fraction = 0.0;
        }

        let mut friction_count = 0;
        while self.friction_fraction > 0.05 {
            friction_count += 1;
            self.friction_fraction -= 0.05;
        }

        self.particle_groups.iter_mut().for_each(|particle_group| {
            particle_group.retain_mut(|particle| {
                let old_life = particle.life;
                particle.life += time_passed;

                particle.vel.y += particle.gravity * time_passed;

                for _ in 0..friction_count {
                    // apply friction
                    particle.vel *= particle.friction;
                }

                // move the point
                let life_diff_vel = particle.life.min(particle.max_lifetime_vel)
                    - old_life.min(particle.max_lifetime_vel);
                if old_life < particle.max_lifetime_vel {
                    let mut vel = particle.vel * life_diff_vel;
                    if particle.collides {
                        let mut bounces = 0;
                        let mut pos = particle.pos * 32.0;
                        let mut inout_vel = vel * 32.0;
                        collision.move_point(
                            &mut pos,
                            &mut inout_vel,
                            0.1 + 0.9 * self.rng.random_float(),
                            &mut bounces,
                        );
                        particle.pos = pos / 32.0;
                        vel = inout_vel / 32.0;
                    } else {
                        particle.pos += vel;
                    }
                    particle.vel = vel * (1.0 / life_diff_vel);
                }

                particle.rot += time_passed * particle.rot_speed;

                // check particle death
                particle.life <= particle.life_span
            })
        });
    }

    fn particle_is_visible_on_screen(state: &State, cur_pos: &vec2, mut cur_size: f32) -> bool {
        let (canvas_x0, canvas_y0, canvas_x1, canvas_y1) = state.get_canvas_mapping();

        // for simplicity assume the worst case rotation, that increases the bounding box around the particle by its diagonal
        let sqrt_of_2 = 2.0_f32.sqrt();
        cur_size *= sqrt_of_2;

        // always uses the mid of the particle
        let size_half = cur_size / 2.0;

        cur_pos.x + size_half >= canvas_x0
            && cur_pos.x - size_half <= canvas_x1
            && cur_pos.y + size_half >= canvas_y0
            && cur_pos.y - size_half <= canvas_y1
    }

    pub fn render_group(
        &self,
        group: ParticleGroup,
        particle_container: &mut ParticlesContainer,
        character_infos: &FxLinkedHashMap<CharacterId, CharacterInfo>,
        camera: &dyn CameraInterface,
    ) {
        if !self.particle_groups[group as usize].is_empty() {
            let mut state = State::new();
            self.canvas_mapping
                .map_canvas_for_ingame_items(&mut state, camera);

            let p = &self.particle_groups[group as usize][0];
            let mut alpha = p.color.a;
            if p.use_alpha_fading {
                let a = p.life / p.life_span;
                alpha = mix(&p.start_alpha, &p.end_alpha, a);
            }

            let part = particle_container.get_or_default_opt(
                p.owner
                    .and_then(|owner| character_infos.get(&owner).map(|c| &c.info.particles)),
            );
            let len = part.len_by_ty(p.ty);
            let last_part = RefCell::new((
                p.ty,
                p.owner,
                p.rng,
                len,
                part.get_by_ty(p.ty, p.rng).clone(),
            ));

            let particle_quad_container = &self.particle_quad_container;
            let state = &state;
            let particle_groups = &self.particle_groups;
            let last_part = &last_part;
            self.stream_handle.fill_sprites_uniform_instance(
                hi_closure!([
                    particle_groups: &[VecDeque<Particle>; ParticleGroup::Count as usize],
                    group: ParticleGroup,
                    state: &State,
                    alpha: f32,
                    last_part: &RefCell<(ParticleType, Option<CharacterId>, u64, usize, TextureContainer)>,
                    particle_container: &mut ParticlesContainer,
                    character_infos: &FxLinkedHashMap<CharacterId, CharacterInfo>,
                ], |mut stream_handle: StreamedSprites<'_>| -> () {
                    for p in particle_groups[group as usize].iter() {
                        let a = p.life / p.life_span;
                        let ppos = p.pos;
                        let size = mix(&p.start_size, &p.end_size, a);
                        let mut alpha = p.color.a;
                        if p.use_alpha_fading {
                            alpha = mix(&p.start_alpha, &p.end_alpha, a);
                        }

                        let ty = p.ty;
                        let rng = p.rng;
                        let owner = p.owner;

                        // the current position, respecting the size, is inside the viewport, render it, else ignore
                        if ParticleManager::particle_is_visible_on_screen(state, &ppos, size) {
                            let last_part_ref = last_part.borrow();
                            let (last_ty, last_owner, last_rng, last_len, _) = *last_part_ref;
                            if (last_ty, last_owner, rng as usize % last_len).ne(&(ty, owner, last_rng as usize % last_len))
                            {
                                drop(last_part_ref);

                                stream_handle.flush();

                                let part = particle_container.get_or_default_opt(
                                    owner
                                        .and_then(|owner| character_infos.get(&owner).map(|c| &c.info.particles)),
                                );
                                let len = part.len_by_ty(ty);

                                *last_part.borrow_mut() = (
                                    ty,
                                    owner,
                                    rng,
                                    len,
                                    part.get_by_ty(ty, rng).clone(),
                                );
                            }

                            stream_handle.add(RenderSpriteInfo {
                                pos: ppos,
                                scale: size,
                                rotation: p.rot,
                                color: ColorRgba::new(
                                    p.color.r,
                                    p.color.g,
                                    p.color.b,
                                    alpha,
                                )
                            });
                        }
                    }
                }),
                hi_closure!([
                    last_part: &RefCell<(ParticleType, Option<CharacterId>, u64, usize, TextureContainer)>,
                    particle_quad_container: &QuadContainer,
                    state: &State,
                ], |instance: usize, particle_count: usize| -> () {
                    let last_part = last_part.borrow();
                    let (_, _, _, _, part_texture) = &*last_part;

                    let mut quad_scope = quad_scope_begin();
                    quad_scope.set_state(state);
                    quad_scope.set_colors_from_single(
                        1.0,
                        1.0,
                        1.0,
                        1.0,
                    );
                    particle_quad_container
                        .render_quad_container_as_sprite_multiple(
                            0,
                            instance,
                            particle_count,
                            quad_scope,
                            part_texture.into(),
                        );
                }),
            );
        }
    }

    pub fn render_groups(
        &self,
        start_group: ParticleGroup,
        particle_container: &mut ParticlesContainer,
        character_infos: &FxLinkedHashMap<CharacterId, CharacterInfo>,
        camera: &dyn CameraInterface,
    ) {
        for i in start_group as usize..ParticleGroup::Count as usize {
            self.render_group(
                ParticleGroup::from_usize(i).unwrap(),
                particle_container,
                character_infos,
                camera,
            );
        }
    }
}
