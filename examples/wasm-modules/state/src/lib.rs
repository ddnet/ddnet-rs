#![allow(clippy::too_many_arguments)]

use base::network_string::{NetworkReducedAsciiString, NetworkString};
use game_interface::interface::{
    GameStateCreate, GameStateCreateOptions, GameStateInterface, GameStateStaticInfo,
    MAX_MAP_NAME_LEN,
};
use state::state::GameState;

pub mod collision;
pub mod entities;
pub mod events;
pub mod game_objects;
pub mod match_manager;
pub mod match_state;
pub mod simulation_pipe;
pub mod snapshot;
pub mod spawns {
    pub use ::vanilla::spawns::*;
}
pub mod stage;
pub mod state;
pub mod types;
pub mod weapons;
pub mod world;

pub mod command_chain {
    pub use ::vanilla::command_chain::*;
}

pub mod sql {
    pub use ::vanilla::sql::*;
}

pub mod config;

pub mod reusable {
    pub use ::vanilla::reusable::*;
}

pub use api::{DB, IO_RUNTIME};
pub use api_state::*;

#[no_mangle]
fn mod_state_new(
    map: Vec<u8>,
    map_name: NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
    options: GameStateCreateOptions,
) -> Result<(Box<dyn GameStateInterface>, GameStateStaticInfo), NetworkString<1024>> {
    let (state, info) = GameState::new(
        map,
        map_name,
        options,
        IO_RUNTIME.with(|d| (*d).clone()),
        DB.with(|d| (*d).clone()),
    )?;
    Ok((Box::new(state), info))
}
