pub mod character_score {
    use std::collections::BTreeMap;

    use game_interface::types::id_types::CharacterId;
    use hiarc::{Hiarc, hiarc_safer_rc_refcell};
    use pool::{datatypes::PoolVec, pool::Pool};
    use rustc_hash::{FxHashMap, FxHashSet};

    /// all characters' hooking relation to each other
    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc)]
    pub struct CharacterScores {
        char_scores: FxHashMap<CharacterId, i64>,
        scores: BTreeMap<i64, FxHashSet<CharacterId>>,

        leading_characters: Pool<Vec<CharacterId>>,
        top_characters: Pool<Vec<(CharacterId, i64)>>,
    }

    #[hiarc_safer_rc_refcell]
    impl Default for CharacterScores {
        fn default() -> Self {
            Self {
                char_scores: Default::default(),
                scores: Default::default(),
                leading_characters: Pool::with_capacity(64),
                top_characters: Pool::with_capacity(64),
            }
        }
    }

    #[hiarc_safer_rc_refcell]
    impl CharacterScores {
        pub(super) fn add_or_set(&mut self, id: CharacterId, score: i64) {
            if let Some(old_score) = self.char_scores.get_mut(&id) {
                let entry = self.scores.entry(*old_score).or_default();
                entry.remove(&id);
                if entry.is_empty() {
                    self.scores.remove(old_score);
                }
                *old_score = score;
            } else {
                self.char_scores.insert(id, score);
            }

            self.scores.entry(score).or_default().insert(id);
        }

        pub(super) fn remove(&mut self, id: &CharacterId) {
            let score = self.char_scores.remove(id).unwrap();
            let entry = self.scores.entry(score).or_default();
            entry.remove(id);
            if entry.is_empty() {
                self.scores.remove(&score);
            }
        }

        pub fn get_score_of(&self, id: &CharacterId) -> i64 {
            *self.char_scores.get(id).unwrap()
        }

        /// The order of the resulting vector is unstable
        pub fn leading_characters(&self) -> Option<(PoolVec<CharacterId>, i64)> {
            self.scores.last_key_value().map(|(score, chars)| {
                let mut leading_chars = self.leading_characters.new();
                leading_chars.extend(chars);
                (leading_chars, *score)
            })
        }

        /// The order of the resulting vector is stable, the first entry is always
        /// the best character, for characters with same score, the first is the lowest id.
        pub fn top_2_leading_characters(&self) -> PoolVec<(CharacterId, i64)> {
            let mut leading_chars = self.top_characters.new();
            for (&score, score_chars) in self.scores.iter().rev() {
                // tmp vec here so the result is in consistent order for the best char to second best
                let mut tmp_leading_chars = self.leading_characters.new();
                tmp_leading_chars.extend(score_chars);
                tmp_leading_chars.sort();
                for char in tmp_leading_chars.iter() {
                    leading_chars.push((*char, score));
                    if leading_chars.len() == 2 {
                        break;
                    }
                }
            }
            leading_chars
        }
    }

    impl CharacterScores {
        pub fn get_new_score(&self, id: CharacterId, score: i64) -> CharacterScore {
            self.add_or_set(id, score);
            CharacterScore {
                scores: self.clone(),
                score,
                id,
            }
        }
    }

    #[derive(Debug, Hiarc)]
    pub struct CharacterScore {
        id: CharacterId,
        score: i64,
        scores: CharacterScores,
    }

    impl CharacterScore {
        pub fn get(&self) -> i64 {
            self.score
        }

        pub fn set(&mut self, score: i64) {
            self.scores.add_or_set(self.id, score);
            self.score = score;
        }
    }

    impl Drop for CharacterScore {
        fn drop(&mut self) {
            self.scores.remove(&self.id);
        }
    }
}
