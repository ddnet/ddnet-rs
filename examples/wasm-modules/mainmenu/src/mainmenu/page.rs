use std::{path::Path, sync::Arc, time::Duration};

use api::IO;
use api_ui_game::render::{
    create_ctf_container, create_emoticons_container, create_entities_container,
    create_flags_container, create_freeze_container, create_game_container, create_hook_container,
    create_hud_container, create_ninja_container, create_particles_container,
    create_skin_container, create_weapon_container,
};
use base_io::{io::Io, runtime::IoRuntimeTask};
use client_containers::{
    ctf::CtfContainer, emoticons::EmoticonsContainer, entities::EntitiesContainer,
    flags::FlagsContainer, freezes::FreezeContainer, game::GameContainer, hooks::HookContainer,
    hud::HudContainer, ninja::NinjaContainer, particles::ParticlesContainer, skins::SkinContainer,
    weapons::WeaponContainer,
};
use client_render_base::{
    map::{map_buffered::TileLayerVisuals, map_pipeline::MapGraphics},
    render::{tee::RenderTee, toolkit::ToolkitRender},
};
use client_ui::{
    events::UiEvents,
    ingame_menu::client_info::ClientInfo,
    main_menu::{
        constants::{MENU_SETTINGS_NAME, MENU_UI_PAGE_QUERY},
        demo_list::{DemoList, DemoListEntry},
        monitors::{UiMonitor, UiMonitorVideoMode, UiMonitors},
        page::MainMenuUi,
        profiles_interface::ProfilesInterface,
        settings::constants::{SETTINGS_SUB_UI_PAGE_QUERY, SETTINGS_UI_PAGE_QUERY},
        theme_container::ThemeContainer,
        user_data::MainMenuInterface,
    },
};
use game_base::server_browser::ServerBrowserPlayer;
use game_base::server_browser::{
    ServerBrowserData, ServerBrowserInfo, ServerBrowserInfoMap, ServerBrowserServer,
    ServerBrowserSkin,
};
use game_config::config::Config;
use graphics::{
    graphics::graphics::Graphics,
    graphics_mt::GraphicsMultiThreaded,
    handles::{
        backend::backend::GraphicsBackendHandle,
        buffer_object::buffer_object::GraphicsBufferObjectHandle,
        canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
        texture::texture::GraphicsTextureHandle,
    },
};
use ui_base::types::{UiRenderPipe, UiState};
use ui_generic::traits::UiPageInterface;

use super::{create_theme_container, profiles::Profiles};

struct MenuImpl {}

impl MainMenuInterface for MenuImpl {
    fn refresh(&mut self) {}

    fn refresh_demo_list(&mut self, _path: &Path) {}
    fn refresh_demo_info(&mut self, _file: Option<&Path>) {}
}

pub struct MainMenu {
    config: Config,

    buffer_object_handle: GraphicsBufferObjectHandle,
    backend_handle: GraphicsBackendHandle,
    stream_handle: GraphicsStreamHandle,
    canvas_handle: GraphicsCanvasHandle,
    texture_handle: GraphicsTextureHandle,
    graphics_mt: GraphicsMultiThreaded,

    skin_container: SkinContainer,
    render_tee: RenderTee,
    flags_container: FlagsContainer,
    toolkit_render: ToolkitRender,
    weapons_container: WeaponContainer,
    hook_container: HookContainer,
    entities_container: EntitiesContainer,
    freeze_container: FreezeContainer,
    emoticons_container: EmoticonsContainer,
    particles_container: ParticlesContainer,
    ninja_container: NinjaContainer,
    game_container: GameContainer,
    hud_container: HudContainer,
    ctf_container: CtfContainer,
    theme_container: ThemeContainer,

    map_render: MapGraphics,
    tile_layer_visuals: Option<TileLayerVisuals>,

    browser_data: ServerBrowserData,
    demos: DemoList,

    servers: Option<IoRuntimeTask<ServerBrowserData>>,

    monitors: UiMonitors,
}

