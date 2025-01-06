use std::path::Path;

use base_io::io::{Io, IoFileSys};
use game_config::config::ConfigGame;

pub fn save(config: &ConfigGame, io: &Io) {
    let save_str = config.to_json_string();

    if let Ok(save_str) = save_str {
        let fs_clone = io.fs.clone();
        io.rt.spawn_without_lifetime(async move {
            fs_clone
                .write_file("cfg_game.json".as_ref(), save_str.as_bytes().to_vec())
                .await
                .unwrap();
            Ok(())
        });
    }
}

pub fn load_in(io: &IoFileSys, path: &Path) -> anyhow::Result<ConfigGame> {
    let fs = io.fs.clone();
    let path = path.to_path_buf();
    let config_file = io
        .rt
        .spawn(async move { Ok(fs.read_file(path.as_ref()).await?) });
    let res = config_file.get_storage()?;
    ConfigGame::from_json_slice(&res)
}

pub fn load(io: &IoFileSys) -> anyhow::Result<ConfigGame> {
    load_in(io, "cfg_game.json".as_ref())
}
