use std::collections::BTreeSet;

use game_interface::types::render::character::{PlayerCameraMode, PlayerIngameMode};
use hiarc::{Hiarc, hiarc_safer_rc_refcell};

#[derive(Debug, Hiarc, Clone)]
pub struct ActiveClientInfo {
    pub ingame_mode: PlayerIngameMode,
    pub stage_names: BTreeSet<String>,
    pub camera_mode: PlayerCameraMode,
}

impl Default for ActiveClientInfo {
    fn default() -> Self {
        Self {
            ingame_mode: PlayerIngameMode::Spectator,
            stage_names: Default::default(),
            camera_mode: PlayerCameraMode::Default,
        }
    }
}

#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc, Default)]
pub struct ClientInfo {
    local_player_count: usize,
    active_client_info: ActiveClientInfo,
    needs_active_client_info_update: bool,
}

#[hiarc_safer_rc_refcell]
impl ClientInfo {
    pub fn set_local_player_count(&mut self, local_player_count: usize) {
        self.local_player_count = local_player_count;
    }

    pub fn local_player_count(&self) -> usize {
        self.local_player_count
    }

    pub fn set_active_client_info(&mut self, active_client_info: ActiveClientInfo) {
        self.active_client_info = active_client_info;
    }

    pub fn active_client_info(&self) -> ActiveClientInfo {
        self.active_client_info.clone()
    }

    pub fn request_active_client_info(&mut self) {
        self.needs_active_client_info_update = true;
    }

    pub fn wants_active_client_info(&mut self) -> bool {
        std::mem::take(&mut self.needs_active_client_info_update)
    }
}
