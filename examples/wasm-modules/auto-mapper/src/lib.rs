#![allow(clippy::too_many_arguments)]

use std::hash::Hasher;

use editor_interface::auto_mapper::{
    AutoMapperInputModes, AutoMapperInterface, AutoMapperModes, AutoMapperOutputModes,
};

pub use api::{DB, IO_RUNTIME};
pub use api_auto_mapper::*;
use math::math::Rng;

#[no_mangle]
fn mod_auto_mapper_new() -> Result<Box<dyn AutoMapperInterface>, String> {
    Ok(Box::new(AutoMapperGrassMain::default()))
}

#[derive(Debug, Default)]
struct AutoMapperGrassMain {}

impl AutoMapperInterface for AutoMapperGrassMain {
    fn supported_modes(&self) -> Vec<AutoMapperModes> {
        vec![AutoMapperModes::DesignTileLayer {
            neighbouring_tiles: Some(2.try_into().unwrap()),
        }]
    }

    fn run(
        &mut self,
        seed: u64,
        input: AutoMapperInputModes,
    ) -> Result<AutoMapperOutputModes, String> {
        // Very simple edge detection, but programatically.
        let AutoMapperInputModes::DesignTileLayer {
            mut tiles,
            width,
            height,
            off_x,
            off_y,
            ..
        } = input;

        // skip, layers of such sizes are not supported.
        if width.get() < 2 || height.get() < 2 {
            log::info!(
                "Skipped layer, since width or height was < 2: {}, {}",
                width,
                height
            );
            return Ok(AutoMapperOutputModes::DesignTileLayer { tiles });
        }

        let mut changed_tiles = 0;
        for y in 1..height.get().saturating_sub(1) as usize {
            for x in 1..width.get().saturating_sub(1) as usize {
                let mut hasher = rustc_hash::FxHasher::default();
                hasher.write_u64(seed);
                hasher.write_u16(off_x + x as u16);
                hasher.write_u16(off_y + y as u16);
                let seed = hasher.finish();
                let mut rng = Rng::new(seed);

                let y_off = y * width.get() as usize;
                let y_off_minus_one = (y - 1) * width.get() as usize;
                let y_off_plus_one = (y + 1) * width.get() as usize;
                // case 1: top left
                if tiles[y_off + x].index != 0
                    && tiles[y_off + x - 1].index == 0
                    && tiles[y_off_minus_one + x - 1].index == 0
                    && tiles[y_off_minus_one + x].index == 0
                {
                    changed_tiles += 1;
                    // set current tile to 32, which is grass top left
                    tiles[y_off + x].index = 32;
                    // just to show some randomness
                    if rng.random_int() % 2 == 0 {
                        tiles[y_off + x].index = 4;
                    }
                    tiles[y_off + x].flags = Default::default();
                }
                // case 2: top right
                if tiles[y_off + x].index != 0
                    && tiles[y_off + x + 1].index == 0
                    && tiles[y_off_minus_one + x + 1].index == 0
                    && tiles[y_off_minus_one + x].index == 0
                {
                    changed_tiles += 1;
                    // set current tile to 33, which is grass bottom left
                    tiles[y_off + x].index = 33;
                    tiles[y_off + x].flags = Default::default();
                }
                // case 3: bottom right
                if tiles[y_off + x].index != 0
                    && tiles[y_off + x + 1].index == 0
                    && tiles[y_off_plus_one + x + 1].index == 0
                    && tiles[y_off_plus_one + x].index == 0
                {
                    changed_tiles += 1;
                    // set current tile to 34, which is grass bottom right
                    tiles[y_off + x].index = 34;
                    tiles[y_off + x].flags = Default::default();
                }
                // case 4: bottom left
                if tiles[y_off + x].index != 0
                    && tiles[y_off + x - 1].index == 0
                    && tiles[y_off_plus_one + x - 1].index == 0
                    && tiles[y_off_plus_one + x].index == 0
                {
                    changed_tiles += 1;
                    // set current tile to 35, which is grass bottom left
                    tiles[y_off + x].index = 35;
                    tiles[y_off + x].flags = Default::default();
                }
            }
        }
        log::info!(
            "Changed {} tiles of total {}, w: {}, h: {}",
            changed_tiles,
            tiles.len(),
            width,
            height,
        );

        Ok(AutoMapperOutputModes::DesignTileLayer { tiles })
    }
}
