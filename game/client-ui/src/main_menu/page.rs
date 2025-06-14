use std::{net::SocketAddr, path::Path, sync::Arc, time::Duration};

use anyhow::anyhow;
use base_io::{io::Io, runtime::IoRuntimeTask};
use base_io_traits::{fs_traits::FileSystemEntryTy, http_traits::HttpClientInterface};
use client_containers::{
    container::{Container, ContainerMaxItems},
    utils::{load_containers, RenderGameContainers},
};
use client_render_base::{
    map::{map_buffered::TileLayerVisuals, map_pipeline::MapGraphics},
    render::{tee::RenderTee, toolkit::ToolkitRender},
};
use client_types::console::ConsoleEntry;
use command_parser::parser::ParserCache;
use demo::{
    utils::{decomp, deser, deser_ex},
    DemoHeader, DemoHeaderExt,
};
use game_base::{
    assets_url::HTTP_RESOURCE_URL,
    server_browser::{
        ServerBrowserData, ServerBrowserInfo, ServerBrowserInfoMap, ServerBrowserPlayer,
        ServerBrowserServer, ServerBrowserSkin,
    },
};

use game_base::local_server_info::LocalServerInfo;
use game_config::config::{Config, ConfigGame};
use game_interface::types::{
    character_info::NetworkSkinInfo, render::character::TeeEye, resource_key::NetworkResourceKey,
};
use graphics::{
    graphics::graphics::Graphics,
    graphics_mt::GraphicsMultiThreaded,
    handles::{
        backend::backend::GraphicsBackendHandle,
        buffer_object::buffer_object::GraphicsBufferObjectHandle,
        canvas::canvas::GraphicsCanvasHandle,
        shader_storage::shader_storage::GraphicsShaderStorageHandle,
        stream::stream::GraphicsStreamHandle, texture::texture::GraphicsTextureHandle,
    },
};
use master_server_types::{addr::Protocol, servers::BrowserServers};
use math::colors::legacy_color_to_rgba;
use sound::{scene_object::SceneObject, sound::SoundManager};
use ui_base::types::{UiRenderPipe, UiState};
use ui_generic::traits::UiPageInterface;

use crate::{
    events::UiEvents,
    ingame_menu::{client_info::ClientInfo, raw_input_info::RawInputInfo},
    main_menu::user_data::MainMenuInterface,
    thumbnail_container::{
        load_thumbnail_container, ThumbnailContainer, DEFAULT_THUMBNAIL_CONTAINER_PATH,
    },
};

use super::{
    communities::CommunityIcons,
    ddnet_info::DdnetInfo,
    demo_list::{DemoList, DemoListEntry},
    features::EnabledFeatures,
    legacy_server_list::LegacyServerList,
    main_frame,
    monitors::UiMonitors,
    player_settings_ntfy::PlayerSettingsSync,
    profiles_interface::ProfilesInterface,
    spatial_chat::SpatialChat,
    theme_container::{ThemeContainer, THEME_CONTAINER_PATH},
    user_data::{ProfileTasks, RenderOptions, UserData},
};

pub struct MainMenuIo {
    pub(crate) io: Io,
    cur_servers_task: Option<IoRuntimeTask<Vec<ServerBrowserServer>>>,
    cur_ddnet_info_task: Option<IoRuntimeTask<String>>,
    cur_demos_task: Option<IoRuntimeTask<DemoList>>,
    cur_demo_info_task: Option<IoRuntimeTask<(DemoHeader, DemoHeaderExt)>>,
    remove_demo_info: bool,
}

impl MainMenuInterface for MainMenuIo {
    fn refresh(&mut self) {
        self.cur_servers_task = Some(MainMenuUi::req_server_list(&self.io));
    }

    fn refresh_demo_list(&mut self, path: &Path) {
        self.cur_demos_task = Some(MainMenuUi::req_demo_list(&self.io, path));
    }

