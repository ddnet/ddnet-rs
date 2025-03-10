use std::time::Duration;

use base::linked_hash_map_view::FxLinkedHashMap;
use client_containers::{
    container::ContainerKey,
    ctf::CtfContainer,
    hud::{Hud, HudContainer},
    skins::SkinContainer,
    weapons::{WeaponContainer, Weapons},
};
use client_render::hud::page::{HudRender, HudRenderPipe};
use client_render_base::render::tee::RenderTee;
use game_interface::types::{
    emoticons::{EnumCount, IntoEnumIterator},
    game::{GameTickType, NonZeroGameTickType},
    id_types::CharacterId,
    render::{
        character::{
            CharacterInfo, LocalCharacterDdrace, LocalCharacterRenderInfo, LocalCharacterVanilla,
        },
        game::GameRenderInfo,
    },
    weapons::WeaponType,
};
use graphics::{
    graphics::graphics::Graphics,
    handles::{
        canvas::canvas::GraphicsCanvasHandle,
        quad_container::quad_container::{QuadContainer, QuadContainerRenderCount},
    },
    quad_container::Quad,
    streaming::quad_scope_begin,
};
use graphics_types::rendering::State;
use math::math::{vector::vec2, PI};
use ui_base::ui::UiCreator;
use vanilla::weapons::definitions::weapon_def::{get_weapon_sprite_scale, get_weapon_visual_scale};

const GRID_SIZE: f32 = 36.0;

pub struct RenderHudPipe<'a> {
    pub hud_container: &'a mut HudContainer,
    pub hud_key: Option<&'a ContainerKey>,
    pub weapon_container: &'a mut WeaponContainer,
    pub weapon_key: Option<&'a ContainerKey>,
    pub local_player_render_info: &'a LocalCharacterRenderInfo,
    pub cur_weapon: WeaponType,
    pub race_timer_counter: &'a GameTickType,
    pub ticks_per_second: &'a NonZeroGameTickType,
    pub cur_time: &'a Duration,
    pub game: Option<&'a GameRenderInfo>,
    pub skin_container: &'a mut SkinContainer,
    pub skin_renderer: &'a RenderTee,
    pub ctf_container: &'a mut CtfContainer,
    pub character_infos: &'a FxLinkedHashMap<CharacterId, CharacterInfo>,
}

pub struct RenderOffsetsVanilla {
    heart_offset: usize,
    shield_offset: usize,
    weapon_ammo_offsets: [usize; WeaponType::COUNT],
}

pub struct RenderOffsetsDdrace {
    pub owned_weapons_offsets: [usize; WeaponType::COUNT],
    pub airjump_offset: usize,
    pub airjump_used_offset: usize,
    pub solo_offset: usize,
    pub disabled_collision_offset: usize,
    pub endless_jump_offset: usize,
    pub endless_hook_offset: usize,
    pub jetpack_offset: usize,
    pub disabled_hook_others_offset: usize,
    pub disabled_weapons_offsets: [usize; WeaponType::COUNT],
    pub tele_grenade_offset: usize,
    pub tele_gun_offset: usize,
    pub tele_laser_offset: usize,
    pub deep_frozen_offset: usize,
    pub live_frozen_offset: usize,
    pub disabled_finish_offset: usize,
    pub dummy_hammer_offset: usize,
    pub dummy_copy_offset: usize,
    pub stage_locked_offset: usize,
    pub team0_mode_offset: usize,
}

pub struct RenderHud {
    quad_container: QuadContainer,

    pub(crate) ui: HudRender,

    vanilla_offsets: RenderOffsetsVanilla,
    ddrace_offsets: RenderOffsetsDdrace,

    canvas_handle: GraphicsCanvasHandle,
}

impl RenderHud {
    pub fn new(graphics: &Graphics, creator: &UiCreator) -> Self {
        let mut quads: Vec<Quad> = Default::default();

        let x = GRID_SIZE / 3.0;
        let y = GRID_SIZE / 3.0;

        let vanilla_offsets = {
            // ammo of the different weapons
            let weapon_ammo_offsets = (0..WeaponType::COUNT)
                .map(|_| {
                    let offset = quads.len();
                    quads.extend((0..10).map(|index| {
                        Quad::new()
                            .from_rect(
                                x + index as f32 * GRID_SIZE + 1.0,
                                y + GRID_SIZE * 2.0,
                                GRID_SIZE * 10.0 / 12.0,
                                GRID_SIZE * 10.0 / 12.0,
                            )
                            .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0))
                    }));
                    offset
                })
                .collect::<Vec<_>>();

