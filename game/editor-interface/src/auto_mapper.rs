use map::{map::groups::layers::tiles::Tile, types::NonZeroU16MinusOne};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum AutoMapperModes {
    DesignTileLayer,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum AutoMapperInputModes {
    DesignTileLayer {
        tiles: Vec<Tile>,
        width: NonZeroU16MinusOne,
        height: NonZeroU16MinusOne,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum AutoMapperOutputModes {
    DesignTileLayer { tiles: Vec<Tile> },
}

pub trait AutoMapperInterface
where
    Self: Sized,
{
    fn supported_modes() -> AutoMapperModes;

    fn run(&mut self, input: AutoMapperInputModes) -> anyhow::Result<AutoMapperOutputModes>;
}
