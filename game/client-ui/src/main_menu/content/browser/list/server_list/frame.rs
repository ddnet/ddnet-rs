use std::net::SocketAddr;

use egui_extras::TableBody;
use game_base::{
    browser_favorite_player::FavoritePlayers,
    local_server_info::LocalServerState,
    server_browser::{ServerBrowserInfo, ServerBrowserServer, ServerFilter, TableSort},
};

use ui_base::types::UiRenderPipe;

use crate::{
    events::UiEvent,
    main_menu::{constants::MENU_LAN_NAME, user_data::UserData},
};

/// server list frame (scrollable)
pub fn render(mut body: TableBody<'_>, pipe: &mut UiRenderPipe<UserData>, cur_page: &str) {
    let ddnet_info = &pipe.user_data.ddnet_info;
    let filter = pipe
        .user_data
        .config
        .storage::<ServerFilter>("browser_filter");
    let favorites = pipe
        .user_data
        .config
        .storage::<FavoritePlayers>("favorite-players");
    let sort = pipe.user_data.config.storage::<TableSort>("browser_sort");
    let servers = pipe.user_data.browser_data.filtered_and_sorted(
        &filter,
        &favorites,
        &sort,
        &ddnet_info.maps,
    );
    struct LanServer {
        server: ServerBrowserServer,
        rcon_secret: Option<[u8; 32]>,
    }
    let server_info = &pipe.user_data.server_info;
    let (sock_addr, rcon_secret, server_cert_hash, server_browser_info, starting) =
        match &*server_info.state.lock().unwrap() {
            LocalServerState::Ready {
                connect_info,
                browser_info,
                ..
            } => (
                Some(connect_info.sock_addr),
                Some(connect_info.rcon_secret),
                Some(connect_info.server_cert_hash),
                browser_info.clone(),
                false,
            ),
            LocalServerState::Starting { .. } => (None, None, None, None, true),
            LocalServerState::None => {
                if cur_page == MENU_LAN_NAME {
                    pipe.user_data.events.push(UiEvent::StartLocalServer);
                }
                (None, None, None, None, true)
            }
        };
    let lan_server = [LanServer {
        server: ServerBrowserServer {
            info: {
                let mut info = server_browser_info.unwrap_or_else(|| ServerBrowserInfo {
                    name: Default::default(),
                    version: Default::default(),
                    game_type: Default::default(),
                    map: Default::default(),
                    players: Default::default(),
                    max_ingame_players: u32::MAX,
                    max_players: u32::MAX,
                    max_players_per_client: u32::MAX,
                    tournament_mode: false,
                    passworded: false,
                    requires_account: false,
                    cert_sha256_fingerprint: Default::default(),
                });

                info.name = if starting {
                    "[Starting...] Internal Server".try_into().unwrap()
                } else {
                    "Internal Server".try_into().unwrap()
                };
                info
            },
            addresses: vec![sock_addr
                .map(|mut addr| {
                    addr.set_ip("127.0.0.1".parse().unwrap());
                    addr
                })
                .unwrap_or(SocketAddr::V4("127.0.0.1:0".parse().unwrap()))],
            location: "default".try_into().unwrap(),
            legacy_server: false,
        },
        rcon_secret,
    }];

    if cur_page == MENU_LAN_NAME {
        pipe.user_data.events.push(UiEvent::CheckLocalServer);
    }

    let select_prev = body
        .ui_mut()
        .ctx()
        .input(|i| i.key_pressed(egui::Key::ArrowUp))
        && body.ui_mut().ctx().memory(|m| m.focused().is_none());
    let select_next = body
        .ui_mut()
        .ctx()
        .input(|i| i.key_pressed(egui::Key::ArrowDown))
        && body.ui_mut().ctx().memory(|m| m.focused().is_none());
    let select_first = body
        .ui_mut()
        .ctx()
        .input(|i| i.key_pressed(egui::Key::PageUp))
        && body.ui_mut().ctx().memory(|m| m.focused().is_none());
    let select_last = body
        .ui_mut()
        .ctx()
        .input(|i| i.key_pressed(egui::Key::PageDown))
        && body.ui_mut().ctx().memory(|m| m.focused().is_none());

    let cur_addr = pipe.user_data.config.storage::<String>("server-addr");

    body.rows(
        30.0,
        if cur_page != MENU_LAN_NAME {
            servers.len()
        } else {
            lan_server.len()
        },
        |mut row| {
            let row_index = row.index();

            let server = if cur_page != MENU_LAN_NAME {
                &servers[row_index]
            } else {
                &lan_server[row_index].server
            };

            let select_index = if select_prev {
                Some(row_index + 1)
            } else if select_next {
                Some(row_index.saturating_sub(1))
            } else if select_first {
                Some(0)
            } else if select_last {
                Some(if cur_page != MENU_LAN_NAME {
                    servers.len().saturating_sub(1)
                } else {
                    lan_server.len().saturating_sub(1)
                })
            } else {
                None
            };

            fn get_addr(addresses: &[SocketAddr]) -> &SocketAddr {
                // generally prefer ipv4
                addresses
                    .iter()
                    .find(|addr| addr.is_ipv4())
                    .unwrap_or(&addresses[0])
            }
            let server_addr = get_addr(&server.addresses);
            let is_selected = server_addr.to_string() == cur_addr;
            row.set_selected(is_selected);
            let (clicked, restart_clicked) =
                super::entry::render(row, server, cur_page == MENU_LAN_NAME);
            let clicked = clicked
                || (cur_page == MENU_LAN_NAME && lan_server.len() == 1)
                || select_index
                    .and_then(|index| {
                        if cur_page != MENU_LAN_NAME {
                            servers.get(index)
                        } else {
                            lan_server.get(index).map(|s| &s.server)
                        }
                    })
                    .is_some_and(|s| get_addr(&s.addresses).to_string() == cur_addr);

            if clicked || is_selected {
                // extra check here, bcs the server addr might be changed by keyboard
                if clicked {
                    pipe.user_data
                        .config
                        .set_storage("server-addr", &server_addr);
                }
                pipe.user_data.config.set_storage(
                    "server-cert",
                    &if cur_page != MENU_LAN_NAME {
                        Some(server.info.cert_sha256_fingerprint)
                    } else {
                        server_cert_hash
                    },
                );
                pipe.user_data
                    .config
                    .set_storage("server-is-legacy", &server.legacy_server);
                if cur_page == MENU_LAN_NAME {
                    pipe.user_data
                        .config
                        .set_storage("rcon-secret", &lan_server[row_index].rcon_secret);
                }
            }
            if restart_clicked {
                *server_info.state.lock().unwrap() = LocalServerState::None;
            }
        },
    );
}
