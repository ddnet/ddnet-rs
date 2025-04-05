use std::num::{NonZero, NonZeroU64};
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use base::hash::Hash;
use base::network_string::{NetworkReducedAsciiString, NetworkString};
use base_io::io::Io;
use base_io::runtime::IoRuntime;
use base_io_traits::fs_traits::FileSystemInterface;
use cache::Cache;
use game_database::traits::DbInterface;
use game_interface::account_info::MAX_ACCOUNT_NAME_LEN;
//use ddnet::Ddnet;
use game_interface::client_commands::ClientCommand;
use game_interface::events::{EventClientInfo, GameEvents};
use game_interface::ghosts::GhostResult;
use game_interface::interface::{
    GameStateCreate, GameStateCreateOptions, GameStateServerOptions, GameStateStaticInfo,
    MAX_MAP_NAME_LEN,
};
use game_interface::rcon_entries::ExecRconInput;
use game_interface::settings::GameStateSettings;
use game_interface::tick_result::TickResult;
use game_interface::types::character_info::NetworkCharacterInfo;
use game_interface::types::emoticons::EmoticonType;
use game_interface::types::game::{GameTickType, NonZeroGameTickType};
use game_interface::types::id_gen::IdGeneratorIdType;
use game_interface::types::id_types::{CharacterId, PlayerId, StageId};
use game_interface::types::input::CharacterInputInfo;
use game_interface::types::network_stats::PlayerNetworkStats;
use game_interface::types::player_info::{AccountId, PlayerClientInfo, PlayerDropReason};
use game_interface::types::render::character::{CharacterInfo, TeeEye};
use game_interface::types::render::scoreboard::Scoreboard;
use game_interface::types::render::stage::StageRenderInfo;
use game_interface::types::ticks::TickOptions;
use game_interface::vote_commands::{VoteCommand, VoteCommandResult};
use math::math::vector::vec2;
use pool::datatypes::{PoolFxLinkedHashMap, PoolVec};
use pool::mt_datatypes::PoolCow as MtPoolCow;
use vanilla::state::state::GameState;
use wasm_runtime::WasmManager;

use game_interface::{
    interface::GameStateInterface,
    types::{
        render::character::LocalCharacterRenderInfo,
        snapshot::{SnapshotClientInfo, SnapshotLocalPlayers},
    },
};

use super::state_wasm::state_wasm::StateWasm;

#[derive(Debug, Clone)]
pub enum GameStateMod {
    Native,
    Ddnet,
    Wasm { file: Vec<u8> },
}

enum GameStateWrapper {
    Native(Box<GameState>),
    //Ddnet(Ddnet),
    Wasm(StateWasm),
}

impl GameStateWrapper {
    pub fn as_ref(&self) -> &dyn GameStateInterface {
        match self {
            Self::Native(state) => state.as_ref(),
            //GameStateWrapper::Ddnet(state) => state,
            Self::Wasm(state) => state,
        }
    }

    pub fn as_mut(&mut self) -> &mut dyn GameStateInterface {
        match self {
            Self::Native(state) => state.as_mut(),
            //GameStateWrapper::Ddnet(state) => state,
            Self::Wasm(state) => state,
        }
    }
}

pub struct GameStateWasmManager {
    state: GameStateWrapper,

    pub info: GameStateStaticInfo,

    pub predicted_game_monotonic_tick: GameTickType,
}

pub const STATE_MODS_PATH: &str = "mods/state";

impl GameStateWasmManager {
    pub async fn load_module(
        fs: &Arc<dyn FileSystemInterface>,
        file: Vec<u8>,
    ) -> anyhow::Result<Vec<u8>> {
        let cache = Arc::new(Cache::<0>::new_async(STATE_MODS_PATH, fs).await);
        cache
            .load_from_binary(file, |wasm_bytes| {
                Box::pin(async move {
                    Ok(WasmManager::compile_module(&wasm_bytes)?
                        .serialize()?
                        .to_vec())
                })
            })
            .await
    }

    pub fn new(
        game_mod: GameStateMod,
        map: Vec<u8>,
        map_name: NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
        options: GameStateCreateOptions,
        io: &Io,
        db: Arc<dyn DbInterface>,
    ) -> anyhow::Result<Self> {
        let (state, info) = match game_mod {
            GameStateMod::Native => {
                let (state, info) = GameState::new(map, map_name, options, io.rt.clone(), db)
                    .map_err(|err| anyhow!(err))?;
                (GameStateWrapper::Native(Box::new(state)), info)
            }
            GameStateMod::Ddnet => {
                // TODO: let (state, info) = <Ddnet as GameStateCreate>::new(map, options);
                // (GameStateWrapper::Ddnet(state), info)
                let (state, info) = GameState::new(map, map_name, options, io.rt.clone(), db)
                    .map_err(|err| anyhow!(err))?;
                (GameStateWrapper::Native(Box::new(state)), info)
            }
            GameStateMod::Wasm { file: wasm_module } => {
                let mut info = GameStateStaticInfo {
                    ticks_in_a_second: NonZero::new(50).unwrap(),
                    chat_commands: Default::default(),
                    rcon_commands: Default::default(),
                    config: None,

                    mod_name: "unknown".try_into().unwrap(),
                    version: "".try_into().unwrap(),
                    options: GameStateServerOptions::default(),

                    initial_rcon_response: Default::default(),
                };
                let state = StateWasm::new(
                    map,
                    map_name,
                    options,
                    &wasm_module,
                    &mut info,
                    io.rt.clone(),
                    db,
                )?;
                (GameStateWrapper::Wasm(state), info)
            }
        };
        Ok(Self {
            state,

            info,

            predicted_game_monotonic_tick: 0,
        })
    }

