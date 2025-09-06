#![allow(clippy::too_many_arguments)]
#![allow(clippy::module_inception)]
#![allow(clippy::multiple_bound_locations)]

pub mod collision;
pub mod command_chain;
pub mod config;
pub mod entities;
pub mod events;
pub mod game_objects;
pub mod match_manager;
pub mod match_state;
pub mod reusable;
pub mod simulation_pipe;
pub mod snapshot;
pub mod spawns;
/// basic sql support
pub mod sql;
pub mod stage;
pub mod state;
pub mod types;
pub mod weapons;
pub mod world;

#[cfg(test)]
mod test {
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };

    use base::linked_hash_map_view::FxLinkedHashMap;
    use base_io::{io::create_runtime, runtime::IoRuntime};
    use game_database::dummy::DummyDb;
    use game_interface::{
        interface::{GameStateCreate, GameStateCreateOptions, GameStateInterface},
        types::{
            character_info::NetworkCharacterInfo,
            id_types::PlayerId,
            input::{CharacterInput, CharacterInputInfo, cursor::CharacterInputCursor},
            network_stats::PlayerNetworkStats,
            player_info::{PlayerClientInfo, PlayerUniqueId},
        },
    };
    use math::math::{
        Rng,
        vector::{dvec2, ivec2, vec2},
    };
    use pool::pool::Pool;

    use crate::{config::config::ConfigVanilla, state::state::GameState};

    fn get_game<const NUM_PLAYERS: usize>() -> GameState {
        let file = include_bytes!("../../../data/map/maps/ctf1.twmap.tar");

        let rt = create_runtime();
        let io_rt = IoRuntime::new(rt);
        let (game, _) = GameState::new(
            file.to_vec(),
            "ctf1".try_into().unwrap(),
            GameStateCreateOptions {
                hint_max_characters: Some(NUM_PLAYERS),
                config: Some(
                    serde_json::to_vec(&ConfigVanilla {
                        max_ingame_players: NUM_PLAYERS as u32,
                        ..Default::default()
                    })
                    .unwrap(),
                ),
                ..Default::default()
            },
            io_rt,
            Arc::new(DummyDb),
        )
        .unwrap();
        game
    }

    #[test]
    fn ticks() {
        const NUM_PLAYERS: usize = 64;
        let mut game = get_game::<NUM_PLAYERS>();

        let mut rng = Rng::new(0);

        let mut inps = vec![CharacterInput::default(); NUM_PLAYERS];

        let game_inps: Pool<FxLinkedHashMap<PlayerId, CharacterInputInfo>> = Pool::with_capacity(1);

        let mut next_inp = |inps: &mut FxLinkedHashMap<PlayerId, CharacterInputInfo>,
                            inp: &mut CharacterInput,
                            id: &PlayerId,
                            force_dir: bool| {
            let mut new_inp = *inp;
            new_inp.state.fire.set(rng.random_int_in(0..=1) != 0);
            new_inp.state.hook.set(rng.random_int_in(0..=1) != 0);
            new_inp.state.jump.set(rng.random_int_in(0..=1) != 0);
            new_inp.state.dir.set(rng.random_int_in(0..=2) as i32 - 1);
            if force_dir && *new_inp.state.dir == 0 {
                new_inp.state.dir.set(-1);
            }
            new_inp
                .cursor
                .set(CharacterInputCursor::from_vec2(&dvec2::new(
                    rng.random_float() as f64,
                    rng.random_float() as f64,
                )));

            let diff = new_inp.consumable.diff(&inp.consumable);
            *inp = new_inp;
            inps.insert(*id, CharacterInputInfo { inp: new_inp, diff });
        };

        let ids: Vec<_> = (0..NUM_PLAYERS)
            .map(|index| {
                let id = game.player_join(&PlayerClientInfo {
                    info: NetworkCharacterInfo::explicit_default(),
                    id: 0,
                    unique_identifier: PlayerUniqueId::Account(0),
                    initial_network_stats: PlayerNetworkStats::default(),
                });

                for _ in 0..2 {
                    let mut game_inps = game_inps.new();
                    next_inp(&mut game_inps, &mut inps[index], &id, true);
                    game.set_player_inputs(game_inps);
                    game.tick(Default::default());
                }

                id
            })
            .collect();

        println!("bench start...");
        let mut bench_inner = || {
            let mut ticks: u64 = 0;
            let now = Instant::now();
            loop {
                let mut game_inps = game_inps.new();
                for (inp, id) in inps.iter_mut().zip(ids.iter()) {
                    next_inp(&mut game_inps, inp, id, false);
                }
                game.set_player_inputs(game_inps);

                game.tick(Default::default());
                game.clear_events();

                ticks += 1;
                if Instant::now().duration_since(now) >= Duration::from_secs(5) {
                    break;
                }
            }

            println!("{} t/s", ticks / 5);
        };
        bench_inner();
        bench_inner();
    }

    #[test]
    fn move_box() {
        let game = get_game::<1>();
        pub const PHYSICAL_SIZE: f32 = 28.0;
        const fn physical_size_vec2() -> ivec2 {
            ivec2 {
                x: PHYSICAL_SIZE as i32,
                y: PHYSICAL_SIZE as i32,
            }
        }
        let bench = || {
            let now = Instant::now();
            let iterations = 1000000;
            let mut pos = Default::default();
            let mut vel = Default::default();
            for _ in 0..iterations {
                pos = vec2::new(10.0, 10.0) * 32.0;
                vel = vec2::new(10.0, 10.0) * 32.0;
                game.collision
                    .move_box(&mut pos, &mut vel, &physical_size_vec2(), 0.0);
            }
            let took = Instant::now().duration_since(now);
            println!("{} s - pos: {:?}, vel: {:?}", took.as_secs_f64(), pos, vel);
            let calls_per_second = iterations as f64 / took.as_secs_f64();
            println!("{calls_per_second} call/s");
        };
        bench();
        bench();
        bench();
        bench();
    }
}
