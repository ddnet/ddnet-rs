/// everything related to a single match/round/race-run
pub mod match_state {
    use game_interface::{
        pooling::GamePooling,
        types::{
            game::{GameTickCooldown, GameTickType},
            id_types::CharacterId,
            render::game::{
                game_match::MatchSide, MatchRoundGameOverWinBy, MatchRoundGameOverWinner,
                MatchRoundGameOverWinnerCharacter, MatchRoundTimeType,
            },
        },
    };
    use hiarc::Hiarc;
    use serde::{Deserialize, Serialize};

    use crate::{
        entities::character::score::character_score::CharacterScores,
        state::state::TICKS_PER_SECOND, types::types::GameOptions, world::world::GameWorld,
    };

    #[derive(Debug, Hiarc, Serialize, Deserialize, Clone, Copy)]
    pub enum MatchWinner {
        Character(CharacterId),
        Side(MatchSide),
    }

    #[derive(Debug, Hiarc, Serialize, Deserialize, Clone, Copy)]
    pub enum MatchState {
        Running {
            /// How long the game round is running.
            round_ticks_passed: GameTickType,
            round_ticks_left: GameTickCooldown,
        },
        Paused {
            /// How long the game round is running.
            round_ticks_passed: GameTickType,
            round_ticks_left: GameTickCooldown,
        },
        SuddenDeath {
            /// How long the game round is running.
            round_ticks_passed: GameTickType,
            by_cooldown: bool,
        },
        PausedSuddenDeath {
            /// How long the game round is running.
            round_ticks_passed: GameTickType,
            by_cooldown: bool,
        },
        GameOver {
            winner: MatchWinner,
            new_game_in: GameTickCooldown,
            /// How long the game round is running.
            round_ticks_passed: GameTickType,
            by_cooldown: bool,
        },
    }

    impl MatchState {
        pub fn passed_ticks(&self) -> GameTickType {
            match self {
                MatchState::Running {
                    round_ticks_passed, ..
                } => *round_ticks_passed,
                MatchState::Paused {
                    round_ticks_passed, ..
                } => *round_ticks_passed,
                MatchState::SuddenDeath {
                    round_ticks_passed, ..
                } => *round_ticks_passed,
                MatchState::PausedSuddenDeath {
                    round_ticks_passed, ..
                } => *round_ticks_passed,
                MatchState::GameOver {
                    round_ticks_passed, ..
                } => *round_ticks_passed,
            }
        }

        pub fn round_ticks_left(
            &self,
            world: &GameWorld,
            pools: &GamePooling,
        ) -> MatchRoundTimeType {
            match self {
                MatchState::Running {
                    round_ticks_left, ..
                }
                | MatchState::Paused {
                    round_ticks_left, ..
                } => round_ticks_left
                    .get()
                    .map(|ticks_left| MatchRoundTimeType::TimeLimit {
                        ticks_left: ticks_left.get(),
                    })
                    .unwrap_or(MatchRoundTimeType::Normal),
                MatchState::SuddenDeath { .. } | MatchState::PausedSuddenDeath { .. } => {
                    MatchRoundTimeType::SuddenDeath
                }
                MatchState::GameOver {
                    winner,
                    by_cooldown,
                    ..
                } => MatchRoundTimeType::GameOver {
                    winner: match winner {
                        MatchWinner::Character(character_id) => {
                            let mut chars = pools.game_over_winner_character_pool.new();
                            for char in world.characters.get(character_id).into_iter() {
                                let info = &char.player_info.player_info;
                                chars.push(MatchRoundGameOverWinnerCharacter {
                                    name: {
                                        let mut name = pools.network_string_name_pool.new();
                                        (*name).clone_from(&info.name);
                                        name
                                    },
                                    skin: {
                                        let mut skin = pools.resource_key_pool.new();
                                        (*skin).clone_from(&info.skin);
                                        skin
                                    },
                                    skin_info: info.skin_info,
                                });
                            }
                            MatchRoundGameOverWinner::Characters(chars)
                        }
                        MatchWinner::Side(side) => {
                            // check if all characters of the winner side are in one team
                            let mut it = world
                                .characters
                                .values()
                                .filter(|char| char.core.side == Some(*side))
                                .peekable();
                            let first = it.peek().map(|c| c.player_info.player_info.clan.as_str());
                            let same_clan = it.all(|char| {
                                Some(char.player_info.player_info.clan.as_str()) == first
                            });
                            if let Some(clan) = same_clan
                                .then_some(first)
                                .flatten()
                                .and_then(|clan| (!clan.is_empty()).then_some(clan))
                            {
                                MatchRoundGameOverWinner::SideNamed({
                                    let mut name = pools.network_string_team_pool.new();

                                    name.try_set(clan).expect(
                                        "clan name len was expected to \
                                        be smaller than the side name len",
                                    );

                                    name
                                })
                            } else {
                                MatchRoundGameOverWinner::Side(*side)
                            }
                        }
                    },
                    by: if *by_cooldown {
                        MatchRoundGameOverWinBy::TimeLimit
                    } else {
                        MatchRoundGameOverWinBy::ScoreLimit
                    },
                },
            }
        }
    }

