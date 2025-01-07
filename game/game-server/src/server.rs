use std::{
    collections::{BTreeMap, HashMap},
    fmt::Debug,
    net::IpAddr,
    num::NonZeroUsize,
    ops::Range,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc, Weak},
    time::Duration,
};

use anyhow::anyhow;
use base::{
    hash::{fmt_hash, generate_hash_for, Hash},
    linked_hash_map_view::{FxLinkedHashMap, FxLinkedHashSet},
    network_string::{NetworkReducedAsciiString, NetworkString},
    system::{System, SystemTimeInterface},
};
use base_fs::filesys::FileSystem;
use base_http::http::HttpClient;
use base_io::{
    io::Io,
    runtime::{IoRuntime, IoRuntimeTask},
};
use base_io_traits::http_traits::HttpClientInterface;
use command_parser::parser::{self, CommandArg, CommandArgType, CommandType, ParserCache, Syn};
use config::{config::ConfigEngine, traits::ConfigInterface};
use ddnet_account_client_http_fs::{
    cert_downloader::CertsDownloader, client::ClientHttpTokioFs, fs::Fs,
};
use ddnet_accounts_shared::game_server::user_id::{UserId, VerifyingKey};
use demo::recorder::{DemoRecorder, DemoRecorderCreateProps, DemoRecorderCreatePropsBase};
use ed25519_dalek::SigningKey;
use either::Either;
use game_config::config::{ConfigDebug, ConfigGame, ConfigServer, ConfigServerDatabase};
use game_database::{
    dummy::DummyDb,
    traits::{DbInterface, DbKind, DbKindExtra},
};
use game_database_backend::GameDbBackend;
use game_state_wasm::game::state_wasm_manager::GameStateWasmManager;
use http_accounts::http::AccountHttp;
use master_server_types::response::RegisterResponse;
use network::network::{
    connection::NetworkConnectionId,
    connection_ban::ConnectionBans,
    connection_limit::MaxConnections,
    connection_per_ip::ConnectionLimitPerIp,
    errors::{BanType, Banned, KickType},
    event::{NetworkEvent, NetworkEventDisconnect},
    networks::Networks,
    packet_compressor::DefaultNetworkPacketCompressor,
    packet_dict::ZstdNetworkDictTrainer,
    plugins::{NetworkPluginConnection, NetworkPluginPacket, NetworkPlugins},
    quinn_network::QuinnNetworks,
    types::{
        NetworkInOrderChannel, NetworkServerCertAndKey, NetworkServerCertMode,
        NetworkServerInitOptions,
    },
};
use pool::{datatypes::PoolFxLinkedHashMap, mt_datatypes::PoolCow, pool::Pool};
use rand::RngCore;
use sql::database::{Database, DatabaseDetails};
use vanilla::{
    command_chain::{Command, CommandChain},
    sql::account_info::AccountInfo,
};
use x509_cert::der::Encode;

use crate::{
    auto_map_votes::AutoMapVotes,
    client::{
        ClientSnapshotForDiff, ClientSnapshotStorage, Clients, ServerClient, ServerClientPlayer,
        ServerNetworkClient, ServerNetworkQueuedClient,
    },
    map_votes::{MapVotes, ServerMapVotes},
    network_plugins::{accounts_only::AccountsOnly, cert_ban::CertBans},
    rcon::{Rcon, ServerRconCommand},
    server_game::{
        ClientAuth, ServerExtraVoteInfo, ServerGame, ServerVote, RESERVED_DDNET_NAMES,
        RESERVED_VANILLA_NAMES,
    },
};

use game_base::{
    game_types::{is_next_tick, time_until_tick},
    local_server_info::{LocalServerConnectInfo, LocalServerInfo, LocalServerState, ServerDbgGame},
    network::{
        messages::{
            AddLocalPlayerResponseError, MsgClChatMsg, MsgClLoadVotes, MsgClReadyResponse,
            MsgClReadyResponseError, MsgClSnapshotAck, MsgSvAddLocalPlayerResponse, MsgSvChatMsg,
            MsgSvServerInfo, PlayerInputChainable,
        },
        types::chat::{ChatPlayerInfo, NetChatMsg, NetChatMsgPlayerChannel},
    },
    server_browser::{
        ServerBrowserInfo, ServerBrowserInfoMap, ServerBrowserPlayer, ServerBrowserSkin,
    },
};

use game_interface::{
    account_info,
    chat_commands::ClientChatCommand,
    client_commands::ClientCommand,
    events::EventClientInfo,
    interface::{GameStateCreateOptions, GameStateInterface, MAX_MAP_NAME_LEN},
    rcon_commands::{AuthLevel, ExecRconCommand, RconCommand, RconCommands},
    tick_result::TickEvent,
    types::{
        game::{GameEntityId, GameTickType},
        id_types::PlayerId,
        input::{CharacterInput, CharacterInputInfo},
        network_stats::PlayerNetworkStats,
        player_info::{
            AccountId, PlayerBanReason, PlayerClientInfo, PlayerDropReason, PlayerKickReason,
            PlayerUniqueId,
        },
        snapshot::SnapshotClientInfo,
    },
    vote_commands::{VoteCommand, VoteCommandResultEvent},
    votes::{
        MapVote, MapVoteDetails, MapVoteKey, MiscVote, MiscVoteKey, VoteIdentifierType, VoteState,
        VoteType, Voted, MAX_CATEGORY_NAME_LEN,
    },
};

use game_network::{
    game_event_generator::{GameEventGenerator, GameEvents},
    messages::{
        ClientToServerMessage, ClientToServerPlayerMessage, MsgSvInputAck, MsgSvLoadVotes,
        MsgSvStartVoteResult, ServerToClientMessage,
    },
};

#[derive(Clone)]
pub struct AccountDb {
    kind: DbKind,
    shared: Arc<ddnet_account_game_server::shared::Shared>,
    info: AccountInfo,
}

type DbSetup = (
    Option<Arc<Database>>,
    Arc<dyn DbInterface>,
    Option<AccountDb>,
);

enum GameServerDbAccount {
    Rename {
        account_id: Option<AccountId>,
        con_id: NetworkConnectionId,
        rename_result: Result<NetworkReducedAsciiString<32>, NetworkString<1024>>,
    },
    Info {
        con_id: NetworkConnectionId,
        account_details: Result<account_info::AccountInfo, NetworkString<1024>>,
    },
    AutoLogin {
        user_id: UserId,
        new_account_was_created: bool,
    },
}

enum GameServerDb {
    Account(GameServerDbAccount),
}

type ReponsesAndCmds = (Vec<Result<String, String>>, Vec<parser::Command>);

pub struct Server {
    pub clients: Clients,
    pub player_count_of_all_clients: usize,

    max_players_all_clients: usize,
    accounts_only: bool,

    rcon_chain: CommandChain<ServerRconCommand>,
    cache: ParserCache,

    // network
    network: QuinnNetworks,
    connection_bans: Arc<ConnectionBans>,

    is_open: Arc<AtomicBool>,

    has_new_events_server: Arc<AtomicBool>,
    game_event_generator_server: Arc<GameEventGenerator<ClientToServerMessage<'static>>>,

    game_server: ServerGame,

    config_game: ConfigGame,
    // for master server register
    server_port_v4: u16,
    server_port_v6: u16,
    thread_pool: Arc<rayon::ThreadPool>,
    io: Io,
    http_v6: Option<Arc<HttpClient>>,

    sys: System,

    last_tick_time: Duration,
    last_register_time: Option<Duration>,
    register_task: Option<IoRuntimeTask<()>>,
    last_register_serial: u32,

    last_network_stats_time: Duration,

    shared_info: Weak<LocalServerInfo>,

    // for server register
    cert_sha256_fingerprint: Hash,

    // rcon
    rcon: Rcon,

    // server side demos
    demo_recorder: Option<DemoRecorder>,

    // votes
    map_votes: ServerMapVotes,
    map_votes_hash: Hash,
    misc_votes: BTreeMap<NetworkString<MAX_CATEGORY_NAME_LEN>, BTreeMap<MiscVoteKey, MiscVote>>,
    misc_votes_hash: Hash,

    // database
    db: Option<Arc<Database>>,
    game_db: Arc<dyn DbInterface>,
    db_requests: Vec<IoRuntimeTask<GameServerDb>>,
    db_requests_helper: Vec<IoRuntimeTask<GameServerDb>>,

    accounts: Option<AccountDb>,
    account_server_certs_downloader: Option<Arc<CertsDownloader>>,
    // intentionally unused
    _account_server_cert_downloader_task: Option<IoRuntimeTask<()>>,

    // pools
    player_ids_pool: Pool<FxLinkedHashSet<PlayerId>>,
    player_snap_pool: Pool<Vec<u8>>,
    player_network_stats_pool: Pool<FxLinkedHashMap<PlayerId, PlayerNetworkStats>>,

    // helpers
    input_deser: Pool<Vec<u8>>,
}

impl Server {
    fn config_ty_to_db_kind(ty: &str) -> anyhow::Result<DbKind> {
        Ok(match ty {
            "mysql" => DbKind::MySql(DbKindExtra::Main),
            "sqlite" => DbKind::Sqlite(DbKindExtra::Main),
            "mysql_backup" => DbKind::MySql(DbKindExtra::Backup),
            "sqlite_backup" => DbKind::Sqlite(DbKindExtra::Backup),
            _ => {
                return Err(anyhow!("Database of type: {ty} is not allowed/supported"));
            }
        })
    }

    pub async fn db_setup(config_db: &ConfigServerDatabase) -> anyhow::Result<Arc<Database>> {
        Ok(Arc::new(
            Database::new(
                config_db
                    .connections
                    .iter()
                    .map(|(ty, con)| {
                        anyhow::Ok((
                            Self::config_ty_to_db_kind(ty.as_str())?,
                            DatabaseDetails {
                                host: con.host.clone(),
                                port: con.port,
                                database: con.database.clone(),
                                username: con.username.clone(),
                                password: con.password.clone(),
                                ca_cert_path: con.ca_cert_path.clone(),
                                connection_count: con.connection_count as usize,
                            },
                        ))
                    })
                    .collect::<anyhow::Result<HashMap<_, _>>>()?,
            )
            .await?,
        ))
    }

    pub fn db_setup_task(
        io_rt: &IoRuntime,
        config_db: ConfigServerDatabase,
    ) -> IoRuntimeTask<DbSetup> {
        io_rt.spawn(async move {
            if !config_db.connections.is_empty() {
                let db = Self::db_setup(&config_db).await?;

                let game_db: Arc<dyn DbInterface> = Arc::new(GameDbBackend::new(db.clone())?);

                let accounts = if !config_db.enable_accounts.is_empty() {
                    let kind = Self::config_ty_to_db_kind(&config_db.enable_accounts)?;
                    let pool = db.pools.get(&kind).ok_or_else(|| {
                        anyhow!(
                            "database connection was not intiailized for {:?}.",
                            config_db.enable_accounts
                        )
                    })?;
                    ddnet_account_game_server::setup::setup(pool).await?;

                    Some(AccountDb {
                        kind,
                        shared: ddnet_account_game_server::prepare::prepare(pool).await?,
                        info: AccountInfo::new(game_db.clone(), Some(kind)).await?,
                    })
                } else {
                    None
                };

                Ok((Some(db), game_db, accounts))
            } else {
                let game_db: Arc<dyn DbInterface> = Arc::new(DummyDb);
                Ok((None, game_db, Default::default()))
            }
        })
    }

    fn read_mod_config(io: &Io, mod_name: &str) -> IoRuntimeTask<Vec<u8>> {
        let mod_name = mod_name.to_string();
        let fs = io.fs.clone();
        io.rt.spawn(async move {
            let config_mod = fs
                .read_file(format!("config/{mod_name}.json").as_ref())
                .await?;

            Ok(config_mod)
        })
    }

    fn config_physics_mod_name(config_game: &ConfigGame) -> String {
        let mut mod_name = config_game.sv.game_mod.clone();
        if RESERVED_VANILLA_NAMES.contains(&mod_name.as_str()) {
            mod_name = "vanilla".to_string();
        } else if RESERVED_DDNET_NAMES.contains(&mod_name.as_str()) {
            mod_name = "ddnet".to_string();
        }
        mod_name
    }

    fn config_render_mod_name(config_game: &ConfigGame) -> (String, Vec<u8>, bool) {
        let mut mod_name = config_game.sv.render_mod.name.clone();
        if RESERVED_VANILLA_NAMES.contains(&mod_name.as_str()) {
            mod_name = "vanilla".to_string();
        } else if RESERVED_DDNET_NAMES.contains(&mod_name.as_str()) {
            mod_name = "ddnet".to_string();
        }
        (
            mod_name,
            config_game.sv.render_mod.hash.clone(),
            config_game.sv.render_mod.required,
        )
    }