    fn refresh_demo_info(&mut self, file: Option<&Path>) {
        if let Some(file) = file {
            self.cur_demo_info_task = Some(MainMenuUi::req_demo_info(&self.io, file));
        } else {
            self.cur_demo_info_task = None;
            self.remove_demo_info = true;
        }
    }
}

pub struct MainMenuUi {
    pub(crate) server_info: Arc<LocalServerInfo>,
    pub(crate) client_info: ClientInfo,
    pub(crate) browser_data: ServerBrowserData,
    pub(crate) ddnet_info: DdnetInfo,
    pub(crate) community_icons: CommunityIcons,

    pub(crate) demos: DemoList,
    pub(crate) demo_info: Option<(DemoHeader, DemoHeaderExt)>,

    menu_io: MainMenuIo,
    io: Io,
    pub(crate) scene: SceneObject,

    events: UiEvents,

    pub shader_storage_handle: GraphicsShaderStorageHandle,
    pub buffer_object_handle: GraphicsBufferObjectHandle,
    pub backend_handle: GraphicsBackendHandle,
    pub stream_handle: GraphicsStreamHandle,
    pub canvas_handle: GraphicsCanvasHandle,
    pub texture_handle: GraphicsTextureHandle,
    pub graphics_mt: GraphicsMultiThreaded,

    pub containers: RenderGameContainers,
    pub theme_container: ThemeContainer,
    pub community_icon_container: ThumbnailContainer,

    pub render_tee: RenderTee,
    pub toolkit_render: ToolkitRender,
    pub map_render: MapGraphics,
    pub tile_layer_visuals: Option<TileLayerVisuals>,

    pub profiles: Arc<dyn ProfilesInterface>,
    pub profile_tasks: ProfileTasks,

    pub monitors: UiMonitors,
    spatial_chat: SpatialChat,
    player_settings_sync: PlayerSettingsSync,

    console_entries: Vec<ConsoleEntry>,
    parser_cache: ParserCache,

    raw_input_info: RawInputInfo,
    features: EnabledFeatures,
}

impl MainMenuUi {
    fn req_demo_list(io: &Io, path: &Path) -> IoRuntimeTask<DemoList> {
        let fs = io.fs.clone();
        let path = path.to_path_buf();
        io.rt
            .spawn(async move {
                Ok(fs
                    .entries_in_dir(&path)
                    .await?
                    .into_iter()
                    .map(|(f, ty)| match ty {
                        FileSystemEntryTy::File { date } => DemoListEntry::File { name: f, date },
                        FileSystemEntryTy::Directory => DemoListEntry::Directory { name: f },
                    })
                    .collect())
            })
            .cancelable()
    }

    fn req_demo_info(io: &Io, file: &Path) -> IoRuntimeTask<(DemoHeader, DemoHeaderExt)> {
        let fs = io.fs.clone();
        let file = file.to_path_buf();
        io.rt
            .spawn(async move {
                let demo = fs.read_file(&file).await?;

                let mut writer: Vec<u8> = Default::default();

                // read header
                let (header, file_off): (DemoHeader, usize) = deser_ex(&demo, true)?;
                let demo = &demo[file_off..];

                // read header ext
                let (header_ext, _): (DemoHeaderExt, usize) =
                    deser(decomp(&demo[0..header.size_ext as usize], &mut writer)?)?;

                Ok((header, header_ext))
            })
            .cancelable()
    }

    pub async fn download_server_list(
        http: &Arc<dyn HttpClientInterface>,
    ) -> anyhow::Result<Vec<ServerBrowserServer>> {
        Self::json_to_server_browser(
            &http
                .download_text(
                    "https://pg.ddnet.org:4444/ddnet/15/servers.json"
                        .try_into()
                        .unwrap(),
                )
                .await?,
        )
    }

