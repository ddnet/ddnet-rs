use std::time::Duration;

use chrono::DateTime;
use client_ui::{connect::user_data::ConnectModes, ingame_menu::server_info::GameServerInfo};
use game_network::game_event_generator::GameEvents;
use math::math::vector::luffixed;

use network::network::event::{
    NetworkEvent, NetworkEventConnectingClosed, NetworkEventConnectingFailed,
    NetworkEventDisconnect,
};
use sound::scene_object::SceneObject;

use crate::game::Game;

use super::game::types::GameMsgPipeline;

pub struct GameEventPipeline<'a> {
    pub game: &'a mut Game,
    pub msgs: &'a mut GameMsgPipeline<'a>,
    pub game_server_info: &'a GameServerInfo,
    pub spatial_chat_scene: &'a SceneObject,
}

pub struct GameEventsClient {}

impl GameEventsClient {
    pub fn update(pipe: &mut GameEventPipeline<'_>) {
        let event_gen = match pipe.game {
            Game::None | Game::Err(_) | Game::PrepareConnect(_) => None,
            Game::Connecting(game) => Some((
                &game.network.has_new_events_client,
                &game.network.game_event_generator_client,
            )),
            Game::Loading(game) => Some((
                &game.network.has_new_events_client,
                &game.network.game_event_generator_client,
            )),
            Game::Active(game) | Game::WaitingForFirstSnapshot(game) => Some((
                &game.network.has_new_events_client,
                &game.network.game_event_generator_client,
            )),
        };

        if event_gen
            .as_ref()
            .is_some_and(|(has_events, _)| has_events.load(std::sync::atomic::Ordering::Relaxed))
        {
            let (has_events, events) = event_gen.unwrap();
            let mut events_guard = events.events.blocking_lock();
            has_events.store(false, std::sync::atomic::Ordering::Relaxed);
            let events = std::mem::take(&mut *events_guard);
            drop(events_guard);

            for (_, timestamp, event) in events {
                match event {
                    GameEvents::NetworkEvent(net_ev) => match net_ev {
                        NetworkEvent::Connected { .. } => {}
                        NetworkEvent::Disconnected(reason) => {
                            if matches!(reason, NetworkEventDisconnect::Graceful) {
                                pipe.msgs.config.ui.path.route("");
                            } else {
                                let connect_info = match pipe.game {
                                    Game::None | Game::Err(_) => None,
                                    Game::PrepareConnect(game) => Some(&game.connect.mode),
                                    Game::Connecting(game) => Some(&game.connect.mode),
                                    Game::Loading(game) => Some(&game.connect.mode),
                                    Game::WaitingForFirstSnapshot(game) => Some(&game.connect.mode),
                                    Game::Active(game) => Some(&game.connect.mode),
                                };
                                if let Some(connect_info) = connect_info {
                                    connect_info.set(ConnectModes::DisconnectErr {
                                        msg: match reason {
                                            NetworkEventDisconnect::ConnectionClosed(
                                                NetworkEventConnectingClosed::Banned(ban),
                                            ) => {
                                                format!(
                                                    "banned {}{}",
                                                    ban.msg,
                                                    if let Some(until) = ban.until {
                                                        format!(
                                                            " until {}",
                                                            <DateTime::<chrono::Local>>::from(
                                                                until
                                                            )
                                                        )
                                                    } else {
                                                        "".to_string()
                                                    }
                                                )
                                            }
                                            _ => reason.to_string(),
                                        },
                                    });
                                }
                                pipe.msgs.config.ui.path.route("connect");
                            }
                            pipe.msgs.ui.is_ui_open = true;
                            *pipe.game = Game::None;
                        }
                        NetworkEvent::NetworkStats(stats) => {
                            if let Game::Active(game) = pipe.game {
                                // Note: we ignore the ping of the connection stats... too unreliable, we use the one
                                // generated by snap/input ack instead.
                                let predict_timing = &mut game.game_data.prediction_timer;
                                predict_timing.add_packet_stats(
                                    timestamp,
                                    stats.packets_sent,
                                    stats.packets_lost,
                                );

                                let byte_stats = &mut game.game_data.net_byte_stats;
                                byte_stats.bytes_per_sec_sent = (byte_stats.bytes_per_sec_sent
                                    * luffixed::from_num(50)
                                    / luffixed::from_num(100))
                                    + luffixed::from_num(
                                        stats.bytes_sent.saturating_sub(byte_stats.last_bytes_sent),
                                    ) / luffixed::from_num(
                                        timestamp
                                            .saturating_sub(byte_stats.last_timestamp)
                                            .max(Duration::from_micros(1))
                                            .as_nanos(),
                                    )
                                    .saturating_div(
                                        luffixed::from_num(Duration::from_secs(1).as_nanos()),
                                    ) * luffixed::from_num(50)
                                        / luffixed::from_num(100);
                                byte_stats.bytes_per_sec_recv = (byte_stats.bytes_per_sec_recv
                                    * luffixed::from_num(50)
                                    / luffixed::from_num(100))
                                    + luffixed::from_num(
                                        stats.bytes_recv.saturating_sub(byte_stats.last_bytes_recv),
                                    ) / luffixed::from_num(
                                        timestamp
                                            .saturating_sub(byte_stats.last_timestamp)
                                            .max(Duration::from_micros(1))
                                            .as_nanos(),
                                    )
                                    .saturating_div(
                                        luffixed::from_num(Duration::from_secs(1).as_nanos()),
                                    ) * luffixed::from_num(50)
                                        / luffixed::from_num(100);

                                byte_stats.last_timestamp = timestamp;
                                byte_stats.last_bytes_sent = stats.bytes_sent;
                                byte_stats.last_bytes_recv = stats.bytes_recv;

                                if game
                                    .game_data
                                    .last_keep_alive_id_and_time
                                    .0
                                    .is_none_or(|last_id| stats.last_keep_alive_id != last_id)
                                    && timestamp >= game.game_data.last_keep_alive_id_and_time.1
                                {
                                    game.game_data.last_keep_alive_id_and_time =
                                        (Some(stats.last_keep_alive_id), timestamp);
                                }
                            }
                        }
                        NetworkEvent::ConnectingFailed(reason) => {
                            if let Game::Connecting(game) = pipe.game {
                                game.connect.mode.set(ConnectModes::ConnectingErr {
                                    msg: match reason {
                                        NetworkEventConnectingFailed::ConnectionClosed(
                                            NetworkEventConnectingClosed::Banned(ban),
                                        ) => {
                                            format!(
                                                "banned {}{}",
                                                ban.msg,
                                                if let Some(until) = ban.until {
                                                    format!(
                                                        " until {}",
                                                        <DateTime::<chrono::Local>>::from(until)
                                                    )
                                                } else {
                                                    "".to_string()
                                                }
                                            )
                                        }
                                        _ => reason.to_string(),
                                    },
                                });
                            }
                            pipe.msgs.config.ui.path.route("connect");
                        }
                    },
                    GameEvents::NetworkMsg(game_msg) => {
                        pipe.game.on_msg(
                            timestamp,
                            game_msg,
                            pipe.msgs,
                            pipe.game_server_info,
                            pipe.spatial_chat_scene,
                        );
                    }
                }
            }
        }
    }
}