    fn new_rcon_cmd_chain() -> CommandChain<ServerRconCommand> {
        let mut rcon_cmds = vec![
            (
                "ban_id".try_into().unwrap(),
                Command {
                    rcon: RconCommand {
                        args: vec![CommandArg {
                            ty: CommandArgType::Number,
                            user_ty: Some("PLAYER_ID".try_into().unwrap()),
                        }],
                        description: "Ban a user with the given player id".try_into().unwrap(),
                        usage: "ban_id <player_id>".try_into().unwrap(),
                    },
                    cmd: ServerRconCommand::BanId,
                },
            ),
            (
                "kick_id".try_into().unwrap(),
                Command {
                    rcon: RconCommand {
                        args: vec![CommandArg {
                            ty: CommandArgType::Number,
                            user_ty: Some("PLAYER_ID".try_into().unwrap()),
                        }],
                        description: "Kick a user with the given player id".try_into().unwrap(),
                        usage: "kick_id <player_id>".try_into().unwrap(),
                    },
                    cmd: ServerRconCommand::KickId,
                },
            ),
            (
                "status".try_into().unwrap(),
                Command {
                    rcon: RconCommand {
                        args: Default::default(),
                        description:
                            "List information about this player such as the connected clients"
                                .try_into()
                                .unwrap(),
                        usage: "status".try_into().unwrap(),
                    },
                    cmd: ServerRconCommand::Status,
                },
            ),
            (
                "record_demo".try_into().unwrap(),
                Command {
                    rcon: RconCommand {
                        args: Default::default(),
                        description: "Start to record a server side demo.".try_into().unwrap(),
                        usage: "record_demo".try_into().unwrap(),
                    },
                    cmd: ServerRconCommand::RecordDemo,
                },
            ),
            (
                "exec".try_into().unwrap(),
                Command {
                    rcon: RconCommand {
                        args: vec![CommandArg {
                            ty: CommandArgType::Text,
                            user_ty: None,
                        }],
                        description: "Executes a file of command lines.".try_into().unwrap(),
                        usage: "exec <file_path>".try_into().unwrap(),
                    },
                    cmd: ServerRconCommand::Exec,
                },
            ),
            (
                "load".try_into().unwrap(),
                Command {
                    rcon: RconCommand {
                        args: vec![CommandArg {
                            ty: CommandArgType::Text,
                            user_ty: None,
                        }],
                        description: "Loads a json config file.".try_into().unwrap(),
                        usage: "load <file_path>".try_into().unwrap(),
                    },
                    cmd: ServerRconCommand::Load,
                },
            ),
        ];

        config::parsing::parse_conf_values_as_str_list(
            "sv".into(),
            &mut |add, _| {
                rcon_cmds.push((
                    add.name.try_into().unwrap(),
                    Command {
                        rcon: RconCommand {
                            args: add.args,
                            usage: add.usage.as_str().try_into().unwrap(),
                            description: add.description.as_str().try_into().unwrap(),
                        },
                        cmd: ServerRconCommand::ConfVariable,
                    },
                ));
            },
            ConfigServer::conf_value(),
            "".into(),
            Default::default(),
        );

        CommandChain::new(rcon_cmds.into_iter().collect())
    }

    fn handle_load_config(
        config: &mut ConfigGame,
        io: &Io,
        cmd: parser::Command,
    ) -> anyhow::Result<String> {
        let Syn::Text(file_path) = &cmd.args[0].0 else {
            panic!("Command parser returned a non requested command arg");
        };
        *config = game_config_fs::fs::load_in(&io.clone().into(), file_path.as_ref())?;
        Ok(format!("New config file was loaded from {file_path}"))
    }

    fn handle_exec(
        config: &mut ConfigGame,
        io: &Io,
        rcon_chain: &CommandChain<ServerRconCommand>,
        cmd: parser::Command,
    ) -> anyhow::Result<ReponsesAndCmds> {
        let Syn::Text(file_path) = &cmd.args[0].0 else {
            panic!("Command parser returned a non requested command arg");
        };
        let file_path: PathBuf = file_path.into();
        let fs = io.fs.clone();
        let cmds_file = io
            .rt
            .spawn(async move {
                fs.read_file(&file_path)
                    .await
                    .map_err(|err| anyhow!(err))
                    .and_then(|file| String::from_utf8(file).map_err(|err| anyhow!(err)))
            })
            .get_storage()?;

        Ok(Self::handle_config_cmd_lines(
            config,
            io,
            rcon_chain,
            cmds_file.lines().map(|s| s.to_string()).collect(),
        ))
    }

    /// Handles config variable from the cmd lines.
    /// Returns the reponses of the cmds and
    /// returns all cmds that are not related to config variables.
    fn handle_config_cmd_lines(
        config: &mut ConfigGame,
        io: &Io,
        rcon_chain: &CommandChain<ServerRconCommand>,
        lines: Vec<String>,
    ) -> ReponsesAndCmds {
        let mut remaining_cmds = Vec::default();
        let mut responses = Vec::default();
        for line in lines {
            let cmds =
                command_parser::parser::parse(&line, &rcon_chain.parser, &mut Default::default());

            for cmd in cmds {
                let handle_cmd = || match cmd {
                    CommandType::Full(cmd) => {
                        let Some(chain_cmd) = rcon_chain.cmds.get(&cmd.ident) else {
                            return Err(anyhow!("Command {} not found", cmd.ident));
                        };

                        match chain_cmd.cmd {
                            ServerRconCommand::ConfVariable => {
                                Self::handle_config_variable_cmd(&cmd, config).map(|msg| {
                                    format!("Updated value for {}: {}", cmd.cmd_text, msg)
                                })
                            }
                            ServerRconCommand::Exec => {
                                match Self::handle_exec(config, io, rcon_chain, cmd) {
                                    Ok((mut res, mut res_remaining_cmds)) => {
                                        responses.append(&mut res);
                                        remaining_cmds.append(&mut res_remaining_cmds);
                                        Ok("".to_string())
                                    }
                                    Err(err) => Err(err),
                                }
                            }
                            ServerRconCommand::Load => Self::handle_load_config(config, io, cmd),
                            _ => {
                                remaining_cmds.push(cmd);
                                Ok("".to_string())
                            }
                        }
                    }
                    CommandType::Partial(cmd) => {
                        let Some(cmd) = cmd.ref_cmd_partial() else {
                            return Err(anyhow!("This command was invalid: {cmd}"));
                        };
                        let Some(chain_cmd) = rcon_chain.cmds.get(&cmd.ident) else {
                            return Err(anyhow!("Command {} not found", cmd.ident));
                        };

                        if let ServerRconCommand::ConfVariable = chain_cmd.cmd {
                            Self::handle_config_variable_cmd(cmd, config)
                                .map(|msg| format!("Updated value for {}: {}", cmd.cmd_text, msg))
                        } else {
                            Err(anyhow!("Failed to handle config variable: {cmd}"))
                        }
                    }
                };

                let cmd_res = handle_cmd();
                responses.push(cmd_res.map_err(|err| err.to_string()));
            }
        }
        (responses, remaining_cmds)
    }

    pub fn new(
        sys: System,
        is_open: Arc<AtomicBool>,
        cert_and_private_key: (x509_cert::Certificate, SigningKey),
        shared_info: Arc<LocalServerInfo>,
        port_v4: u16,
        port_v6: u16,
        config_engine: ConfigEngine,
        config_game: ConfigGame,
        thread_pool: Arc<rayon::ThreadPool>,
        io: Io,
        rcon_chain: CommandChain<ServerRconCommand>,
    ) -> anyhow::Result<Self> {
        let config_db = config_game.sv.db.clone();
        let accounts_enabled = !config_db.enable_accounts.is_empty();
        let task = Self::db_setup_task(&io.rt, config_db);
        let auto_map_votes = (shared_info.is_internal_server || config_game.sv.auto_map_votes)
            .then(|| {
                let fs = io.fs.clone();
                io.rt.spawn(async move { AutoMapVotes::new(&fs).await })
            });
        let map_votes_file = {
            let fs = io.fs.clone();
            io.rt.spawn(async move { MapVotes::new(&fs).await })
        };

        let fs = io.fs.clone();
        let zstd_dicts = io.rt.spawn(async move {
            let client_send = fs.read_file("dict/client_send".as_ref()).await;
            let server_send = fs.read_file("dict/server_send".as_ref()).await;

            Ok(client_send.and_then(|c| server_send.map(|s| (c, s)))?)
        });

        // load mod config
        let physics_mod_name = Self::config_physics_mod_name(&config_game);
        let (render_mod_name, render_mod_hash, render_mod_required) =
            Self::config_render_mod_name(&config_game);
        let config_mod_task = Self::read_mod_config(&io, &physics_mod_name);

        let http = io.http.clone();
        let path = io.fs.get_secure_path();
        let http_accounts = io
            .rt
            .spawn(async move {
                Ok(Arc::new(ClientHttpTokioFs {
                    http: vec![Arc::new(AccountHttp::new_with_url(
                        "https://pg.ddnet.org:5555/".try_into().unwrap(),
                        http.clone(),
                    ))],
                    cur_http: Default::default(),
                    fs: Fs::new(path).await?,
                }))
            })
            .get_storage()?;
        let http_accounts_clone = http_accounts.clone();
        let account_certs_downloader = io.rt.spawn(async move {
            if accounts_enabled {
                CertsDownloader::new(http_accounts_clone).await
            } else {
                Err(anyhow!("Accounts are disabled."))
            }
        });

        let has_new_events_server = Arc::new(AtomicBool::new(false));
        let game_event_generator_server =
            Arc::new(GameEventGenerator::new(has_new_events_server.clone()));

        let account_certs_downloader = account_certs_downloader.get_storage().ok();
        let accounts_only = config_game.sv.account_only && account_certs_downloader.is_some();

        let mut connection_plugins: Vec<Arc<dyn NetworkPluginConnection>> = vec![];
        let connection_bans = Arc::new(ConnectionBans::default());
        connection_plugins.push(connection_bans.clone());

        connection_plugins.push(Arc::new(MaxConnections::new(
            (config_game.sv.max_connections.clamp(1, u32::MAX) as u64)
                .try_into()
                .unwrap(),
        )));
        connection_plugins.push(Arc::new(ConnectionLimitPerIp::new(
            (config_game.sv.max_connections_per_ip.clamp(1, u32::MAX) as u64)
                .try_into()
                .unwrap(),
        )));

        if let Some(account_certs_downloader) = account_certs_downloader.as_ref() {
            if accounts_only {
                connection_plugins.push(Arc::new(AccountsOnly::new(
                    account_certs_downloader.clone(),
                )));
            }

            connection_plugins.push(Arc::new(CertBans::new(account_certs_downloader.clone())));
        }

        let mut packet_plugins: Vec<Arc<dyn NetworkPluginPacket>> = vec![];

        if config_game.sv.train_packet_dictionary {
            packet_plugins.push(Arc::new(ZstdNetworkDictTrainer::new(
                config_game.sv.train_packet_dictionary_max_size as usize,
            )));
        }

        if let Ok((client_send, server_send)) = zstd_dicts.get_storage() {
            packet_plugins.push(Arc::new(DefaultNetworkPacketCompressor::new_with_dict(
                server_send,
                client_send,
            )));
        } else {
            packet_plugins.push(Arc::new(DefaultNetworkPacketCompressor::new()));
        }

        let cert_sha256_fingerprint = cert_and_private_key
            .0
            .tbs_certificate
            .subject_public_key_info
            .fingerprint_bytes()?;

        let (network_server, _cert, sock_addrs, _notifer_server) = Networks::init_server(
            config_game.sv.bind_addr_v4.parse()?,
            config_game.sv.bind_addr_v6.parse()?,
            port_v4,
            port_v6,
            game_event_generator_server.clone(),
            NetworkServerCertMode::FromCertAndPrivateKey(Box::new(NetworkServerCertAndKey {
                cert: cert_and_private_key.0,
                private_key: cert_and_private_key.1,
            })),
            &sys,
            NetworkServerInitOptions::new()
                .with_max_thread_count(if shared_info.is_internal_server { 2 } else { 6 })
                .with_disable_retry_on_connect(
                    config_engine.net.disable_retry_on_connect || shared_info.is_internal_server,
                )
                .with_packet_capacity_and_size(
                    if shared_info.is_internal_server {
                        8
                    } else {
                        64
                    },
                    256,
                )
                //.with_ack_config(5, Duration::from_millis(50), 5 - 1)
                // since there are many packets, increase loss detection thresholds
                //.with_loss_detection_cfg(25, 2.0)
                .with_timeout(config_engine.net.timeout),
            NetworkPlugins {
                packet_plugins: Arc::new(packet_plugins),
                connection_plugins: Arc::new(connection_plugins),
            },
        )?;

        let (db, game_db, accounts) = task.get_storage()?;

        let account_server_cert_downloader_task = if let Some(account_certs_downloader) =
            account_certs_downloader.clone()
        {
            Some(
                io.rt
                    .spawn::<(), _>(async move { account_certs_downloader.download_task().await })
                    .abortable(),
            )
        } else {
            None
        };

        let map_votes: BTreeMap<_, _> =
            if let Some(Ok(votes)) = auto_map_votes.map(|task| task.get_storage()) {
                votes
                    .map_files
                    .into_iter()
                    .filter_map(|map| {
                        map.file_stem()
                            .and_then(|s| s.to_str().and_then(|s| s.try_into().ok()))
                            .map(|name| {
                                (
                                    MapVoteKey { name, hash: None },
                                    MapVote {
                                        thumbnail_resource: None,
                                        details: MapVoteDetails::None,
                                        is_default_map: true,
                                    },
                                )
                            })
                    })
                    .collect()
            } else {
                Default::default()
            };
        let map_votes: BTreeMap<_, _> = if map_votes.is_empty() {
            Default::default()
        } else {
            [("Auto".try_into().unwrap(), map_votes)]
                .into_iter()
                .collect()
        };
        let mut map_votes = ServerMapVotes {
            categories: map_votes,
            has_unfinished_map_votes: false,
        };

        match map_votes_file.get_storage() {
            Ok(map_votes_file) => {
                map_votes.categories.extend(map_votes_file.votes.categories);
                map_votes.has_unfinished_map_votes = map_votes_file.votes.has_unfinished_map_votes;
            }
            Err(err) => {
                log::info!("No map votes were loaded: {err}");
            }
        }

        let map_votes_hash = generate_hash_for(
            &bincode::serde::encode_to_vec(&map_votes, bincode::config::standard()).unwrap(),
        );

        let config_mod = config_mod_task.get_storage().ok();

        let rcon = Rcon::new(&io);

        // write local server info if required.
        {
            let mut state = shared_info.state.lock().unwrap();
            if let LocalServerState::Starting {
                server_cert_hash,
                thread,
            } = std::mem::take(&mut *state)
            {
                *state = LocalServerState::Ready {
                    connect_info: LocalServerConnectInfo {
                        sock_addr: sock_addrs[0],
                        dbg_games: Default::default(),
                        // share secret with client (if exists)
                        rcon_secret: rcon.rcon_secret,
                        server_cert_hash,
                    },
                    thread,
                    browser_info: None,
                };
            }
        }

        Ok(Self {
            clients: Clients::new(
                config_game.sv.max_players as usize,
                config_game.sv.max_players_per_client as usize,
            ),
            player_count_of_all_clients: 0,
            accounts_only,

            max_players_all_clients: config_game.sv.max_players as usize,

            rcon_chain,
            cache: Default::default(),

            network: network_server,
            connection_bans,

            is_open,

            has_new_events_server,
            game_event_generator_server,

            game_server: ServerGame::new(
                &config_game.sv.map.as_str().try_into().unwrap(),
                &physics_mod_name,
                &render_mod_name,
                &render_mod_hash.try_into().unwrap_or_default(),
                render_mod_required,
                config_mod,
                &thread_pool,
                &io,
                &game_db,
                accounts.as_ref().map(|a| a.kind),
                config_game.sv.spatial_chat,
                config_game.sv.download_server_port_v4,
                config_game.sv.download_server_port_v6,
            )?,

            last_tick_time: sys.time_get(),
            last_register_time: None,
            register_task: None,
            last_register_serial: 0,

            last_network_stats_time: sys.time_get(),

            sys,

            shared_info: Arc::downgrade(&shared_info),

            // for server register
            cert_sha256_fingerprint,

            // rcon
            rcon,

            // server side demo recorder
            demo_recorder: None,

            // votes
            map_votes,
            map_votes_hash,
            misc_votes: Default::default(),
            misc_votes_hash: generate_hash_for(&[]),

            // database
            db,
            game_db,
            db_requests: Default::default(),
            db_requests_helper: Default::default(),

            accounts,
            account_server_certs_downloader: account_certs_downloader,
            _account_server_cert_downloader_task: account_server_cert_downloader_task,

            player_ids_pool: Pool::with_sized(
                (config_game.sv.max_players as usize).min(512),
                || {
                    FxLinkedHashSet::with_capacity_and_hasher(
                        (config_game.sv.max_players_per_client as usize).min(8),
                        rustc_hash::FxBuildHasher,
                    )
                },
            ),
            player_snap_pool: Pool::with_capacity(2),
            player_network_stats_pool: Pool::with_capacity(
                (config_game.sv.max_players as usize).min(512),
            ),

            // helpers
            input_deser: Pool::with_capacity(3),

            thread_pool,
            io,
            http_v6: HttpClient::new_with_bind_addr("::0".parse().unwrap()).map(Arc::new),

            config_game,
            server_port_v4: sock_addrs[0].port(),
            server_port_v6: sock_addrs[1].port(),
        })
    }