    /// Never 0
    pub fn game_tick_speed(&self) -> NonZeroGameTickType {
        self.info.ticks_in_a_second
    }
}

impl GameStateCreate for GameStateWasmManager {
    fn new(
        _map: Vec<u8>,
        _map_name: NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
        _options: GameStateCreateOptions,
        _io_rt: IoRuntime,
        _db: Arc<dyn DbInterface>,
    ) -> Result<(Self, GameStateStaticInfo), NetworkString<1024>>
    where
        Self: Sized,
    {
        panic!("intentionally not implemented for this type.")
    }
}

impl GameStateInterface for GameStateWasmManager {
    fn collect_characters_info(&self) -> PoolFxLinkedHashMap<CharacterId, CharacterInfo> {
        self.state.as_ref().collect_characters_info()
    }

    fn collect_render_ext(&self) -> PoolVec<u8> {
        self.state.as_ref().collect_render_ext()
    }

    fn collect_scoreboard_info(&self) -> Scoreboard {
        self.state.as_ref().collect_scoreboard_info()
    }

    fn all_stages(&self, ratio: f64) -> PoolFxLinkedHashMap<StageId, StageRenderInfo> {
        self.state.as_ref().all_stages(ratio)
    }

    fn collect_character_local_render_info(
        &self,
        player_id: &PlayerId,
    ) -> LocalCharacterRenderInfo {
        self.state
            .as_ref()
            .collect_character_local_render_info(player_id)
    }

    fn get_client_camera_join_pos(&self) -> vec2 {
        self.state.as_ref().get_client_camera_join_pos()
    }

    fn player_join(&mut self, player_info: &PlayerClientInfo) -> PlayerId {
        self.state.as_mut().player_join(player_info)
    }

    fn player_drop(&mut self, player_id: &PlayerId, reason: PlayerDropReason) {
        self.state.as_mut().player_drop(player_id, reason)
    }

    fn try_overwrite_player_character_info(
        &mut self,
        id: &PlayerId,
        info: &NetworkCharacterInfo,
        version: NonZeroU64,
    ) {
        self.state
            .as_mut()
            .try_overwrite_player_character_info(id, info, version)
    }

    fn account_created(&mut self, account_id: AccountId, cert_fingerprint: Hash) {
        self.state
            .as_mut()
            .account_created(account_id, cert_fingerprint)
    }

    fn account_renamed(
        &mut self,
        account_id: AccountId,
        new_name: &NetworkReducedAsciiString<MAX_ACCOUNT_NAME_LEN>,
    ) {
        self.state.as_mut().account_renamed(account_id, new_name)
    }

    fn network_stats(&mut self, stats: PoolFxLinkedHashMap<PlayerId, PlayerNetworkStats>) {
        self.state.as_mut().network_stats(stats)
    }

    fn settings(&self) -> GameStateSettings {
        self.state.as_ref().settings()
    }

    fn client_command(&mut self, player_id: &PlayerId, cmd: ClientCommand) {
        self.state.as_mut().client_command(player_id, cmd)
    }

    fn rcon_command(
        &mut self,
        player_id: Option<PlayerId>,
        cmd: ExecRconInput,
    ) -> Vec<Result<NetworkString<65536>, NetworkString<65536>>> {
        self.state.as_mut().rcon_command(player_id, cmd)
    }

    fn vote_command(&mut self, cmd: VoteCommand) -> VoteCommandResult {
        self.state.as_mut().vote_command(cmd)
    }

    fn voted_player(&mut self, player_id: Option<PlayerId>) {
        self.state.as_mut().voted_player(player_id)
    }

    fn set_player_inputs(&mut self, inps: PoolFxLinkedHashMap<PlayerId, CharacterInputInfo>) {
        self.state.as_mut().set_player_inputs(inps)
    }

    fn set_player_emoticon(&mut self, player_id: &PlayerId, emoticon: EmoticonType) {
        self.state.as_mut().set_player_emoticon(player_id, emoticon)
    }

    fn set_player_eye(&mut self, player_id: &PlayerId, eye: TeeEye, duration: Duration) {
        self.state.as_mut().set_player_eye(player_id, eye, duration)
    }

    fn tick(&mut self, options: TickOptions) -> TickResult {
        self.state.as_mut().tick(options)
    }

    fn snapshot_for(&self, client: SnapshotClientInfo) -> MtPoolCow<'static, [u8]> {
        self.state.as_ref().snapshot_for(client)
    }

    fn build_from_snapshot(&mut self, snapshot: &MtPoolCow<'static, [u8]>) -> SnapshotLocalPlayers {
        self.state.as_mut().build_from_snapshot(snapshot)
    }

    fn snapshot_for_hotreload(&self) -> Option<MtPoolCow<'static, [u8]>> {
        self.state.as_ref().snapshot_for_hotreload()
    }

    fn build_from_snapshot_by_hotreload(&mut self, snapshot: &MtPoolCow<'static, [u8]>) {
        self.state
            .as_mut()
            .build_from_snapshot_by_hotreload(snapshot)
    }

    fn build_from_snapshot_for_prev(&mut self, snapshot: &MtPoolCow<'static, [u8]>) {
        self.state.as_mut().build_from_snapshot_for_prev(snapshot)
    }

    fn build_ghosts_from_snapshot(&self, snapshot: &MtPoolCow<'static, [u8]>) -> GhostResult {
        self.state.as_ref().build_ghosts_from_snapshot(snapshot)
    }

    fn events_for(&self, client: EventClientInfo) -> GameEvents {
        self.state.as_ref().events_for(client)
    }

    fn clear_events(&mut self) {
        self.state.as_mut().clear_events()
    }

    fn sync_event_id(&self, event_id: IdGeneratorIdType) {
        self.state.as_ref().sync_event_id(event_id)
    }
}
