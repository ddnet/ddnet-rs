use std::num::NonZeroU16;

use map::{map::groups::layers::tiles::Tile, types::NonZeroU16MinusOne};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AutoMapperModes {
    DesignTileLayer {
        /// If `Some` this allows auto mappers
        /// to run automatically for only a few tiles.
        ///
        /// A neighbouring size of 1 would mean all tiles around
        /// the current tile.
        /// A neighbouring size of 2 would mean all tiles around the
        /// tiles around the current tile and so on.
        ///
        /// If `None` it will disable the auto mode, this is
        /// also useful if the whole layer is always needed anyway.
        neighbouring_tiles: Option<NonZeroU16>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutoMapperInputModes {
    DesignTileLayer {
        tiles: Vec<Tile>,
        width: NonZeroU16MinusOne,
        height: NonZeroU16MinusOne,
        /// X offset within the layer
        /// should not be used for accessing the tiles.
        off_x: u16,
        /// Y offset within the layer
        /// should not be used for accessing the tiles.
        off_y: u16,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutoMapperOutputModes {
    DesignTileLayer { tiles: Vec<Tile> },
}

pub trait AutoMapperInterface {
    /// Returns a list of supported auto mapper features
    fn supported_modes(&self) -> Vec<AutoMapperModes>;

    /// Tries to run the auto mapper on the given input.
    fn run(
        &mut self,
        seed: u64,
        input: AutoMapperInputModes,
    ) -> Result<AutoMapperOutputModes, String>;
}
