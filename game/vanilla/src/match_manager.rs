/// everything related to a single match/round/race-run
pub mod match_manager {
    use std::time::Duration;

    use game_interface::types::{game::GameTickType, render::game::game_match::MatchSide};
    use hiarc::{hi_closure, Hiarc};

    use crate::{
        config::config::ConfigGameType,
        events::events::{CharacterEvent, CharacterEventMod, FlagEvent},
        match_state::match_state::{Match, MatchState, MatchType},
        simulation_pipe::simulation_pipe::{
            SimulationEventWorldEntityType, SimulationStageEvents, SimulationWorldEvent,
        },
        state::state::TICKS_PER_SECOND,
        types::types::{GameOptions, GameType},
        world::world::GameWorld,
    };

    #[derive(Debug, Hiarc)]
    pub struct MatchManager {
        pub(crate) game_options: GameOptions,
        simulation_events: SimulationStageEvents,

        pub(crate) game_match: Match,
    }

    impl MatchManager {
        pub fn new(game_options: GameOptions, simulation_events: &SimulationStageEvents) -> Self {
            Self {
                game_match: Match {
                    ty: match game_options.ty() {
                        GameType::Solo => MatchType::Solo,
                        GameType::Sided => MatchType::Sided {
                            scores: Default::default(),
                        },
                    },
                    state: MatchState::Running {
                        round_ticks_passed: Default::default(),
                        round_ticks_left: game_options
                            .time_limit()
                            .map(|time| {
                                ((time.as_micros() * TICKS_PER_SECOND as u128)
                                    / Duration::from_secs(1).as_micros())
                                    as GameTickType
                            })
                            .unwrap_or_default()
                            .into(),
                    },
                    balance_tick: Default::default(),
                },
                game_options,
                simulation_events: simulation_events.clone(),
            }
        }

        fn mod_event(
            _world: &mut GameWorld,
            _game_match: &mut Match,
            _game_options: &GameOptions,
            _ev: &CharacterEventMod,
        ) {
        }

        /// How much points does one side get if a player
        /// kills a player from the other side.
        fn side_score_player_kill(game_options: &GameOptions) -> i64 {
            match game_options.game_ty() {
                ConfigGameType::Dm | ConfigGameType::Ctf => 0,
            }
        }

        fn handle_events(&mut self, world: &mut GameWorld) {
            let game_match = &mut self.game_match;
            let game_options = &self.game_options;
            self.simulation_events
                .for_each(hi_closure!([game_match: &mut Match, game_options: &GameOptions, world: &mut GameWorld], |ev: &SimulationWorldEvent| -> () {
                    match ev {
                        SimulationWorldEvent::Entity(entity_ev) => match &entity_ev.ev {
                            SimulationEventWorldEntityType::Character { ev, .. } => {
                                match ev {
                                    CharacterEvent::Despawn { killer_id, id: victim_id, .. } => {
                                        if let Some(char) = killer_id.and_then(|killer_id| world.characters.get_mut(&killer_id)) {
                                            if Some(*victim_id) == *killer_id {
                                                char.score.set(char.score.get() - 1);
                                            }
                                            else {
                                                char.score.set(char.score.get() + 1);
                                                if let (MatchType::Sided { scores }, Some(score)) = (&mut game_match.ty, char.core.side) {
                                                    scores[score as usize] += MatchManager::side_score_player_kill(game_options);
                                                }
                                            }
                                            game_match.win_check(game_options, &world.scores, false);
                                        }
                                    }
                                    CharacterEvent::Mod(mod_ev) => {
                                        MatchManager::mod_event(world, game_match,game_options, mod_ev);
                                    }
                                }
                            },
                            SimulationEventWorldEntityType::Flag { ev, .. } => {
                                match ev {
                                    FlagEvent::Capture { by, .. } => {
                                        if let Some(char) = world.characters.get_mut(by) {
                                            char.score.set(char.score.get() + 5);
                                            if let (MatchType::Sided { scores }, Some(score)) = (&mut game_match.ty, char.core.side) {
                                                scores[score as usize] += 100;
                                            }
                                            game_match.win_check(game_options, &world.scores, false);
                                        }
                                    },
                                    FlagEvent::Collect { by } => {
                                        if let Some(char) = world.characters.get_mut(by) {
                                            char.score.set(char.score.get() + 1);
                                            if let (MatchType::Sided { scores }, Some(side)) = (&mut game_match.ty, char.core.side) {
                                                scores[side as usize] += 1;
                                            }
                                            game_match.win_check(game_options, &world.scores, false);
                                        }
                                    },
                                    FlagEvent:: Despawn {
                                      ..
                                    } => {
                                        // ignore
                                    }
                                }
                            }
                            SimulationEventWorldEntityType::Projectile { .. } | SimulationEventWorldEntityType::Pickup { .. }  |  SimulationEventWorldEntityType::Laser { .. } => {
                                // ignore
                            }
                        },
                    }
                }));
        }

        pub fn needs_sided_balance(world: &GameWorld) -> bool {
            let (red, blue) = world.count_sides();

            red.abs_diff(blue) > 1
        }

        fn auto_sided_balance(&mut self, world: &mut GameWorld) {
            if Self::needs_sided_balance(world) {
                if self.game_match.balance_tick.is_none() {
                    self.game_match.balance_tick = self
                        .game_options
                        .sided_balance_time()
                        .map(|time| {
                            ((time.as_micros() * TICKS_PER_SECOND as u128)
                                / Duration::from_secs(1).as_micros())
                                as GameTickType
                        })
                        .unwrap_or_default()
                        .into();
                } else if self.game_match.balance_tick.tick().unwrap_or_default() {
                    // force auto balance
                    let (red, blue) = world.count_sides();

                    let diff = red.abs_diff(blue);

                    let side = if red > blue {
                        MatchSide::Red
                    } else {
                        MatchSide::Blue
                    };
                    let join_side = match side {
                        MatchSide::Red => MatchSide::Blue,
                        MatchSide::Blue => MatchSide::Red,
                    };

                    world
                        .characters
                        .values_mut()
                        .filter(|character| character.core.side == Some(side))
                        .take(diff / 2)
                        .for_each(|character| character.core.side = Some(join_side));
                }
            } else {
                self.game_match.balance_tick = Default::default();
            }
        }

        /// returns true, if match needs a restart
        #[must_use]
        pub fn tick(&mut self, world: &mut GameWorld) -> bool {
            self.handle_events(world);

            if let MatchState::GameOver { new_game_in, .. } = &mut self.game_match.state {
                if new_game_in.tick().unwrap_or_default() {
                    self.game_match.state = MatchState::Running {
                        round_ticks_passed: Default::default(),
                        round_ticks_left: self
                            .game_options
                            .time_limit()
                            .map(|time| {
                                ((time.as_micros() * TICKS_PER_SECOND as u128)
                                    / Duration::from_secs(1).as_micros())
                                    as GameTickType
                            })
                            .unwrap_or_default()
                            .into(),
                    };
                    world.characters.values_mut().for_each(|char| {
                        char.score.set(0);
                        char.despawn_to_respawn(false);
                    });
                    true
                } else {
                    false
                }
            } else {
                self.auto_sided_balance(world);
                false
            }
        }
    }
}
