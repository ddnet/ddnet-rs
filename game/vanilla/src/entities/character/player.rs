pub mod player {
    use std::marker::PhantomData;

    use base::linked_hash_map_view::FxLinkedHashMap;
    use base::network_string::NetworkReducedAsciiString;
    use game_interface::account_info::MAX_ACCOUNT_NAME_LEN;
    use game_interface::client_commands::ClientCameraMode;
    use game_interface::types::character_info::NetworkCharacterInfo;
    use game_interface::types::game::{GameTickCooldown, GameTickType};
    use game_interface::types::id_types::{CharacterId, PlayerId, StageId};
    use game_interface::types::input::CharacterInput;
    use game_interface::types::network_stats::PlayerNetworkStats;
    use game_interface::types::player_info::PlayerUniqueId;
    use game_interface::types::render::character::{PlayerCameraMode, TeeEye};
    use game_interface::types::snapshot::SnapshotLocalPlayer;
    use hiarc::{hiarc_safer_rc_refcell, Hiarc};
    use hiarc::{HiFnMut, HiFnOnce};
    use pool::datatypes::{PoolFxHashSet, PoolFxLinkedHashMap, PoolVec};
    use pool::pool::Pool;
    use pool::rc::PoolRc;
    use rustc_hash::FxHashSet;
    use serde::{Deserialize, Serialize};

    use crate::snapshot::snapshot::SnapshotSpectatorPlayer;

    /// This purposely does not implement [`Clone`].
    /// Instead the user should always query the current character info.
    /// (it might have been changed by other logic as a side effect)
    #[derive(Debug, Hiarc, Serialize, Deserialize)]
    pub struct PlayerCharacterInfo {
        pub(in super::super::super::character) stage_id: StageId,
    }

    impl PlayerCharacterInfo {
        pub fn stage_id(&self) -> StageId {
            self.stage_id
        }
    }

    #[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
    pub struct PlayerInfo {
        pub player_info: PoolRc<NetworkCharacterInfo>,
        pub version: u64,

        pub unique_identifier: PlayerUniqueId,
        pub account_name: Option<NetworkReducedAsciiString<MAX_ACCOUNT_NAME_LEN>>,
        /// The id given by the client to this player
        pub id: u64,
    }

    pub type Player = PlayerCharacterInfo;

    /// A slim wrapper around the character info around the player.
    ///
    /// A player contains no additional information, instead the player info
    /// is stored in the character info.
    /// This is different compared to a [`SpectatorPlayer`], which does contain the
    /// player info and other stuff.
    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc, Default)]
    pub struct Players {
        players: FxLinkedHashMap<PlayerId, Player>,

        _p: PhantomData<PoolVec<(PlayerId, Player)>>,
    }

    #[hiarc_safer_rc_refcell]
    impl Players {
        pub fn new() -> Self {
            Self {
                players: Default::default(),
                _p: PhantomData,
            }
        }

        pub fn player(&self, id: &PlayerId) -> Option<Player> {
            let player = self.players.get(id)?;
            Some(Player {
                stage_id: player.stage_id,
            })
        }

        pub(in super::super::super::character) fn insert(&mut self, id: PlayerId, player: Player) {
            self.players.insert(id, player);
        }
        pub(in super::super::super::character) fn remove(&mut self, id: &PlayerId) {
            self.players.remove(id);
        }
        pub(crate) fn move_to_back(&mut self, id: &PlayerId) {
            self.players.to_back(id);
        }
        pub(crate) fn pooled_clone_into(&self, copy_pool: &mut PoolVec<(PlayerId, Player)>) {
            copy_pool.extend(self.players.iter().map(|(id, player)| {
                (
                    *id,
                    Player {
                        stage_id: player.stage_id,
                    },
                )
            }));
        }
    }

    #[derive(Debug, Hiarc, Serialize, Deserialize)]
    pub struct SpectatorPlayer {
        pub player_info: PlayerInfo,
        pub player_input: CharacterInput,
        pub id: PlayerId,
        pub spectated_characters: PoolFxHashSet<CharacterId>,
        pub default_eye: TeeEye,
        pub default_eye_reset_in: GameTickCooldown,

        pub network_stats: PlayerNetworkStats,
    }

    impl SpectatorPlayer {
        pub fn new(
            player_info: PlayerInfo,
            player_input: CharacterInput,
            id: &PlayerId,
            spectated_characters: PoolFxHashSet<CharacterId>,
            default_eye: TeeEye,
            default_eye_reset_in: GameTickCooldown,
            network_stats: PlayerNetworkStats,
        ) -> Self {
            Self {
                player_info,
                player_input,
                id: *id,
                spectated_characters,
                default_eye,
                default_eye_reset_in,

                network_stats,
            }
        }
    }

    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc, Default)]
    pub struct SpectatorPlayers {
        players: FxLinkedHashMap<PlayerId, SpectatorPlayer>,

        // force higher hierarchy val
        _passed: PhantomData<PoolFxLinkedHashMap<PlayerId, SpectatorPlayer>>,
        _n: PhantomData<PoolFxLinkedHashMap<PlayerId, SnapshotSpectatorPlayer>>,
    }

    #[hiarc_safer_rc_refcell]
    impl SpectatorPlayers {
        pub fn new() -> Self {
            Self {
                players: Default::default(),

                _passed: Default::default(),
                _n: Default::default(),
            }
        }
        pub fn to_snapshot_local_player(&self, id: &PlayerId) -> Option<SnapshotLocalPlayer> {
            self.players.get(id).map(|p| SnapshotLocalPlayer {
                id: p.player_info.id,
                input_cam_mode: if p.spectated_characters.is_empty() {
                    PlayerCameraMode::Free
                } else {
                    PlayerCameraMode::LockedOn {
                        character_ids: p.spectated_characters.clone(),
                        locked_ingame: false,
                    }
                },
            })
        }
        pub fn set_camera_mode(
            &mut self,
            id: &PlayerId,
            pool: &Pool<FxHashSet<CharacterId>>,
            mode: ClientCameraMode,
        ) {
            if let Some(spectator) = self.players.get_mut(id) {
                spectator.spectated_characters = match mode {
                    ClientCameraMode::None => pool.new(),
                    ClientCameraMode::FreeCam(characters)
                    | ClientCameraMode::PhasedFreeCam(characters) => {
                        let mut item = pool.new();
                        (*item).clone_from(&characters);
                        item
                    }
                };
            }
        }
        pub fn set_default_eye(&mut self, id: &PlayerId, eye: TeeEye, normal_in: GameTickType) {
            if let Some(spectator) = self.players.get_mut(id) {
                spectator.default_eye = eye;
                spectator.default_eye_reset_in = normal_in.into();
            }
        }
        pub fn contains_key(&self, id: &PlayerId) -> bool {
            self.players.get(id).is_some()
        }
        pub fn any_with_name(&self, except_id: Option<PlayerId>, name: &str) -> bool {
            self.players
                .iter()
                .filter_map(|(id, p)| (Some(*id) != except_id).then_some(p))
                .any(|p| p.player_info.player_info.name.as_str() == name)
        }

        pub fn insert(&mut self, id: PlayerId, player: SpectatorPlayer) {
            self.players.insert(id, player);
        }
        pub fn remove(&mut self, id: &PlayerId) -> Option<SpectatorPlayer> {
            self.players.remove(id)
        }
        pub(crate) fn move_to_back(&mut self, id: &PlayerId) {
            self.players.to_back(id);
        }
        pub(crate) fn pooled_clone_into(
            &self,
            copy_pool: &mut PoolFxLinkedHashMap<PlayerId, SpectatorPlayer>,
        ) {
            for (id, player) in self.players.iter() {
                copy_pool.insert(*id, {
                    SpectatorPlayer::new(
                        player.player_info.clone(),
                        player.player_input,
                        id,
                        player.spectated_characters.clone(),
                        player.default_eye,
                        player.default_eye_reset_in,
                        player.network_stats,
                    )
                });
            }
        }
        pub(crate) fn retain_with_order<F>(&mut self, mut f: F)
        where
            for<'a> F: HiFnMut<(&'a PlayerId, &'a mut SpectatorPlayer), bool>,
        {
            self.players
                .retain_with_order(|id, player| f.call_mut((id, player)))
        }
        /// handle a spectator player
        /// returns false if the player did not exist, else true
        pub(crate) fn handle_mut<F>(&mut self, id: &PlayerId, f: F) -> bool
        where
            for<'a> F: HiFnOnce<&'a mut SpectatorPlayer, ()>,
        {
            match self.players.get_mut(id) {
                Some(player) => {
                    f.call_once(player);
                    true
                }
                None => false,
            }
        }
    }
}
