pub mod collision {
    use anyhow::anyhow;
    use bitflags::bitflags;
    use config::{traits::ConfigInterface, ConfigInterface};
    use game_base::mapdef_06::DdraceTileNum;
    use hiarc::Hiarc;
    use map::map::groups::{
        layers::{
            physics::MapLayerPhysics,
            tiles::{
                rotation_180, rotation_270, SpeedupTile, SwitchTile, TeleTile, TileBase, TuneTile,
                ROTATION_0, ROTATION_90,
            },
        },
        MapGroupPhysics,
    };
    use serde::{Deserialize, Serialize};

    use math::math::{
        distance, dot, mix, round_to_int,
        vector::{ivec2, vec2},
    };

    use crate::state::state::TICKS_PER_SECOND;

    #[derive(Debug, Hiarc, Copy, Clone, Serialize, Deserialize, ConfigInterface)]
    pub struct Tunings {
        pub ground_control_speed: f32,
        pub ground_control_accel: f32,
        pub ground_friction: f32,
        pub ground_jump_impulse: f32,
        pub air_jump_impulse: f32,
        pub air_control_speed: f32,
        pub air_control_accel: f32,
        pub air_friction: f32,
        pub hook_length: f32,
        pub hook_fire_speed: f32,
        pub hook_drag_accel: f32,
        pub hook_drag_speed: f32,
        pub gravity: f32,
        pub velramp_start: f32,
        pub velramp_range: f32,
        pub velramp_curvature: f32,
        pub gun_curvature: f32,
        pub gun_speed: f32,
        pub gun_lifetime: f32,
        pub shotgun_curvature: f32,
        pub shotgun_speed: f32,
        pub shotgun_speeddiff: f32,
        pub shotgun_lifetime: f32,
        pub grenade_curvature: f32,
        pub grenade_speed: f32,
        pub grenade_lifetime: f32,
        pub laser_reach: f32,
        pub laser_bounce_delay: f32,
        pub laser_bounce_num: f32,
        pub laser_bounce_cost: f32,
        pub laser_damage: f32,
        pub player_collision: f32,
        pub player_hooking: f32,
        pub jetpack_strength: f32,
        pub shotgun_strength: f32,
        pub explosion_strength: f32,
        pub hammer_strength: f32,
        pub hook_duration: f32,
        pub hammer_fire_delay: f32,
        pub gun_fire_delay: f32,
        pub shotgun_fire_delay: f32,
        pub grenade_fire_delay: f32,
        pub laser_fire_delay: f32,
        pub ninja_fire_delay: f32,
        pub hammer_hit_fire_delay: f32,
    }

    impl Default for Tunings {
        fn default() -> Self {
            Self {
                ground_control_speed: 10.0,
                ground_control_accel: 100.0 / TICKS_PER_SECOND as f32,
                ground_friction: 0.5,
                ground_jump_impulse: 13.2,
                air_jump_impulse: 12.0,
                air_control_speed: 250.0 / TICKS_PER_SECOND as f32,
                air_control_accel: 1.5,
                air_friction: 0.95,
                hook_length: 380.0,
                hook_fire_speed: 80.0,
                hook_drag_accel: 3.0,
                hook_drag_speed: 15.0,
                gravity: 0.5,
                velramp_start: 550.0,
                velramp_range: 2000.0,
                velramp_curvature: 1.4,
                gun_curvature: 1.25,
                gun_speed: 2200.0,
                gun_lifetime: 2.0,
                shotgun_curvature: 1.25,
                shotgun_speed: 2750.0,
                shotgun_speeddiff: 0.8,
                shotgun_lifetime: 0.20,
                grenade_curvature: 7.0,
                grenade_speed: 1000.0,
                grenade_lifetime: 2.0,
                laser_reach: 800.0,
                laser_bounce_delay: 150.0,
                laser_bounce_num: 1.0,
                laser_bounce_cost: 0.0,
                laser_damage: 5.0,
                player_collision: 1.0,
                player_hooking: 1.0,
                jetpack_strength: 400.0,
                shotgun_strength: 10.0,
                explosion_strength: 6.0,
                hammer_strength: 1.0,
                hook_duration: 1.25,
                hammer_fire_delay: 333.0,
                gun_fire_delay: 125.0,
                shotgun_fire_delay: 500.0,
                grenade_fire_delay: 500.0,
                laser_fire_delay: 800.0,
                ninja_fire_delay: 800.0,
                hammer_hit_fire_delay: 320.0,
            }
        }
    }

