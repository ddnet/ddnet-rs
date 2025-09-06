use std::time::Duration;

use client_containers::particles::ParticleType;
use game_interface::types::id_types::CharacterId;
use graphics_types::rendering::ColorRgba;
use math::math::{
    PI, Rng, mix,
    vector::{vec2, vec4},
};

use super::{
    particle::Particle,
    particle_manager::{ParticleGroup, ParticleManager},
};

pub struct Effects<'a> {
    particle_manager: &'a mut ParticleManager,
    rate_5_time: Duration,
    rate_10_time: Duration,
    rate_50_time: Duration,
    rate_100_time: Duration,
}

impl<'a> Effects<'a> {
    pub fn new(particle_manager: &'a mut ParticleManager, cur_time: Duration) -> Self {
        Self {
            particle_manager,

            rate_5_time: Duration::from_nanos(
                ((cur_time.as_nanos() / Duration::from_millis(1000 / 5).as_nanos())
                    * Duration::from_millis(1000 / 5).as_nanos()) as u64,
            ),
            rate_10_time: Duration::from_nanos(
                ((cur_time.as_nanos() / Duration::from_millis(1000 / 10).as_nanos())
                    * Duration::from_millis(1000 / 10).as_nanos()) as u64,
            ),
            rate_50_time: Duration::from_nanos(
                ((cur_time.as_nanos() / Duration::from_millis(1000 / 50).as_nanos())
                    * Duration::from_millis(1000 / 50).as_nanos()) as u64,
            ),
            rate_100_time: Duration::from_nanos(
                ((cur_time.as_nanos() / Duration::from_millis(1000 / 100).as_nanos())
                    * Duration::from_millis(1000 / 100).as_nanos()) as u64,
            ),
        }
    }

    pub fn is_rate_10(&self) -> bool {
        self.rate_10_time >= self.particle_manager.last_10_time
    }

    fn random_dir(rng: &mut Rng) -> vec2 {
        let angle = 2.0 * PI * rng.random_float();
        vec2::new(angle.cos(), angle.sin())
    }

    pub fn air_jump(&mut self, pos: &vec2, owner: Option<CharacterId>) {
        let mut p = Particle {
            ty: ParticleType::Airjump,
            rng: self.particle_manager.rng.random_int(),
            owner,
            pos: *pos + vec2::new(-6.0, 16.0) / 32.0,
            vel: vec2::new(0.0, -200.0 / 32.0),
            life_span: 0.5,
            start_size: 1.5,
            end_size: 0.0,
            rot: self.particle_manager.rng.random_float() * PI * 2.0,
            rot_speed: PI * 2.0,
            gravity: 500.0 / 32.0,
            friction: 0.7,
            flow_affected: 0.0,
            ..Default::default()
        };
        self.particle_manager
            .add(ParticleGroup::General, p.clone(), 0.0);

        p.pos = *pos + vec2::new(6.0, 16.0) / 32.0;
        self.particle_manager.add(ParticleGroup::General, p, 0.0);
    }

    pub fn powerup_shine(&mut self, pos: &vec2, size: &vec2, owner: Option<CharacterId>) {
        if self.rate_50_time < self.particle_manager.last_50_time {
            return;
        }

        let p = Particle {
            ty: ParticleType::Slice,
            rng: self.particle_manager.rng.random_int(),
            owner,
            pos: *pos
                + vec2::new(
                    (self.particle_manager.rng.random_float() - 0.5) * size.x,
                    (self.particle_manager.rng.random_float() - 0.5) * size.y,
                ),
            vel: vec2::new(0.0, 0.0),
            life_span: 0.5,
            start_size: 0.5,
            end_size: 0.0,
            rot: self.particle_manager.rng.random_float() * PI * 2.0,
            rot_speed: PI * 2.0,
            gravity: 500.0 / 32.0,
            friction: 0.9,
            flow_affected: 0.0,
            ..Default::default()
        };
        self.particle_manager.add(ParticleGroup::General, p, 0.0);
    }