    fn can_another_player_connect(&self) -> bool {
        self.player_count_of_all_clients + self.clients.network_clients.len()
            < self.max_players_all_clients
    }

    fn can_client_join_another_player(client: &ServerClient, config_game: &ConfigGame) -> bool {
        client.players.len() < config_game.sv.max_players_per_client as usize
    }

    fn can_client_player_id_join(client: &ServerClient, id: u64) -> bool {
        client.players.values().all(|p| p.id != id)
    }

    pub fn try_client_connect(
        &mut self,
        con_id: &NetworkConnectionId,
        timestamp: &Duration,
        ip: IpAddr,
        cert: Arc<x509_cert::Certificate>,
        network_stats: PlayerNetworkStats,
    ) {
        // check if the client can be part of the game
        if self.can_another_player_connect() {
            self.clients.network_clients.insert(
                *con_id,
                ServerNetworkClient::new(timestamp, ip, cert, network_stats),
            );

            // tell the client about all data required to join the server
            let server_info = MsgSvServerInfo {
                map: self.game_server.map.name.as_str().try_into().unwrap(),
                map_blake3_hash: self.game_server.map_blake3_hash,
                required_resources: self.game_server.required_resources.clone(),
                game_mod: self.game_server.game_mod.clone(),
                render_mod: self.game_server.render_mod.clone(),
                mod_config: self.game_server.game.info.config.clone(),
                resource_server_fallback: self.game_server.http_server.as_ref().map(|server| {
                    match ip {
                        IpAddr::V4(_) => server.port_v4,
                        IpAddr::V6(_) => server.port_v6,
                    }
                }),
                hint_start_camera_pos: self.game_server.game.get_client_camera_join_pos(),
                server_options: self.game_server.game.info.options.clone(),
                spatial_chat: self.config_game.sv.spatial_chat,
            };
            self.network.send_unordered_to(
                &ServerToClientMessage::ServerInfo {
                    info: server_info,
                    overhead: self.sys.time_get().saturating_sub(*timestamp),
                },
                con_id,
            );

            self.player_count_of_all_clients += 1;
        } else {
            // else add it to the network queue and inform it about that
            self.clients.network_queued_clients.insert(
                *con_id,
                ServerNetworkQueuedClient::new(
                    timestamp,
                    ip,
                    ClientAuth {
                        cert,
                        level: Default::default(),
                    },
                    network_stats,
                ),
            );

            self.network.send_unordered_to(
                &ServerToClientMessage::QueueInfo(
                    format!(
                        "The server is full.\nYou are queued at position: #{}",
                        self.clients.network_queued_clients.len()
                    )
                    .as_str()
                    .try_into()
                    .unwrap(),
                ),
                con_id,
            );
        }
    }

    fn drop_client_from_queue(
        &mut self,
        con_id: &NetworkConnectionId,
    ) -> Option<ServerNetworkQueuedClient> {
        let mut iter = self
            .clients
            .network_queued_clients
            .iter_at_key(con_id)
            .unwrap();
        iter.next();

        iter.enumerate().for_each(|(index, (net_id, _))| {
            self.network.send_unordered_to(
                &ServerToClientMessage::QueueInfo(
                    format!(
                        "The server is full.\nYou are queued at position: #{}",
                        index
                    )
                    .as_str()
                    .try_into()
                    .unwrap(),
                ),
                net_id,
            );
        });
        self.clients.network_queued_clients.remove(con_id)
    }

    pub fn client_disconnect(
        &mut self,
        con_id: &NetworkConnectionId,
        _reason: &str,
    ) -> Option<PoolFxLinkedHashMap<PlayerId, ServerClientPlayer>> {
        // find client in queued clients
        if self.clients.network_queued_clients.contains_key(con_id) {
            self.drop_client_from_queue(con_id);
            return None;
        }

        // else find in waiting clients, connect the waiting client
        let found = self.clients.network_clients.remove(con_id);
        if found.is_some() {
            self.player_count_of_all_clients -= 1;
            if !self.clients.network_queued_clients.is_empty() {
                let con_id_queue = *self.clients.network_queued_clients.front().unwrap().0;
                let timestamp_queue = self
                    .clients
                    .network_queued_clients
                    .front()
                    .unwrap()
                    .1
                    .connect_timestamp;
                let p = self.drop_client_from_queue(&con_id_queue).unwrap();
                self.try_client_connect(
                    &con_id_queue,
                    &timestamp_queue,
                    p.ip,
                    p.auth.cert,
                    p.network_stats,
                );
            }
            return None;
        }

        // else find in clients, connect one from queue if this client disconnected
        let found = self.clients.clients.remove(con_id);
        if let Some(p) = found {
            // update vote if nessecary
            if let Some(vote) = &mut self.game_server.cur_vote {
                if let Some(voted) = vote.participating_ip.remove(&p.ip) {
                    match voted {
                        Voted::Yes => vote.state.yes_votes -= 1,
                        Voted::No => vote.state.no_votes -= 1,
                    }
                }
                vote.state.allowed_to_vote_count = self.clients.allowed_to_vote_count() as u64;

                let vote_state = vote.state.clone();
                let started_at = vote.started_at;
                self.send_vote(Some(vote_state), started_at);
            }
            // update spatial world
            if let Some(spatial_world) = &mut self.game_server.spatial_world {
                spatial_world.on_client_drop(con_id);
            }

            self.player_count_of_all_clients -= p.players.len();
            for _ in 0..p.players.len() {
                if !self.clients.network_queued_clients.is_empty() {
                    let con_id_queue = *self.clients.network_queued_clients.front().unwrap().0;
                    let timestamp_queue = self
                        .clients
                        .network_queued_clients
                        .front()
                        .unwrap()
                        .1
                        .connect_timestamp;
                    let drop_player = self.drop_client_from_queue(&con_id_queue).unwrap();
                    self.try_client_connect(
                        &con_id_queue,
                        &timestamp_queue,
                        drop_player.ip,
                        drop_player.auth.cert,
                        drop_player.network_stats,
                    );
                }
            }
            return Some(p.players);
        }
        None
    }

    fn broadcast_in_order(
        &self,
        packet: ServerToClientMessage<'_>,
        channel: NetworkInOrderChannel,
    ) {
        self.clients.clients.keys().for_each(|send_con_id| {
            self.network.send_in_order_to(&packet, send_con_id, channel);
        });
    }

    fn send_vote(&self, vote_state: Option<VoteState>, start_time: Duration) {
        self.broadcast_in_order(
            ServerToClientMessage::Vote(vote_state.map(|mut vote_state| {
                vote_state.remaining_time = Duration::from_secs(25)
                    .saturating_sub(self.sys.time_get().saturating_sub(start_time));
                vote_state
            })),
            NetworkInOrderChannel::Custom(7013), // This number reads as "vote".
        )
    }

    fn add_player_for_client(
        &mut self,
        con_id: &NetworkConnectionId,
        player_info: PlayerClientInfo,
        is_additional_player: bool,
    ) -> Option<PlayerId> {
        if let Some(client) = self.clients.clients.get_mut(con_id) {
            let player_id = self.game_server.player_join(con_id, &player_info);
            client.players.insert(
                player_id,
                ServerClientPlayer {
                    input_storage: Default::default(),
                    id: player_info.id,
                },
            );
            if is_additional_player {
                self.player_count_of_all_clients += 1;
            }

            // if this is the first connect to the server, send a snapshot
            if client.players.len() == 1 {
                let mut client_player_ids_dummy = self.player_ids_pool.new();
                client_player_ids_dummy.insert(player_id);
                let snap_client = SnapshotClientInfo::ForPlayerIds(client_player_ids_dummy);
                let snap_id = client.snap_id;
                client.snap_id += 1;

                let snap = self.game_server.game.snapshot_for(snap_client);

                client.client_snap_storage.insert(
                    snap_id,
                    ClientSnapshotStorage {
                        snapshot: snap.to_vec(),
                        monotonic_tick: self.game_server.cur_monotonic_tick,
                    },
                );

                self.network.send_unordered_auto_to(
                    &ServerToClientMessage::Snapshot {
                        overhead_time: self.sys.time_get().saturating_sub(self.last_tick_time),
                        snapshot: snap,
                        diff_id: None,
                        snap_id_diffed: snap_id,
                        game_monotonic_tick_diff: self.game_server.cur_monotonic_tick,
                        as_diff: true,
                        input_ack: PoolCow::new_without_pool(),
                    },
                    con_id,
                );
            }

            Some(player_id)
        } else {
            None
        }
    }

