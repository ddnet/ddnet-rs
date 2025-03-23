use std::{path::PathBuf, sync::Arc};

use assets_base::{AssetDelete, AssetDeleteResponse, AssetsMeta};
use axum::Json;
use tokio::sync::Mutex;

use crate::index_dir::Index;

pub async fn asset_delete(
    write_lock: Arc<Mutex<()>>,
    index: Arc<parking_lot::RwLock<Index>>,
    base_path: String,
    upload_password: Arc<String>,
    Json(data): Json<AssetDelete>,
) -> Json<AssetDeleteResponse> {
    let base_path: PathBuf = base_path.into();
    if upload_password.as_str() != data.upload_password.as_str() {
        return Json(AssetDeleteResponse::IncorrectPassword);
    }

    // acquire the write lock
    let g = write_lock.lock().await;

    // read & write the meta data
    let metadata_path = base_path.join("meta.json");
    let mut metadata: AssetsMeta = match tokio::fs::read(&metadata_path).await {
        Ok(file) => match serde_json::from_slice(&file) {
            Ok(r) => r,
            Err(err) => {
                log::error!("{err}");
                Default::default()
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Default::default(),
        Err(err) => {
            log::error!("{err}");
            return Json(AssetDeleteResponse::DeletingFailed);
        }
    };

    metadata.remove(&data.name);

    let metadata = match serde_json::to_vec(&metadata) {
        Ok(metadata) => metadata,
        Err(err) => {
            log::error!("{err}");
            return Json(AssetDeleteResponse::DeletingFailed);
        }
    };
    match tokio::fs::write(metadata_path, metadata).await {
        Ok(_) => {}
        Err(_) => {
            return Json(AssetDeleteResponse::DeletingFailed);
        }
    }

    // now remove from index & write to disk
    let index_json = {
        let mut index = index.write();
        index.remove(&data.name);
        index.to_json()
    };

    let path = base_path.join("index.json");
    match tokio::fs::write(path, index_json).await {
        Ok(_) => {}
        Err(_) => {
            return Json(AssetDeleteResponse::DeletingFailed);
        }
    }

    drop(g);

    Json(AssetDeleteResponse::Success)
}
