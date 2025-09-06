//! Inform the game about various settings related changes
//! E.g. skin changes, name changes, changes regarding team name etc.
//! Basically all the stuff that was to be sync'd with the server.

use hiarc::{Hiarc, hiarc_safer_rc_refcell};

/// Notifies about setting changes in the UI
/// that are related to the player.
///
/// E.g. player info on the server
/// Binds of the player etc.
#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc, Default)]
pub struct PlayerSettingsSync {
    player_info_changed: bool,
    controls_changed: bool,
    team_settings_changed: bool,
}

#[hiarc_safer_rc_refcell]
impl PlayerSettingsSync {
    pub fn set_player_info_changed(&mut self) {
        self.player_info_changed = true;
    }
    pub fn did_player_info_change(&mut self) -> bool {
        std::mem::take(&mut self.player_info_changed)
    }
    pub fn set_controls_changed(&mut self) {
        self.controls_changed = true;
    }
    pub fn did_controls_change(&mut self) -> bool {
        std::mem::take(&mut self.controls_changed)
    }
    pub fn set_team_settings_changed(&mut self) {
        self.team_settings_changed = true;
    }
    pub fn did_team_settings_change(&mut self) -> bool {
        std::mem::take(&mut self.team_settings_changed)
    }
}