    fn handle_player_msg(
        &mut self,
        con_id: &NetworkConnectionId,
        player_id: &PlayerId,
        player_msg: ClientToServerPlayerMessage,
    ) {
        let client = self.clients.clients.get_mut(con_id);
        if let Some(player) = client {
            if player.players.contains_key(player_id) {
                match player_msg {
                    ClientToServerPlayerMessage::Custom(_) => {
                        // ignore
                    }
                    ClientToServerPlayerMessage::RemLocalPlayer => {
                        if player.players.len() > 1 && player.players.remove(player_id).is_some() {
                            self.game_server
                                .player_drop(player_id, PlayerDropReason::Disconnect);
                        }
                    }
                    ClientToServerPlayerMessage::Chat(msg) => {
                        fn prepare_msg(msg: &str) -> String {
                            msg.trim_matches(char::is_whitespace)
                                .replace(|c: char| c.is_control(), "")
                        }
                        let mut handle_msg = |msg: &str, channel: NetChatMsgPlayerChannel| {
                            if !prepare_msg(msg).is_empty() {
                                if self
                                    .game_server
                                    .game
                                    .info
                                    .chat_commands
                                    .prefixes
                                    .contains(&msg.chars().next().unwrap())
                                {
                                    self.game_server.game.client_command(
                                        player_id,
                                        ClientCommand::Chat(ClientChatCommand {
                                            raw: msg
                                                .chars()
                                                .skip(1)
                                                .collect::<String>()
                                                .as_str()
                                                .try_into()
                                                .unwrap(),
                                        }),
                                    );
                                } else if let Some(own_char_info) =
                                    self.game_server.cached_character_infos.get(player_id)
                                {
                                    let msg = NetChatMsg {
                                        sender: ChatPlayerInfo {
                                            id: *player_id,
                                            name: own_char_info.info.name.clone(),
                                            skin: own_char_info.info.skin.clone(),
                                            skin_info: own_char_info.info.skin_info,
                                        },
                                        msg: msg.to_string(),
                                        channel: channel.clone(),
                                    };

                                    if let Some(recorder) = &mut self.demo_recorder {
                                        recorder.add_event(
                                            self.game_server.cur_monotonic_tick,
                                            demo::DemoEvent::Chat(Box::new(msg.clone())),
                                        );
                                    }

                                    let net_channel = NetworkInOrderChannel::Custom(3841); // This number reads as "chat".
                                    let pkt = ServerToClientMessage::Chat(MsgSvChatMsg { msg });
                                    if matches!(channel, NetChatMsgPlayerChannel::Global) {
                                        self.broadcast_in_order(pkt, net_channel);
                                    } else {
                                        let side = if own_char_info.stage_id.is_none() {
                                            Some(None)
                                        } else {
                                            own_char_info.side.map(Some)
                                        };
                                        let stage_id = own_char_info.stage_id;

                                        // Send the msg to all players in own stage, and if a side is given only to those
                                        for net_id in self
                                            .game_server
                                            .cached_character_infos
                                            .iter()
                                            .filter_map(|(player_id, char_info)| {
                                                if char_info.stage_id == stage_id
                                                    && side
                                                        .is_none_or(|side| char_info.side == side)
                                                {
                                                    if let Some(client) =
                                                        self.game_server.players.get(player_id)
                                                    {
                                                        return Some(client.network_id);
                                                    }
                                                }
                                                None
                                            })
                                        {
                                            self.network.send_in_order_to(
                                                &pkt,
                                                &net_id,
                                                net_channel,
                                            );
                                        }
                                    }
                                }
                            }
                        };
                        match msg {
                            MsgClChatMsg::Global { msg } => {
                                handle_msg(&msg, NetChatMsgPlayerChannel::Global);
                            }
                            MsgClChatMsg::GameTeam { msg } => {
                                handle_msg(&msg, NetChatMsgPlayerChannel::GameTeam);
                            }
                            MsgClChatMsg::Whisper { receiver_id, msg } => {
                                if !prepare_msg(&msg).is_empty() {
                                    if let (
                                        Some(own_char_info),
                                        Some(recv_char_info),
                                        Some(recv_client),
                                    ) = (
                                        self.game_server.cached_character_infos.get(player_id),
                                        self.game_server.cached_character_infos.get(&receiver_id),
                                        self.game_server.players.get(&receiver_id),
                                    ) {
                                        let net_channel = NetworkInOrderChannel::Custom(3841); // This number reads as "chat".
                                        let pkt = ServerToClientMessage::Chat(MsgSvChatMsg {
                                            msg: NetChatMsg {
                                                sender: ChatPlayerInfo {
                                                    id: *player_id,
                                                    name: own_char_info.info.name.clone(),
                                                    skin: own_char_info.info.skin.clone(),
                                                    skin_info: own_char_info.info.skin_info,
                                                },
                                                msg: msg.to_string(),
                                                channel: NetChatMsgPlayerChannel::Whisper(
                                                    ChatPlayerInfo {
                                                        id: receiver_id,
                                                        name: recv_char_info.info.name.clone(),
                                                        skin: recv_char_info.info.skin.clone(),
                                                        skin_info: recv_char_info.info.skin_info,
                                                    },
                                                ),
                                            },
                                        });

                                        self.network.send_in_order_to(
                                            &pkt,
                                            &recv_client.network_id,
                                            net_channel,
                                        );
                                        // and also send it back to the sender
                                        self.network.send_in_order_to(&pkt, con_id, net_channel);
                                    }
                                }
                            }
                        }
                    }
                    ClientToServerPlayerMessage::Kill => {
                        self.game_server
                            .game
                            .client_command(player_id, ClientCommand::Kill);
                    }
                    ClientToServerPlayerMessage::JoinSpectator => {
                        self.game_server
                            .game
                            .client_command(player_id, ClientCommand::JoinSpectator);
                    }
                    ClientToServerPlayerMessage::StartVote(vote) => {
                        // if no current vote exist, try the vote
                        let is_ingame = self
                            .game_server
                            .cached_character_infos
                            .get(player_id)
                            .is_some_and(|c| c.stage_id.is_some());
                        let player = self.clients.clients.get(con_id).expect("logic error");
                        let res = if is_ingame && self.game_server.cur_vote.is_none() {
                            let vote = match vote {
                                VoteIdentifierType::Map(key) => self
                                    .map_votes
                                    .categories
                                    .get(&key.category)
                                    .and_then(|maps| maps.get(&key.map))
                                    .map(|vote| {
                                        Either::Left((
                                            VoteType::Map {
                                                map: vote.clone(),
                                                key,
                                            },
                                            ServerExtraVoteInfo::None,
                                            None,
                                        ))
                                    })
                                    .unwrap_or_else(|| {
                                        Either::Right(MsgSvStartVoteResult::MapVoteDoesNotExist)
                                    }),
                                VoteIdentifierType::RandomUnfinishedMap(key) => {
                                    if self.map_votes.has_unfinished_map_votes {
                                        Either::Left((
                                            VoteType::RandomUnfinishedMap { key },
                                            ServerExtraVoteInfo::None,
                                            None,
                                        ))
                                    } else {
                                        Either::Right(
                                            MsgSvStartVoteResult::RandomUnfinishedMapUnsupported,
                                        )
                                    }
                                }
                                VoteIdentifierType::Misc(key) => self
                                    .misc_votes
                                    .get(&key.category)
                                    .and_then(|votes| votes.get(&key.vote_key))
                                    .map(|vote| {
                                        Either::Left((
                                            VoteType::Misc {
                                                key,
                                                vote: vote.clone(),
                                            },
                                            ServerExtraVoteInfo::None,
                                            None,
                                        ))
                                    })
                                    .unwrap_or_else(|| {
                                        Either::Right(MsgSvStartVoteResult::MiscVoteDoesNotExist)
                                    }),
                                VoteIdentifierType::VoteSpecPlayer(ref key)
                                | VoteIdentifierType::VoteKickPlayer(ref key) => {
                                    let kicked =
                                        matches!(vote, VoteIdentifierType::VoteKickPlayer { .. });
                                    let is_same_player = *player_id == key.voted_player_id;
                                    let enough_players_to_vote =
                                        self.clients.allowed_to_vote_count() > 2;
                                    let is_same_stage = self
                                        .game_server
                                        .cached_character_infos
                                        .get(player_id)
                                        .zip(
                                            self.game_server
                                                .cached_character_infos
                                                .get(&key.voted_player_id),
                                        )
                                        .is_some_and(|(c1, c2)| c1.stage_id == c2.stage_id);
                                    if !is_same_player && enough_players_to_vote && is_same_stage {
                                        if let Some((kick_con_id, voted_player, character_info)) =
                                            self.game_server
                                                .players
                                                .get(&key.voted_player_id)
                                                .and_then(|p| {
                                                    self.clients
                                                        .clients
                                                        .get(&p.network_id)
                                                        .zip(
                                                            self.game_server
                                                                .cached_character_infos
                                                                .get(&key.voted_player_id),
                                                        )
                                                        .map(|(c, pc)| (p.network_id, c, pc))
                                                })
                                        {
                                            // if the player exists and no current vote exists, start the vote
                                            let can_be_kicked = !matches!(
                                                voted_player.auth.level,
                                                AuthLevel::Admin | AuthLevel::Moderator
                                            );
                                            let is_same_client = kick_con_id == *con_id;
                                            let is_same_network = voted_player.ip == player.ip;
                                            if can_be_kicked && !is_same_client && !is_same_network
                                            {
                                                self.game_server
                                                    .game
                                                    .voted_player(Some(key.voted_player_id));
                                                Either::Left((
                                                    if kicked {
                                                        VoteType::VoteKickPlayer {
                                                            key: key.clone(),
                                                            name: character_info.info.name.clone(),
                                                            skin: character_info.info.skin.clone(),
                                                            skin_info: character_info.skin_info,
                                                        }
                                                    } else {
                                                        VoteType::VoteSpecPlayer {
                                                            key: key.clone(),
                                                            name: character_info.info.name.clone(),
                                                            skin: character_info.info.skin.clone(),
                                                            skin_info: character_info.skin_info,
                                                        }
                                                    },
                                                    ServerExtraVoteInfo::Player {
                                                        to_kick_player: kick_con_id,
                                                        ip: voted_player.ip,
                                                    },
                                                    Some(voted_player.ip),
                                                ))
                                            } else if !can_be_kicked {
                                                Either::Right(
                                                    MsgSvStartVoteResult::CantVoteAdminOrModerator,
                                                )
                                            } else if is_same_client {
                                                Either::Right(MsgSvStartVoteResult::CantSameClient)
                                            } else if is_same_network {
                                                Either::Right(MsgSvStartVoteResult::CantSameNetwork)
                                            } else {
                                                Either::Right(
                                                    MsgSvStartVoteResult::CantVoteAdminOrModerator,
                                                )
                                            }
                                        } else {
                                            Either::Right(MsgSvStartVoteResult::PlayerDoesNotExist)
                                        }
                                    } else if is_same_player {
                                        Either::Right(MsgSvStartVoteResult::CantVoteSelf)
                                    } else if !is_same_stage {
                                        Either::Right(MsgSvStartVoteResult::CantVoteFromOtherStage)
                                    } else {
                                        Either::Right(MsgSvStartVoteResult::TooFewPlayersToVote)
                                    }
                                }
                            };
                            match vote {
                                Either::Left((vote, extra_vote_info, no_voter)) => {
                                    self.game_server.cur_vote = Some(ServerVote {
                                        state: VoteState {
                                            vote,
                                            // filled on the fly instead
                                            remaining_time: Duration::ZERO,
                                            // vote starter get a yes vote
                                            yes_votes: 1,
                                            no_votes: if no_voter.is_some() { 1 } else { 0 },
                                            allowed_to_vote_count: self
                                                .clients
                                                .allowed_to_vote_count()
                                                as u64,
                                        },
                                        extra_vote_info,
                                        started_at: self.sys.time_get(),
                                        participating_ip: [(player.ip, Voted::Yes)]
                                            .into_iter()
                                            .chain(
                                                no_voter
                                                    .into_iter()
                                                    .map(|con_id| (con_id, Voted::No)),
                                            )
                                            .collect(),
                                    });
                                    let vote_state = self.game_server.cur_vote.as_ref().map(|v| {
                                        let mut state = v.state.clone();
                                        state.remaining_time = Duration::from_secs(25);
                                        state
                                    });
                                    self.broadcast_in_order(
                                        ServerToClientMessage::Vote(vote_state),
                                        NetworkInOrderChannel::Custom(7013), // This number reads as "vote".
                                    );
                                    MsgSvStartVoteResult::Success
                                }
                                Either::Right(res) => res,
                            }
                        } else if is_ingame {
                            MsgSvStartVoteResult::AnotherVoteAlreadyActive
                        } else {
                            MsgSvStartVoteResult::CantVoteAsSpectator
                        };

                        self.network.send_in_order_to(
                            &ServerToClientMessage::StartVoteRes(res),
                            con_id,
                            NetworkInOrderChannel::Custom(7013), // This number reads as "vote".
                        );
                    }
                    ClientToServerPlayerMessage::Voted(voted) => {
                        if let Some(vote) = &mut self.game_server.cur_vote {
                            let prev_vote = vote.participating_ip.insert(player.ip, voted);
                            match voted {
                                Voted::Yes => vote.state.yes_votes += 1,
                                Voted::No => vote.state.no_votes += 1,
                            }
                            if let Some(prev_vote) = prev_vote {
                                match prev_vote {
                                    Voted::Yes => vote.state.yes_votes -= 1,
                                    Voted::No => vote.state.no_votes -= 1,
                                }
                            }
                            let vote_state = vote.state.clone();
                            let started_at = vote.started_at;
                            self.send_vote(Some(vote_state), started_at);
                        }
                    }
                    ClientToServerPlayerMessage::Emoticon(emoticon) => {
                        self.game_server.set_player_emoticon(player_id, emoticon);
                    }
                    ClientToServerPlayerMessage::ChangeEyes { eye, duration } => {
                        self.game_server.set_player_eye(player_id, eye, duration);
                    }
                    ClientToServerPlayerMessage::JoinStage(join_stage) => {
                        self.game_server
                            .game
                            .client_command(player_id, ClientCommand::JoinStage(join_stage));
                    }
                    ClientToServerPlayerMessage::JoinVanillaSide(side) => {
                        self.game_server
                            .game
                            .client_command(player_id, ClientCommand::JoinSide(side));
                    }
                    ClientToServerPlayerMessage::SwitchToCamera(mode) => {
                        self.game_server
                            .game
                            .client_command(player_id, ClientCommand::SetCameraMode(mode));
                    }
                    ClientToServerPlayerMessage::UpdateCharacterInfo { info, version } => {
                        self.game_server
                            .game
                            .try_overwrite_player_character_info(player_id, &info, version);
                    }
                    ClientToServerPlayerMessage::RconExec { name, args } => {
                        let auth_level = player.auth.level;
                        if matches!(auth_level, AuthLevel::Moderator | AuthLevel::Admin) {
                            let res = if self
                                .game_server
                                .game
                                .info
                                .rcon_commands
                                .cmds
                                .contains_key(name.as_str())
                            {
                                self.game_server.game.rcon_command(
                                    Some(*player_id),
                                    ExecRconCommand {
                                        raw: format!("{} {}", name.as_str(), args.as_str())
                                            .as_str()
                                            .try_into()
                                            .unwrap(),
                                        auth_level,
                                    },
                                )
                            } else {
                                // if not a mod rcon, try to execute it inside the server
                                let cmds = command_parser::parser::parse(
                                    &format!("{} {}", name.as_str(), args.as_str()),
                                    &self.rcon_chain.parser,
                                    &mut self.cache,
                                );
                                self.handle_rcon_commands(Some(player_id), auth_level, cmds)
                            };
                            self.network.send_in_order_to(
                                &ServerToClientMessage::RconExecResult { results: res },
                                con_id,
                                NetworkInOrderChannel::Custom(
                                    7302, // reads as "rcon"
                                ),
                            );
                        }
                    }
                }
            }
        }
    }

    fn user_id(account_server_public_key: &[VerifyingKey], auth: &ClientAuth) -> UserId {
        ddnet_accounts_shared::game_server::user_id::user_id_from_cert(
            account_server_public_key,
            auth.cert.to_der().unwrap(),
        )
    }

    fn user_id_to_player_unique_id(user_id: &UserId) -> PlayerUniqueId {
        user_id
            .account_id
            .map(PlayerUniqueId::Account)
            .unwrap_or_else(|| PlayerUniqueId::CertFingerprint(user_id.public_key))
    }

    fn client_snap_ack(client: &mut ServerClient, snap_id: u64) {
        if let Some(snap) = client.client_snap_storage.remove(&snap_id) {
            client.latest_client_snap = Some(ClientSnapshotForDiff {
                snap_id,
                snapshot: snap.snapshot,
                monotonic_tick: snap.monotonic_tick,
            });
        }
        while client
            .client_snap_storage
            .first_entry()
            .is_some_and(|entry| *entry.key() < snap_id)
        {
            client.client_snap_storage.pop_first();
        }
    }