impl MainMenu {
    pub fn new(graphics: &Graphics, io: Io) -> Self {
        let mut config = Config::default();
        config.engine.ui.path.query.insert(
            MENU_UI_PAGE_QUERY.to_string(),
            MENU_SETTINGS_NAME.to_string(),
        );
        config
            .engine
            .ui
            .path
            .query
            .insert(SETTINGS_UI_PAGE_QUERY.to_string(), "Player".to_string());
        config
            .engine
            .ui
            .path
            .query
            .insert(SETTINGS_SUB_UI_PAGE_QUERY.to_string(), "Tee".to_string());

        let mut servers = Vec::new();
        for i in 0..100 {
            servers.push(ServerBrowserServer {
                info: ServerBrowserInfo {
                    name: format!("demo_server {i}").as_str().try_into().unwrap(),
                    game_type: format!("demo_server {i}").as_str().try_into().unwrap(),
                    version: format!("demo_version {i}").as_str().try_into().unwrap(),
                    map: ServerBrowserInfoMap {
                        name: format!("demo_server {i}").as_str().try_into().unwrap(),
                        blake3: Default::default(),
                        size: 0,
                    },
                    players: {
                        let mut players = Vec::new();
                        for _ in 0..100 {
                            players.push(ServerBrowserPlayer {
                                name: "nameless_tee".try_into().unwrap(),
                                score: "999".try_into().unwrap(),
                                skin: ServerBrowserSkin::default(),
                                clan: "brainless".try_into().unwrap(),
                                account_name: Some("test".try_into().unwrap()),
                                flag: "GB".try_into().unwrap(),
                            });
                        }
                        players
                    },
                    max_ingame_players: 64,
                    max_players: 64,
                    max_players_per_client: 4,
                    tournament_mode: false,
                    passworded: false,
                    requires_account: false,
                    cert_sha256_fingerprint: Default::default(),
                },
                addresses: vec![format!("127.0.0.1:{i}").parse().unwrap()],
                location: "default".try_into().unwrap(),
            });
        }
        servers.push(ServerBrowserServer {
            info: ServerBrowserInfo {
                name: "0123456789012345678901234567890123456789"
                    .try_into()
                    .unwrap(),
                game_type: "01234567890123456".try_into().unwrap(),
                version: "01234567890123456".try_into().unwrap(),
                map: ServerBrowserInfoMap {
                    name: "01234567890123456789012345678901".try_into().unwrap(),
                    blake3: Default::default(),
                    size: 0,
                },
                players: {
                    let mut players = Vec::new();
                    for _ in 0..100 {
                        players.push(ServerBrowserPlayer {
                            name: "0123456789012345".try_into().unwrap(),
                            score: "-999999".try_into().unwrap(),
                            skin: ServerBrowserSkin::default(),
                            clan: "012345678901".try_into().unwrap(),
                            account_name: Some("test".try_into().unwrap()),
                            flag: "GB".try_into().unwrap(),
                        });
                    }
                    players
                },
                max_ingame_players: 64,
                max_players: 64,
                max_players_per_client: 4,
                tournament_mode: false,
                passworded: false,
                requires_account: false,
                cert_sha256_fingerprint: Default::default(),
            },
            addresses: vec!["127.0.0.1:1337".parse().unwrap()],
            location: "default".try_into().unwrap(),
        });

        let servers_task = MainMenuUi::req_server_list(&io);

        let mut demos: DemoList = Default::default();
        demos.push(DemoListEntry::Directory {
            name: "auto".to_string(),
        });
        for i in 0..25 {
            demos.push(DemoListEntry::File {
                name: format!("demo{i}.twdemo"),
                date: "2024-07-10".to_string(),
            });
        }

        let mut video_modes = vec![UiMonitorVideoMode {
            width: 1920,
            height: 1080,
            refresh_rate_mhz: 60000,
        }];
        for i in 0..130 {
            video_modes.push(UiMonitorVideoMode {
                width: 2560,
                height: 1440,
                refresh_rate_mhz: 60000 + i * 100,
            })
        }

        let monitors = UiMonitors::new(vec![UiMonitor {
            name: "".to_string(),
            video_modes,
        }]);

        Self {
            config,
            buffer_object_handle: graphics.buffer_object_handle.clone(),
            backend_handle: graphics.backend_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),
            canvas_handle: graphics.canvas_handle.clone(),
            texture_handle: graphics.texture_handle.clone(),
            graphics_mt: graphics.get_graphics_mt(),

            skin_container: create_skin_container(),
            render_tee: RenderTee::new(graphics),
            flags_container: create_flags_container(),
            toolkit_render: ToolkitRender::new(graphics),
            weapons_container: create_weapon_container(),
            hook_container: create_hook_container(),
            entities_container: create_entities_container(),
            freeze_container: create_freeze_container(),
            emoticons_container: create_emoticons_container(),
            particles_container: create_particles_container(),
            ninja_container: create_ninja_container(),
            game_container: create_game_container(),
            hud_container: create_hud_container(),
            ctf_container: create_ctf_container(),
            theme_container: create_theme_container(),

            map_render: MapGraphics::new(&graphics.backend_handle),
            tile_layer_visuals: None,

            browser_data: ServerBrowserData::new(servers, Duration::ZERO),
            demos,

            servers: Some(servers_task).and(None),
            monitors,
        }
    }

    fn render_impl(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut UiRenderPipe<()>,
        ui_state: &mut UiState,
    ) {
        client_ui::main_menu::main_frame::render(
            ui,
            &mut UiRenderPipe::new(
                pipe.cur_time,
                &mut client_ui::main_menu::user_data::UserData {
                    browser_data: &mut self.browser_data,
                    ddnet_info: &Default::default(),
                    demos: &self.demos,
                    demo_info: &None,
                    icons: &mut Default::default(),

                    server_info: &Default::default(),
                    render_options: client_ui::main_menu::user_data::RenderOptions {
                        hide_buttons_icons: false,
                    },
                    main_menu: &mut MenuImpl {},
                    config: &mut self.config,
                    events: &UiEvents::new(),
                    client_info: &ClientInfo::default(),

                    backend_handle: &self.backend_handle,
                    buffer_object_handle: &self.buffer_object_handle,
                    stream_handle: &self.stream_handle,
                    canvas_handle: &self.canvas_handle,
                    texture_handle: &self.texture_handle,
                    graphics_mt: &self.graphics_mt,

                    render_tee: &self.render_tee,
                    skin_container: &mut self.skin_container,
                    flags_container: &mut self.flags_container,
                    toolkit_render: &mut self.toolkit_render,
                    weapons_container: &mut self.weapons_container,
                    hook_container: &mut self.hook_container,
                    entities_container: &mut self.entities_container,
                    freeze_container: &mut self.freeze_container,
                    emoticons_container: &mut self.emoticons_container,
                    particles_container: &mut self.particles_container,
                    ninja_container: &mut self.ninja_container,
                    game_container: &mut self.game_container,
                    hud_container: &mut self.hud_container,
                    ctf_container: &mut self.ctf_container,
                    theme_container: &mut self.theme_container,

                    map_render: &self.map_render,
                    tile_set_preview: &mut self.tile_layer_visuals,

                    spatial_chat: &Default::default(),
                    player_settings_sync: &Default::default(),

                    profiles: &{
                        let profiles: Arc<dyn ProfilesInterface> = Arc::new(Profiles);
                        profiles
                    },
                    profile_tasks: &mut Default::default(),
                    io: &IO.with(|g| (*g).clone()),
                    monitors: &self.monitors,

                    console_entries: &[],
                    parser_cache: &mut Default::default(),

                    raw_input: &Default::default(),
                    features: &Default::default(),
                },
            ),
            ui_state,
        );
    }
}

impl UiPageInterface<()> for MainMenu {
    fn render(&mut self, ui: &mut egui::Ui, pipe: &mut UiRenderPipe<()>, ui_state: &mut UiState) {
        if self
            .servers
            .as_ref()
            .is_some_and(|servers| servers.is_finished())
        {
            if let Ok(servers) = self.servers.take().unwrap().get_storage() {
                self.browser_data = servers;
            }
        }

        self.render_impl(ui, pipe, ui_state)
    }

    fn unmount(&mut self) {
        self.skin_container.clear_except_default();
    }
}
