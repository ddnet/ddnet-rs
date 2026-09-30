use crate::{config, relay};

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use std::{
    net::{SocketAddr, SocketAddrV4, SocketAddrV6},
    path::PathBuf,
};

#[derive(Parser)]
#[command(about = "Authenticated DDNet QUIC reverse proxy")]
struct Cli {
    /// Relative directory within the game server's automatic config storage.
    #[arg(long, global = true, default_value = "proxy", value_parser = relative_config_dir)]
    config_dir: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Replace the proxy key/config and trust list.
    Init {
        /// Preserve existing config and add its hash to the existing trust list.
        #[arg(long)]
        keep: bool,
        #[arg(long, default_value = "0.0.0.0:8310")]
        listen_game_v4: SocketAddrV4,
        #[arg(long, default_value = "[::]:8311")]
        listen_game_v6: SocketAddrV6,
        /// Backend HTTPS control endpoint.
        #[arg(long, default_value = "127.0.0.1:8315")]
        backend_s2s: SocketAddr,
        /// Backend server's SHA-256 SPKI fingerprint. Required before running.
        /// Set sv.private_key_file on the server to keep its hash stable.
        #[arg(long)]
        backend_hash: String,
    },
    /// Load configuration and relay client connections to the backend.
    Run,
    /// Print this proxy's public-key hash only.
    Export,
    /// Add a public-key hash to the trust list.
    Import { hash: String },
}

pub fn run(legacy: bool) -> anyhow::Result<()> {
    let command = Cli::command();
    let command = if legacy {
        command
            .name("legacy-server-proxy")
            .about("Legacy DDNet UDP frontend for the authenticated reverse proxy")
    } else {
        command
    };
    let args = Cli::from_arg_matches(&command.get_matches())?;
    let rt = base_io::io::create_runtime();
    let fs = base_fs::filesys::FileSystem::new(&rt, "org", "", "DDNet-Rs-Alpha", "DDNet-Accounts")?;
    if legacy && matches!(args.command, Command::Run) {
        let config = rt.block_on(config::Config::load(&fs, &args.config_dir))?;
        let handle = rt.handle().clone();
        let socket_rt = base_io::runtime::IoRuntime::new(rt);
        return crate::legacy::run(config, std::sync::Arc::new(fs), &handle, &socket_rt);
    }
    rt.block_on(async move {
        match args.command {
            Command::Init {
                keep,
                listen_game_v4,
                listen_game_v6,
                backend_s2s,
                backend_hash,
            } => {
                config::init(
                    &fs,
                    &args.config_dir,
                    config::InitOptions {
                        listen_game_v4,
                        listen_game_v6,
                        backend_hash,
                        backend_s2s,
                    },
                    keep,
                )
                .await?;
                println!(
                    "config: {}",
                    args.config_dir.join(config::CONFIG_FILE).display()
                );
                println!(
                    "trusted hashes: {}",
                    args.config_dir.join(config::HASH_FILE).display()
                );
                println!(
                    "public-key hash: {}",
                    base::hash::fmt_hash(
                        &config::Config::load(&fs, &args.config_dir).await?.hash()?
                    )
                );
            }
            Command::Run => {
                relay::run(
                    config::Config::load(&fs, &args.config_dir).await?,
                    game_base::server_list_urls::load(&fs).await,
                )
                .await?
            }
            Command::Export => println!(
                "{}",
                base::hash::fmt_hash(&config::Config::load(&fs, &args.config_dir).await?.hash()?)
            ),
            Command::Import { hash } => {
                config::import_hash(&fs, &args.config_dir, config::parse_hash(&hash)?).await?;
            }
        }
        Ok(())
    })
}

fn relative_config_dir(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if path.components().any(|component| {
        !matches!(
            component,
            std::path::Component::Normal(_) | std::path::Component::CurDir
        )
    }) {
        return Err("config directory must be relative to storage without '..'".into());
    }
    Ok(path)
}