    fn send_rcon_commands(&self, con_id: &NetworkConnectionId) {
        let mut rcon_commands = RconCommands {
            cmds: self
                .rcon_chain
                .cmds
                .iter()
                .map(|(name, cmd)| (name.clone(), cmd.rcon.clone()))
                .collect(),
        };
        // mod rcon commands have higher prio
        rcon_commands
            .cmds
            .extend(self.game_server.game.info.rcon_commands.cmds.clone());
        self.network.send_in_order_to(
            &ServerToClientMessage::RconCommands(rcon_commands),
            con_id,
            NetworkInOrderChannel::Custom(
                7302, // reads as "rcon"
            ),
        );
    }

    fn handle_config_variable_cmd(
        cmd: &parser::Command,
        config: &mut ConfigGame,
    ) -> anyhow::Result<String> {
        fn syn_vec_to_config_val(args: &[(Syn, Range<usize>)]) -> Option<String> {
            args.first().map(|(arg, _)| match arg {
                parser::Syn::Command(cmd) => cmd.cmd_text.clone(),
                parser::Syn::Commands(cmds) => cmds
                    .first()
                    .map(|cmd| cmd.cmd_text.clone())
                    .unwrap_or_default(),
                parser::Syn::Text(text) => text.clone(),
                parser::Syn::Number(num) => num.clone(),
                parser::Syn::Float(num) => num.clone(),
                parser::Syn::JsonObjectLike(obj) => obj.clone(),
                parser::Syn::JsonArrayLike(obj) => obj.clone(),
            })
        }
        Ok(config.try_set_from_str(
            cmd.cmd_text.clone(),
            None,
            syn_vec_to_config_val(&cmd.args),
            None,
            config::traits::ConfigFromStrOperation::Set,
        )?)
    }