    #[derive(
        Debug, Hiarc, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
    )]
    pub struct CollisionTypes(u32);
    bitflags! {
        impl CollisionTypes: u32 {
            /// Hitting solid ground
            const SOLID = (1 << 0);
            /// Hitting player tele
            const PLAYER_TELE = (1 << 1);
            /// Hitting hook tele
            const HOOK_TELE = (1 << 2);
            /// Hitting weapon tele
            const WEAPON_TELE = (1 << 3);
            /// Enable hook trough tiles for solid collision check
            const HOOK_TROUGH = (1 << 4);
        }
    }

    #[derive(Debug, Hiarc, PartialEq, Eq, PartialOrd, Ord)]
    pub enum CollisionTile {
        None,
        Solid(DdraceTileNum),
        PlayerTele(u8),
        HookTele(u8),
        WeaponTele(u8),
    }

    #[derive(Debug, Hiarc)]
    pub enum HitTile<'a> {
        Game(&'a TileBase),
        Front(&'a TileBase),
        Tele(&'a TeleTile),
        Speedup(&'a SpeedupTile),
        Switch(&'a SwitchTile),
        Tune(&'a TuneTile),
    }

    #[derive(Debug)]
    pub struct Collision {
        tiles: Vec<TileBase>,
        front_tiles: Vec<TileBase>,
        tune_tiles: Vec<TuneTile>,
        tele_tiles: Vec<TeleTile>,
        speedup_tiles: Vec<SpeedupTile>,
        switch_tiles: Vec<SwitchTile>,
        width: u32,
        height: u32,

        pub(crate) tune_zones: [Tunings; u8::MAX as usize + 1],
    }

    // TODO: use u8 or an enum for tile indices, instead of i32
    impl Collision {
        pub fn new(
            physics_group: &MapGroupPhysics,
            load_all_layers: bool,
        ) -> anyhow::Result<Box<Self>> {
            let width = physics_group.attr.width.get() as u32;
            let height = physics_group.attr.height.get() as u32;

            let mut game_layer = None;
            let mut front_layer = None;
            let mut tune_layer = None;
            let mut tele_layer = None;
            physics_group.layers.iter().for_each(|layer| match layer {
                MapLayerPhysics::Arbitrary(_) => {}
                MapLayerPhysics::Game(layer) => {
                    game_layer = Some(layer);
                }
                MapLayerPhysics::Front(layer) => {
                    front_layer = load_all_layers.then_some(layer);
                }
                MapLayerPhysics::Tele(layer) => {
                    tele_layer = load_all_layers.then_some(layer);
                }
                MapLayerPhysics::Speedup(_) => {}
                MapLayerPhysics::Switch(_) => {}
                MapLayerPhysics::Tune(layer) => {
                    tune_layer = load_all_layers.then_some(layer);
                }
            });

            let game_layer = game_layer.ok_or_else(|| anyhow!("no game layer found"))?;

            let tune_zones_and_tiles = tune_layer.as_ref().map(|tune_layer| {
                let tune_tiles = &tune_layer.base.tiles;
                (
                    {
                        let mut tune_zones = vec![Tunings::default(); 256];

                        for (zone_index, tunes) in tune_layer.tune_zones.iter() {
                            let zone = &mut tune_zones[*zone_index as usize];
                            for (tune, val) in &tunes.tunes {
                                if let Err(err) = zone.try_set_from_str(
                                    tune.clone(),
                                    None,
                                    Some(val.value.clone()),
                                    None,
                                    Default::default(),
                                ) {
                                    log::info!(
                                        "failed to apply tune: {err} \
                                            for {tune} - val {} \
                                            with index {zone_index}",
                                        val.value
                                    );
                                }
                            }
                        }

                        tune_zones
                    },
                    tune_tiles.as_slice(),
                )
            });

            let mut tune_zones = vec![Tunings::default(); 256];
            let tune_tiles: Vec<_> =
                if let Some((tune_zone_list, tune_tiles)) = tune_zones_and_tiles {
                    tune_zones = tune_zone_list;
                    let mut tune_tiles = tune_tiles.to_vec();
                    tune_tiles.shrink_to_fit();
                    tune_tiles
                } else {
                    let mut tune_tiles = vec![TuneTile::default(); game_layer.tiles.len()];
                    tune_tiles.shrink_to_fit();
                    tune_tiles
                };

            Ok(Box::new(Self {
                width,
                height,
                tiles: {
                    let mut tiles = game_layer.tiles.to_vec();
                    tiles.shrink_to_fit();
                    tiles
                },
                tune_tiles,
                tune_zones: tune_zones.try_into().unwrap(),
                front_tiles: front_layer
                    .map(|l| l.tiles.to_vec())
                    .unwrap_or_else(|| vec![Default::default(); game_layer.tiles.len()]),
                tele_tiles: tele_layer
                    .map(|l| l.base.tiles.to_vec())
                    .unwrap_or_else(|| vec![Default::default(); game_layer.tiles.len()]),
                speedup_tiles: vec![Default::default(); game_layer.tiles.len()],
                switch_tiles: vec![Default::default(); game_layer.tiles.len()],
            }))
        }

        pub fn get_playfield_width(&self) -> u32 {
            self.width
        }

        pub fn get_playfield_height(&self) -> u32 {
            self.height
        }

        #[inline(always)]
        pub fn get_tile(&self, x: i32, y: i32) -> u8 {
            let pos = self.tile_index(x, y);

            self.tiles[pos].index
        }

        #[inline(always)]
        pub fn is_solid(&self, x: i32, y: i32) -> bool {
            let index = self.get_tile(x, y);
            index == DdraceTileNum::Solid as u8 || index == DdraceTileNum::NoHook as u8
        }

        pub fn is_death(&self, x: f32, y: f32) -> bool {
            let index = self.get_tile(round_to_int(x), round_to_int(y));
            index == DdraceTileNum::Death as u8
        }

        #[inline(always)]
        pub fn check_point(&self, x: i32, y: i32) -> bool {
            self.is_solid(x, y)
        }

        pub fn check_pointf(&self, x: f32, y: f32) -> bool {
            self.is_solid(round_to_int(x), round_to_int(y))
        }

        #[inline(always)]
        pub fn test_box(&self, pos: &ivec2, size_param: &ivec2) -> bool {
            let size = *size_param / 2;
            self.check_point(pos.x - size.x, pos.y + size.y)
                || self.check_point(pos.x + size.x, pos.y + size.y)
                || self.check_point(pos.x - size.x, pos.y - size.y)
                || self.check_point(pos.x + size.x, pos.y - size.y)
        }

        pub fn move_point(
            &self,
            inout_pos: &mut vec2,
            inout_vel: &mut vec2,
            elasticity: f32,
            bounces: &mut i32,
        ) {
            *bounces = 0;

            let pos = *inout_pos;
            let vel = *inout_vel;
            let pos_vel = pos + vel;
            if self.check_pointf(pos_vel.x, pos_vel.y) {
                let mut affected = 0;
                if self.check_pointf(pos.x + vel.x, pos.y) {
                    inout_vel.x *= -elasticity;
                    *bounces += 1;
                    affected += 2;
                }

                if self.check_pointf(pos.x, pos.y + vel.y) {
                    inout_vel.y *= -elasticity;
                    *bounces += 1;
                    affected += 1;
                }

                if affected == 0 {
                    inout_vel.x *= -elasticity;
                    inout_vel.y *= -elasticity;
                }
            } else {
                *inout_pos = pos + vel;
            }
        }

        pub fn move_box(
            &self,
            in_out_pos: &mut vec2,
            in_out_vel: &mut vec2,
            size: &ivec2,
            elasticity: f32,
        ) {
            // do the move
            let mut pos = *in_out_pos;
            let mut vel = *in_out_vel;

            let vel_distance = dot(&vel, &vel);

            enum CollisionCoords {
                X,
                Y,
                XY,
                /// No collision was detected
                None,
                /// No collision happened before
                /// or the state is unknown
                Unknown,
            }
            let mut last_collision_coords = CollisionCoords::Unknown;

            if vel_distance > 0.00001 {
                let mut last_pos_x = round_to_int(pos.x);
                let mut last_pos_y = round_to_int(pos.y);

                let mut prev_last_pos_x = last_pos_x;
                let mut prev_last_pos_y = last_pos_y;

                let max = vel_distance.sqrt() as i32;

                let fraction = 1.0 / (max + 1) as f32;
                for _i in 0..=max {
                    // Early break as optimization to stop checking for collisions for
                    // large distances after the obstacles we have already hit reduced
                    // our speed to exactly 0.
                    if vel == vec2::new(0.0, 0.0) {
                        break;
                    }

                    let mut new_pos = pos + vel * fraction; // TODO: this row is not nice

                    // Fraction can be very small and thus the calculation has no effect, no
                    // reason to continue calculating.
                    if new_pos == pos {
                        break;
                    }

                    let mut new_pos_x = round_to_int(new_pos.x);
                    let mut new_pos_y = round_to_int(new_pos.y);

                    if !((new_pos_x == last_pos_x && new_pos_y == last_pos_y)
                        && (last_pos_x == prev_last_pos_x && last_pos_y == prev_last_pos_y))
                    {
                        last_collision_coords = CollisionCoords::Unknown;
                    }

                    if matches!(last_collision_coords, CollisionCoords::Unknown) {
                        if self.test_box(&ivec2::new(new_pos_x, new_pos_y), size) {
                            let mut hits = 0;

                            if self.test_box(&ivec2::new(last_pos_x, new_pos_y), size) {
                                last_collision_coords = CollisionCoords::X;
                                hits += 1;
                            }

                            if self.test_box(&ivec2::new(new_pos_x, last_pos_y), size) {
                                if matches!(last_collision_coords, CollisionCoords::X) {
                                    last_collision_coords = CollisionCoords::XY;
                                } else {
                                    last_collision_coords = CollisionCoords::Y;
                                }
                                hits += 1;
                            }

                            // neither of the tests got a collision.
                            // this is a real _corner case_!
                            if hits == 0 {
                                last_collision_coords = CollisionCoords::XY;
                            }
                        } else {
                            last_collision_coords = CollisionCoords::None;
                        }
                    }

                    match last_collision_coords {
                        CollisionCoords::X => {
                            new_pos.y = pos.y;
                            new_pos_y = last_pos_y;
                            vel.y *= -elasticity;
                        }
                        CollisionCoords::Y => {
                            new_pos.x = pos.x;
                            new_pos_x = last_pos_x;
                            vel.x *= -elasticity;
                        }
                        CollisionCoords::XY => {
                            new_pos.y = pos.y;
                            new_pos_y = last_pos_y;
                            vel.y *= -elasticity;
                            new_pos.x = pos.x;
                            new_pos_x = last_pos_x;
                            vel.x *= -elasticity;
                        }
                        CollisionCoords::None | CollisionCoords::Unknown => {
                            // nothing to do
                        }
                    }

                    prev_last_pos_x = last_pos_x;
                    prev_last_pos_y = last_pos_y;
                    last_pos_x = new_pos_x;
                    last_pos_y = new_pos_y;
                    pos = new_pos;
                }
            }

            *in_out_pos = pos;
            *in_out_vel = vel;
        }

        fn is_teleport(&self, index: usize) -> Option<u8> {
            let tile = &self.tele_tiles[index];
            (tile.base.index == DdraceTileNum::TeleIn as u8).then_some(tile.number)
        }

        fn is_teleport_hook(&self, index: usize) -> Option<u8> {
            let tile = &self.tele_tiles[index];
            (tile.base.index == DdraceTileNum::TeleInHook as u8).then_some(tile.number)
        }

        fn is_teleport_weapon(&self, index: usize) -> Option<u8> {
            let tile = &self.tele_tiles[index];
            (tile.base.index == DdraceTileNum::TeleInWeapon as u8).then_some(tile.number)
        }

        fn is_hook_blocker(&self, x: i32, y: i32, pos0: &vec2, pos1: &vec2) -> bool {
            let index = self.tile_index(x, y);
            let tile = &self.tiles[index];
            let front_tile = &self.front_tiles[index];
            if tile.index == DdraceTileNum::ThroughAll as u8
                || front_tile.index == DdraceTileNum::ThroughAll as u8
            {
                return true;
            }
            let through_dir = |tile: &TileBase| {
                tile.index == DdraceTileNum::ThroughDir as u8
                    && ((tile.flags == ROTATION_0 && pos0.y < pos1.y)
                        || (tile.flags == ROTATION_90 && pos0.x > pos1.x)
                        || (tile.flags == rotation_180() && pos0.y > pos1.y)
                        || (tile.flags == rotation_270() && pos0.x < pos1.x))
            };
            through_dir(tile) || through_dir(front_tile)
        }

        fn is_hook_through(
            &self,
            x: i32,
            y: i32,
            xoff: i32,
            yoff: i32,
            pos0: &vec2,
            pos1: &vec2,
        ) -> bool {
            let index = self.tile_index(x, y);
            let front_tile = &self.front_tiles[index];
            if front_tile.index == DdraceTileNum::ThroughAll as u8
                || front_tile.index == DdraceTileNum::ThroughCut as u8
            {
                return true;
            }
            if front_tile.index == DdraceTileNum::ThroughDir as u8
                && ((front_tile.flags == ROTATION_0 && pos0.y < pos1.y)
                    || (front_tile.flags == ROTATION_90 && pos0.x > pos1.x)
                    || (front_tile.flags == rotation_180() && pos0.y > pos1.y)
                    || (front_tile.flags == rotation_270() && pos0.x < pos1.x))
            {
                return true;
            }
            let off_index = self.tile_index(x + xoff, y + yoff);
            let tile = &self.tiles[off_index];
            let front_tile = &self.front_tiles[off_index];
            tile.index == DdraceTileNum::Through as u8
                || front_tile.index == DdraceTileNum::Through as u8
        }

        fn get_collision_at(&self, x: f32, y: f32) -> DdraceTileNum {
            let index = self.get_tile(round_to_int(x), round_to_int(y));

            if index == DdraceTileNum::Solid as u8 {
                DdraceTileNum::Solid
            } else if index == DdraceTileNum::Death as u8 {
                DdraceTileNum::Death
            } else if index == DdraceTileNum::NoHook as u8 {
                DdraceTileNum::NoHook
            } else if index == DdraceTileNum::NoLaser as u8 {
                DdraceTileNum::NoLaser
            } else {
                DdraceTileNum::Air
            }
        }

        #[inline(always)]
        fn tile_index(&self, x: i32, y: i32) -> usize {
            let nx = (x / 32).clamp(0, self.width as i32 - 1);
            let ny = (y / 32).clamp(0, self.height as i32 - 1);
            ny as usize * self.width as usize + nx as usize
        }

        fn tile_indexf(&self, x: f32, y: f32) -> usize {
            self.tile_index(round_to_int(x), round_to_int(y))
        }

        fn through_offset(&self, pos0: &vec2, pos1: &vec2) -> (i32, i32) {
            let x = pos0.x - pos1.x;
            let y = pos0.y - pos1.y;
            if x.abs() > y.abs() {
                if x < 0.0 {
                    (-32, 0)
                } else {
                    (32, 0)
                }
            } else if y < 0.0 {
                (0, -32)
            } else {
                (0, 32)
            }
        }

        pub fn intersect_line(
            &self,
            pos_0: &vec2,
            pos_1: &vec2,
            out_collision: &mut vec2,
            out_before_collision: &mut vec2,
            collisions: CollisionTypes,
        ) -> CollisionTile {
            let d = distance(pos_0, pos_1);
            let end = (d + 1.0) as i32;
            let mut last_pos = *pos_0;
            let (offset_x, offset_y) = self.through_offset(pos_0, pos_1);
            for i in 0..=end {
                let a = i as f32 / end as f32;
                let pos = mix(pos_0, pos_1, a);
                // Temporary position for checking collision
                let ix = round_to_int(pos.x);
                let iy = round_to_int(pos.y);

                let index = self.tile_indexf(pos.x, pos.y);
                if let Some(number) = collisions
                    .contains(CollisionTypes::PLAYER_TELE)
                    .then(|| self.is_teleport(index))
                    .flatten()
                {
                    *out_collision = pos;
                    *out_before_collision = last_pos;
                    return CollisionTile::PlayerTele(number);
                }

                if let Some(number) = collisions
                    .contains(CollisionTypes::WEAPON_TELE)
                    .then(|| self.is_teleport_weapon(index))
                    .flatten()
                {
                    *out_collision = pos;
                    *out_before_collision = last_pos;
                    return CollisionTile::WeaponTele(number);
                }

                if let Some(number) = collisions
                    .contains(CollisionTypes::HOOK_TELE)
                    .then(|| self.is_teleport_hook(index))
                    .flatten()
                {
                    *out_collision = pos;
                    *out_before_collision = last_pos;
                    return CollisionTile::HookTele(number);
                }

                if collisions.contains(CollisionTypes::SOLID) {
                    if self.check_point(ix, iy) {
                        if !collisions.contains(CollisionTypes::HOOK_TROUGH)
                            || !self.is_hook_through(ix, iy, offset_x, offset_y, pos_0, pos_1)
                        {
                            *out_collision = pos;
                            *out_before_collision = last_pos;
                            return CollisionTile::Solid(
                                self.get_collision_at(ix as f32, iy as f32),
                            );
                        }
                    } else if collisions.contains(CollisionTypes::HOOK_TROUGH)
                        && self.is_hook_blocker(ix, iy, pos_0, pos_1)
                    {
                        *out_collision = pos;
                        *out_before_collision = last_pos;
                        return CollisionTile::Solid(DdraceTileNum::NoHook);
                    }
                }

                last_pos = pos;
            }
            *out_collision = *pos_1;
            *out_before_collision = *pos_1;
            CollisionTile::None
        }

        pub fn intersect_line_feedback(
            &self,
            pos0: &vec2,
            pos1: &vec2,
            mut on_tile: impl FnMut(HitTile<'_>),
        ) {
            let d = distance(pos0, pos1).max(1.0);
            let end = (d + 1.0) as i32;

            let mut last_tile_index = None;
            for i in 0..end {
                let a = i as f32 / d;
                let tmp = mix(pos0, pos1, a);
                let tile_index = self.tile_indexf(tmp.x, tmp.y);

                if last_tile_index.is_none_or(|last_tile_index| last_tile_index != tile_index) {
                    let tile = &self.tiles[tile_index];
                    if tile.index > 0 {
                        on_tile(HitTile::Game(tile));
                    }
                    let front_tile = &self.front_tiles[tile_index];
                    if front_tile.index > 0 {
                        on_tile(HitTile::Front(front_tile));
                    }
                    let tele_tile = &self.tele_tiles[tile_index];
                    if tele_tile.base.index > 0 {
                        on_tile(HitTile::Tele(tele_tile));
                    }
                    let speedup_tile = &self.speedup_tiles[tile_index];
                    if speedup_tile.base.index > 0 {
                        on_tile(HitTile::Speedup(speedup_tile));
                    }
                    let switch_tile = &self.switch_tiles[tile_index];
                    if switch_tile.base.index > 0 {
                        on_tile(HitTile::Switch(switch_tile));
                    }
                    let tune_tile = &self.tune_tiles[tile_index];
                    if tune_tile.base.index > 0 {
                        on_tile(HitTile::Tune(tune_tile));
                    }
                    last_tile_index = Some(tile_index);
                }
            }
        }

        pub fn get_tune_at(&self, pos: &vec2) -> &Tunings {
            let tune_tile = &self.tune_tiles[self.tile_indexf(pos.x, pos.y)];
            &self.tune_zones[tune_tile.number as usize]
        }
    }
}
