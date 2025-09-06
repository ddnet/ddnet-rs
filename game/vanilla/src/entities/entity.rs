pub mod entity {
    use hiarc::Hiarc;
    use math::math::{round_to_int, vector::vec2};

    use crate::reusable::ReusableCore;
    use serde::{Serialize, de::DeserializeOwned};

    #[derive(Debug, PartialEq, Eq)]
    pub enum EntityTickResult {
        None,
        RemoveEntity,
    }

    #[derive(Debug, Hiarc, PartialEq, Eq)]
    pub enum DropMode {
        None,
        Silent,
        NoEvents,
    }

    use super::super::super::collision::collision::Collision;
    pub trait EntityInterface<
        C: Copy + Clone + Serialize + DeserializeOwned + 'static,
        R: ReusableCore + Serialize + DeserializeOwned + 'static,
        P,
    >
    {
        #[must_use]
        fn pre_tick(&mut self, pipe: &mut P) -> EntityTickResult;
        #[must_use]
        fn tick(&mut self, pipe: &mut P) -> EntityTickResult;
        #[must_use]
        fn tick_deferred(&mut self, pipe: &mut P) -> EntityTickResult;
        /// The entity dropped as a result of a logic side effect (e.g. snapshots),
        /// and not because of game logic.
        fn drop_mode(&mut self, mode: DropMode);
    }

    #[derive(Debug, Hiarc)]
    pub struct Entity<I> {
        pub game_element_id: I,

        pub drop_mode: DropMode,
    }

    impl<I> Entity<I> {
        pub fn new(game_el_id: &I) -> Self
        where
            I: Copy,
        {
            Self {
                game_element_id: *game_el_id,

                drop_mode: DropMode::None,
            }
        }

        pub fn outside_of_playfield(check_pos: &vec2, collision: &Collision) -> bool {
            let rx = round_to_int(check_pos.x) / 32;
            let ry = round_to_int(check_pos.y) / 32;
            (rx < -200 || rx >= collision.get_playfield_width() as i32 + 200)
                || (ry < -200 || ry >= collision.get_playfield_height() as i32 + 200)
        }
    }

    pub fn calc_pos_and_vel(
        pos: &mut vec2,
        vel: &mut vec2,
        curvature: f32,
        speed: f32,
        mut time: f32,
    ) {
        time *= speed;
        pos.x += vel.x * time;

        let curvature = curvature / 10000.0;
        pos.y += vel.y * time + curvature * (time * time);
        vel.y += curvature * 2.0 * time; // derivation of time to above
    }
}
