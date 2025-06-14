use base_io::io::IoFileSys;
use config::config::ConfigEngine;

pub fn save(config: &ConfigEngine, io: &IoFileSys) {
    let save_str = config.to_json_string();

    if let Ok(save_str) = save_str {
        let fs_clone = io.fs.clone();
        io.rt.spawn_without_lifetime(async move {
            fs_clone
                .write_file("cfg_engine.json".as_ref(), save_str.as_bytes().to_vec())
                .await
                .unwrap();
            Ok(())
        });
    }
}

pub fn load(io: &IoFileSys) -> anyhow::Result<ConfigEngine> {
    let fs = io.fs.clone();
    let config_file = io
        .rt
        .spawn(async move { Ok(fs.read_file("cfg_engine.json".as_ref()).await?) });
    let res = config_file.get()?;
    ConfigEngine::from_json_slice(&res)
}
