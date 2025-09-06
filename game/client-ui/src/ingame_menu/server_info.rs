pub mod main_frame;

use game_interface::interface::GameStateServerOptions;
use hiarc::{Hiarc, hiarc_safer_rc_refcell};

#[derive(Debug, Hiarc, Default, Clone)]
pub struct GameInfo {
    pub map_name: String,
}

#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc, Default)]
pub struct GameServerInfo {
    game_info: GameInfo,
    options: GameStateServerOptions,
}

#[hiarc_safer_rc_refcell]
impl GameServerInfo {
    pub fn fill_game_info(&mut self, game_info: GameInfo) {
        self.game_info = game_info;
    }

    pub fn game_info(&self) -> GameInfo {
        self.game_info.clone()
    }

    pub fn fill_server_options(&mut self, options: GameStateServerOptions) {
        self.options = options
    }

    pub fn server_options(&mut self) -> GameStateServerOptions {
        self.options.clone()
    }
}