    pub fn freezing_flakes(&mut self, pos: &vec2, size: &vec2, owner: Option<CharacterId>) {
        if self.rate_5_time < self.particle_manager.last_5_time {
            return;
        }

        let rng = &mut self.particle_manager.rng;

        let start_size = (rng.random_float() + 0.5) * 0.5;
        let p = Particle {
            ty: ParticleType::Snowflake,
            rng: rng.random_int(),
            owner,
            pos: *pos
                + vec2::new(
                    (rng.random_float() - 0.5) * size.x,
                    (rng.random_float() - 0.5) * size.y,
                ),
            vel: vec2::default(),
            life_span: 1.5,
            start_size,
            end_size: start_size * 0.5,
            use_alpha_fading: true,
            start_alpha: 1.0,
            end_alpha: 0.0,
            rot: rng.random_float() * PI * 2.0,
            rot_speed: PI,
            gravity: rng.random_float() * 250.0 / 32.0,
            friction: 0.9,
            flow_affected: 0.0,
            collides: false,
            ..Default::default()
        };
        self.particle_manager.add(ParticleGroup::Extra, p, 0.0);
    }

    pub fn smoke_trail(
        &mut self,
        pos: &vec2,
        vel: &vec2,
        alpha: f32,
        time_passed: f32,
        owner: Option<CharacterId>,
    ) {
        if self.rate_50_time < self.particle_manager.last_50_time {
            return;
        }

        let mut p = Particle {
            ty: ParticleType::Smoke,
            rng: self.particle_manager.rng.random_int(),
            owner,
            pos: *pos,
            vel: *vel + (Self::random_dir(&mut self.particle_manager.rng) * 50.0 / 32.0),
            life_span: 0.5 + self.particle_manager.rng.random_float() * 0.5,
            start_size: 3.0 / 8.0 + self.particle_manager.rng.random_float() * 0.25,
            end_size: 0.0,
            friction: 0.7,
            gravity: (self.particle_manager.rng.random_float() * -500.0) / 32.0,

            ..Default::default()
        };
        p.color.a *= alpha;
        self.particle_manager
            .add(ParticleGroup::ProjectileTrail, p, time_passed);
    }

    pub fn skid_trail(&mut self, pos: &vec2, vel: &vec2, owner: Option<CharacterId>) {
        if self.rate_100_time < self.particle_manager.last_100_time {
            return;
        }

        let rng = &mut self.particle_manager.rng;
        let p = Particle {
            ty: ParticleType::Smoke,
            rng: rng.random_int(),
            owner,
            pos: *pos,
            vel: *vel + Self::random_dir(rng) * 50.0 / 32.0,
            life_span: 0.5 + rng.random_float() * 0.5,
            start_size: 0.75 + rng.random_float() * 12.0 / 32.0,
            end_size: 0.0,
            friction: 0.7,
            gravity: rng.random_float() * -500.0 / 32.0,
            color: ColorRgba::new(0.75, 0.75, 0.75, 1.0),
            ..Default::default()
        };
        self.particle_manager.add(ParticleGroup::General, p, 0.0);
    }

    pub fn bullet_trail(&mut self, pos: &vec2, alpha: f32, owner: Option<CharacterId>) {
        if self.rate_100_time < self.particle_manager.last_100_time {
            return;
        }

        let mut p = Particle {
            ty: ParticleType::Ball,
            rng: self.particle_manager.rng.random_int(),
            owner,
            pos: *pos,
            life_span: 0.25 + self.particle_manager.rng.random_float() * 0.25,
            start_size: 8.0 / 32.0,
            end_size: 0.0,
            friction: 0.7,
            ..Default::default()
        };
        p.color.a *= alpha;
        self.particle_manager
            .add(ParticleGroup::ProjectileTrail, p, 0.0);
    }

    pub fn player_spawn(&mut self, pos: &vec2, owner: Option<CharacterId>) {
        let rng_val = self.particle_manager.rng.random_int();
        for i in 0..32 {
            let rng = &mut self.particle_manager.rng;
            let p = Particle {
                ty: ParticleType::Shell,
                rng: rng_val + i / 16,
                owner,
                pos: *pos,
                vel: Self::random_dir(rng) * (rng.random_float().powf(3.0) * 600.0 / 32.0),
                life_span: 0.3 + rng.random_float() * 0.3,
                start_size: 2.0 + rng.random_float(),
                end_size: 0.0,
                rot: rng.random_float() * PI * 2.0,
                rot_speed: rng.random_float(),
                gravity: rng.random_float() * -400.0 / 32.0,
                friction: 0.7,
                color: ColorRgba::new(
                    0xb5 as f32 / 255.0,
                    0x50 as f32 / 255.0,
                    0xcb as f32 / 255.0,
                    1.0,
                ),
                ..Default::default()
            };
            self.particle_manager.add(ParticleGroup::General, p, 0.0);
        }
    }