    #[derive(Debug, Hiarc, Serialize, Deserialize, Clone, Copy)]
    pub enum MatchType {
        Solo,
        Sided { scores: [i64; 2] },
    }

    /// the snappable part of the match manager
    #[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
    pub struct Match {
        pub ty: MatchType,
        pub state: MatchState,
        pub balance_tick: GameTickCooldown,
    }

    impl Match {
        // TODO: random 4 seconds
        const TICKS_UNTIL_NEW_GAME: GameTickType = TICKS_PER_SECOND * 4;

        // TODO: sudden death solo
        pub fn win_check(
            &mut self,
            game_options: &GameOptions,
            scores: &CharacterScores,
            round_time_limit_reached: bool,
        ) {
            let cur_tick = self.state.passed_ticks();
            let round_time_limit_reached = round_time_limit_reached
                | matches!(
                    self.state,
                    MatchState::SuddenDeath { .. } | MatchState::PausedSuddenDeath { .. }
                );
            match self.ty {
                MatchType::Solo => {
                    if let Some((leading_characters, score)) = scores.leading_characters() {
                        // check if the character has hit a specific score
                        if let Some(leading_characters) = ((score >= 0
                            && score as u64 >= game_options.score_limit())
                            || round_time_limit_reached)
                            .then_some(leading_characters)
                        {
                            if leading_characters.len() == 1 {
                                // TODO:
                                self.state = MatchState::GameOver {
                                    winner: MatchWinner::Character(
                                        leading_characters.iter().next().copied().unwrap(),
                                    ),
                                    new_game_in: Self::TICKS_UNTIL_NEW_GAME.into(),
                                    round_ticks_passed: cur_tick,
                                    by_cooldown: round_time_limit_reached,
                                }
                            } else if round_time_limit_reached {
                                self.state = MatchState::SuddenDeath {
                                    round_ticks_passed: cur_tick,
                                    by_cooldown: round_time_limit_reached,
                                };
                            }
                        }
                    }
                }
                MatchType::Sided { scores } => {
                    let leading_side = match scores[0].cmp(&scores[1]) {
                        std::cmp::Ordering::Less => Some((scores[1], MatchSide::Blue)),
                        std::cmp::Ordering::Equal => None,
                        std::cmp::Ordering::Greater => Some((scores[0], MatchSide::Red)),
                    };

                    if let Some(side) = leading_side.and_then(|(score, side)| {
                        ((score >= 0 && score as u64 >= game_options.score_limit())
                            || round_time_limit_reached)
                            .then_some(side)
                    }) {
                        // TODO:
                        self.state = MatchState::GameOver {
                            winner: MatchWinner::Side(side),
                            new_game_in: Self::TICKS_UNTIL_NEW_GAME.into(),
                            round_ticks_passed: cur_tick,
                            by_cooldown: round_time_limit_reached,
                        };
                    } else if round_time_limit_reached {
                        self.state = MatchState::SuddenDeath {
                            round_ticks_passed: cur_tick,
                            by_cooldown: round_time_limit_reached,
                        };
                    }
                }
            }
        }

        pub fn tick(&mut self, game_options: &GameOptions, scores: &CharacterScores) {
            match &mut self.state {
                MatchState::Running {
                    round_ticks_passed,
                    round_ticks_left,
                } => {
                    *round_ticks_passed += 1;
                    if round_ticks_left.tick().unwrap_or_default() {
                        self.win_check(game_options, scores, true);
                    }
                }
                MatchState::SuddenDeath {
                    round_ticks_passed, ..
                } => {
                    *round_ticks_passed += 1;
                }
                MatchState::Paused { .. }
                | MatchState::PausedSuddenDeath { .. }
                | MatchState::GameOver { .. } => {
                    // nothing to do
                }
            }
        }
    }
}
