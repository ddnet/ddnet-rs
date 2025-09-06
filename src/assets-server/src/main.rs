pub mod delete;
pub mod index_dir;
pub mod upload;

use std::{net::SocketAddr, num::NonZeroU32, sync::Arc};

use anyhow::anyhow;
use assets_base::AssetsIndex;
use axum::{Json, Router, extract::DefaultBodyLimit};
use base::hash::{fmt_hash, generate_hash_for};
use clap::{Parser, command};
use delete::asset_delete;
use image_utils::png::PngValidatorOptions;
use index_dir::IndexDir;
use tokio::sync::Mutex;
use tower_http::trace::TraceLayer;
use upload::{
    asset_upload,
    verify::{AllowedResource, AllowedResources},
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The port this server should listen on
    #[arg(short, long)]
    port: Option<u16>,
    /// Don't use any cached entry, basically forcing them to recreate them.
    #[arg(short, long)]
    no_cache: bool,
}

struct AssetUploadRouter {
    upload: Router,
    delete: Router,
}

struct AssetRouter {
    download: Router,
    upload: Option<AssetUploadRouter>,
}

impl AssetRouter {
    fn merge(self, other: Self) -> Self {
        Self {
            download: self.download.merge(other.download),
            upload: match (self.upload, other.upload) {
                (None, None) => None,
                (None, Some(r)) | (Some(r), None) => Some(r),
                (Some(r1), Some(r2)) => Some(AssetUploadRouter {
                    upload: r1.upload.merge(r2.upload),
                    delete: r1.delete.merge(r2.delete),
                }),
            },
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv()?;
    if std::env::var("RUST_LOG").is_err() {
        unsafe { std::env::set_var("RUST_LOG", "info") };
    }
    env_logger::init();

    let args = Args::parse();

    let port = args.port.unwrap_or(3002);
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;

    let upload_password = std::env::var("ASSETS_UPLOAD_PASSWORD").ok().map(Arc::new);

    if upload_password.is_none() {
        log::warn!(
            "No upload password given, uploading will be disabled. (ASSETS_UPLOAD_PASSWORD env var)"
        );
    }

    let write_lock = Arc::new(Mutex::new(()));
    let skin_limits = AllowedResource::Png(PngValidatorOptions {
        max_width: NonZeroU32::new(256).unwrap(),
        max_height: NonZeroU32::new(128).unwrap(),
        min_width: Some(NonZeroU32::new(256).unwrap()),
        min_height: Some(NonZeroU32::new(128).unwrap()),
        divisible_width: None,
        divisible_height: None,
    });
    let entities_limits = AllowedResource::Png(PngValidatorOptions {
        max_width: NonZeroU32::new(1024).unwrap(),
        max_height: NonZeroU32::new(1024).unwrap(),
        min_width: Some(NonZeroU32::new(1024).unwrap()),
        min_height: Some(NonZeroU32::new(1024).unwrap()),
        divisible_width: None,
        divisible_height: None,
    });
    let default_png = AllowedResource::Png(Default::default());
    let emoticons_limits = AllowedResource::Png(PngValidatorOptions {
        max_width: NonZeroU32::new(512).unwrap(),
        max_height: NonZeroU32::new(512).unwrap(),
        min_width: Some(NonZeroU32::new(512).unwrap()),
        min_height: Some(NonZeroU32::new(512).unwrap()),
        divisible_width: None,
        divisible_height: None,
    });
    let hud_limits = AllowedResource::Png(PngValidatorOptions {
        max_width: NonZeroU32::new(512).unwrap(),
        max_height: NonZeroU32::new(512).unwrap(),
        min_width: Some(NonZeroU32::new(512).unwrap()),
        min_height: Some(NonZeroU32::new(512).unwrap()),
        divisible_width: None,
        divisible_height: None,
    });
    let particles_limits = AllowedResource::Png(PngValidatorOptions {
        max_width: NonZeroU32::new(512).unwrap(),
        max_height: NonZeroU32::new(512).unwrap(),
        min_width: Some(NonZeroU32::new(512).unwrap()),
        min_height: Some(NonZeroU32::new(512).unwrap()),
        divisible_width: None,
        divisible_height: None,
    });
    let map_resources_img = AllowedResource::PngCategory {
        per_category: vec![
            (
                "tileset".to_string(),
                PngValidatorOptions {
                    max_width: NonZeroU32::new(1024).unwrap(),
                    max_height: NonZeroU32::new(1024).unwrap(),
                    min_width: Some(NonZeroU32::new(1024).unwrap()),
                    min_height: Some(NonZeroU32::new(1024).unwrap()),
                    divisible_width: None,
                    divisible_height: None,
                },
            ),
            ("img".to_string(), Default::default()),
        ]
        .into_iter()
        .collect(),
        fallback: None,
    };
    let map_resources_snd = AllowedResource::Ogg;
    let app = skins(
        args.no_cache,
        &upload_password,
        vec![
            AllowedResources::File(skin_limits.clone()),
            AllowedResources::Tar(vec![
                skin_limits,
                AllowedResource::Txt,
                AllowedResource::Ogg,
            ]),
        ],
        write_lock.clone(),
    )
    .await?
    .merge(
        entities(
            args.no_cache,
            &upload_password,
            vec![AllowedResources::File(entities_limits)],
            write_lock.clone(),
        )
        .await?,
    )
    .merge(
        ctfs(
            args.no_cache,
            &upload_password,
            vec![AllowedResources::Tar(vec![
                default_png.clone(),
                AllowedResource::Txt,
                AllowedResource::Ogg,
            ])],
            write_lock.clone(),
        )
        .await?,
    )
    .merge(
        emoticons(
            args.no_cache,
            &upload_password,
            vec![
                AllowedResources::File(emoticons_limits),
                AllowedResources::Tar(vec![
                    default_png.clone(),
                    AllowedResource::Txt,
                    AllowedResource::Ogg,
                ]),
            ],
            write_lock.clone(),
        )
        .await?,
    )
    .merge(
        flags(
            args.no_cache,
            &upload_password,
            vec![AllowedResources::Tar(vec![
                default_png.clone(),
                AllowedResource::Txt,
                AllowedResource::Ogg,
            ])],
            write_lock.clone(),
        )
        .await?,
    )
    .merge(
        freezes(
            args.no_cache,
            &upload_password,
            vec![AllowedResources::Tar(vec![
                default_png.clone(),
                AllowedResource::Txt,
                AllowedResource::Ogg,
            ])],
            write_lock.clone(),
        )
        .await?,
    )
    .merge(
        games(
            args.no_cache,
            &upload_password,
            vec![AllowedResources::Tar(vec![
                default_png.clone(),
                AllowedResource::Txt,
                AllowedResource::Ogg,
            ])],
            write_lock.clone(),
        )
        .await?,
    )
    .merge(
        hooks(
            args.no_cache,
            &upload_password,
            vec![AllowedResources::Tar(vec![
                default_png.clone(),
                AllowedResource::Txt,
                AllowedResource::Ogg,
            ])],
            write_lock.clone(),
        )
        .await?,
    )
    .merge(
        huds(
            args.no_cache,
            &upload_password,
            vec![
                AllowedResources::File(hud_limits),
                AllowedResources::Tar(vec![
                    default_png.clone(),
                    AllowedResource::Txt,
                    AllowedResource::Ogg,
                ]),
            ],
            write_lock.clone(),
        )
        .await?,
    )
    .merge(
        ninjas(
            args.no_cache,
            &upload_password,
            vec![AllowedResources::Tar(vec![
                default_png.clone(),
                AllowedResource::Txt,
                AllowedResource::Ogg,
            ])],
            write_lock.clone(),
        )
        .await?,
    )
    .merge(
        particles(
            args.no_cache,
            &upload_password,
            vec![
                AllowedResources::File(particles_limits),
                AllowedResources::Tar(vec![
                    default_png.clone(),
                    AllowedResource::Txt,
                    AllowedResource::Ogg,
                ]),
            ],
            write_lock.clone(),
        )
        .await?,
    )
    .merge(
        weapons(
            args.no_cache,
            &upload_password,
            vec![AllowedResources::Tar(vec![
                default_png,
                AllowedResource::Txt,
                AllowedResource::Ogg,
            ])],
            write_lock.clone(),
        )
        .await?,
    )
    .merge(
        map_resources_images(
            args.no_cache,
            &upload_password,
            vec![AllowedResources::File(map_resources_img)],
            write_lock.clone(),
        )
        .await?,
    )
    .merge(
        map_resources_sounds(
            args.no_cache,
            &upload_password,
            vec![AllowedResources::File(map_resources_snd)],
            write_lock.clone(),
        )
        .await?,
    )
    .merge(
        editor_rules(
            args.no_cache,
            &upload_password,
            vec![AllowedResources::File(AllowedResource::Txt)],
            write_lock,
        )
        .await?,
    );
    let app = match app.upload {
        Some(r) => app.download.merge(r.upload.merge(r.delete)),
        None => app.download,
    };
    axum::serve(
        listener,
        // 3 MiB for uploads
        app.layer(DefaultBodyLimit::max(1024 * 1024 * 3))
            .layer(TraceLayer::new_for_http()),
    )
    .await?;

    Ok(())
}

async fn assets_generic(
    base_path: &str,
    ignore_cached: bool,
    upload_password: &Option<Arc<String>>,
    allowed_resources: Vec<AllowedResources>,
    write_lock: Arc<Mutex<()>>,
) -> anyhow::Result<AssetRouter> {
    assert!(
        base_path != "upload" && base_path != "delete",
        "upload & delete are reserved words and cannot be used."
    );
    // make sure there is an index file for this path
    let index_path = format!("{base_path}/index.json");
    if !tokio::fs::try_exists(&index_path).await.unwrap_or_default() || ignore_cached {
        let index = prepare_index_generic(base_path).await?;

        tokio::fs::write(index_path, serde_json::to_vec(&index)?).await?;
    }

    let index_dir = IndexDir::new(base_path).await?;
    let index = index_dir.index.clone();
    Ok(AssetRouter {
        download: Router::new().nest_service(&format!("/{base_path}"), index_dir),
        upload: upload_password.clone().map(|upload_password| {
            let base_path_upload = base_path.into();
            let write_lock_task = write_lock.clone();
            let index_task = index.clone();
            let upload_password_task = upload_password.clone();
            let upload = Router::new().route(
                &format!("/upload/{base_path}"),
                axum::routing::post(move |payload: Json<_>| {
                    asset_upload(
                        write_lock_task,
                        index_task,
                        base_path_upload,
                        upload_password_task,
                        allowed_resources,
                        payload,
                    )
                }),
            );
            let base_path_delete = base_path.into();
            let delete = Router::new().route(
                &format!("/delete/{base_path}"),
                axum::routing::post(move |payload: Json<_>| {
                    asset_delete(
                        write_lock,
                        index,
                        base_path_delete,
                        upload_password,
                        payload,
                    )
                }),
            );
            AssetUploadRouter { upload, delete }
        }),
    })
}

async fn skins(
    ignore_cached: bool,
    upload_password: &Option<Arc<String>>,
    allowed_resources: Vec<AllowedResources>,
    write_lock: Arc<Mutex<()>>,
) -> anyhow::Result<AssetRouter> {
    assets_generic(
        "skins",
        ignore_cached,
        upload_password,
        allowed_resources,
        write_lock,
    )
    .await
}

async fn entities(
    ignore_cached: bool,
    upload_password: &Option<Arc<String>>,
    allowed_resources: Vec<AllowedResources>,
    write_lock: Arc<Mutex<()>>,
) -> anyhow::Result<AssetRouter> {
    assets_generic(
        "entities",
        ignore_cached,
        upload_password,
        allowed_resources,
        write_lock,
    )
    .await
}

async fn ctfs(
    ignore_cached: bool,
    upload_password: &Option<Arc<String>>,
    allowed_resources: Vec<AllowedResources>,
    write_lock: Arc<Mutex<()>>,
) -> anyhow::Result<AssetRouter> {
    assets_generic(
        "ctfs",
        ignore_cached,
        upload_password,
        allowed_resources,
        write_lock,
    )
    .await
}

async fn emoticons(
    ignore_cached: bool,
    upload_password: &Option<Arc<String>>,
    allowed_resources: Vec<AllowedResources>,
    write_lock: Arc<Mutex<()>>,
) -> anyhow::Result<AssetRouter> {
    assets_generic(
        "emoticons",
        ignore_cached,
        upload_password,
        allowed_resources,
        write_lock,
    )
    .await
}

async fn flags(
    ignore_cached: bool,
    upload_password: &Option<Arc<String>>,
    allowed_resources: Vec<AllowedResources>,
    write_lock: Arc<Mutex<()>>,
) -> anyhow::Result<AssetRouter> {
    assets_generic(
        "flags",
        ignore_cached,
        upload_password,
        allowed_resources,
        write_lock,
    )
    .await
}

async fn freezes(
    ignore_cached: bool,
    upload_password: &Option<Arc<String>>,
    allowed_resources: Vec<AllowedResources>,
    write_lock: Arc<Mutex<()>>,
) -> anyhow::Result<AssetRouter> {
    assets_generic(
        "freezes",
        ignore_cached,
        upload_password,
        allowed_resources,
        write_lock,
    )
    .await
}

async fn games(
    ignore_cached: bool,
    upload_password: &Option<Arc<String>>,
    allowed_resources: Vec<AllowedResources>,
    write_lock: Arc<Mutex<()>>,
) -> anyhow::Result<AssetRouter> {
    assets_generic(
        "games",
        ignore_cached,
        upload_password,
        allowed_resources,
        write_lock,
    )
    .await
}

async fn hooks(
    ignore_cached: bool,
    upload_password: &Option<Arc<String>>,
    allowed_resources: Vec<AllowedResources>,
    write_lock: Arc<Mutex<()>>,
) -> anyhow::Result<AssetRouter> {
    assets_generic(
        "hooks",
        ignore_cached,
        upload_password,
        allowed_resources,
        write_lock,
    )
    .await
}

async fn huds(
    ignore_cached: bool,
    upload_password: &Option<Arc<String>>,
    allowed_resources: Vec<AllowedResources>,
    write_lock: Arc<Mutex<()>>,
) -> anyhow::Result<AssetRouter> {
    assets_generic(
        "huds",
        ignore_cached,
        upload_password,
        allowed_resources,
        write_lock,
    )
    .await
}

async fn ninjas(
    ignore_cached: bool,
    upload_password: &Option<Arc<String>>,
    allowed_resources: Vec<AllowedResources>,
    write_lock: Arc<Mutex<()>>,
) -> anyhow::Result<AssetRouter> {
    assets_generic(
        "ninjas",
        ignore_cached,
        upload_password,
        allowed_resources,
        write_lock,
    )
    .await
}

async fn particles(
    ignore_cached: bool,
    upload_password: &Option<Arc<String>>,
    allowed_resources: Vec<AllowedResources>,
    write_lock: Arc<Mutex<()>>,
) -> anyhow::Result<AssetRouter> {
    assets_generic(
        "particles",
        ignore_cached,
        upload_password,
        allowed_resources,
        write_lock,
    )
    .await
}

async fn weapons(
    ignore_cached: bool,
    upload_password: &Option<Arc<String>>,
    allowed_resources: Vec<AllowedResources>,
    write_lock: Arc<Mutex<()>>,
) -> anyhow::Result<AssetRouter> {
    assets_generic(
        "weapons",
        ignore_cached,
        upload_password,
        allowed_resources,
        write_lock,
    )
    .await
}

async fn map_resources_images(
    ignore_cached: bool,
    upload_password: &Option<Arc<String>>,
    allowed_resources: Vec<AllowedResources>,
    write_lock: Arc<Mutex<()>>,
) -> anyhow::Result<AssetRouter> {
    assets_generic(
        "map/resources/images",
        ignore_cached,
        upload_password,
        allowed_resources,
        write_lock,
    )
    .await
}

async fn map_resources_sounds(
    ignore_cached: bool,
    upload_password: &Option<Arc<String>>,
    allowed_resources: Vec<AllowedResources>,
    write_lock: Arc<Mutex<()>>,
) -> anyhow::Result<AssetRouter> {
    assets_generic(
        "map/resources/sounds",
        ignore_cached,
        upload_password,
        allowed_resources,
        write_lock,
    )
    .await
}

async fn editor_rules(
    ignore_cached: bool,
    upload_password: &Option<Arc<String>>,
    allowed_resources: Vec<AllowedResources>,
    write_lock: Arc<Mutex<()>>,
) -> anyhow::Result<AssetRouter> {
    assets_generic(
        "editor/rules",
        ignore_cached,
        upload_password,
        allowed_resources,
        write_lock,
    )
    .await
}

async fn prepare_index_generic(base_path: &str) -> anyhow::Result<AssetsIndex> {
    let mut res: AssetsIndex = Default::default();

    let mut files = tokio::fs::read_dir(base_path)
        .await
        .map_err(|err| anyhow!("can't dir find {base_path:?}: {err}"))?;

    while let Some(file) = files.next_entry().await? {
        anyhow::ensure!(
            file.metadata().await?.is_file(),
            "only files are allowed as assets files currently."
        );
        let path = file.path();

        // ignore all json files for now
        if path.extension().is_some_and(|ext| ext.eq("json")) {
            continue;
        }

        let file_name = path
            .file_stem()
            .ok_or_else(|| anyhow!("Only files with proper names are allowed"))?
            .to_string_lossy()
            .to_string();
        let file_ext = path
            .extension()
            .ok_or_else(|| anyhow!("Files need proper file endings"))?
            .to_string_lossy()
            .to_string();

        let file = tokio::fs::read(&path)
            .await
            .map_err(|err| anyhow!("can't find {path:?}: {err}"))?;

        let hash = generate_hash_for(&file);

        anyhow::ensure!(
            !file_name.ends_with(&format!("_{}", fmt_hash(&hash))),
            "Only files without their hashes are allowed."
        );

        res.insert(
            file_name,
            assets_base::AssetIndexEntry {
                ty: file_ext,
                hash,
                size: file.len() as u64,
            },
        );
    }

    Ok(res)
}
