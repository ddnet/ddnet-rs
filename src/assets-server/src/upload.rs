pub mod verify;

use std::{path::PathBuf, sync::Arc};

use assets_base::{AssetIndexEntry, AssetUpload, AssetUploadResponse, AssetsMeta};
use axum::Json;
use base::hash::generate_hash_for;
use tokio::sync::Mutex;
use verify::{verify_resource, AllowedResources};

use crate::index_dir::Index;

pub async fn asset_upload(
    write_lock: Arc<Mutex<()>>,
    index: Arc<parking_lot::RwLock<Index>>,
    base_path: String,
    upload_password: Arc<String>,
    allowed_resources: Vec<AllowedResources>,
    Json(data): Json<AssetUpload>,
) -> Json<AssetUploadResponse> {
    let base_path: PathBuf = base_path.into();
    if upload_password.as_str() != data.upload_password.as_str() {
        return Json(AssetUploadResponse::IncorrectPassword);
    }

    if data.meta.authors.is_empty() || data.meta.licenses.is_empty() {
        return Json(AssetUploadResponse::IncompleteMetadata);
    }

    if !data
        .name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Json(AssetUploadResponse::InvalidName);
    }

    let verify_res = verify_resource(
        &data.extension,
        &data.name,
        &data.data,
        &data.meta.category,
        &allowed_resources,
    );

    if !matches!(verify_res, AssetUploadResponse::Success) {
        return Json(verify_res);
    }

    // acquire the write lock
    let g = write_lock.lock().await;

    let path = base_path.join(&data.name).with_extension(&data.extension);
    match tokio::fs::write(path, &data.data).await {
        Ok(_) => {}
        Err(_) => {
            return Json(AssetUploadResponse::WritingFailed);
        }
    }

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
            return Json(AssetUploadResponse::WritingFailed);
        }
    };

    metadata.insert(data.name.clone(), data.meta);

    let metadata = match serde_json::to_vec(&metadata) {
        Ok(metadata) => metadata,
        Err(err) => {
            log::error!("{err}");
            return Json(AssetUploadResponse::WritingFailed);
        }
    };
    match tokio::fs::write(metadata_path, metadata).await {
        Ok(_) => {}
        Err(_) => {
            return Json(AssetUploadResponse::WritingFailed);
        }
    }

    // now insert into index & write to disk
    let index_json = {
        let mut index = index.write();
        index.insert(
            data.name,
            AssetIndexEntry {
                ty: data.extension,
                hash: generate_hash_for(&data.data),
                size: data.data.len() as u64,
            },
        );
        index.to_json()
    };

    let path = base_path.join("index.json");
    match tokio::fs::write(path, index_json).await {
        Ok(_) => {}
        Err(_) => {
            return Json(AssetUploadResponse::WritingFailed);
        }
    }

    drop(g);

    Json(AssetUploadResponse::Success)
}
