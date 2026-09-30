use anyhow::{Context, ensure};
use base::hash::{Hash, fmt_hash, generate_hash_for};
use base_io_traits::fs_traits::FileSystemInterface;
use game_base::network::messages::MsgSvServerInfo;
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

pub struct LegacyMap {
    pub name: String,
    pub bytes: Vec<u8>,
    pub crc: i32,
    pub tuning: Option<MapTuning>,
}

async fn resource(
    fs: &dyn FileSystemInterface,
    client: &reqwest::Client,
    origin: Option<&url::Url>,
    path: &str,
    hash: Hash,
) -> anyhow::Result<Vec<u8>> {
    let local = match fs.read_file(path.as_ref()).await {
        Ok(bytes) => Ok(bytes),
        Err(_) => {
            fs.read_file(path.replace(&format!("_{}", fmt_hash(&hash)), "").as_ref())
                .await
        }
    };
    let bytes = match local.ok().filter(|bytes| generate_hash_for(bytes) == hash) {
        Some(bytes) => bytes,
        None => {
            let origin = origin
                .context("resource missing locally and backend has no resource download URL")?;
            let mut response = client
                .get(origin.join(path)?)
                .send()
                .await?
                .error_for_status()?;
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                ensure!(
                    bytes.len() + chunk.len() <= 128 * 1024 * 1024,
                    "map resource exceeds 128 MiB"
                );
                bytes.extend_from_slice(&chunk);
            }
            bytes
        }
    };
    ensure!(
        generate_hash_for(&bytes) == hash,
        "resource hash mismatch: {path}"
    );
    Ok(bytes)
}

pub async fn load(
    fs: Arc<dyn FileSystemInterface>,
    backend: IpAddr,
    info: MsgSvServerInfo,
    tp: Arc<rayon::ThreadPool>,
    external: Option<url::Url>,
) -> anyhow::Result<LegacyMap> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(60))
        .build()?;
    let origin = info
        .resource_server_fallback
        .map(|port| {
            url::Url::parse(&format!("http://{}/", SocketAddr::new(backend, port))).unwrap()
        })
        .or(external);
    if let Some(origin) = &origin {
        ensure!(
            matches!(origin.scheme(), "http" | "https"),
            "unsupported resource URL scheme"
        );
    }
    let path = format!(
        "map/maps/{}_{}.twmap.tar",
        info.map.as_str(),
        fmt_hash(&info.map_blake3_hash)
    );
    let bytes = resource(&*fs, &client, origin.as_ref(), &path, info.map_blake3_hash).await?;
    let mut tuning = None;
    let default_tune = if info.server_options.physics_group_name.as_str() == "ddnet" {
        vanilla::collision::Tunings::race_default(50)
    } else {
        vanilla::collision::Tunings::vanilla_default(50)
    };
    let converted = map_convert_lib::new_to_legacy::new_to_legacy_from_buf_async(
        &bytes,
        |map| {
            tuning = Some(MapTuning::new(map.groups.physics.clone(), default_tune));
            let resources = map.resources.clone();
            Box::pin(async move {
                let mut groups = Vec::new();
                for (kind, entries) in [
                    ("images", resources.images),
                    ("images", resources.image_arrays),
                    ("sounds", resources.sounds),
                ] {
                    let mut group = Vec::new();
                    for item in entries {
                        let path = format!(
                            "map/resources/{kind}/{}_{}.{}",
                            item.name.as_str(),
                            fmt_hash(&item.meta.blake3_hash),
                            item.meta.ty.as_str()
                        );
                        group.push(
                            resource(&*fs, &client, origin.as_ref(), &path, item.meta.blake3_hash)
                                .await?,
                        );
                    }
                    groups.push(group);
                }
                let mut groups = groups.into_iter();
                Ok((
                    groups.next().unwrap(),
                    groups.next().unwrap(),
                    groups.next().unwrap(),
                ))
            })
        },
        &tp,
    )
    .await?;
    ensure!(
        converted.map.len() <= i32::MAX as usize,
        "legacy map too large"
    );
    Ok(LegacyMap {
        name: info.map.to_string(),
        crc: crc32fast::hash(&converted.map) as i32,
        tuning: Some(tuning.context("map conversion did not load physics")??),
        bytes: converted.map,
    })
}

/// Shared across sessions; concurrent joins convert each map only once.
pub(super) type Cache = Arc<tokio::sync::Mutex<std::collections::VecDeque<(Hash, Arc<LegacyMap>)>>>;

pub(super) async fn cached(
    cache: Cache,
    fs: Arc<dyn FileSystemInterface>,
    backend: IpAddr,
    info: MsgSvServerInfo,
    tp: Arc<rayon::ThreadPool>,
    external: Option<url::Url>,
) -> anyhow::Result<Arc<LegacyMap>> {
    let mut cache = cache.lock().await;
    if let Some((_, map)) = cache.iter().find(|(hash, _)| *hash == info.map_blake3_hash) {
        return Ok(map.clone());
    }
    let hash = info.map_blake3_hash;
    let map = Arc::new(load(fs, backend, info, tp, external).await?);
    cache.push_back((hash, map.clone()));
    while cache.len() > 2 {
        cache.pop_front();
    }
    Ok(map)
}

pub struct MapTuning {
    collision: Box<vanilla::collision::Collision>,
    zones: Vec<u8>,
    width: usize,
    height: usize,
}

impl MapTuning {
    fn new(
        physics: map::map::groups::MapGroupPhysics,
        tune: vanilla::collision::Tunings,
    ) -> anyhow::Result<Self> {
        let width = physics.attr.width.get() as usize;
        let height = physics.attr.height.get() as usize;
        let zones = physics
            .layers
            .iter()
            .find_map(|l| match l {
                map::map::groups::layers::physics::MapLayerPhysics::Tune(t) => {
                    Some(t.base.tiles.iter().map(|t| t.number).collect())
                }
                _ => None,
            })
            .unwrap_or_default();
        Ok(Self {
            collision: vanilla::collision::Collision::with_default_tune(physics, true, tune)?,
            zones,
            width,
            height,
        })
    }

    pub fn at<'a>(
        &'a self,
        pos: &math::math::vector::vec2,
        global: &'a vanilla::collision::Tunings,
    ) -> &'a vanilla::collision::Tunings {
        let x = (pos.x.round() as i32 / 32).clamp(0, self.width as i32 - 1) as usize;
        let y = (pos.y.round() as i32 / 32).clamp(0, self.height as i32 - 1) as usize;
        if self.zones.get(y * self.width + x).copied().unwrap_or(0) == 0 {
            global
        } else {
            self.collision.get_tune_at(pos)
        }
    }
}
