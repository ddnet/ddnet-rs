use std::time::Duration;

use base::linked_hash_map_view::FxLinkedHashMap;
use camera::CameraInterface;
use client_containers::{
    ctf::CtfContainer, game::GameContainer, ninja::NinjaContainer, weapons::WeaponContainer,
};
use client_render_base::{
    map::render_pipe::GameTimeInfo,
    render::{
        canvas_mapping::CanvasMappingIngame,
        effects::Effects,
        particle_manager::ParticleManager,
        toolkit::{get_ninja_as_quad, get_weapon_as_quad, pickup_scale},
    },
};
use game_base::game_types::intra_tick_time_to_ratio;
use game_interface::types::{
    emoticons::EnumCount,
    flag::FlagType,
    id_types::{CharacterId, CtfFlagId, LaserId, PickupId, ProjectileId},
    laser::LaserType,
    pickup::PickupType,
    render::{
        character::CharacterInfo,
        flag::FlagRenderInfo,
        laser::LaserRenderInfo,
        pickup::PickupRenderInfo,
        projectiles::{ProjectileRenderInfo, WeaponWithProjectile},
    },
    weapons::WeaponType,
};
use graphics::{
    graphics::graphics::Graphics,
    handles::{
        quad_container::quad_container::QuadContainer, stream::stream::GraphicsStreamHandle,
        stream_types::StreamedQuad, texture::texture::TextureType,
    },
    quad_container::Quad,
    streaming::quad_scope_begin,
};
use graphics_types::rendering::{ColorRgba, State};
use math::math::{
    angle, distance, length, normalize_pre_length,
    vector::{ubvec4, vec2, vec4},
    PI_F64,
};
use num_traits::FromPrimitive;

pub struct GameObjectsRender {
    items_quad_container: QuadContainer,
    canvas_mapping: CanvasMappingIngame,
    stream_handle: GraphicsStreamHandle,

    // offsets
    ctf_flag_offset: usize,
    projectile_sprite_offset: usize,
    pickup_sprite_off: usize,
    particle_splat_off: usize,

    weapon_quad_offsets: [usize; WeaponType::COUNT],
    ninja_quad_offset: usize,
}

pub struct GameObjectsRenderPipe<'a> {
    pub particle_manager: &'a mut ParticleManager,
    pub cur_time: &'a Duration,

    pub game_time_info: &'a GameTimeInfo,
    pub character_infos: &'a FxLinkedHashMap<CharacterId, CharacterInfo>,
    pub projectiles: &'a FxLinkedHashMap<ProjectileId, ProjectileRenderInfo>,
    pub flags: &'a FxLinkedHashMap<CtfFlagId, FlagRenderInfo>,
    pub lasers: &'a FxLinkedHashMap<LaserId, LaserRenderInfo>,
    pub pickups: &'a FxLinkedHashMap<PickupId, PickupRenderInfo>,

    pub ctf_container: &'a mut CtfContainer,
    pub game_container: &'a mut GameContainer,
    pub ninja_container: &'a mut NinjaContainer,
    pub weapon_container: &'a mut WeaponContainer,

    pub local_character_id: Option<&'a CharacterId>,

    pub camera: &'a dyn CameraInterface,
    pub phased_alpha: f32,
    pub phased: bool,
}

