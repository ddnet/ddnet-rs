use game_base::player_input::PlayerInput;
use game_interface::types::{input::cursor::CharacterInputCursor, weapons::WeaponType};
use libtw2_gamenet_ddnet::snap_obj;
use math::math::vector::dvec2;

/// Time until the intended tick, measured when the backend received the input.
/// Snapshot overhead locates its send time relative to the snapshot tick;
/// acknowledgement overhead moves that clock back to input arrival.
pub fn time_left(
    input_tick: u64,
    snapshot_tick: u64,
    snapshot_overhead: std::time::Duration,
    acknowledgement_overhead: std::time::Duration,
) -> i32 {
    let nanos = (input_tick as i128 - snapshot_tick as i128) * 20_000_000
        - snapshot_overhead.as_nanos() as i128
        + acknowledgement_overhead.as_nanos() as i128;
    (nanos / 1_000_000).clamp(i32::MIN as i128, i32::MAX as i128) as i32
}

/// Legacy counters wrap at 64; the new protocol uses cumulative click counts.
#[derive(Default)]
pub struct Inputs {
    previous: snap_obj::PlayerInput,
    pub race: bool,
    pub current: PlayerInput,
    last_tick: Option<i32>,
}

fn presses(old: i32, new: i32) -> u64 {
    let distance = (new.wrapping_sub(old) & 63) as u64;
    (distance + u64::from(old & 1 == 0)) / 2
}

impl Inputs {
    pub fn translate(&mut self, tick: i32, input: snap_obj::PlayerInput) -> Option<PlayerInput> {
        if self.last_tick.is_some_and(|last| {
            tick < last || (tick == last && input.encode() == self.previous.encode())
        }) {
            return None;
        }
        self.last_tick = Some(tick);
        let c = &mut self.current.inp;
        let cursor = CharacterInputCursor::from_vec2(&dvec2::new(
            input.target_x as f64 / 32.0,
            input.target_y as f64 / 32.0,
        ));
        c.cursor.set(cursor);
        use game_interface::types::input::{CharacterInputFlags as F, CharacterInputMethodFlags};
        let mut flags = F::empty();
        flags.set(F::MENU_UI, input.player_flags & 2 != 0);
        flags.set(F::CHATTING, input.player_flags & 4 != 0);
        flags.set(F::SCOREBOARD, input.player_flags & 8 != 0);
        flags.set(F::HOOK_COLLISION_LINE, input.player_flags & 16 != 0);
        c.state.flags.set(flags);
        c.state
            .input_method_flags
            .set(CharacterInputMethodFlags::MOUSE_KEYBOARD);
        c.state.dir.set(input.direction.clamp(-1, 1));
        c.state.jump.set(input.jump != 0);
        c.state.hook.set(input.hook != 0);
        c.state.fire.set(input.fire & 1 != 0);
        c.consumable
            .jump
            .add(u64::from(input.jump != 0 && self.previous.jump == 0));
        c.consumable.hook.add(
            u64::from(input.hook != 0 && self.previous.hook == 0),
            cursor,
        );
        c.consumable
            .fire
            .add(presses(self.previous.fire, input.fire), cursor);
        c.consumable.weapon_diff.add(
            presses(self.previous.next_weapon, input.next_weapon) as i64
                - presses(self.previous.prev_weapon, input.prev_weapon) as i64,
        );
        // DDNet handles direct input before updating its predicted input, so
        // weapon selection reads the previous input's wanted weapon.
        c.consumable.set_weapon_req(if input.wanted_weapon != 0 {
            match self.previous.wanted_weapon {
                1 => Some(WeaponType::Hammer),
                2 => Some(WeaponType::Gun),
                3 => Some(if self.race {
                    WeaponType::Puller
                } else {
                    WeaponType::Shotgun
                }),
                4 => Some(WeaponType::Grenade),
                5 => Some(WeaponType::Laser),
                _ => None,
            }
        } else {
            None
        });
        self.previous = input;
        self.current.inc_version();
        Some(self.current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timing_uses_backend_arrival_instead_of_last_received_snapshot() {
        use std::time::Duration;
        // Snapshot tick 100 was sent 5ms late. Input arrived 15ms before that.
        assert_eq!(
            time_left(
                101,
                100,
                Duration::from_millis(5),
                Duration::from_millis(15)
            ),
            30
        );
        assert_eq!(
            time_left(99, 100, Duration::from_millis(5), Duration::from_millis(2)),
            -23
        );
        // A later acknowledgement of the same arrival gives the same result.
        assert_eq!(
            time_left(
                101,
                102,
                Duration::from_millis(5),
                Duration::from_millis(55)
            ),
            30
        );
    }

    #[test]
    fn repeated_target_weapon_fire_keeps_requesting_shotgun() {
        for (race, weapon) in [(false, WeaponType::Shotgun), (true, WeaponType::Puller)] {
            let mut inputs = Inputs {
                race,
                ..Default::default()
            };
            let first = inputs
                .translate(
                    10,
                    snap_obj::PlayerInput {
                        wanted_weapon: 3,
                        fire: 1,
                        ..Default::default()
                    },
                )
                .unwrap();
            let next = inputs
                .translate(
                    11,
                    snap_obj::PlayerInput {
                        wanted_weapon: 3,
                        fire: 3,
                        ..Default::default()
                    },
                )
                .unwrap();
            let diff = next.inp.consumable.diff(&first.inp.consumable);
            assert_eq!(diff.weapon_req, Some(weapon));
            assert!(diff.fire.is_some());
        }
    }
    #[test]
    fn weapon_switch_matches_ddnet_direct_input_order() {
        for wanted_weapon in [3, 4] {
            let mut inputs = Inputs::default();
            let mut previous = inputs
                .translate(
                    1,
                    snap_obj::PlayerInput {
                        wanted_weapon: 1,
                        ..Default::default()
                    },
                )
                .unwrap();
            for (tick, wanted, fire, expected) in [
                (2, 1, 0, Some(WeaponType::Hammer)),
                (3, wanted_weapon, 1, Some(WeaponType::Hammer)),
                (
                    4,
                    wanted_weapon,
                    1,
                    Some(if wanted_weapon == 3 {
                        WeaponType::Shotgun
                    } else {
                        WeaponType::Grenade
                    }),
                ),
                (5, 0, 2, None),
            ] {
                let current = inputs
                    .translate(
                        tick,
                        snap_obj::PlayerInput {
                            wanted_weapon: wanted,
                            fire,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                assert_eq!(
                    current
                        .inp
                        .consumable
                        .diff(&previous.inp.consumable)
                        .weapon_req,
                    expected
                );
                previous = current;
            }
        }
    }

    #[test]
    fn counts_wrapped_edges_and_ignores_old_packets() {
        assert_eq!(presses(62, 1), 2);
        assert_eq!(presses(1, 2), 0);
        let mut inputs = Inputs::default();
        let raw = snap_obj::PlayerInput {
            fire: 1,
            ..Default::default()
        };
        let first = inputs.translate(10, raw).unwrap();
        assert_eq!(
            first
                .inp
                .consumable
                .diff(&Default::default())
                .fire
                .unwrap()
                .0
                .get(),
            1
        );
        assert!(inputs.translate(9, raw).is_none());
        assert!(inputs.translate(10, raw).is_none());
        let second = inputs.translate(11, raw).unwrap();
        assert!(
            second
                .inp
                .consumable
                .diff(&first.inp.consumable)
                .fire
                .is_none()
        );
    }
}