    pub fn player_death(&mut self, pos: &vec2, bloor_color: ColorRgba, owner: Option<CharacterId>) {
        let rng_val = self.particle_manager.rng.random_int();
        for i in 0..64 {
            let rng = &mut self.particle_manager.rng;
            let mut p = Particle {
                ty: ParticleType::Splats,
                rng: rng_val + i / 32,
                owner,
                pos: *pos,
                vel: Self::random_dir(rng) * ((rng.random_float() + 0.1) * 900.0 / 32.0),
                life_span: 0.3 + rng.random_float() * 0.3,
                start_size: 0.75 + rng.random_float() * 0.5,
                end_size: 0.0,
                rot: rng.random_float() * PI * 2.0,
                rot_speed: (rng.random_float() - 0.5) * PI,
                gravity: 25.0,
                friction: 0.8,
                ..Default::default()
            };
            let c = vec4::new(bloor_color.r, bloor_color.g, bloor_color.b, bloor_color.a)
                * (0.75 + rng.random_float() * 0.25);
            p.color = ColorRgba::new(c.r(), c.g(), c.b(), 0.75);
            self.particle_manager.add(ParticleGroup::General, p, 0.0);
        }
    }

    pub fn explosion(&mut self, pos: &vec2, owner: Option<CharacterId>) {
        let rng = &mut self.particle_manager.rng;
        // add the explosion
        let p = Particle {
            ty: ParticleType::Explosions,
            rng: rng.random_int(),
            owner,
            pos: *pos,
            life_span: 0.4,
            start_size: 150.0 / 32.0,
            end_size: 0.0,
            rot: rng.random_float() * PI * 2.0,
            ..Default::default()
        };
        self.particle_manager.add(ParticleGroup::Explosions, p, 0.0);

        // add the smoke
        let rng_val = self.particle_manager.rng.random_int();
        for i in 0..24 {
            let rng = &mut self.particle_manager.rng;
            let mut p = Particle {
                ty: ParticleType::Smoke,
                rng: rng_val + i / 12,
                owner,
                pos: *pos,
                vel: Self::random_dir(rng) * ((1.0 + rng.random_float() * 0.2) * 1000.0 / 32.0),
                life_span: 0.5 + rng.random_float() * 0.4,
                start_size: 1.0 + rng.random_float() * 0.25,
                end_size: 0.0,
                gravity: rng.random_float() * -25.0,
                friction: 0.4,
                ..Default::default()
            };
            let color = mix(
                &vec4::new(0.75, 0.75, 0.75, 1.0),
                &vec4::new(0.5, 0.5, 0.5, 1.0),
                rng.random_float(),
            );
            p.color = ColorRgba::new(color.x, color.y, color.z, color.w);
            self.particle_manager.add(ParticleGroup::General, p, 0.0);
        }
    }

    pub fn hammer_hit(&mut self, pos: &vec2, owner: Option<CharacterId>) {
        let rng = &mut self.particle_manager.rng;
        // add the explosion
        let p = Particle {
            ty: ParticleType::Hits,
            rng: rng.random_int(),
            owner,
            pos: *pos,
            life_span: 0.3,
            start_size: 120.0 / 32.0,
            end_size: 0.0,
            rot: rng.random_float() * PI * 2.0,
            ..Default::default()
        };
        self.particle_manager.add(ParticleGroup::Explosions, p, 0.0);
    }

    pub fn damage_ind(&mut self, pos: &vec2, vel: &vec2, owner: Option<CharacterId>) {
        let rng = &mut self.particle_manager.rng;
        // add the explosion
        let p = Particle {
            ty: ParticleType::Stars,
            // vanilla wants no rng. maybe allow it optionally?
            rng: 0,
            owner,
            pos: *pos,
            vel: *vel * 1.0,
            life_span: 0.75,
            max_lifetime_vel: 0.15,
            start_size: 1.0,
            end_size: 1.0,
            rot: (rng.random_float() - 1.0) * PI * 2.0,
            rot_speed: 2.0,
            gravity: 0.0,
            friction: 1.0,
            collides: false,
            ..Default::default()
        };
        self.particle_manager.add(ParticleGroup::Explosions, p, 0.0);
    }
}