    pub fn legacy_json_to_server_browser(
        servers_raw: &str,
    ) -> anyhow::Result<Vec<ServerBrowserServer>> {
        let servers: LegacyServerList = match serde_json::from_str(servers_raw) {
            Ok(servers) => servers,
            Err(err) => {
                log::error!("could not parse servers json: {err}");
                return Err(err.into());
            }
        };

        let parsed_servers: Vec<ServerBrowserServer> = servers
            .servers
            .into_iter()
            .filter_map(|server| {
                if server
                    .addresses
                    .iter()
                    .any(|addr| addr.protocol == Protocol::V6)
                {
                    let info = server.info;
                    Some(ServerBrowserServer {
                        addresses: server
                            .addresses
                            .into_iter()
                            .filter(|addr| addr.protocol == Protocol::V6)
                            .map(|addr| SocketAddr::new(addr.ip, addr.port))
                            .collect(),
                        info: ServerBrowserInfo {
                            name: info.name.try_into().unwrap_or_default(),
                            game_type: info.game_type.try_into().unwrap_or_default(),
                            version: info.version.try_into().unwrap_or_default(),
                            map: ServerBrowserInfoMap {
                                name: info.map.name.as_str().try_into().unwrap_or_default(),
                                blake3: Default::default(),
                                size: 0,
                            },
                            players: info
                                .clients
                                .into_iter()
                                .map(|c| ServerBrowserPlayer {
                                    score: c.score.to_string().try_into().unwrap_or_default(),
                                    skin: c
                                        .skin
                                        .map(|s| ServerBrowserSkin {
                                            name: s
                                                .name
                                                .and_then(|n| n.as_str().try_into().ok())
                                                .unwrap_or_else(|| {
                                                    NetworkResourceKey::new("").unwrap()
                                                }),
                                            info: {
                                                if let Some((color_body, color_feet)) =
                                                    s.color_body.zip(s.color_feet)
                                                {
                                                    let body_color = legacy_color_to_rgba(
                                                        color_body, true, true,
                                                    );
                                                    let feet_color = legacy_color_to_rgba(
                                                        color_feet, true, true,
                                                    );
                                                    NetworkSkinInfo::Custom {
                                                        body_color,
                                                        feet_color,
                                                    }
                                                } else {
                                                    NetworkSkinInfo::Original
                                                }
                                            },
                                            eye: if c.afk { TeeEye::Blink } else { TeeEye::Normal },
                                        })
                                        .unwrap_or_default(),
                                    name: c.name.try_into().unwrap_or_default(),
                                    clan: c.clan.try_into().unwrap_or_default(),
                                    account_name: None,
                                    // TODO
                                    flag: "".try_into().unwrap(),
                                })
                                .collect(),
                            max_ingame_players: info.max_players,
                            max_players: info.max_players,
                            max_players_per_client: 1,
                            passworded: info.passworded,
                            tournament_mode: false,
                            cert_sha256_fingerprint: Default::default(),
                            requires_account: info.requires_login,
                        },
                        location: server.location.try_into().unwrap_or_default(),

                        legacy_server: true,
                    })
                } else {
                    None
                }
            })
            .collect();
        Ok(parsed_servers)
    }

    pub async fn download_legacy_server_list(
        http: &Arc<dyn HttpClientInterface>,
    ) -> anyhow::Result<Vec<ServerBrowserServer>> {
        Self::legacy_json_to_server_browser(
            &http
                .download_text(
                    "https://master1.ddnet.org/ddnet/15/servers.json"
                        .try_into()
                        .unwrap(),
                )
                .await?,
        )
    }

    pub fn req_server_list(io: &Io) -> IoRuntimeTask<Vec<ServerBrowserServer>> {
        let http = io.http.clone();
        io.rt
            .spawn(async move {
                let res = Self::download_server_list(&http).await;

                let res_legacy = Self::download_legacy_server_list(&http).await;

                match (res, res_legacy) {
                    (Ok(res), Ok(res_legacy)) => Ok([res, res_legacy].concat()),
                    (Ok(res), Err(_)) | (Err(_), Ok(res)) => Ok(res),
                    (Err(err), Err(_)) => Err(err),
                }
            })
            .cancelable()
    }