            // hearts
            let heart_offset = quads.len();
            quads.extend((0..10).map(|index| {
                Quad::new()
                    .from_rect(x + index as f32 * GRID_SIZE, y, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0))
            }));

            // shields
            let shield_offset = quads.len();
            quads.extend((0..10).map(|index| {
                Quad::new()
                    .from_rect(
                        x + index as f32 * GRID_SIZE,
                        y + GRID_SIZE,
                        GRID_SIZE,
                        GRID_SIZE,
                    )
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0))
            }));
            RenderOffsetsVanilla {
                heart_offset,
                shield_offset,

                weapon_ammo_offsets: weapon_ammo_offsets.try_into().unwrap(),
            }
        };

        let ddrace_offsets = {
            // Quads for displaying the available and used jumps
            let airjump_offset = quads.len();
            quads.extend((0..10).map(|index| {
                Quad::new()
                    .from_rect(x + index as f32 * GRID_SIZE, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0))
            }));

            let airjump_used_offset = quads.len();
            quads.extend((0..10).map(|index| {
                Quad::new()
                    .from_rect(x + index as f32 * GRID_SIZE, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0))
            }));

            // Quads for displaying weapons
            let owned_weapons_offsets: [usize; WeaponType::COUNT] = WeaponType::iter()
                .map(|w| {
                    let size = get_weapon_visual_scale(&w) * 8.0 * GRID_SIZE / 12.0;
                    let scale = get_weapon_sprite_scale(&w);
                    let quad = Quad::new()
                        .from_width_and_height_centered(size * scale.0, size * scale.1)
                        .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0));
                    let offset = quads.len();
                    quads.push(quad);
                    offset
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();

            // Quads for displaying capabilities
            let endless_jump_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );
            let endless_hook_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );
            let jetpack_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );
            let tele_grenade_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );
            let tele_gun_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );
            let tele_laser_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );

            // Quads for displaying prohibited capabilities
            let solo_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );
            let disabled_collision_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );
            let disabled_hook_others_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );
            let disabled_hammer_hit_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );
            let disabled_gun_hit_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );
            let disabled_shotgun_hit_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );
            let disabled_grenade_hit_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );
            let disabled_laser_hit_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );

            // Quads for displaying freeze status
            let deep_frozen_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );
            let live_frozen_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );

            // Quads for displaying dummy actions
            let dummy_hammer_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );
            let dummy_copy_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );

            // Quads for displaying team modes
            let disabled_finish_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );
            let stage_locked_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );
            let team0_mode_offset = quads.len();
            quads.push(
                Quad::new()
                    .from_rect(0.0, 0.0, GRID_SIZE, GRID_SIZE)
                    .with_uv_from_points(&vec2::new(0.0, 0.0), &vec2::new(1.0, 1.0)),
            );

            RenderOffsetsDdrace {
                owned_weapons_offsets,
                airjump_offset,
                airjump_used_offset,
                solo_offset,
                disabled_collision_offset,
                endless_jump_offset,
                endless_hook_offset,
                jetpack_offset,
                disabled_hook_others_offset,
                disabled_weapons_offsets: [
                    disabled_hammer_hit_offset,
                    disabled_gun_hit_offset,
                    disabled_shotgun_hit_offset,
                    disabled_grenade_hit_offset,
                    disabled_laser_hit_offset,
                ],
                tele_grenade_offset,
                tele_gun_offset,
                tele_laser_offset,
                deep_frozen_offset,
                live_frozen_offset,
                disabled_finish_offset,
                dummy_hammer_offset,
                dummy_copy_offset,
                stage_locked_offset,
                team0_mode_offset,
            }
        };

        let quad_container = graphics.quad_container_handle.create_quad_container(quads);

        Self {
            quad_container,

            vanilla_offsets,
            ddrace_offsets,

            canvas_handle: graphics.canvas_handle.clone(),

            ui: HudRender::new(graphics, creator),
        }
    }

    fn render_vanilla(
        &mut self,
        state: &State,
        info: &LocalCharacterVanilla,
        hud: &Hud,
        weapons: &Weapons,
        cur_weapon: WeaponType,
    ) {
        let mut draw_scope = quad_scope_begin();
        draw_scope.set_state(state);
        draw_scope.set_colors_from_single(1.0, 1.0, 1.0, 1.0);

        // render heart
        let cur_health = info.health.min(10) as usize;
        let texture = &hud.vanilla.heart;
        self.quad_container.render_quad_container(
            self.vanilla_offsets.heart_offset,
            &QuadContainerRenderCount::Count(cur_health),
            0.0,
            0.0,
            1.0,
            1.0,
            draw_scope,
            texture.into(),
        );
        let texture = &hud.vanilla.heart_empty;
        self.quad_container.render_quad_container(
            self.vanilla_offsets.heart_offset + cur_health,
            &QuadContainerRenderCount::Count(10 - cur_health),
            0.0,
            0.0,
            1.0,
            1.0,
            draw_scope,
            texture.into(),
        );

        // render shields
        let cur_armor = info.armor.min(10) as usize;
        let texture = &hud.vanilla.shield;
        self.quad_container.render_quad_container(
            self.vanilla_offsets.shield_offset,
            &QuadContainerRenderCount::Count(cur_armor),
            0.0,
            0.0,
            1.0,
            1.0,
            draw_scope,
            texture.into(),
        );

        let texture = &hud.vanilla.shield_empty;
        self.quad_container.render_quad_container(
            self.vanilla_offsets.shield_offset + cur_armor,
            &QuadContainerRenderCount::Count(10 - cur_armor),
            0.0,
            0.0,
            1.0,
            1.0,
            draw_scope,
            texture.into(),
        );

        // render ammo
        let projectile = match cur_weapon {
            WeaponType::Hammer => None,
            WeaponType::Gun => Some(&weapons.gun.projectile),
            WeaponType::Shotgun => Some(&weapons.shotgun.projectile),
            WeaponType::Grenade => Some(&weapons.grenade.projectile),
            WeaponType::Laser => Some(&weapons.laser.projectile),
        };
        if let Some(projectile) = info
            .ammo_of_weapon
            .is_some()
            .then_some(projectile)
            .flatten()
        {
            let cur_ammo_of_weapon = info.ammo_of_weapon.unwrap().min(10) as usize;
            let texture = &projectile.projectile;
            self.quad_container.render_quad_container(
                self.vanilla_offsets.weapon_ammo_offsets[cur_weapon as usize],
                &QuadContainerRenderCount::Count(cur_ammo_of_weapon),
                0.0,
                0.0,
                1.0,
                1.0,
                draw_scope,
                texture.into(),
            );
        }
    }

    fn render_ddrace(
        &mut self,
        state: &State,
        info: &LocalCharacterDdrace,
        hud: &Hud,
        weapons: &Weapons,
        cur_weapon: WeaponType,
    ) {
        let mut draw_scope = quad_scope_begin();
        draw_scope.set_state(state);
        draw_scope.set_colors_from_single(1.0, 1.0, 1.0, 1.0);

        let max_jumps = info.max_jumps.map(|m| m.get().min(10)).unwrap_or_default() as usize;
        let jumps = (info.jumps as usize).min(10).min(max_jumps);

        let y = GRID_SIZE / 3.0 + GRID_SIZE * 2.0;

        // render available and used jumps
        let texture = &hud.ddrace.jump;
        self.quad_container.render_quad_container(
            self.ddrace_offsets.airjump_offset,
            &QuadContainerRenderCount::Count(jumps),
            0.0,
            y,
            1.0,
            1.0,
            draw_scope,
            texture.into(),
        );
        let texture = &hud.ddrace.jump_used;
        let jumps_to_display = max_jumps - jumps;
        self.quad_container.render_quad_container(
            self.ddrace_offsets.airjump_used_offset + jumps,
            &QuadContainerRenderCount::Count(jumps_to_display),
            0.0,
            y,
            1.0,
            1.0,
            draw_scope,
            texture.into(),
        );

        let x = GRID_SIZE / 3.0 + GRID_SIZE;
        let y = GRID_SIZE / 3.0 + GRID_SIZE;

        // render weapons
        {
            let mut x = x;
            let weapon_widths = [
                GRID_SIZE * 1.3333,
                GRID_SIZE,
                GRID_SIZE,
                GRID_SIZE,
                GRID_SIZE,
            ];
            let weapon_initial_offsets = [
                -GRID_SIZE / 4.0,
                -GRID_SIZE / 3.0,
                -GRID_SIZE / 12.0,
                -GRID_SIZE / 12.0,
                -GRID_SIZE / 6.0,
            ];
            let mut initial_offset_added = false;
            for ty in WeaponType::iter().filter(|w| info.owned_weapons.contains(w)) {
                if !initial_offset_added {
                    x += weapon_initial_offsets[ty as usize];
                    initial_offset_added = true;
                }
                let mut draw_scope = draw_scope;
                if cur_weapon != ty {
                    draw_scope.set_colors_from_single(1.0, 1.0, 1.0, 0.4);
                }
                draw_scope.set_rotation(PI * 7.0 / 4.0);
                let texture = weapons.by_type(ty);
                self.quad_container.render_quad_container(
                    self.ddrace_offsets.owned_weapons_offsets[ty as usize],
                    &QuadContainerRenderCount::Count(1),
                    x,
                    y,
                    1.0,
                    1.0,
                    draw_scope,
                    (&texture.tex).into(),
                );
                x += weapon_widths[ty as usize];
            }
        }

        // render capabilities
        let mut x = GRID_SIZE / 3.0;
        let mut y = y
            + GRID_SIZE
            + if jumps > 0 || jumps_to_display > 0 {
                GRID_SIZE
            } else {
                0.0
            };

        let mut has_any_capability = false;
        if info.max_jumps.is_none() {
            has_any_capability = true;
            let texture = &hud.ddrace.endless_jump;
            self.quad_container.render_quad_container(
                self.ddrace_offsets.endless_jump_offset,
                &QuadContainerRenderCount::Count(1),
                x,
                y,
                1.0,
                1.0,
                draw_scope,
                texture.into(),
            );
            x += GRID_SIZE;
        }
        if info.endless_hook {
            has_any_capability = true;
            let texture = &hud.ddrace.endless_hook;
            self.quad_container.render_quad_container(
                self.ddrace_offsets.endless_hook_offset,
                &QuadContainerRenderCount::Count(1),
                x,
                y,
                1.0,
                1.0,
                draw_scope,
                texture.into(),
            );
            x += GRID_SIZE;
        }
        if info.jetpack {
            has_any_capability = true;
            let texture = &hud.ddrace.jetpack;
            self.quad_container.render_quad_container(
                self.ddrace_offsets.jetpack_offset,
                &QuadContainerRenderCount::Count(1),
                x,
                y,
                1.0,
                1.0,
                draw_scope,
                texture.into(),
            );
            x += GRID_SIZE;
        }
        if info.tele_weapons.contains(&WeaponType::Gun)
            && info.owned_weapons.contains(&WeaponType::Gun)
        {
            has_any_capability = true;
            let texture = &hud.ddrace.tele_pistol;
            self.quad_container.render_quad_container(
                self.ddrace_offsets.tele_gun_offset,
                &QuadContainerRenderCount::Count(1),
                x,
                y,
                1.0,
                1.0,
                draw_scope,
                texture.into(),
            );
            x += GRID_SIZE;
        }
        if info.tele_weapons.contains(&WeaponType::Grenade)
            && info.owned_weapons.contains(&WeaponType::Grenade)
        {
            has_any_capability = true;
            let texture = &hud.ddrace.tele_grenade;
            self.quad_container.render_quad_container(
                self.ddrace_offsets.tele_grenade_offset,
                &QuadContainerRenderCount::Count(1),
                x,
                y,
                1.0,
                1.0,
                draw_scope,
                texture.into(),
            );
            x += GRID_SIZE;
        }
        if info.tele_weapons.contains(&WeaponType::Laser)
            && info.owned_weapons.contains(&WeaponType::Laser)
        {
            has_any_capability = true;
            let texture = &hud.ddrace.tele_laser;
            self.quad_container.render_quad_container(
                self.ddrace_offsets.tele_laser_offset,
                &QuadContainerRenderCount::Count(1),
                x,
                y,
                1.0,
                1.0,
                draw_scope,
                texture.into(),
            );
        }

        // render prohibited capabilities
        let mut x = GRID_SIZE / 3.0;
        if has_any_capability {
            y += GRID_SIZE;
        }
        let mut hash_any_prohibited_capabilities = false;
        if info.solo {
            hash_any_prohibited_capabilities = true;
            let texture = &hud.ddrace.solo;
            self.quad_container.render_quad_container(
                self.ddrace_offsets.solo_offset,
                &QuadContainerRenderCount::Count(1),
                x,
                y,
                1.0,
                1.0,
                draw_scope,
                texture.into(),
            );
            x += GRID_SIZE;
        }
        if !info.can_collide {
            hash_any_prohibited_capabilities = true;
            let texture = &hud.ddrace.collision_off;
            self.quad_container.render_quad_container(
                self.ddrace_offsets.disabled_collision_offset,
                &QuadContainerRenderCount::Count(1),
                x,
                y,
                1.0,
                1.0,
                draw_scope,
                texture.into(),
            );
            x += GRID_SIZE;
        }
        if !info.can_hook_others {
            hash_any_prohibited_capabilities = true;
            let texture = &hud.ddrace.disabled_hook_others;
            self.quad_container.render_quad_container(
                self.ddrace_offsets.disabled_hook_others_offset,
                &QuadContainerRenderCount::Count(1),
                x,
                y,
                1.0,
                1.0,
                draw_scope,
                texture.into(),
            );
            x += GRID_SIZE;
        }
        for ty in WeaponType::iter() {
            if info.disabled_weapons.contains(&ty) && info.owned_weapons.contains(&ty) {
                hash_any_prohibited_capabilities = true;
                let texture = &hud.ddrace.disabled_weapons[ty as usize];
                self.quad_container.render_quad_container(
                    self.ddrace_offsets.disabled_weapons_offsets[ty as usize],
                    &QuadContainerRenderCount::Count(1),
                    x,
                    y,
                    1.0,
                    1.0,
                    draw_scope,
                    texture.into(),
                );
                x += GRID_SIZE;
            }
        }

        // render dummy actions and freeze state
        let mut x = GRID_SIZE / 3.0;
        if hash_any_prohibited_capabilities {
            y += GRID_SIZE;
        }
        if info.stage_locked {
            let texture = &hud.ddrace.stage_locked;
            self.quad_container.render_quad_container(
                self.ddrace_offsets.stage_locked_offset,
                &QuadContainerRenderCount::Count(1),
                x,
                y,
                1.0,
                1.0,
                draw_scope,
                texture.into(),
            );
            x += GRID_SIZE;
        }
        if !info.can_finish {
            let texture = &hud.ddrace.disabled_finish;
            self.quad_container.render_quad_container(
                self.ddrace_offsets.disabled_finish_offset,
                &QuadContainerRenderCount::Count(1),
                x,
                y,
                1.0,
                1.0,
                draw_scope,
                texture.into(),
            );
            x += GRID_SIZE;
        }
        if info.team0_mode {
            let texture = &hud.ddrace.team0_mode;
            self.quad_container.render_quad_container(
                self.ddrace_offsets.team0_mode_offset,
                &QuadContainerRenderCount::Count(1),
                x,
                y,
                1.0,
                1.0,
                draw_scope,
                texture.into(),
            );
            x += GRID_SIZE;
        }
        if info.deep_frozen {
            let texture = &hud.ddrace.deep_frozen;
            self.quad_container.render_quad_container(
                self.ddrace_offsets.deep_frozen_offset,
                &QuadContainerRenderCount::Count(1),
                x,
                y,
                1.0,
                1.0,
                draw_scope,
                texture.into(),
            );
            x += GRID_SIZE;
        }
        if info.live_frozen {
            let texture = &hud.ddrace.live_frozen;
            self.quad_container.render_quad_container(
                self.ddrace_offsets.live_frozen_offset,
                &QuadContainerRenderCount::Count(1),
                x,
                y,
                1.0,
                1.0,
                draw_scope,
                texture.into(),
            );
        }
    }

    pub fn render(&mut self, pipe: &mut RenderHudPipe) {
        self.ui.render(&mut HudRenderPipe {
            cur_time: pipe.cur_time,
            race_timer_counter: pipe.race_timer_counter,
            ticks_per_second: pipe.ticks_per_second,
            game: pipe.game,
            skin_container: pipe.skin_container,
            skin_renderer: pipe.skin_renderer,
            ctf_container: pipe.ctf_container,
            character_infos: pipe.character_infos,
        });

        let hud = pipe.hud_container.get_or_default_opt(pipe.hud_key);
        let weapon = pipe.weapon_container.get_or_default_opt(pipe.weapon_key);
        let mut state = State::default();
        let ppp = self
            .ui
            .ui
            .zoom_level
            .get()
            .unwrap_or(self.canvas_handle.pixels_per_point());
        state.map_canvas(
            0.0,
            0.0,
            self.canvas_handle.canvas_width() as f32 / ppp,
            self.canvas_handle.canvas_height() as f32 / ppp,
        );

        match pipe.local_player_render_info {
            LocalCharacterRenderInfo::Vanilla(info) => {
                self.render_vanilla(&state, info, hud, weapon, pipe.cur_weapon);
            }
            LocalCharacterRenderInfo::Ddrace(info) => {
                self.render_ddrace(&state, info, hud, weapon, pipe.cur_weapon);
            }
            LocalCharacterRenderInfo::Unavailable => {
                // nothing to do
            }
        }
    }
}
