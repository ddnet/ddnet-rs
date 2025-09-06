use std::sync::Arc;

use traits::SteamClient;
use traits::SteamRaii;

#[cfg(feature = "runtime")]
pub mod runtime;

pub mod stub;
pub mod traits;

#[cfg(feature = "runtime")]
pub fn init_steam(app_id: u32) -> anyhow::Result<(Arc<dyn SteamClient>, Box<dyn SteamRaii>)> {
    use tokio::sync::Mutex;
    let steam_res = steamworks::Client::init_app(steamworks::AppId(app_id));
    if let Err(err) = &steam_res {
        log::warn!(target: "steam", "Failed to load steam client: {err:?}");
    }
    let client = steam_res?;

    let steam_mutex: Arc<Mutex<()>> = Default::default();
    Ok((
        Arc::new(runtime::SteamMt::new(client.clone(), steam_mutex.clone())),
        Box::new(runtime::SteamSt::new(client, steam_mutex)),
    ))
}

#[cfg(not(feature = "runtime"))]
pub fn init_steam(_app_id: u32) -> anyhow::Result<(Arc<dyn SteamClient>, Box<dyn SteamRaii>)> {
    Ok((Arc::new(stub::SteamMt), Box::new(stub::SteamSt)))
}