    fn req_ddnet_info(io: &Io, name: &str) -> IoRuntimeTask<String> {
        let http = io.http.clone();
        let name = name.to_string();
        io.rt
            .spawn(async move {
                Ok(http
                    .download_text(
                        format!(
                            "https://info.ddnet.org/info?name={}",
                            urlencoding::encode(&name)
                        )
                        .as_str()
                        .try_into()
                        .unwrap(),
                    )
                    .await?)
            })
            .cancelable()
    }

    pub fn new(
        graphics: &Graphics,
        sound: &SoundManager,
        server_info: Arc<LocalServerInfo>,
        client_info: ClientInfo,
        events: UiEvents,
        io: Io,
        tp: Arc<rayon::ThreadPool>,
        profiles: Arc<dyn ProfilesInterface>,
        monitors: UiMonitors,
        spatial_chat: SpatialChat,
        player_settings_sync: PlayerSettingsSync,
        config_game: &ConfigGame,
        console_entries: Vec<ConsoleEntry>,
        raw_input_info: RawInputInfo,
        browser_data: ServerBrowserData,
        features: EnabledFeatures,
    ) -> Self {
        let cur_servers_task = Self::req_server_list(&io);
        let cur_ddnet_info_task = Self::req_ddnet_info(
            &io,
            config_game
                .players
                .get(config_game.profiles.main as usize)
                .map(|p| p.name.as_str())
                .unwrap_or(""),
        );

        let mut profile_tasks: ProfileTasks = Default::default();
        let profiles_task = profiles.clone();
        profile_tasks.user_interactions.push(
            io.rt
                .spawn(async move { profiles_task.user_interaction().await })
                .cancelable(),
        );

        let scene = sound.scene_handle.create(Default::default());

        let containers = load_containers(
            &io,
            &tp,
            Some(HTTP_RESOURCE_URL.try_into().unwrap()),
            None,
            true,
            graphics,
            sound,
            &scene,
        );

        let load_thumbnail_container = |path: &str, container_name: &str| {
            load_thumbnail_container(
                io.clone(),
                tp.clone(),
                path,
                container_name,
                graphics,
                sound,
                scene.clone(),
                None,
            )
        };
        let theme_container = load_thumbnail_container(THEME_CONTAINER_PATH, "theme-container");
        let community_icon_container =
            load_thumbnail_container(DEFAULT_THUMBNAIL_CONTAINER_PATH, "community-icon-container");

        let tile_layer_visuals = None;
        Self {
            server_info,
            client_info,

            browser_data,
            ddnet_info: DdnetInfo::default(),
            demos: DemoList::default(),
            demo_info: None,
            community_icons: Default::default(),

            menu_io: MainMenuIo {
                io: io.clone(),
                cur_ddnet_info_task: Some(cur_ddnet_info_task),
                cur_servers_task: Some(cur_servers_task),
                cur_demos_task: None,
                cur_demo_info_task: None,
                remove_demo_info: false,
            },
            io: io.clone(),
            scene,

            events,

            shader_storage_handle: graphics.shader_storage_handle.clone(),
            buffer_object_handle: graphics.buffer_object_handle.clone(),
            backend_handle: graphics.backend_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),
            canvas_handle: graphics.canvas_handle.clone(),
            texture_handle: graphics.texture_handle.clone(),
            graphics_mt: graphics.get_graphics_mt(),

            render_tee: RenderTee::new(graphics),
            toolkit_render: ToolkitRender::new(graphics),
            containers,
            theme_container,
            community_icon_container,
            map_render: MapGraphics::new(&graphics.backend_handle),
            tile_layer_visuals,

            profiles,
            profile_tasks,
            monitors,
            spatial_chat,
            player_settings_sync,

            console_entries,
            parser_cache: Default::default(),

            raw_input_info,
            features,
        }
    }

    pub(crate) fn get_user_data<'a>(
        &'a mut self,
        config: &'a mut Config,
        hide_buttons_right: bool,
    ) -> UserData<'a> {
        UserData {
            server_info: &self.server_info,
            client_info: &self.client_info,
            ddnet_info: &self.ddnet_info,
            icons: &mut self.community_icons,

            browser_data: &mut self.browser_data,
            demos: &self.demos,
            demo_info: &self.demo_info,

            render_options: RenderOptions {
                hide_buttons_icons: hide_buttons_right,
            },

            main_menu: &mut self.menu_io,
            config,
            events: &self.events,

            backend_handle: &self.backend_handle,
            shader_storage_handle: &self.shader_storage_handle,
            buffer_object_handle: &self.buffer_object_handle,
            stream_handle: &self.stream_handle,
            canvas_handle: &self.canvas_handle,
            texture_handle: &self.texture_handle,
            graphics_mt: &self.graphics_mt,

            render_tee: &self.render_tee,
            skin_container: &mut self.containers.skin_container,
            flags_container: &mut self.containers.flags_container,

            toolkit_render: &self.toolkit_render,
            weapons_container: &mut self.containers.weapon_container,
            hook_container: &mut self.containers.hook_container,
            entities_container: &mut self.containers.entities_container,
            freeze_container: &mut self.containers.freeze_container,
            emoticons_container: &mut self.containers.emoticons_container,
            particles_container: &mut self.containers.particles_container,
            ninja_container: &mut self.containers.ninja_container,
            game_container: &mut self.containers.game_container,
            hud_container: &mut self.containers.hud_container,
            ctf_container: &mut self.containers.ctf_container,
            theme_container: &mut self.theme_container,

            map_render: &self.map_render,
            tile_set_preview: &mut self.tile_layer_visuals,

            spatial_chat: &self.spatial_chat,
            player_settings_sync: &self.player_settings_sync,

            profiles: &self.profiles,
            profile_tasks: &mut self.profile_tasks,
            io: &self.io,
            monitors: &self.monitors,

            console_entries: &self.console_entries,
            parser_cache: &mut self.parser_cache,

            raw_input: &self.raw_input_info,
            features: &self.features,
        }
    }

    pub fn json_to_server_browser(servers_raw: &str) -> anyhow::Result<Vec<ServerBrowserServer>> {
        let servers: BrowserServers = match serde_json::from_str(servers_raw) {
            Ok(servers) => servers,
            Err(err) => {
                log::error!("could not parse servers json: {err}");
                return Err(err.into());
            }
        };

        let parsed_servers: Vec<ServerBrowserServer> = servers
            .servers
            .into_iter()
            .filter_map(|server| {
                if server
                    .addresses
                    .iter()
                    .any(|addr| addr.protocol == Protocol::VPg)
                {
                    let info: serde_json::Result<ServerBrowserInfo> =
                        serde_json::from_str(server.info.get());
                    match info {
                        Ok(info) => Some(ServerBrowserServer {
                            addresses: server
                                .addresses
                                .0
                                .into_iter()
                                .filter(|addr| addr.protocol == Protocol::VPg)
                                .map(|addr| SocketAddr::new(addr.ip, addr.port))
                                .collect(),
                            info,
                            location: server
                                .location
                                .map(|l| l.as_str().try_into().unwrap())
                                .unwrap_or_default(),

                            legacy_server: false,
                        }),
                        Err(err) => {
                            log::error!("ServerBrowserInfo could not be parsed: {err}");
                            None
                        }
                    }
                } else {
                    None
                }
            })
            .collect();
        Ok(parsed_servers)
    }

    pub fn check_tasks(&mut self, cur_time: &Duration) {
        if let Some(server_task) = &self.menu_io.cur_servers_task {
            if server_task.is_finished() {
                match self.menu_io.cur_servers_task.take().unwrap().get() {
                    Ok(servers_raw) => {
                        self.browser_data.set_servers(servers_raw, *cur_time);
                    }
                    Err(err) => {
                        log::error!("failed to download master server list: {err}");
                    }
                }
            }
        }
        if let Some(server_task) = &self.menu_io.cur_ddnet_info_task {
            if server_task.is_finished() {
                match self
                    .menu_io
                    .cur_ddnet_info_task
                    .take()
                    .unwrap()
                    .get()
                    .and_then(|s| serde_json::from_str(&s).map_err(|err| anyhow!(err)))
                {
                    Ok(ddnet_info) => {
                        self.ddnet_info = ddnet_info;
                    }
                    Err(err) => {
                        log::error!("failed to download ddnet info: {err}");
                    }
                }
            }
        }
        if let Some(task) = &self.menu_io.cur_demos_task {
            if task.is_finished() {
                match self.menu_io.cur_demos_task.take().unwrap().get() {
                    Ok(demos) => {
                        self.demos = demos;
                    }
                    Err(err) => {
                        log::error!("failed to get demo list: {err}");
                    }
                }
            }
        }
        if let Some(task) = &self.menu_io.cur_demo_info_task {
            if task.is_finished() {
                match self.menu_io.cur_demo_info_task.take().unwrap().get() {
                    Ok((header, header_ext)) => {
                        self.demo_info = Some((header, header_ext));
                    }
                    Err(err) => {
                        log::error!("failed to get demo info: {err}");
                    }
                }
            }
        }
        if std::mem::take(&mut self.menu_io.remove_demo_info) {
            self.demo_info = None;
        }
    }

    pub(crate) fn update_container<A, L>(container: &mut Container<A, L>, cur_time: &Duration)
    where
        L: client_containers::container::ContainerLoad<A> + Sync + Send + 'static,
    {
        let el = Duration::from_secs(10);
        let ui = Duration::from_secs(1);
        let max_items_el = Duration::from_millis(100);
        let max_items = Some(ContainerMaxItems {
            count: 256.try_into().unwrap(),
            entry_lifetime: &max_items_el,
        });
        // only update if default is already loaded, else this loads all default items
        if container.is_default_loaded() {
            container.update(cur_time, &el, &ui, [].iter(), max_items);
        }
    }

    pub fn update(&mut self, cur_time: &Duration) {
        Self::update_container(&mut self.containers.ctf_container, cur_time);
        Self::update_container(&mut self.containers.emoticons_container, cur_time);
        Self::update_container(&mut self.containers.entities_container, cur_time);
        Self::update_container(&mut self.containers.freeze_container, cur_time);
        Self::update_container(&mut self.containers.game_container, cur_time);
        Self::update_container(&mut self.containers.hook_container, cur_time);
        Self::update_container(&mut self.containers.hud_container, cur_time);
        Self::update_container(&mut self.containers.ninja_container, cur_time);
        Self::update_container(&mut self.containers.particles_container, cur_time);
        Self::update_container(&mut self.containers.weapon_container, cur_time);
        Self::update_container(&mut self.containers.flags_container, cur_time);
        Self::update_container(&mut self.containers.skin_container, cur_time);
        Self::update_container(&mut self.theme_container, cur_time);
        Self::update_container(&mut self.community_icon_container, cur_time);
    }
}

impl UiPageInterface<Config> for MainMenuUi {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut UiRenderPipe<Config>,
        ui_state: &mut UiState,
    ) {
        self.check_tasks(&pipe.cur_time);

        main_frame::render(
            ui,
            &mut UiRenderPipe {
                cur_time: pipe.cur_time,
                user_data: &mut self.get_user_data(pipe.user_data, false),
            },
            ui_state,
        );

        self.update(&pipe.cur_time);
    }

    fn unmount(&mut self) {
        self.containers.clear_except_default();
        self.theme_container.clear_except_default();
        self.community_icon_container.clear_except_default();
        self.profile_tasks = Default::default();
        self.menu_io.cur_servers_task = None;
    }
}
