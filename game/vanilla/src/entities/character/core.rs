pub mod character_core {
    use std::ops::{AddAssign, ControlFlow};

    use crate::reusable::{CloneWithCopyableElements, ReusableCore};
    use game_base::mapdef_06::DdraceTileNum;
    use game_interface::{
        events::{
            GameCharacterEffectEvent, GameCharacterEventEffect, GameCharacterEventSound,
            GameCharacterSoundEvent, GameWorldEntityEffectEvent, GameWorldEntitySoundEvent,
        },
        types::{
            id_types::CharacterId,
            input::{CharacterInput, CharacterInputState},
            render::character::{HookCollisionLine, HookCollisionLineColor},
        },
    };
    use hiarc::Hiarc;
    use num::FromPrimitive;

    use crate::{
        collision::collision::{Collision, CollisionTile, CollisionTypes},
        entities::character::{
            character::{Characters, CharactersView},
            hook::character_hook::{CharacterHook, Hook, HookState},
            pos::character_pos::{CharacterPos, CharacterPositionPlayfield},
        },
        simulation_pipe::simulation_pipe::{
            GameWorldPendingEvents, SimulationPipeCharactersGetter,
        },
        state::state::TICKS_PER_SECOND,
        world::world::GameWorld,
    };

    use math::math::{
        closest_point_on_line, distance, distance_squared, dot, length, mix, normalize,
        round_f64_to_int, round_to_int,
        vector::{dvec2, ivec2, vec2},
    };

    use pool::traits::Recyclable;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Hiarc, Serialize, Deserialize, Clone)]
    pub struct CoreReusable {}

    impl CloneWithCopyableElements for CoreReusable {
        fn copy_clone_from(&mut self, _other: &Self) {}
    }

    impl Recyclable for CoreReusable {
        fn new() -> Self {
            Self {}
        }

        fn reset(&mut self) {}
    }

    impl ReusableCore for CoreReusable {}

    #[derive(Debug, Hiarc, Copy, Clone, Default, Serialize, Deserialize)]
    pub struct QueuedHook {
        pub clicked: u64,
        pub cursor: dvec2,
    }

    #[derive(Debug, Hiarc, Copy, Clone, Default, Serialize, Deserialize)]
    pub struct Core {
        pub vel: vec2,

        new_hook: bool,

        pub(crate) queued_hooks: QueuedHook,

        pub(crate) jumped: i32,
        // counts the jumps performed in the air
        jumped_total: i32,
        jumps: i32,
        pub(crate) queued_jumps: u64,

        direction: i32,

        // DDRace
        colliding: i32,
        left_wall: bool,

        // DDNet Character
        solo: bool,
        collision_disabled: bool,
        hook_hit_disabled: bool,
        is_super: bool,

        move_restrictions: i32,
    }

    pub struct CorePipe<'a> {
        pub(crate) characters: &'a mut dyn SimulationPipeCharactersGetter,
        pub(crate) input: &'a CharacterInput,
    }

    impl CorePipe<'_> {
        fn get_other_character_id_and_cores_iter_by_ids_mut(
            &mut self,
            ids: &[CharacterId],
            for_each_func: &mut dyn FnMut(
                &CharacterId,
                &mut Core,
                &mut CoreReusable,
                &mut CharacterPos,
            ) -> ControlFlow<()>,
        ) -> ControlFlow<()> {
            self.characters
                .get_other_character_id_and_cores_iter_by_ids_mut(ids, for_each_func)
        }

        fn get_other_character_pos_by_id(&self, other_char_id: &CharacterId) -> &vec2 {
            self.characters.get_other_character_pos_by_id(other_char_id)
        }
    }

    pub struct CoreEvents<'a> {
        pub(crate) game_pending_events: &'a GameWorldPendingEvents,
        pub(crate) character_id: &'a CharacterId,
    }

    impl CoreEvents<'_> {
        fn push_effect(&self, pos: vec2, ev: GameCharacterEventEffect) {
            self.game_pending_events.push_effect(
                Some(*self.character_id),
                pos,
                GameWorldEntityEffectEvent::Character(GameCharacterEffectEvent::Effect(ev)),
            );
        }
        fn push_sound(&self, pos: vec2, ev: GameCharacterEventSound) {
            self.game_pending_events.push_sound(
                Some(*self.character_id),
                Some(pos),
                GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Sound(ev)),
            );
        }
    }

    enum CannotMove {
        Left = 1 << 0,
        Right = 1 << 1,
        Up = 1 << 2,
        Down = 1 << 3,
    }

    pub const PHYSICAL_SIZE: f32 = 28.0;
    const fn physical_size() -> f32 {
        PHYSICAL_SIZE
    }
    const HOOK_RADIUS: f32 = 2.0;
    const fn physical_size_vec2() -> ivec2 {
        ivec2 {
            x: PHYSICAL_SIZE as i32,
            y: PHYSICAL_SIZE as i32,
        }
    }

    impl Core {
        fn clamp_vel(move_restriction: i32, vel_param: &vec2) -> vec2 {
            let mut vel = *vel_param;
            if vel.x > 0.0 && (move_restriction & CannotMove::Right as i32) != 0 {
                vel.x = 0.0;
            }
            if vel.x < 0.0 && (move_restriction & CannotMove::Left as i32) != 0 {
                vel.x = 0.0;
            }
            if vel.y > 0.0 && (move_restriction & CannotMove::Down as i32) != 0 {
                vel.y = 0.0;
            }
            if vel.y < 0.0 && (move_restriction & CannotMove::Up as i32) != 0 {
                vel.y = 0.0;
            }
            vel
        }

        fn saturated_add<T: AddAssign + PartialOrd + num::traits::Zero>(
            min_val: T,
            max_val: T,
            current: T,
            modifier: T,
        ) -> T {
            let mut cur = current;
            if modifier < T::zero() {
                if cur < min_val {
                    return cur;
                }
                cur += modifier;
                if cur < min_val {
                    cur = min_val;
                }
                cur
            } else {
                if cur > max_val {
                    return cur;
                }
                cur += modifier;
                if cur > max_val {
                    cur = max_val;
                }
                cur
            }
        }

        fn get_gravity(collision: &Collision, pos: &vec2) -> f32 {
            let tuning = collision.get_tune_at(pos);
            tuning.gravity
        }

        pub fn physics_tick(
            &mut self,
            pos: &mut CharacterPos,
            char_hook: &mut CharacterHook,
            use_input: bool,
            do_deferred_tick: bool,
            pipe: &mut CorePipe,
            collision: &Collision,
            character_events: CoreEvents<'_>,
        ) {
            let CharacterInput {
                cursor,
                state:
                    CharacterInputState {
                        dir, hook, jump, ..
                    },
                ..
            } = &pipe.input;
            self.move_restrictions = 0; // TODO core.m_pCollision->GetMoveRestrictions(UseInput ? IsSwitchActiveCb : 0, this, core.m_Pos);

            // get ground state
            let grounded: bool = collision.check_pointf(
                pos.pos().x + physical_size() / 2.0,
                pos.pos().y + physical_size() / 2.0 + 5.0,
            ) || collision.check_pointf(
                pos.pos().x - physical_size() / 2.0,
                pos.pos().y + physical_size() / 2.0 + 5.0,
            );
            let cursor_vec = cursor.to_vec2();
            let cursor = vec2::new(cursor_vec.x as f32, cursor_vec.y as f32);
            let target_direction: vec2 = normalize(&cursor);

            self.vel.y += Self::get_gravity(collision, pos.pos());

            let tuning = collision.get_tune_at(pos.pos());
            let max_speed = if grounded {
                tuning.ground_control_speed
            } else {
                tuning.air_control_speed
            };
            let accel = if grounded {
                tuning.ground_control_accel
            } else {
                tuning.air_control_accel
            };
            let friction = if grounded {
                tuning.ground_friction
            } else {
                tuning.air_friction
            };

            // handle input
            if use_input {
                self.direction = **dir;

                // Special jump cases:
                // self.jumped == -1: A tee may only make one ground jump. Second jumped bit is always set
                // self.jumped == 0: A tee may not make a jump. Second jumped bit is always set
                // self.jumped == 1: A tee may do either a ground jump or an air jump. Second jumped bit is set after the first jump
                // The second jumped bit can be overridden by special tiles so that the tee can nevertheless jump.

                // handle jump
                if self.queued_jumps > 0 || **jump {
                    if (self.jumped & 1) == 0 {
                        if grounded && ((self.jumped & 2) == 0 || self.jumps != 0) {
                            character_events
                                .push_sound(*pos.pos(), GameCharacterEventSound::GroundJump);
                            self.vel.y = -tuning.ground_jump_impulse;
                            if self.jumps > 1 {
                                self.jumped |= 1;
                            } else {
                                self.jumped |= 3;
                            }
                            self.jumped_total = 0;
                        } else if (self.jumped & 2) == 0 {
                            character_events
                                .push_sound(*pos.pos(), GameCharacterEventSound::AirJump);
                            character_events
                                .push_effect(*pos.pos(), GameCharacterEventEffect::AirJump);
                            self.vel.y = -tuning.air_jump_impulse;
                            self.jumped |= 3;
                            self.jumped_total += 1;
                        }
                    }
                } else {
                    self.jumped &= !1;
                }
                self.queued_jumps = 0;

                // handle hook
                if self.queued_hooks.clicked > 0 || **hook {
                    if let (Hook::None, Some(_)) = (
                        char_hook.hook(),
                        (self.queued_hooks.clicked > 0).then_some(()),
                    ) {
                        let cursor = self.queued_hooks.cursor;
                        let cursor = vec2::new(cursor.x as f32, cursor.y as f32);
                        let target_direction: vec2 = normalize(&cursor);

                        char_hook.set(
                            Hook::Active {
                                hook_pos: *pos.pos() + target_direction * physical_size() * 1.5,
                                hook_dir: target_direction,
                                hook_tele_base: vec2::default(),
                                hook_tick: 0,
                                hook_state: HookState::HookFlying,
                            },
                            None,
                        );
                        // self.triggered_events |= CoreEvent::HookLaunch as i32;
                    }
                } else {
                    char_hook.set(Hook::None, None);
                }
                self.queued_hooks.clicked = 0;
            }

            // handle jumping
            // 1 bit = to keep track if a jump has been made on this input (character is holding space bar)
            // 2 bit = to track if all air-jumps have been used up (tee gets dark feet)
            if grounded {
                self.jumped &= !2;
                self.jumped_total = 0;
            }

            // add the speed modification according to players wanted direction
            if self.direction < 0 {
                self.vel.x = Self::saturated_add(-max_speed, max_speed, self.vel.x, -accel);
            }
            if self.direction > 0 {
                self.vel.x = Self::saturated_add(-max_speed, max_speed, self.vel.x, accel);
            }
            if self.direction == 0 {
                self.vel.x *= friction;
            }

            // do hook
            let (mut hook_tmp, mut hooked_char) = char_hook.get();
            if let Hook::None = hook_tmp {
                char_hook.set(Hook::None, None);
            } else if let Hook::Active {
                hook_pos,
                hook_dir,
                hook_tele_base,
                hook_state,
                ..
            } = &mut hook_tmp
            {
                if *hook_state >= HookState::RetractStart && *hook_state < HookState::RetractEnd {
                    *hook_state = HookState::from_i32(*hook_state as i32 + 1).unwrap();
                } else if *hook_state == HookState::RetractEnd {
                    hook_tmp = Hook::WaitsForRelease;
                    hooked_char = None;
                } else if *hook_state == HookState::HookFlying {
                    let hook_old_tunings = collision.get_tune_at(hook_pos);
                    let mut new_pos = *hook_pos + *hook_dir * hook_old_tunings.hook_fire_speed;
                    let hook_new_tunings = collision.get_tune_at(hook_pos);
                    if (!self.new_hook
                        && distance_squared(pos.pos(), &new_pos)
                            > hook_new_tunings.hook_length.powf(2.0))
                        || (self.new_hook
                            && distance_squared(&*hook_tele_base, &new_pos)
                                > hook_new_tunings.hook_length.powf(2.0))
                    {
                        *hook_state = HookState::RetractStart;
                        new_pos = *pos.pos()
                            + normalize(&(new_pos - *pos.pos())) * hook_new_tunings.hook_length;
                    }

                    // make sure that the hook doesn't go though the ground
                    let mut before_col = vec2::default();
                    let pos_1 = new_pos;
                    let hit = collision.intersect_line(
                        hook_pos,
                        &pos_1,
                        &mut new_pos,
                        &mut before_col,
                        CollisionTypes::SOLID
                            | CollisionTypes::HOOK_TELE
                            | CollisionTypes::HOOK_TROUGH,
                    );

                    // Check against other players first
                    if !self.hook_hit_disabled && tuning.player_hooking > 0.0 {
                        let mut distance_hook = 0.0;
                        let (is_super, solo) = (self.is_super, self.solo);
                        let hook_len = length(&(new_pos - *hook_pos));
                        let ids = pos
                            .field
                            .by_distancef(hook_pos, hook_len + (physical_size() + 2.0));
                        pipe.get_other_character_id_and_cores_iter_by_ids_mut(
                            &ids,
                            &mut |char_id, char_core, _, char_pos| {
                                if !(is_super || char_core.is_super) && (char_core.solo || solo) {
                                    return ControlFlow::Continue(());
                                }

                                let mut closest_point: vec2 = vec2::default();
                                if closest_point_on_line(
                                    hook_pos,
                                    &new_pos,
                                    char_pos.pos(),
                                    &mut closest_point,
                                ) && distance_squared(char_pos.pos(), &closest_point)
                                    < (physical_size() + HOOK_RADIUS).powf(2.0)
                                    && (hooked_char.is_none()
                                        || distance_squared(hook_pos, char_pos.pos())
                                            < distance_hook)
                                {
                                    character_events.push_sound(
                                        *pos.pos(),
                                        GameCharacterEventSound::HookHitPlayer {
                                            hook_pos: Some(*hook_pos / 32.0),
                                        },
                                    );
                                    *hook_state = HookState::HookGrabbed;
                                    hooked_char = Some(*char_id);
                                    distance_hook = distance_squared(hook_pos, char_pos.pos());
                                }

                                ControlFlow::Continue(())
                            },
                        );
                    }

                    if *hook_state == HookState::HookFlying {
                        match hit {
                            CollisionTile::None => {
                                *hook_pos = new_pos;
                            }
                            CollisionTile::Solid(tile) => {
                                if tile == DdraceTileNum::Solid {
                                    character_events.push_sound(
                                        *pos.pos(),
                                        GameCharacterEventSound::HookHitHookable {
                                            hook_pos: Some(*hook_pos / 32.0),
                                        },
                                    );
                                    *hook_state = HookState::HookGrabbed;
                                } else {
                                    character_events.push_sound(
                                        *pos.pos(),
                                        GameCharacterEventSound::HookHitUnhookable {
                                            hook_pos: Some(*hook_pos / 32.0),
                                        },
                                    );
                                    *hook_state = HookState::RetractStart;
                                }
                                *hook_pos = new_pos;
                            }
                            CollisionTile::PlayerTele(_) => todo!(),
                            CollisionTile::HookTele(_) => {
                                if false
                                /* TODO: && self.m_pTeleOuts && !self.m_pTeleOuts->empty() && !(*self.m_pTeleOuts)[teleNr - 1].empty()*/
                                {
                                    hooked_char = None;
                                    self.new_hook = true;
                                    let _random_out = 0; // TODO: self.m_pWorld->RandomOr0((*self.m_pTeleOuts)[teleNr - 1].size());
                                    *hook_pos = vec2::default(); // TODO: (*self.m_pTeleOuts)[teleNr - 1][RandomOut] + TargetDirection * PhysicalSize() * 1.5f;
                                    *hook_dir = target_direction;
                                    *hook_tele_base = *hook_pos;
                                }
                            }
                            CollisionTile::WeaponTele(_) => todo!(),
                        }
                    }
                }

                char_hook.set(hook_tmp, hooked_char);
            }

            let (mut hook_tmp, mut hooked_char) = char_hook.get();
            if let Hook::Active {
                hook_pos,
                hook_tick,
                hook_state,
                ..
            } = &mut hook_tmp
            {
                if *hook_state == HookState::HookGrabbed {
                    if let Some(hooked_char_id) = hooked_char {
                        let other_char_pos = pipe.get_other_character_pos_by_id(&hooked_char_id);
                        *hook_pos = *other_char_pos;
                    }

                    // don't do this hook rutine when we are hook to a character
                    let hook_tunings = collision.get_tune_at(hook_pos);
                    if hooked_char.is_none()
                        && distance_squared(&*hook_pos, pos.pos()) > 46.0 * 46.0
                    {
                        let mut hook_vel =
                            normalize(&(*hook_pos - *pos.pos())) * hook_tunings.hook_drag_accel;
                        // the hook as more power to drag you up then down.
                        // this makes it easier to get on top of an platform
                        if hook_vel.y > 0.0 {
                            hook_vel.y *= 0.3;
                        }

                        // the hook will boost it's power if the character wants to move
                        // in that direction. otherwise it will dampen everything abit
                        if (hook_vel.x < 0.0 && self.direction < 0)
                            || (hook_vel.x > 0.0 && self.direction > 0)
                        {
                            hook_vel.x *= 0.95;
                        } else {
                            hook_vel.x *= 0.75;
                        }

                        let new_vel = self.vel + hook_vel;

                        // check if we are under the legal limit for the hook
                        if length(&new_vel) < hook_tunings.hook_drag_speed
                            || length(&new_vel) < length(&self.vel)
                        {
                            self.vel = new_vel; // no problem. apply
                        }
                    }

                    // release hook (max default hook time is 1.25 s)
                    *hook_tick += 1;
                    if hooked_char.is_some() {
                        let hook_duration =
                            (TICKS_PER_SECOND as f32 * hook_tunings.hook_duration) as i32;
                        if *hook_tick > hook_duration {
                            hooked_char = None;
                            hook_tmp = Hook::WaitsForRelease;
                        }
                    }
                }

                char_hook.set(hook_tmp, hooked_char);
            }

            if do_deferred_tick {
                self.physics_tick_deferred(pos, char_hook, collision, pipe);
            }
        }

        pub fn physics_tick_deferred(
            &mut self,
            pos: &mut CharacterPos,
            char_hook: &mut CharacterHook,
            collision: &Collision,
            pipe: &mut CorePipe,
        ) {
            let hooked_player = char_hook.hooked_char();

            let tunings = collision.get_tune_at(pos.pos());
            const PHY_RANGE_COLLISION: i32 = (physical_size() * 1.25) as i32;
            let mut ids = pos.in_range(PHY_RANGE_COLLISION);
            pipe.get_other_character_id_and_cores_iter_by_ids_mut(
                &ids,
                &mut |_, char_core, _, char_pos| {
                    if !(self.is_super || char_core.is_super) && (self.solo || char_core.solo) {
                        return ControlFlow::Continue(());
                    }

                    // handle character <-> character collision
                    let distance_sqr_pos = distance_squared(pos.pos(), char_pos.pos());
                    if distance_sqr_pos > 0.0 {
                        let can_collide = (self.is_super || char_core.is_super)
                            || (!self.collision_disabled
                                && !char_core.collision_disabled
                                && tunings.player_collision > 0.0);

                        if can_collide && distance_sqr_pos < (physical_size() * 1.25).powf(2.0) {
                            let dir = normalize(&(*pos.pos() - *char_pos.pos()));

                            let dist = distance_sqr_pos.sqrt();
                            let a = physical_size() * 1.45 - dist;
                            let mut velocity = 0.5;

                            // make sure that we don't add excess force by checking the
                            // direction against the current velocity. if not zero.
                            if length(&self.vel) > 0.0001 {
                                velocity = 1.0 - (dot(&normalize(&self.vel), &dir) + 1.0) / 2.0;
                            }

                            self.vel += dir * a * (velocity * 0.75);
                            self.vel *= 0.85;
                        }
                    }
                    ControlFlow::Continue(())
                },
            );

            if let Some(hooked_player) = hooked_player {
                // reuse the previous ids here
                ids.clear();
                ids.push(hooked_player);
                pipe.get_other_character_id_and_cores_iter_by_ids_mut(
                    &ids,
                    &mut |char_id, char_core, _, char_pos| {
                        if !(self.is_super || char_core.is_super) && (self.solo || char_core.solo) {
                            return ControlFlow::Continue(());
                        }
                        let distance_sqr_pos = distance_squared(pos.pos(), char_pos.pos());
                        if distance_sqr_pos > 0.0 {
                            let dir = normalize(&(*pos.pos() - *char_pos.pos()));
                            // handle hook influence
                            let other_tunings = collision.get_tune_at(char_pos.pos());
                            if !self.hook_hit_disabled
                                && hooked_player == *char_id
                                && other_tunings.player_hooking > 0.0
                            {
                                let dist = distance_sqr_pos.sqrt();
                                if dist > physical_size() * 1.50 {
                                    let hook_accel = other_tunings.hook_drag_accel
                                        * (dist / other_tunings.hook_length);
                                    let drag_speed = other_tunings.hook_drag_speed;

                                    // add force to the hooked character
                                    let mut temp = vec2::new(
                                        Self::saturated_add(
                                            -drag_speed,
                                            drag_speed,
                                            char_core.vel.x,
                                            hook_accel * dir.x * 1.5,
                                        ),
                                        Self::saturated_add(
                                            -drag_speed,
                                            drag_speed,
                                            char_core.vel.y,
                                            hook_accel * dir.y * 1.5,
                                        ),
                                    );
                                    char_core.vel =
                                        Self::clamp_vel(char_core.move_restrictions, &temp);
                                    // add a little bit force to the guy who has the grip
                                    temp.x = Self::saturated_add(
                                        -drag_speed,
                                        drag_speed,
                                        self.vel.x,
                                        -hook_accel * dir.x * 0.25,
                                    );
                                    temp.y = Self::saturated_add(
                                        -drag_speed,
                                        drag_speed,
                                        self.vel.y,
                                        -hook_accel * dir.y * 0.25,
                                    );
                                    self.vel = Self::clamp_vel(self.move_restrictions, &temp);
                                }
                            }
                        }

                        ControlFlow::Continue(())
                    },
                );
            }

            if let Hook::Active {
                hook_state: HookState::HookFlying,
                ..
            } = char_hook.hook()
            {
                self.new_hook = false;
            }

            // clamp the velocity to something sane
            if length(&self.vel) > 6000.0 {
                self.vel = normalize(&self.vel) * 6000.0;
            }
        }

        fn velocity_ramp(value: f32, start: f32, range: f32, curvature: f32) -> f32 {
            if value < start {
                return 1.0;
            }
            1.0 / curvature.powf((value - start) / range)
        }

        pub fn physics_move(
            &mut self,
            char_pos: &mut CharacterPos,
            pipe: &mut CorePipe,
            collision: &Collision,
        ) {
            let tuning = collision.get_tune_at(char_pos.pos());
            let ramp_value = Self::velocity_ramp(
                length(&self.vel) * 50.0,
                tuning.velramp_start,
                tuning.velramp_range,
                tuning.velramp_curvature,
            );

            self.vel.x *= ramp_value;

            let mut new_pos = *char_pos.pos();

            let old_vel = self.vel;
            collision.move_box(&mut new_pos, &mut self.vel, &physical_size_vec2(), 0.0);

            self.colliding = 0;
            if self.vel.x < 0.001 && self.vel.x > -0.001 {
                if old_vel.x > 0.0 {
                    self.colliding = 1;
                } else if old_vel.x < 0.0 {
                    self.colliding = 2;
                }
            } else {
                self.left_wall = true;
            }

            self.vel.x *= 1.0 / ramp_value;

            let tuning = collision.get_tune_at(char_pos.pos());
            if self.is_super
                || (tuning.player_collision > 0.0 && !self.collision_disabled && !self.solo)
            {
                // check character collision
                let distance_pos = distance(char_pos.pos(), &new_pos);
                if distance_pos > 0.0 {
                    let end = distance_pos + 1.0;
                    let mut last_pos = *char_pos.pos();
                    let mut core_pos = *char_pos.pos();

                    let ids = char_pos.in_rangef(physical_size() + distance_pos);

                    for i in 0..end as i32 {
                        let a = i as f32 / distance_pos;
                        let pos = mix(&core_pos, &new_pos, a);
                        let (is_super, solo) = (self.is_super, self.solo);
                        if matches!(
                            pipe.get_other_character_id_and_cores_iter_by_ids_mut(
                                &ids,
                                &mut |_, char_core, _, other_char_pos| {
                                    if !(char_core.is_super || is_super)
                                        && (solo || char_core.solo || char_core.collision_disabled)
                                    {
                                        return ControlFlow::Continue(());
                                    }
                                    let d = distance_squared(&pos, other_char_pos.pos());
                                    if d < physical_size() * physical_size() {
                                        if a > 0.0 {
                                            core_pos = last_pos;
                                        } else if distance_squared(&new_pos, other_char_pos.pos())
                                            > d
                                        {
                                            core_pos = new_pos;
                                        }
                                        char_pos.move_pos(core_pos);
                                        return ControlFlow::Break(());
                                    }
                                    ControlFlow::Continue(())
                                },
                            ),
                            ControlFlow::Break(_)
                        ) {
                            return;
                        }
                        last_pos = pos;
                    }
                }
            }

            char_pos.move_pos(new_pos);
        }

        pub fn physics_quantize(&mut self, pos: &mut CharacterPos, hook: &mut CharacterHook) {
            let vel_x = round_to_int(self.vel.x * 256.0);
            let vel_y = round_to_int(self.vel.y * 256.0);
            self.vel.x = vel_x as f32 / 256.0;
            self.vel.y = vel_y as f32 / 256.0;

            pos.quantinize();
            hook.quantinize();
        }

        pub fn hook_collision(
            pos: vec2,
            cursor_pos: dvec2,
            collision: &Collision,
            field: &CharacterPositionPlayfield,
            characters: &Characters,
            own_character_id: CharacterId,
        ) -> HookCollisionLine {
            let mut color = HookCollisionLineColor::Nothing;

            let dir = normalize(&cursor_pos);
            let dir = vec2::new(
                round_f64_to_int(dir.x * 256.0) as f32 / 256.0,
                round_f64_to_int(dir.y * 256.0) as f32 / 256.0,
            );

            let mut old_pos = pos + dir * PHYSICAL_SIZE * 1.5;
            let mut new_pos = old_pos;
            let mut finish_pos = new_pos;

            let mut do_break = false;

            loop {
                let tune = collision.get_tune_at(&old_pos);
                old_pos = new_pos;
                new_pos = old_pos + dir * tune.hook_fire_speed;

                if distance(&pos, &new_pos) > tune.hook_length {
                    new_pos = pos + normalize(&(new_pos - pos)) * tune.hook_length;
                    do_break = true;
                }

                let hit = collision.intersect_line(
                    &old_pos,
                    &new_pos,
                    &mut finish_pos,
                    &mut vec2::default(),
                    CollisionTypes::SOLID | CollisionTypes::HOOK_TELE | CollisionTypes::HOOK_TROUGH,
                );

                if !do_break && matches!(hit, CollisionTile::Solid(_)) {
                    if matches!(hit, CollisionTile::Solid(DdraceTileNum::NoHook)) {
                        color = HookCollisionLineColor::Unhookable;
                    } else {
                        color = HookCollisionLineColor::Hookable;
                    }
                }

                let col = GameWorld::intersect_character_id_on_line(
                    field,
                    CharactersView::new(
                        characters,
                        |id| *id != own_character_id,
                        |v| !v.phased.is_dead(),
                    ),
                    &old_pos,
                    &finish_pos,
                    HOOK_RADIUS,
                );
                if let Some((_, pos, _)) = col {
                    finish_pos = pos;
                    color = HookCollisionLineColor::Player;
                    break;
                }

                if !matches!(hit, CollisionTile::None) {
                    break;
                }

                new_pos.x = round_to_int(new_pos.x) as f32;
                new_pos.y = round_to_int(new_pos.y) as f32;
                finish_pos = new_pos;

                if old_pos == new_pos {
                    break;
                }

                if do_break {
                    break;
                }
            }

            HookCollisionLine {
                start: pos / 32.0,
                end: finish_pos / 32.0,
                color,
            }
        }
    }
}