    fn handle_cmd_full(
        &mut self,
        cmd: parser::Command,
        responses: &mut Vec<Result<String, String>>,
        remaining_cmds: &mut Vec<parser::Command>,
    ) -> anyhow::Result<String> {
        let Some(chain_cmd) = self.rcon_chain.cmds.get(&cmd.ident) else {
            return Err(anyhow!("Command {} not found", cmd.ident));
        };

        fn ban_or_kick(
            cmd: &parser::Command,
            game_server: &ServerGame,
            clients: &mut Clients,
            process: impl FnOnce(&mut ServerClient, NetworkConnectionId),
        ) -> anyhow::Result<()> {
            let Syn::Number(num) = &cmd.args[0].0 else {
                panic!("Command parser returned a non requested command arg");
            };
            let ban_id: GameEntityId = num.parse()?;
            let ban_id: PlayerId = ban_id.into();
            if let Some((client, network_id)) =
                game_server.players.get(&ban_id).and_then(|player| {
                    clients
                        .clients
                        .get_mut(&player.network_id)
                        .map(|c| (c, player.network_id))
                })
            {
                process(client, network_id);
            }

            Ok(())
        }

        match chain_cmd.cmd {
            ServerRconCommand::BanId => {
                let mut res = String::new();
                ban_or_kick(&cmd, &self.game_server, &mut self.clients, |client, _| {
                    let ty = BanType::Admin;
                    let until = None;

                    client.drop_reason = Some(PlayerDropReason::Banned {
                        reason: PlayerBanReason::Rcon,
                        until,
                    });

                    // ban the player
                    let ids = self.connection_bans.ban_ip(client.ip, ty.clone(), until);
                    for id in &ids {
                        self.network.kick(
                            id,
                            KickType::Ban(Banned {
                                msg: ty.clone(),
                                until,
                            }),
                        );
                    }
                    let text: String = ids
                        .into_iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    res = format!("Banned the following id(s): {}", text);
                })?;
                anyhow::Ok(res)
            }
            ServerRconCommand::KickId => {
                let mut res = String::new();
                ban_or_kick(
                    &cmd,
                    &self.game_server,
                    &mut self.clients,
                    |c, network_id| {
                        c.drop_reason = Some(PlayerDropReason::Kicked(PlayerKickReason::Rcon));

                        self.network
                            .kick(&network_id, KickType::Kick("by admin".to_string()));
                        let text: String = c
                            .players
                            .keys()
                            .map(|id| id.to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        res = format!("Kicked the following id(s): {}", text);
                    },
                )?;
                anyhow::Ok(res)
            }
            ServerRconCommand::Status => {
                let mut res: Vec<String> = Default::default();
                for client in self.clients.clients.values() {
                    res.push(format!("client ip: {}", client.ip));
                    for (player_id, player) in client.players.iter() {
                        res.push(format!(
                            "    player_id: {}, client_id: {}",
                            player_id, player.id
                        ));
                    }
                }
                Ok(res.join("\n"))
            }
            ServerRconCommand::ConfVariable => {
                Self::handle_config_variable_cmd(&cmd, &mut self.config_game)
                    .map(|msg| format!("Updated value for {}: {}", cmd.cmd_text, msg))
            }
            ServerRconCommand::Exec => {
                match Self::handle_exec(&mut self.config_game, &self.io, &self.rcon_chain, cmd) {
                    Ok((mut res, mut res_remaining_cmds)) => {
                        responses.append(&mut res);
                        remaining_cmds.append(&mut res_remaining_cmds);
                        Ok("".to_string())
                    }
                    Err(err) => Err(err),
                }
            }
            ServerRconCommand::Load => {
                Self::handle_load_config(&mut self.config_game, &self.io, cmd)
            }
            ServerRconCommand::RecordDemo => {
                let had_demo_recorder = self.demo_recorder.is_some();
                self.demo_recorder = Some(DemoRecorder::new(
                    DemoRecorderCreateProps {
                        base: DemoRecorderCreatePropsBase {
                            map: self.game_server.map.name.as_str().try_into().unwrap(),
                            map_hash: generate_hash_for(&self.game_server.map.map_file),
                            game_options: GameStateCreateOptions {
                                hint_max_characters: Some(self.config_game.sv.max_players as usize),
                                account_db: None,
                                config: self.game_server.game.info.config.clone(),
                            },
                            required_resources: Default::default(), /* TODO: */
                            physics_module: self.game_server.game_mod.clone(),
                            render_module: self.game_server.render_mod.clone(),
                            physics_group_name: self
                                .game_server
                                .game
                                .info
                                .options
                                .physics_group_name
                                .clone(),
                        },
                        io: self.io.clone(),
                        in_memory: None,
                    },
                    self.game_server.game.info.ticks_in_a_second,
                    Some("server_demos".as_ref()),
                    None,
                ));
                Ok(format!(
                    "Started demo recording.{}",
                    if had_demo_recorder {
                        "\nA previous recording was stopped in that process."
                    } else {
                        ""
                    }
                ))
            }
        }
    }

    fn handle_cmd(
        &mut self,
        cmd: parser::CommandType,
        responses: &mut Vec<Result<String, String>>,
        remaining_cmds: &mut Vec<parser::Command>,
    ) -> anyhow::Result<String> {
        match cmd {
            CommandType::Full(cmd) => self.handle_cmd_full(cmd, responses, remaining_cmds),
            CommandType::Partial(cmd) => {
                let Some(cmd) = cmd.ref_cmd_partial() else {
                    return Err(anyhow!("This command was invalid: {cmd}"));
                };
                let Some(chain_cmd) = self.rcon_chain.cmds.get(&cmd.ident) else {
                    return Err(anyhow!("Command {} not found", cmd.ident));
                };

                if let ServerRconCommand::ConfVariable = chain_cmd.cmd {
                    Self::handle_config_variable_cmd(cmd, &mut self.config_game)
                        .map(|msg| format!("Updated value for {}: {}", cmd.cmd_text, msg))
                } else {
                    Err(anyhow!("This command was invalid: {cmd}"))
                }
            }
        }
    }

    fn handle_rcon_commands(
        &mut self,
        _player_id: Option<&PlayerId>,
        _auth: AuthLevel,
        cmds: Vec<CommandType>,
    ) -> Vec<NetworkString<65536>> {
        let mut remaining_cmds = Vec::default();
        let mut responses = Vec::default();
        let mut res: Vec<NetworkString<65536>> = Default::default();
        for cmd in cmds {
            match self.handle_cmd(cmd, &mut responses, &mut remaining_cmds) {
                Ok(msg) => {
                    res.push(NetworkString::new_lossy(msg));
                }
                Err(err) => {
                    res.push(NetworkString::new_lossy(err.to_string()));
                }
            }
            // directly add the current reponses after command handling
            res.extend(responses.drain(..).map(|s| {
                NetworkString::new_lossy(match s {
                    Ok(s) => s,
                    Err(s) => s,
                })
            }));
        }
        if !remaining_cmds.is_empty() {
            res.append(&mut self.handle_rcon_commands(
                _player_id,
                _auth,
                remaining_cmds.into_iter().map(CommandType::Full).collect(),
            ));
        }
        res
    }

    fn handle_msg(
        &mut self,
        timestamp: &Duration,
        con_id: &NetworkConnectionId,
        game_msg: ClientToServerMessage<'_>,
    ) {
        match game_msg {
            ClientToServerMessage::Custom(_) => {
                // ignore
            }
            ClientToServerMessage::Ready(ready_info) => {
                if !ready_info.players.is_empty() {
                    // if client is actually waiting, make it part of the game
                    let account_server_public_keys = self
                        .account_server_certs_downloader
                        .as_ref()
                        .map(|c| c.public_keys())
                        .unwrap_or_default();
                    let client = self.clients.try_client_ready(con_id);
                    let check_vote = client.is_some();
                    if let Some(client) = client {
                        let user_id = Self::user_id(&account_server_public_keys, &client.auth);
                        let unique_identifier = Self::user_id_to_player_unique_id(&user_id);

                        let send_rcon = self.rcon.try_rcon_auth(
                            client,
                            ready_info.rcon_secret.as_ref(),
                            &unique_identifier,
                        );

                        let initial_network_stats = client.network_stats;

                        let mut joined_players: Vec<(u64, PlayerId)> = Default::default();
                        let mut non_joined_players: Vec<u64> = Default::default();
                        let expected_join_len = ready_info.players.len();
                        for (index, player) in ready_info.players.into_iter().enumerate() {
                            if index == 0 {
                                let player_id = self
                                    .add_player_for_client(
                                        con_id,
                                        PlayerClientInfo {
                                            info: player.player_info,
                                            id: player.id,
                                            unique_identifier,
                                            initial_network_stats,
                                        },
                                        false,
                                    )
                                    .unwrap();
                                joined_players.push((player.id, player_id));
                            } else {
                                let client = self.clients.clients.get(con_id).unwrap();
                                if self.can_another_player_connect()
                                    && Self::can_client_join_another_player(
                                        client,
                                        &self.config_game,
                                    )
                                    && Self::can_client_player_id_join(client, player.id)
                                {
                                    let player_id = self
                                        .add_player_for_client(
                                            con_id,
                                            PlayerClientInfo {
                                                info: player.player_info,
                                                id: player.id,
                                                unique_identifier,
                                                initial_network_stats,
                                            },
                                            true,
                                        )
                                        .unwrap();
                                    joined_players.push((player.id, player_id));
                                } else {
                                    non_joined_players.push(player.id);
                                }
                            }
                        }
                        if send_rcon {
                            self.send_rcon_commands(con_id);
                        }

                        if let Some((accounts, db)) = self.accounts.as_ref().zip(self.db.as_ref()) {
                            if let Some(pool) = db.pools.get(&accounts.kind) {
                                let shared = accounts.shared.clone();
                                let pool = pool.clone();
                                self.db_requests.push(self.io.rt.spawn(async move {
                                    let new_account_was_created =
                                        ddnet_account_game_server::auto_login::auto_login(
                                            shared, &pool, &user_id,
                                        )
                                        .await?;
                                    Ok(GameServerDb::Account(GameServerDbAccount::AutoLogin {
                                        user_id,
                                        new_account_was_created,
                                    }))
                                }));
                            }
                        }

                        self.network.send_unordered_to(
                            &ServerToClientMessage::ReadyResponse(
                                if joined_players.len() == expected_join_len {
                                    MsgClReadyResponse::Success {
                                        joined_ids: joined_players,
                                    }
                                } else {
                                    MsgClReadyResponse::PartialSuccess {
                                        joined_ids: joined_players,
                                        non_joined_ids: non_joined_players,
                                    }
                                },
                            ),
                            con_id,
                        );
                    } else {
                        self.network.send_unordered_to(
                            &ServerToClientMessage::ReadyResponse(MsgClReadyResponse::Error {
                                err: if self.clients.clients.contains_key(con_id) {
                                    MsgClReadyResponseError::ClientAlreadyReady
                                } else {
                                    MsgClReadyResponseError::ClientIsNotConnecting
                                },
                                non_joined_ids: ready_info
                                    .players
                                    .into_iter()
                                    .map(|p| p.id)
                                    .collect(),
                            }),
                            con_id,
                        );
                    }

                    if check_vote {
                        // update vote if nessecary
                        if let Some(vote) = &mut self.game_server.cur_vote {
                            vote.state.allowed_to_vote_count =
                                self.clients.allowed_to_vote_count() as u64;

                            let vote_state = vote.state.clone();
                            let started_at = vote.started_at;
                            self.send_vote(Some(vote_state), started_at);
                        }
                    }
                } else {
                    self.network.send_unordered_to(
                        &ServerToClientMessage::ReadyResponse(MsgClReadyResponse::Error {
                            err: MsgClReadyResponseError::NoPlayersJoined,
                            non_joined_ids: Default::default(),
                        }),
                        con_id,
                    );
                }
            }
            ClientToServerMessage::AddLocalPlayer(player_info) => {
                let client_id = player_info.id;
                let connect = || {
                    if self.can_another_player_connect() {
                        if let Some(client) = self.clients.clients.get(con_id) {
                            assert!(
                                !client.players.is_empty(),
                                "client had no players, even tho it was \
                                        connected. this is a server bug"
                            );
                            let can_join_another_player =
                                Self::can_client_join_another_player(client, &self.config_game);
                            let can_join_player_with_id =
                                Self::can_client_player_id_join(client, client_id);
                            if can_join_another_player && can_join_player_with_id {
                                let player_info = PlayerClientInfo {
                                    info: player_info.player_info,
                                    id: client_id,
                                    unique_identifier: Self::user_id_to_player_unique_id(
                                        &Self::user_id(
                                            &self
                                                .account_server_certs_downloader
                                                .as_ref()
                                                .map(|c| c.public_keys())
                                                .unwrap_or_default(),
                                            &client.auth,
                                        ),
                                    ),
                                    initial_network_stats: client.network_stats,
                                };
                                Ok(self
                                    .add_player_for_client(con_id, player_info, true)
                                    .unwrap())
                            } else if !can_join_another_player {
                                Err(AddLocalPlayerResponseError::MaxPlayersPerClient)
                            } else if !can_join_player_with_id {
                                Err(AddLocalPlayerResponseError::PlayerIdAlreadyUsedByClient)
                            } else {
                                panic!("Unhandled error variant during connecting another local player");
                            }
                        } else {
                            Err(AddLocalPlayerResponseError::ClientWasNotReady)
                        }
                    } else {
                        Err(AddLocalPlayerResponseError::MaxPlayers)
                    }
                };
                let res = match connect() {
                    Ok(player_id) => MsgSvAddLocalPlayerResponse::Success {
                        id: client_id,
                        player_id,
                    },
                    Err(err) => MsgSvAddLocalPlayerResponse::Err { err, id: client_id },
                };
                self.network
                    .send_unordered_to(&ServerToClientMessage::AddLocalPlayerResponse(res), con_id);
            }
            ClientToServerMessage::PlayerMsg((player_id, player_msg)) => {
                self.handle_player_msg(con_id, &player_id, player_msg);
            }
            ClientToServerMessage::Inputs {
                inputs,
                snap_ack,
                id,
            } => {
                let client = self.clients.clients.get_mut(con_id);
                if let Some(client) = client {
                    // add ack early to make the timing more accurate
                    client.inputs_to_ack.push(MsgSvInputAck {
                        id,
                        // reuse this field this one time
                        logic_overhead: *timestamp,
                    });

                    for (player_id, inp_chain) in inputs.iter() {
                        if let Some(player) = client.players.get_mut(player_id) {
                            let Some(def_inp) = (if let Some(diff_id) = inp_chain.diff_id {
                                player.input_storage.get(&diff_id).copied()
                            } else {
                                Some(PlayerInputChainable::default())
                            }) else {
                                log::debug!(target: "server", "had to drop an input from the client for diff id: {:?}", inp_chain.diff_id);
                                continue;
                            };

                            let mut def = self.input_deser.new();
                            let def_len = bincode::serde::encode_into_std_write(
                                def_inp,
                                &mut *def,
                                bincode::config::standard().with_fixed_int_encoding(),
                            )
                            .unwrap();
                            let mut old = def;
                            let mut offset = 0;

                            while let Some(patch) = inp_chain.data.get(offset..offset + def_len) {
                                let mut new = self.input_deser.new();
                                bin_patch::patch_exact_size(&old, patch, &mut new).unwrap();

                                if let Ok((inp, _)) =
                                    bincode::serde::decode_from_slice::<PlayerInputChainable, _>(
                                        &new,
                                        bincode::config::standard()
                                            .with_fixed_int_encoding()
                                            .with_limit::<{ 1024 * 1024 * 4 }>(),
                                    )
                                {
                                    let as_diff = inp_chain.as_diff;
                                    if as_diff {
                                        // this should be higher than the number of inputs saved on the client
                                        // (since reordering of packets etc.)
                                        while player.input_storage.len() >= 50 {
                                            player.input_storage.pop_first();
                                        }
                                        player.input_storage.insert(id, inp);
                                    }

                                    self.game_server.player_inp(
                                        player_id,
                                        inp.inp,
                                        inp.for_monotonic_tick,
                                    );
                                }

                                offset += def_len;
                                old = new;
                            }
                        }
                    }
                    for MsgClSnapshotAck { snap_id } in snap_ack.iter() {
                        Self::client_snap_ack(client, *snap_id);
                    }
                }
            }
            ClientToServerMessage::LoadVotes(votes) => {
                if let Some(client) = self.clients.clients.get_mut(con_id) {
                    match votes {
                        MsgClLoadVotes::Map { cached_votes } => {
                            if !client.loaded_map_votes {
                                client.loaded_map_votes = true;

                                if cached_votes.is_none_or(|hash| hash != self.map_votes_hash) {
                                    self.network.send_unordered_to(
                                        &ServerToClientMessage::LoadVote(MsgSvLoadVotes::Map {
                                            categories: self.map_votes.categories.clone(),
                                            has_unfinished_map_votes: self
                                                .map_votes
                                                .has_unfinished_map_votes,
                                        }),
                                        con_id,
                                    );
                                }
                            }
                        }
                        MsgClLoadVotes::Misc { cached_votes } => {
                            if !client.loaded_misc_votes {
                                client.loaded_misc_votes = true;

                                if cached_votes.is_none_or(|hash| hash != self.misc_votes_hash) {
                                    self.network.send_unordered_to(
                                        &ServerToClientMessage::LoadVote(MsgSvLoadVotes::Misc {
                                            votes: self.misc_votes.clone(),
                                        }),
                                        con_id,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            ClientToServerMessage::AccountChangeName { new_name } => {
                if let Some(client) = self.clients.clients.get_mut(con_id) {
                    if !std::mem::replace(&mut client.requested_account_rename, true) {
                        let user_id = Self::user_id(
                            &self
                                .account_server_certs_downloader
                                .as_ref()
                                .map(|c| c.public_keys())
                                .unwrap_or_default(),
                            &client.auth,
                        );

                        if let Some((accounts, db)) = self.accounts.as_ref().zip(self.db.as_ref()) {
                            if let Some(pool) = db.pools.get(&accounts.kind) {
                                let shared = accounts.shared.clone();
                                let pool = pool.clone();
                                let con_id = *con_id;
                                self.db_requests.push(self.io.rt.spawn(async move {
                                    let rename_res = ddnet_account_game_server::rename::rename(
                                        shared,
                                        &pool,
                                        &user_id,
                                        new_name.as_str(),
                                    )
                                    .await
                                    .map(|_| ());
                                    Ok(GameServerDb::Account(GameServerDbAccount::Rename {
                                        con_id,
                                        rename_result: rename_res
                                            .map_err(|err| {
                                                NetworkString::new_lossy(err.to_string())
                                            })
                                            .map(|_| new_name),
                                        account_id: user_id.account_id,
                                    }))
                                }));
                            }
                        } else {
                            self.network.send_unordered_to(
                                &ServerToClientMessage::AccountRenameRes(Err(
                                    if self.accounts.is_some() {
                                        "user had no account".try_into().unwrap()
                                    } else {
                                        "accounts are not enabled on this server"
                                            .try_into()
                                            .unwrap()
                                    },
                                )),
                                con_id,
                            );
                        }
                    } else {
                        self.network.send_unordered_to(
                            &ServerToClientMessage::AccountRenameRes(Err(
                                "cannot change name that often".try_into().unwrap(),
                            )),
                            con_id,
                        );
                    }
                }
            }
            ClientToServerMessage::AccountRequestInfo => {
                if let Some(client) = self.clients.clients.get_mut(con_id) {
                    if !std::mem::replace(&mut client.requested_account_details, true) {
                        let user_id = Self::user_id(
                            &self
                                .account_server_certs_downloader
                                .as_ref()
                                .map(|c| c.public_keys())
                                .unwrap_or_default(),
                            &client.auth,
                        );
                        if let Some((account_info, account_id)) = self
                            .accounts
                            .as_ref()
                            .map(|a| &a.info)
                            .zip(user_id.account_id)
                        {
                            let account_info = account_info.clone();
                            let con_id = *con_id;
                            self.db_requests.push(self.io.rt.spawn(async move {
                                let details_res = account_info.fetch(account_id).await;
                                Ok(GameServerDb::Account(GameServerDbAccount::Info {
                                    con_id,
                                    account_details: details_res
                                        .map_err(|err| NetworkString::new_lossy(err.to_string()))
                                        .and_then(|res| {
                                            res.name
                                                .as_str()
                                                .try_into()
                                                .map(|name| account_info::AccountInfo {
                                                    name,
                                                    creation_date: res.create_time,
                                                })
                                                .map_err(|err| {
                                                    NetworkString::new_lossy(err.to_string())
                                                })
                                        }),
                                }))
                            }));
                        } else {
                            self.network.send_unordered_to(
                                &ServerToClientMessage::AccountDetails(Err(
                                    if self.accounts.is_some() {
                                        "user has no account".try_into().unwrap()
                                    } else {
                                        "accounts are not enabled on this server"
                                            .try_into()
                                            .unwrap()
                                    },
                                )),
                                con_id,
                            );
                        }
                    }
                }
            }
            ClientToServerMessage::SpatialChat { opus_frames, id } => {
                if let Some(spatial_chat) = &mut self.game_server.spatial_world {
                    if let Some((player_id, auth)) = self
                        .clients
                        .clients
                        .get_mut(con_id)
                        .and_then(|c| c.players.front().map(|(id, _)| (id, &c.auth)))
                    {
                        let account_server_public_keys = self
                            .account_server_certs_downloader
                            .as_ref()
                            .map(|c| c.public_keys())
                            .unwrap_or_default();
                        let player_unique_id = Self::user_id_to_player_unique_id(&Self::user_id(
                            &account_server_public_keys,
                            auth,
                        ));
                        spatial_chat.chat_sound(
                            *con_id,
                            *player_id,
                            player_unique_id,
                            id,
                            opus_frames,
                        );
                    }
                }
            }
            ClientToServerMessage::SpatialChatDeactivated => {
                if let Some(spatial_chat) = &mut self.game_server.spatial_world {
                    if self.clients.clients.contains_key(con_id) {
                        spatial_chat.on_client_drop(con_id);
                    }
                }
            }
        }
    }

    pub fn dbg_game<'a>(
        config: &ConfigDebug,
        last_tick_time: &Duration,
        game: &mut GameStateWasmManager,
        inputs: Option<impl Iterator<Item = &'a CharacterInput> + Debug>,
        cur_tick: GameTickType,
        ticks_in_a_second: GameTickType,
        shared_info: &Arc<LocalServerInfo>,
        caller_name: &str,
    ) {
        if config.client_server_sync_log.time
            || config.client_server_sync_log.inputs
            || config.client_server_sync_log.players
            || config.client_server_sync_log.projectiles
        {
            // breaks prediction, but currently only way to get useful information
            let cur_snap = game.snapshot_for(SnapshotClientInfo::Everything);
            game.build_from_snapshot_for_prev(&cur_snap);

            let stages = game.all_stages(1.0);
            let player_infos: FxLinkedHashMap<_, _> = stages
                .iter()
                .flat_map(|s| s.1.world.characters.iter())
                .collect();
            let projectiles: FxLinkedHashMap<_, _> = stages
                .iter()
                .flat_map(|s| s.1.world.projectiles.iter())
                .collect();

            let players = format!("{:?}", player_infos);
            let projectiles = format!("{:?}", projectiles);
            let inputs = format!("{:?}", inputs.map(|inp| inp.collect::<Vec<_>>()));

            if let LocalServerState::Ready {
                connect_info: LocalServerConnectInfo { dbg_games, .. },
                ..
            } = &mut *shared_info.state.lock().unwrap()
            {
                let dbg_game = dbg_games.get(&cur_tick);
                if let Some(dbg_game) = dbg_game {
                    let now = std::time::Instant::now();
                    if ((*last_tick_time).max(dbg_game.tick_time)
                        - (*last_tick_time).min(dbg_game.tick_time)
                        > Duration::from_millis(1000 / ticks_in_a_second)
                        || now.duration_since(dbg_game.time)
                            > Duration::from_millis(1000 / ticks_in_a_second))
                        && config.client_server_sync_log.time
                    {
                        println!(
                            "out of sync: instant: {:?}, tick_time: {:?}, tick: {:?}",
                            now.duration_since(dbg_game.time),
                            (*last_tick_time).max(dbg_game.tick_time)
                                - (*last_tick_time).min(dbg_game.tick_time),
                            cur_tick,
                        );
                    }
                    let diff_players = difference::Changeset::new(&dbg_game.players, &players, " ");
                    if diff_players
                        .diffs
                        .iter()
                        .any(|diff| !matches!(&diff, difference::Difference::Same(_)))
                        && config.client_server_sync_log.players
                    {
                        println!(
                            "players-{} vs {caller_name}:\n{}",
                            dbg_game.caller, diff_players
                        );
                    }
                    let diff_projectiles =
                        difference::Changeset::new(&dbg_game.projectiles, &projectiles, " ");
                    if diff_projectiles
                        .diffs
                        .iter()
                        .any(|diff| !matches!(&diff, difference::Difference::Same(_)))
                        && config.client_server_sync_log.projectiles
                    {
                        println!(
                            "projectiles-{} vs {caller_name}:\n{}",
                            dbg_game.caller, diff_projectiles
                        );
                    }
                    let diff_inputs = difference::Changeset::new(&dbg_game.inputs, &inputs, " ");
                    if diff_inputs
                        .diffs
                        .iter()
                        .any(|diff| !matches!(&diff, difference::Difference::Same(_)))
                        && config.client_server_sync_log.inputs
                    {
                        println!(
                            "inputs-{} vs {caller_name}:\n{}",
                            dbg_game.caller, diff_inputs
                        );
                    }
                } else {
                    dbg_games.insert(
                        cur_tick,
                        ServerDbgGame {
                            time: std::time::Instant::now(),
                            tick_time: *last_tick_time,
                            players,
                            inputs,
                            projectiles,
                            caller: caller_name.to_string(),
                        },
                    );
                    while dbg_games.len() > 250 {
                        dbg_games.pop_front();
                    }
                }
            }
        }
    }

    pub fn register(&mut self) {
        let master_servers = [
            //"https://master1.ddnet.org/ddnet/15/register",
            "https://pg.ddnet.org:4444/ddnet/15/register",
        ];

        let http_v4 = self.io.http.clone();
        let http_v6 = self.http_v6.clone();
        let port_v4 = self.server_port_v4;
        let port_v6 = self.server_port_v6;

        let characters = &self.game_server.cached_character_infos;

        let settings = self.game_server.game.settings();
        let mut register_info = ServerBrowserInfo {
            name: self.config_game.sv.name.as_str().try_into().unwrap(),
            game_type: self.game_server.game.info.mod_name.clone(),
            version: self.game_server.game.info.version.clone(),
            map: ServerBrowserInfoMap {
                name: self.game_server.map.name.clone(),
                blake3: self.game_server.map_blake3_hash,
                size: self.game_server.map.map_file.len(),
            },
            players: characters
                .iter()
                .filter(|(_, c)| c.player_info.is_some())
                .map(|(_, c)| ServerBrowserPlayer {
                    score: (*c.browser_score).clone(),
                    name: c.info.name.clone(),
                    clan: c.info.clan.clone(),
                    account_name: c.account_name.as_ref().map(|a| (**a).clone()),
                    flag: c.info.flag.clone(),
                    skin: ServerBrowserSkin {
                        name: c.info.skin.clone(),
                        info: c.skin_info,
                        eye: c.browser_eye,
                    },
                })
                .collect::<Vec<_>>(),
            max_ingame_players: settings.max_ingame_players,
            max_players: self.config_game.sv.max_players,
            max_players_per_client: self.config_game.sv.max_players_per_client,
            tournament_mode: settings.tournament_mode,
            passworded: false, // TODO:
            cert_sha256_fingerprint: self.cert_sha256_fingerprint,
            requires_account: self.accounts_only,
        };

        if let Some(LocalServerState::Ready { browser_info, .. }) = self
            .shared_info
            .upgrade()
            .as_ref()
            .and_then(|info| info.state.lock().ok())
            .as_deref_mut()
        {
            *browser_info = Some(register_info.clone())
        }

        let register_info = loop {
            let json = serde_json::to_string(&register_info).unwrap();
            if json.len() <= 16 * 1024 {
                break json;
            } else {
                // make sure no endless loop exists
                // in worst case don't register at all.
                if register_info.players.is_empty() {
                    return;
                }
                // truncate players in half
                register_info
                    .players
                    .truncate(register_info.players.len() / 2);
            }
        };

        if !self.config_game.sv.register {
            return;
        }

        let next_serial = self.last_register_serial + 1;
        let serial = std::mem::replace(&mut self.last_register_serial, next_serial);

        self.register_task = Some(
            self.io
                .rt
                .spawn(async move {
                    let mut secret: [u8; 32] = Default::default();
                    rand::rngs::OsRng.fill_bytes(&mut secret);
                    let mut challenge_secret: [u8; 32] = Default::default();
                    rand::rngs::OsRng.fill_bytes(&mut challenge_secret);
                    let register = |register_info: String,
                                    http: Arc<dyn HttpClientInterface>,
                                    ipv4: bool,
                                    port: u16| {
                        Box::pin(async move {
                            for master_server in master_servers {
                                match http
                                    .custom_request(
                                        master_server.try_into().unwrap(),
                                        vec![
                                            (
                                                "Address",
                                                format!(
                                                    "ddrs-0.1+quic://connecting-address.invalid:{}",
                                                    port
                                                )
                                                .as_str(),
                                            )
                                                .into(),
                                            ("Secret", fmt_hash(&secret).as_str()).into(),
                                            (
                                                "Challenge-Secret",
                                                fmt_hash(&challenge_secret).as_str(),
                                            )
                                                .into(),
                                            ("Info-Serial", serial.to_string().as_str()).into(),
                                            ("content-type", "application/json").into(),
                                        ],
                                        Some(register_info.as_bytes().to_vec()),
                                    )
                                    .await
                                    .map_err(|err| anyhow!(err))
                                    .and_then(|res| {
                                        serde_json::from_slice::<RegisterResponse>(&res)
                                            .map_err(|err| anyhow!(err))
                                    })
                                    .and_then(|res| match res {
                                        RegisterResponse::Success => Ok(()),
                                        RegisterResponse::NeedChallenge => {
                                            Err(anyhow!("Challenge is not supported."))
                                        }
                                        RegisterResponse::NeedInfo => {
                                            Err(anyhow!("Need info is not supported."))
                                        }
                                        RegisterResponse::Error(err) => Err(anyhow!(err.message)),
                                    }) {
                                    Ok(_) => {
                                        log::info!(
                                            "registered server on {} with {}",
                                            master_server,
                                            if ipv4 { "ipv4" } else { "ipv6" }
                                        );
                                        return Ok(());
                                    }
                                    Err(err) => {
                                        log::debug!("{:?}", (master_server, err, &register_info));
                                    }
                                }
                            }

                            Err(anyhow!(
                                "server not registered with {}",
                                if ipv4 { "ipv4" } else { "ipv6" }
                            ))
                        })
                    };

                    let res_v4 = register(register_info.clone(), http_v4, true, port_v4).await;
                    let res_v6 = if let Some(http_v6) = http_v6 {
                        register(register_info, http_v6, false, port_v6).await
                    } else {
                        Err(anyhow!("ipv6 support not given by the operating system"))
                    };

                    res_v4.or(res_v6)
                })
                .abortable(),
        );
    }

    pub fn run(&mut self) {
        let mut cur_time = self.sys.time_get();
        self.last_tick_time = cur_time;
        self.last_register_time = None;

        let game_event_generator = self.game_event_generator_server.clone();
        while self.is_open.load(std::sync::atomic::Ordering::Relaxed) {
            cur_time = self.sys.time_get();
            if self
                .last_register_time
                .is_none_or(|time| cur_time - time > Duration::from_secs(10))
            {
                self.register();
                self.last_register_time = Some(cur_time);
            }

            if self
                .has_new_events_server
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                let game_ev_gen = &game_event_generator;
                let mut events = game_ev_gen.events.blocking_lock();
                for (con_id, timestamp, event) in events.drain(..) {
                    match event {
                        GameEvents::NetworkEvent(net_ev) => match net_ev {
                            NetworkEvent::Connected {
                                cert,
                                initial_network_stats,
                                addr,
                            } => {
                                log::debug!(target: "server", "connect time sv: {}", timestamp.as_nanos());
                                self.try_client_connect(
                                    &con_id,
                                    &timestamp,
                                    addr.ip(),
                                    cert,
                                    PlayerNetworkStats {
                                        ping: initial_network_stats.ping,
                                        packet_loss: initial_network_stats.packets_lost as f32
                                            / initial_network_stats.packets_sent.clamp(1, u64::MAX)
                                                as f32,
                                    },
                                );
                            }
                            NetworkEvent::Disconnected(reason) => {
                                log::debug!(target: "server", "got disconnected event from network");

                                let drop_reason = self
                                    .clients
                                    .clients
                                    .get(&con_id)
                                    .and_then(|c| c.drop_reason.clone());
                                if let Some(players) =
                                    self.client_disconnect(&con_id, &reason.to_string())
                                {
                                    for player_id in players.keys() {
                                        self.game_server.player_drop(
                                            player_id,
                                            if let Some(drop_reason) = drop_reason.clone() {
                                                drop_reason
                                            } else if matches!(
                                                reason,
                                                NetworkEventDisconnect::Graceful
                                                    | NetworkEventDisconnect::ConnectionClosed(_)
                                            ) {
                                                PlayerDropReason::Disconnect
                                            } else {
                                                PlayerDropReason::Timeout
                                            },
                                        );
                                    }
                                }
                            }
                            NetworkEvent::NetworkStats(stats) => {
                                log::debug!(target: "server", "server ping: {}", stats.ping.as_millis());
                                let network_stats = PlayerNetworkStats {
                                    ping: stats.ping,
                                    packet_loss: stats.packets_lost as f32
                                        / stats.packets_sent.clamp(1, u64::MAX) as f32,
                                };
                                if let Some(client) = self.clients.clients.get_mut(&con_id) {
                                    client.network_stats = network_stats;
                                } else if let Some(client) =
                                    self.clients.network_clients.get_mut(&con_id)
                                {
                                    client.network_stats = network_stats;
                                } else if let Some(client) =
                                    self.clients.network_queued_clients.get_mut(&con_id)
                                {
                                    client.network_stats = network_stats;
                                }
                                // every second
                                let cur_time = self.sys.time_get();
                                if cur_time - self.last_network_stats_time > Duration::from_secs(1)
                                {
                                    self.last_network_stats_time = cur_time;
                                    let mut player_stats = self.player_network_stats_pool.new();
                                    for client in self.clients.clients.values() {
                                        for player_id in client.players.keys() {
                                            player_stats.insert(*player_id, client.network_stats);
                                        }
                                    }
                                    self.game_server.game.network_stats(player_stats);
                                }
                            }
                            NetworkEvent::ConnectingFailed(_) => {
                                // server usually does not connect, so does not care
                            }
                        },
                        GameEvents::NetworkMsg(game_msg) => {
                            self.handle_msg(&timestamp, &con_id, game_msg)
                        }
                    }
                }
                game_ev_gen
                    .has_events
                    .store(false, std::sync::atomic::Ordering::Relaxed);
            }

            let ticks_in_a_second = self.game_server.game.game_tick_speed();

            // get time before checking ticks
            cur_time = self.sys.time_get();

            // update vote
            if let Some(vote) = &mut self.game_server.cur_vote {
                // check if vote is over
                if vote.state.yes_votes == vote.state.allowed_to_vote_count
                    || vote.state.no_votes == vote.state.allowed_to_vote_count
                    || (vote.state.yes_votes + vote.state.no_votes)
                        == vote.state.allowed_to_vote_count
                    || cur_time - vote.started_at > Duration::from_secs(25)
                {
                    let vote = self.game_server.cur_vote.take().unwrap();
                    // fake democracy
                    if vote.state.yes_votes > vote.state.no_votes {
                        let vote_result =
                            match vote.state.vote {
                                VoteType::Map { key, .. } => {
                                    self.load_map(&key.map.name);
                                    None
                                }
                                VoteType::RandomUnfinishedMap { key } => Some(
                                    self.game_server
                                        .game
                                        .vote_command(VoteCommand::RandomUnfinishedMap(key)),
                                ),
                                VoteType::VoteKickPlayer { .. } => {
                                    if let ServerExtraVoteInfo::Player { to_kick_player, ip } =
                                        &vote.extra_vote_info
                                    {
                                        let until =
                                            Some(chrono::Utc::now() + Duration::from_secs(60 * 15));

                                        let ty = BanType::Custom("by vote".to_string());

                                        // kick that player
                                        let ids =
                                            self.connection_bans.ban_ip(*ip, ty.clone(), until);
                                        for id in ids {
                                            if let Some(c) = self.clients.clients.get_mut(&id) {
                                                c.drop_reason = Some(PlayerDropReason::Banned {
                                                    reason: PlayerBanReason::Vote,
                                                    until,
                                                });
                                            }

                                            self.network.kick(
                                                &id,
                                                KickType::Ban(Banned {
                                                    msg: ty.clone(),
                                                    until,
                                                }),
                                            );
                                        }
                                        self.network.kick(
                                            to_kick_player,
                                            KickType::Ban(Banned { msg: ty, until }),
                                        );
                                    }
                                    None
                                }
                                VoteType::VoteSpecPlayer { key, .. } => {
                                    // try to move player to spec
                                    Some(self.game_server.game.vote_command(
                                        VoteCommand::JoinSpectator(key.voted_player_id),
                                    ))
                                }
                                VoteType::Misc { vote, .. } => {
                                    // exec the vote command in the game
                                    Some(
                                        self.game_server
                                            .game
                                            .vote_command(VoteCommand::Misc(vote.command.clone())),
                                    )
                                }
                            };

                        if let Some(vote_result) = vote_result {
                            for ev in vote_result.events {
                                match ev {
                                    VoteCommandResultEvent::LoadMap { map } => {
                                        self.load_map(&map);
                                    }
                                }
                            }
                        }
                    }

                    self.send_vote(None, Duration::ZERO);
                    self.game_server.game.voted_player(None);
                }
            }

            while is_next_tick(cur_time, &mut self.last_tick_time, ticks_in_a_second) {
                // apply all queued inputs
                if let Some(mut inputs) = self
                    .game_server
                    .queued_inputs
                    .remove(&(self.game_server.cur_monotonic_tick + 1))
                {
                    let mut inps = self.game_server.inps_pool.new();
                    for (player_id, inp) in inputs.drain() {
                        if let Some(player) = self.game_server.players.get_mut(&player_id) {
                            if let Some(diff) =
                                player.inp.try_overwrite(&inp.inp, inp.version(), false)
                            {
                                inps.insert(player_id, CharacterInputInfo { inp: inp.inp, diff });
                            }
                        }
                    }
                    self.game_server.game.set_player_inputs(inps);
                }

                self.game_server.cur_monotonic_tick += 1;

                // game ticks
                let mut tick_res = self.game_server.game.tick(Default::default());

                for event in tick_res.events.drain(..) {
                    match event {
                        TickEvent::Kick { player_id, reason } => {
                            if let Some(player) = self.game_server.players.get(&player_id) {
                                self.network.kick(
                                    &player.network_id,
                                    KickType::Kick(match reason {
                                        PlayerKickReason::Rcon => "by a moderator".to_string(),
                                        PlayerKickReason::Custom(reason) => reason.to_string(),
                                    }),
                                );
                            }
                        }
                        TickEvent::Ban {
                            player_id,
                            until,
                            reason,
                        } => {
                            if let Some(client) =
                                self.game_server.players.get(&player_id).and_then(|player| {
                                    self.clients.clients.get_mut(&player.network_id)
                                })
                            {
                                let ty = match &reason {
                                    PlayerBanReason::Vote => BanType::Custom("by vote".to_string()),
                                    PlayerBanReason::Rcon => BanType::Admin,
                                    PlayerBanReason::Custom(reason) => {
                                        BanType::Custom(reason.to_string())
                                    }
                                };

                                client.drop_reason =
                                    Some(PlayerDropReason::Banned { reason, until });

                                // ban the player
                                let ids = self.connection_bans.ban_ip(client.ip, ty.clone(), until);
                                for id in &ids {
                                    self.network.kick(
                                        id,
                                        KickType::Ban(Banned {
                                            msg: ty.clone(),
                                            until,
                                        }),
                                    );
                                }
                            }
                        }
                    }
                }

                if let Some(shared_info) = self.shared_info.upgrade() {
                    Self::dbg_game(
                        &self.config_game.dbg,
                        &self.last_tick_time,
                        &mut self.game_server.game,
                        Some(self.game_server.players.values().map(|p| &p.inp.inp)),
                        self.game_server.cur_monotonic_tick,
                        ticks_in_a_second.get(),
                        &shared_info,
                        "server",
                    );
                }

                if let Some(recorder) = &mut self.demo_recorder {
                    recorder.add_snapshot(
                        self.game_server.cur_monotonic_tick,
                        self.game_server
                            .game
                            .snapshot_for(SnapshotClientInfo::Everything)
                            .to_vec(),
                    );
                    let events = self.game_server.game.events_for(EventClientInfo {
                        client_player_ids: self.player_ids_pool.new(),
                        everything: true,
                        other_stages: true,
                    });
                    recorder.add_event(
                        self.game_server.cur_monotonic_tick,
                        demo::DemoEvent::Game(events),
                    );
                }

                // snap shot building
                for (con_id, client) in &mut self.clients.clients {
                    let mut player_ids = self.player_ids_pool.new();
                    player_ids.extend(client.players.keys());
                    let snap_client = SnapshotClientInfo::ForPlayerIds(player_ids);

                    let snap_id = client.snap_id;
                    client.snap_id += 1;

                    if client.snap_id % self.config_game.sv.ticks_per_snapshot == 0 {
                        let mut snap = self.game_server.game.snapshot_for(snap_client);

                        // this should be smaller than the number of snapshots saved on the client
                        let as_diff = if client.client_snap_storage.len() < 10 {
                            client.client_snap_storage.insert(
                                snap_id,
                                ClientSnapshotStorage {
                                    snapshot: snap.to_vec(),
                                    monotonic_tick: self.game_server.cur_monotonic_tick,
                                },
                            );
                            true
                        } else {
                            false
                        };

                        let (snap_diff, diff_id, diff_monotonic_tick) =
                            if let Some(latest_client_snap) = &client.latest_client_snap {
                                let mut new_snap = self.player_snap_pool.new();
                                new_snap.resize(snap.len(), Default::default());
                                new_snap.clone_from_slice(&snap);
                                let snap_vec = snap.to_mut();
                                snap_vec.clear();
                                if bin_patch::diff(
                                    &latest_client_snap.snapshot,
                                    &new_snap,
                                    snap_vec,
                                )
                                .is_ok()
                                {
                                    (
                                        snap,
                                        Some(latest_client_snap.snap_id),
                                        Some(latest_client_snap.monotonic_tick),
                                    )
                                } else {
                                    snap_vec.clear();
                                    snap_vec.append(&mut new_snap);

                                    (snap, None, None)
                                }
                            } else {
                                (snap, None, None)
                            };

                        // quickly rewrite the input ack's logic overhead
                        let cur_time = self.sys.time_get();
                        client.inputs_to_ack.iter_mut().for_each(|inp| {
                            inp.logic_overhead = cur_time.saturating_sub(inp.logic_overhead);
                        });
                        self.network.send_unordered_auto_to(
                            &ServerToClientMessage::Snapshot {
                                overhead_time: (self.sys.time_get() - self.last_tick_time),
                                snapshot: snap_diff.as_ref().into(),
                                diff_id,
                                snap_id_diffed: diff_id
                                    .map(|diff_id| snap_id - diff_id)
                                    .unwrap_or(snap_id),
                                game_monotonic_tick_diff: diff_monotonic_tick
                                    .map(|diff_monotonic_tick| {
                                        self.game_server.cur_monotonic_tick - diff_monotonic_tick
                                    })
                                    .unwrap_or(self.game_server.cur_monotonic_tick),
                                as_diff,
                                input_ack: client.inputs_to_ack.as_slice().into(),
                            },
                            con_id,
                        );
                        client.inputs_to_ack.clear();
                    }

                    // events building
                    let mut player_ids = self.player_ids_pool.new();
                    player_ids.extend(client.players.keys());
                    let events = self.game_server.game.events_for(EventClientInfo {
                        client_player_ids: player_ids,
                        everything: false,
                        other_stages: false,
                    });
                    if !events.is_empty() {
                        self.network.send_in_order_to(
                            &ServerToClientMessage::Events {
                                game_monotonic_tick: self.game_server.cur_monotonic_tick,
                                events,
                            },
                            con_id,
                            // If you cannot see "events" in the number 373215, skill issue
                            NetworkInOrderChannel::Custom(373215),
                        );
                    }
                }

                self.game_server.game.clear_events();
            }

            self.game_server.cached_character_infos =
                self.game_server.game.collect_characters_info();

            if let Some(spatial_world) = &mut self.game_server.spatial_world {
                spatial_world.update(&self.network);
            }

            // after tick checks
            // if the game should reload, reload all game related stuff
            // send the client a load event, which is used for map reloads etc.
            if self.game_server.should_reload() {
                self.reload();
            }

            // check db requests
            self.db_requests_helper.clear();
            for db_req in self.db_requests.drain(..) {
                if db_req.is_finished() {
                    match db_req.get_storage() {
                        Ok(req) => match req {
                            GameServerDb::Account(ev) => match ev {
                                GameServerDbAccount::Rename {
                                    account_id,
                                    con_id,
                                    rename_result,
                                } => {
                                    if let (Ok(name), Some(account_id)) =
                                        (&rename_result, account_id)
                                    {
                                        self.game_server.game.account_renamed(account_id, name);
                                    }
                                    if self.clients.clients.contains_key(&con_id) {
                                        self.network.send_unordered_to(
                                            &ServerToClientMessage::AccountRenameRes(rename_result),
                                            &con_id,
                                        );
                                    }
                                }
                                GameServerDbAccount::Info {
                                    con_id,
                                    account_details,
                                } => {
                                    if self.clients.clients.contains_key(&con_id) {
                                        self.network.send_unordered_to(
                                            &ServerToClientMessage::AccountDetails(account_details),
                                            &con_id,
                                        );
                                    }
                                }
                                GameServerDbAccount::AutoLogin {
                                    user_id,
                                    new_account_was_created,
                                } => {
                                    if let Some(account_id) = new_account_was_created
                                        .then_some(user_id.account_id)
                                        .flatten()
                                    {
                                        // A new account was created, tell the game mod
                                        self.game_server
                                            .game
                                            .account_created(account_id, user_id.public_key);
                                    }
                                }
                            },
                        },
                        Err(err) => {
                            log::error!(target: "server-db-requests", "{err}");
                        }
                    }
                } else {
                    self.db_requests_helper.push(db_req);
                }
            }
            std::mem::swap(&mut self.db_requests_helper, &mut self.db_requests);

            // time and sleeps
            cur_time = self.sys.time_get();

            if is_next_tick(
                cur_time,
                &mut self.last_tick_time.clone(), /* <-- dummy */
                ticks_in_a_second,
            ) {
                std::thread::yield_now();
            } else {
                let next_tick_time =
                    time_until_tick(ticks_in_a_second) - (cur_time - self.last_tick_time);

                //let mut guard = self.game_event_generator_server.blocking_lock();
                //guard = guard.ev_cond.wait_timeout(guard.into(), next_tick_time);
                std::thread::sleep(next_tick_time);
            }
        }
    }

    /// Reload the game server with a new map,
    /// and from an optional snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if the game server failed to be set.
    ///
    /// # Panics
    ///
    /// Panics if the game server was successfully set, but an operation
    /// afterwards failed.
    fn load_impl(
        &mut self,
        snapshot: Option<PoolCow<'static, [u8]>>,
        map: &NetworkReducedAsciiString<MAX_MAP_NAME_LEN>,
    ) -> anyhow::Result<()> {
        // reload the whole game server, including the map
        let mod_name = Self::config_physics_mod_name(&self.config_game);
        let (render_mod_name, render_mod_hash, render_mod_required) =
            Self::config_render_mod_name(&self.config_game);
        let config = Self::read_mod_config(&self.io, &mod_name)
            .get_storage()
            .ok();
        self.game_server = ServerGame::new(
            map,
            &mod_name,
            &render_mod_name,
            &render_mod_hash.try_into().unwrap_or_default(),
            render_mod_required,
            config,
            &self.thread_pool,
            &self.io,
            &self.game_db,
            self.accounts.as_ref().map(|a| a.kind),
            self.config_game.sv.spatial_chat,
            self.config_game.sv.download_server_port_v4,
            self.config_game.sv.download_server_port_v6,
        )?;
        if let Some(snapshot) = snapshot {
            self.game_server
                .game
                .build_from_snapshot_by_hotreload(&snapshot);
        }
        // put all players back to a loading state
        self.clients.clients.drain().for_each(|(net_id, client)| {
            self.clients.network_clients.insert(
                net_id,
                ServerNetworkClient {
                    connect_timestamp: client.connect_timestamp,
                    ip: client.ip,
                    auth: client.auth,
                    network_stats: client.network_stats,
                },
            );
        });
        self.clients
            .network_clients
            .iter()
            .for_each(|(net_id, client)| {
                let server_info = MsgSvServerInfo {
                    map: self.game_server.map.name.as_str().try_into().unwrap(),
                    map_blake3_hash: self.game_server.map_blake3_hash,
                    required_resources: self.game_server.required_resources.clone(),
                    game_mod: self.game_server.game_mod.clone(),
                    render_mod: self.game_server.render_mod.clone(),
                    hint_start_camera_pos: self.game_server.game.get_client_camera_join_pos(),
                    resource_server_fallback: self.game_server.http_server.as_ref().map(|server| {
                        match client.ip {
                            IpAddr::V4(_) => server.port_v4,
                            IpAddr::V6(_) => server.port_v6,
                        }
                    }),
                    mod_config: self.game_server.game.info.config.clone(),
                    server_options: self.game_server.game.info.options.clone(),
                    spatial_chat: self.config_game.sv.spatial_chat,
                };
                self.network
                    .send_unordered_to(&ServerToClientMessage::Load(server_info.clone()), net_id);
            });
        self.last_tick_time = self.sys.time_get();

        Ok(())
    }

    fn reload(&mut self) {
        let snapshot = self.game_server.game.snapshot_for_hotreload();
        if let Err(err) = self.load_impl(
            snapshot,
            &self.config_game.sv.map.as_str().try_into().unwrap(),
        ) {
            log::error!("Fatal error during reload: {err}");
        }
    }

    fn load_map(&mut self, map: &NetworkReducedAsciiString<MAX_MAP_NAME_LEN>) {
        self.config_game.sv.map = map.to_string();
        if let Err(err) = self.load_impl(None, map) {
            log::error!("Fatal error during map load: {err}");
        }
    }
}

pub fn load_config() -> (Io, ConfigEngine, ConfigGame) {
    let io = Io::new(
        |rt| {
            Arc::new(
                FileSystem::new(rt, "org", "", "DDNet-Rs-Alpha", "DDNet-Accounts")
                    .expect("most like you are missing a data directory"),
            )
        },
        Arc::new(HttpClient::new_with_bind_addr("0.0.0.0".parse().unwrap()).unwrap_or_default()),
    );

    let config_engine = config_fs::load(&io.clone().into());
    let config_game = game_config_fs::fs::load(&io.clone().into());

    (
        io,
        config_engine.unwrap_or_default(),
        config_game.unwrap_or_default(),
    )
}

pub fn ddnet_server_main<const IS_INTERNAL_SERVER: bool>(
    sys: System,
    cert_and_private_key: (x509_cert::Certificate, SigningKey),
    is_open: Arc<AtomicBool>,
    shared_info: Arc<LocalServerInfo>,
    args: Vec<String>,
    config_overwrite: Option<(ConfigEngine, ConfigGame)>,
) -> anyhow::Result<()> {
    let thread_pool = Arc::new(
        rayon::ThreadPoolBuilder::new()
            .thread_name(|index| format!("server-rayon {index}"))
            .num_threads(
                std::thread::available_parallelism()
                    .unwrap_or(NonZeroUsize::new(2).unwrap())
                    .get()
                    .max(4)
                    - 2,
            )
            .start_handler(|_| {
                if let Err(err) = thread_priority::set_current_thread_priority(
                    thread_priority::ThreadPriority::Min,
                ) {
                    log::info!("failed to apply thread priority to rayon builder: {err}");
                }
            })
            .build()?,
    );

    let (io, config_engine, config_game) = load_config();

    let (config_engine, mut config_game) =
        if let Some((config_engine, config_game)) = config_overwrite {
            (config_engine, config_game)
        } else {
            (config_engine, config_game)
        };

    let rcon_chain = Server::new_rcon_cmd_chain();

    let (msgs, remaining_cmds) =
        Server::handle_config_cmd_lines(&mut config_game, &io, &rcon_chain, args);
    for msg in msgs {
        match msg {
            Ok(msg) => log::info!("{msg}"),
            Err(err) => log::error!("{err}"),
        }
    }

    let mut server = Server::new(
        sys,
        is_open,
        cert_and_private_key,
        shared_info,
        if IS_INTERNAL_SERVER {
            config_game.sv.port_internal
        } else {
            config_game.sv.port_v4
        },
        if IS_INTERNAL_SERVER {
            // force random here, since ipv6 is not used
            0
        } else {
            config_game.sv.port_v6
        },
        config_engine,
        config_game,
        thread_pool,
        io,
        rcon_chain,
    )?;

    // Handle remaining args after the server started.
    for cmd in remaining_cmds {
        match server.handle_cmd_full(cmd, &mut Default::default(), &mut Default::default()) {
            Ok(res) => log::info!("{res}"),
            Err(err) => log::error!("{err}"),
        }
    }

    server.run();

    Ok(())
}