impl GameObjectsRender {
    pub fn new(graphics: &Graphics) -> Self {
        let mut quads: Vec<Quad> = Default::default();

        let quad = Quad::new()
            .from_rect(-21.0 / 32.0, -42.0 / 32.0, 42.0 / 32.0, 84.0 / 32.0)
            .with_color(&ubvec4::new(255, 255, 255, 255))
            .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0));

        let ctf_flag_offset = quads.len();
        quads.push(quad);

        let sprite_scale = pickup_scale();
        let quad = Quad::new()
            .from_width_and_height_centered(2.0 * sprite_scale.0, 2.0 * sprite_scale.1)
            .with_color(&ubvec4::new(255, 255, 255, 255))
            .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0));

        let pickup_sprite_off = quads.len();
        quads.push(quad);

        let mut weapon_quad_offsets: [usize; WeaponType::COUNT] = Default::default();

        (0..WeaponType::COUNT).enumerate().for_each(|(index, wi)| {
            let quad = get_weapon_as_quad(&FromPrimitive::from_usize(wi).unwrap())
                .with_color(&ubvec4::new(255, 255, 255, 255))
                .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0));
            let offset_normal = quads.len();
            quads.push(quad);
            weapon_quad_offsets[index] = offset_normal;
        });

        let quad = get_ninja_as_quad(true)
            .with_color(&ubvec4::new(255, 255, 255, 255))
            .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0));
        let offset_normal = quads.len();
        quads.push(quad);
        let ninja_quad_offset = offset_normal;

        let quad = Quad::new()
            .from_width_and_height_centered(1.0, 1.0)
            .with_color(&ubvec4::new(255, 255, 255, 255))
            .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0));

        let projectile_sprite_off = quads.len();
        quads.push(quad);

        let quad = Quad::new()
            .from_width_and_height_centered(0.75, 0.75)
            .with_color(&ubvec4::new(255, 255, 255, 255))
            .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0));

        let particle_splat_off = quads.len();
        quads.push(quad);

        let items_quad_container = graphics.quad_container_handle.create_quad_container(quads);

        Self {
            items_quad_container,
            canvas_mapping: CanvasMappingIngame::new(graphics),
            stream_handle: graphics.stream_handle.clone(),

            ctf_flag_offset,
            projectile_sprite_offset: projectile_sprite_off,
            pickup_sprite_off,
            particle_splat_off,

            weapon_quad_offsets,
            ninja_quad_offset,
        }
    }

    pub fn render(&mut self, pipe: &mut GameObjectsRenderPipe) {
        let mut base_state = State::default();
        self.canvas_mapping
            .map_canvas_for_ingame_items(&mut base_state, pipe.camera);

        pipe.projectiles.values().for_each(|proj| {
            self.render_projectile(pipe, proj, pipe.character_infos, &base_state);
        });
        pipe.flags.values().for_each(|flag| {
            self.render_flag(pipe, flag, pipe.character_infos, &base_state);
        });
        pipe.lasers.values().for_each(|laser| {
            self.render_laser(pipe, laser, pipe.character_infos, &base_state);
        });
        pipe.pickups.values().for_each(|pickup| {
            self.render_pickup(pipe, pickup, &base_state);
        });
    }

    pub fn render_projectile(
        &mut self,
        pipe: &mut GameObjectsRenderPipe,
        proj: &ProjectileRenderInfo,
        character_infos: &FxLinkedHashMap<CharacterId, CharacterInfo>,
        base_state: &State,
    ) {
        let ty = proj.ty;
        let pos = proj.pos;
        let vel = proj.vel;

        let weapon_name = proj
            .owner_id
            .and_then(|id| character_infos.get(&id))
            .map(|c| &c.info.weapon);
        let weapon = pipe.weapon_container.get_or_default_opt(weapon_name);

        let mut quad_scope = quad_scope_begin();
        quad_scope.set_state(base_state);

        let phased_alpha =
            if pipe.phased || (proj.phased && proj.owner_id.as_ref() != pipe.local_character_id) {
                pipe.phased_alpha
            } else {
                1.0
            };

        // add particle for this projectile
        // don't check for validity of the projectile for the current weapon here, so particle effects are rendered for mod compatibility
        if ty == WeaponWithProjectile::Grenade {
            let mut effects = Effects::new(pipe.particle_manager, *pipe.cur_time);
            effects.smoke_trail(&pos, &(vel * -1.0), phased_alpha, 0.0, proj.owner_id);

            quad_scope
                .set_rotation((pipe.cur_time.as_secs_f32() as f64 * PI_F64 * 2.0 * 2.0) as f32);
        } else {
            let mut effects = Effects::new(pipe.particle_manager, *pipe.cur_time);
            effects.bullet_trail(&pos, phased_alpha, proj.owner_id);

            if length(&vel) > 0.00001 {
                quad_scope.set_rotation(angle(&vel));
            } else {
                quad_scope.set_rotation(0.0);
            }
        }

        let texture = match ty {
            WeaponWithProjectile::Gun => &weapon.gun.projectile.projectile,
            WeaponWithProjectile::Shotgun => &weapon.shotgun.projectile.projectile,
            WeaponWithProjectile::Grenade => &weapon.grenade.projectile.projectile,
        };
        quad_scope.set_colors_from_single(1.0, 1.0, 1.0, phased_alpha);
        self.items_quad_container.render_quad_container_as_sprite(
            self.projectile_sprite_offset,
            pos.x,
            pos.y,
            1.0,
            1.0,
            quad_scope,
            texture.into(),
        );
    }

    pub fn render_pickup(
        &mut self,
        pipe: &mut GameObjectsRenderPipe,
        pickup: &PickupRenderInfo,
        base_state: &State,
    ) {
        let ty = pickup.ty;
        let angle = 0.0;

        let mut pos = pickup.pos;

        let phased_alpha = if pipe.phased
            || (pickup.phased && pickup.owner_id.as_ref() != pipe.local_character_id)
        {
            pipe.phased_alpha
        } else {
            1.0
        };

        let mut quad_scope = quad_scope_begin();
        quad_scope.set_state(base_state);
        let (texture, quad_offset) = match ty {
            PickupType::PowerupHealth => {
                let key = pickup
                    .owner_id
                    .and_then(|id| pipe.character_infos.get(&id))
                    .or_else(|| {
                        pipe.local_character_id
                            .and_then(|id| pipe.character_infos.get(id))
                    })
                    .map(|c| &c.info.game);

                (
                    &pipe.game_container.get_or_default_opt(key).heart.tex,
                    self.pickup_sprite_off,
                )
            }
            PickupType::PowerupArmor => {
                let key = pickup
                    .owner_id
                    .and_then(|id| pipe.character_infos.get(&id))
                    .or_else(|| {
                        pipe.local_character_id
                            .and_then(|id| pipe.character_infos.get(id))
                    })
                    .map(|c| &c.info.game);
                (
                    &pipe.game_container.get_or_default_opt(key).shield.tex,
                    self.pickup_sprite_off,
                )
            }
            PickupType::PowerupWeapon(weapon) => {
                let key = pickup
                    .owner_id
                    .and_then(|id| pipe.character_infos.get(&id))
                    .or_else(|| {
                        pipe.local_character_id
                            .and_then(|id| pipe.character_infos.get(id))
                    })
                    .map(|c| &c.info.weapon);
                // go by weapon type instead
                let weapon_tex = pipe.weapon_container.get_or_default_opt(key);
                (
                    &weapon_tex.by_type(weapon).tex,
                    self.weapon_quad_offsets[weapon as usize],
                )
            }
            PickupType::PowerupNinja => {
                // randomly move the pickup a bit to the left
                pos.x -= 10.0 / 32.0;
                Effects::new(pipe.particle_manager, *pipe.cur_time).powerup_shine(
                    &pos,
                    &vec2::new(3.0, 18.0 / 32.0),
                    pickup.owner_id,
                );

                let key = pickup
                    .owner_id
                    .and_then(|id| pipe.character_infos.get(&id))
                    .or_else(|| {
                        pipe.local_character_id
                            .and_then(|id| pipe.character_infos.get(id))
                    })
                    .map(|c| &c.info.ninja);
                (
                    &pipe.ninja_container.get_or_default_opt(key).weapon, // TODO:
                    self.ninja_quad_offset,
                )
            }
        };
        /* TODO:
        else if(pCurrent.m_Type >= POWERUP_ARMOR_SHOTGUN && pCurrent.m_Type <= POWERUP_ARMOR_LASER)
        {
            QuadOffset = m_aPickupWeaponArmorOffset[pCurrent.m_Type - POWERUP_ARMOR_SHOTGUN];
            Graphics()->TextureSet(GameClient()->m_GameSkin.m_aSpritePickupWeaponArmor[pCurrent.m_Type - POWERUP_ARMOR_SHOTGUN]);
        }*/
        quad_scope.set_colors_from_single(1.0, 1.0, 1.0, phased_alpha);
        quad_scope.set_rotation(angle);

        let offset = pos.y + pos.x;
        let cur_time_f = pipe.cur_time.as_secs_f32();
        pos.x += (cur_time_f * 2.0 + offset).cos() * 2.5 / 32.0;
        pos.y += (cur_time_f * 2.0 + offset).sin() * 2.5 / 32.0;

        self.items_quad_container.render_quad_container_as_sprite(
            quad_offset,
            pos.x,
            pos.y,
            1.0,
            1.0,
            quad_scope,
            texture.into(),
        );
    }

    pub fn render_flag(
        &mut self,
        pipe: &mut GameObjectsRenderPipe,
        flag: &FlagRenderInfo,
        character_infos: &FxLinkedHashMap<CharacterId, CharacterInfo>,
        base_state: &State,
    ) {
        let angle = 0.0;
        let size = 42.0 / 32.0;
        let ty = flag.ty;

        let phased_alpha =
            if pipe.phased || (flag.phased && flag.owner_id.as_ref() != pipe.local_character_id) {
                pipe.phased_alpha
            } else {
                1.0
            };

        let key = flag
            .owner_id
            .and_then(|id| character_infos.get(&id))
            .or_else(|| {
                pipe.local_character_id
                    .and_then(|id| pipe.character_infos.get(id))
            })
            .map(|c| &c.info.ctf);
        let ctf_tex = pipe.ctf_container.get_or_default_opt(key);

        let mut quad_scope = quad_scope_begin();
        quad_scope.set_state(base_state);
        let texture = if let FlagType::Red = ty {
            &ctf_tex.flag_red
        } else {
            &ctf_tex.flag_blue
        };
        quad_scope.set_colors_from_single(1.0, 1.0, 1.0, phased_alpha);

        quad_scope.set_rotation(angle);

        let pos = flag.pos;

        self.items_quad_container.render_quad_container_as_sprite(
            self.ctf_flag_offset,
            pos.x,
            pos.y - size * 0.75,
            1.0,
            1.0,
            quad_scope,
            texture.into(),
        );
    }

    pub fn render_laser(
        &mut self,
        pipe: &mut GameObjectsRenderPipe,
        cur: &LaserRenderInfo,
        character_infos: &FxLinkedHashMap<CharacterId, CharacterInfo>,
        base_state: &State,
    ) {
        let owner = cur.owner_id.and_then(|id| character_infos.get(&id));
        let from = cur.from;
        let pos = cur.pos;
        let laser_len = distance(&pos, &from);

        let phased_alpha =
            if pipe.phased || (cur.phased && cur.owner_id.as_ref() != pipe.local_character_id) {
                pipe.phased_alpha
            } else {
                1.0
            };

        let (inner_color, outer_color) = match owner {
            Some(owner) => match cur.ty {
                LaserType::Rifle => (
                    owner.laser_info.inner_color.into(),
                    owner.laser_info.outer_color.into(),
                ),
                LaserType::Door | LaserType::Freeze | LaserType::Shotgun => (
                    ColorRgba {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: 1.0,
                    },
                    ColorRgba {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: 1.0,
                    },
                ),
            },
            None => match cur.ty {
                LaserType::Rifle | LaserType::Shotgun | LaserType::Door | LaserType::Freeze => (
                    ColorRgba {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: 1.0,
                    },
                    ColorRgba {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: 1.0,
                    },
                ),
            },
        };

        if laser_len > 0.0 {
            let dir = normalize_pre_length(&(pos - from), laser_len);

            let ticks_per_second = pipe.game_time_info.ticks_per_second;
            let intra_tick_ratio =
                intra_tick_time_to_ratio(pipe.game_time_info.intra_tick_time, ticks_per_second);
            let ticks = cur.eval_tick_ratio.map(|(eval_tick, lifetime)| {
                (eval_tick as f64 + intra_tick_ratio) / lifetime.get() as f64
            });

            let ms = ticks.unwrap_or(1.0) as f32;
            let mut a = ms;
            a = a.clamp(0.0, 1.0);
            let ia = 1.0 - a;

            // do outline
            let out = vec2::new(dir.y, -dir.x) * (7.0 / 32.0 * ia);
            let outter = StreamedQuad::default()
                .pos_free_form(
                    vec2::new(from.x - out.x, from.y - out.y),
                    vec2::new(from.x + out.x, from.y + out.y),
                    vec2::new(pos.x - out.x, pos.y - out.y),
                    vec2::new(pos.x + out.x, pos.y + out.y),
                )
                .colorf(vec4::new(
                    outer_color.r,
                    outer_color.g,
                    outer_color.b,
                    outer_color.a * phased_alpha,
                ));

            // do inner
            let out = vec2::new(dir.y, -dir.x) * (5.0 / 32.0 * ia);
            let inner = StreamedQuad::default()
                .pos_free_form(
                    vec2::new(from.x - out.x, from.y - out.y),
                    vec2::new(from.x + out.x, from.y + out.y),
                    vec2::new(pos.x - out.x, pos.y - out.y),
                    vec2::new(pos.x + out.x, pos.y + out.y),
                )
                .colorf(vec4::new(
                    inner_color.r,
                    inner_color.g,
                    inner_color.b,
                    inner_color.a * phased_alpha,
                ));
            self.stream_handle
                .render_quads(&[outter, inner], *base_state, TextureType::None);
        }

        // render head
        let key = owner.map(|c| &c.info.weapon);
        let heads = &pipe.weapon_container.get_or_default_opt(key).laser.heads;
        {
            let head_index = pipe.particle_manager.rng.random_int_in(0..=2) as usize;
            let mut quad_scope = quad_scope_begin();
            quad_scope.set_state(base_state);
            quad_scope.set_rotation(
                (pipe.cur_time.as_secs_f64() * pipe.game_time_info.ticks_per_second.get() as f64)
                    .rem_euclid(pipe.game_time_info.ticks_per_second.get() as f64)
                    as f32,
            );
            quad_scope.set_colors_from_single(
                outer_color.r,
                outer_color.g,
                outer_color.b,
                outer_color.a * phased_alpha,
            );
            self.items_quad_container.render_quad_container_as_sprite(
                self.particle_splat_off,
                pos.x,
                pos.y,
                1.0,
                1.0,
                quad_scope,
                (&heads[head_index]).into(),
            );
            // inner
            let mut quad_scope = quad_scope_begin();
            quad_scope.set_state(base_state);
            quad_scope.set_rotation(
                (pipe.cur_time.as_secs_f64() * pipe.game_time_info.ticks_per_second.get() as f64)
                    .rem_euclid(pipe.game_time_info.ticks_per_second.get() as f64)
                    as f32,
            );
            quad_scope.set_colors_from_single(
                inner_color.r,
                inner_color.g,
                inner_color.b,
                inner_color.a * phased_alpha,
            );
            self.items_quad_container.render_quad_container_as_sprite(
                self.particle_splat_off,
                pos.x,
                pos.y,
                20.0 / 24.0,
                20.0 / 24.0,
                quad_scope,
                (&heads[head_index]).into(),
            );
        }
    }
}
