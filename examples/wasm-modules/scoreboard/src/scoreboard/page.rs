use std::time::Duration;

use api_ui_game::render::{create_flags_container, create_skin_container};
use base::{
    linked_hash_map_view::FxLinkedHashMap,
    network_string::{NetworkString, PoolNetworkString},
};
use client_containers::{flags::FlagsContainer, skins::SkinContainer};
use client_render_base::render::tee::RenderTee;
use game_interface::types::{
    character_info::{NetworkCharacterInfo, NetworkSkinInfo},
    id_gen::IdGenerator,
    id_types::CharacterId,
    network_stats::PlayerNetworkStats,
    render::{
        character::{
            CharacterInfo, CharacterPlayerInfo, PlayerCameraMode, PlayerIngameMode, TeeEye,
        },
        scoreboard::{
            ScoreboardCharacterInfo, ScoreboardConnectionType, ScoreboardGameOptions,
            ScoreboardGameType, ScoreboardGameTypeOptions, ScoreboardScoreType,
            ScoreboardStageInfo,
        },
    },
};
use graphics::{
    graphics::graphics::Graphics,
    handles::{canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle},
};
use math::math::vector::ubvec4;
use pool::{
    datatypes::{PoolFxLinkedHashMap, PoolVec},
    rc::PoolRc,
};
use ui_base::types::{UiRenderPipe, UiState};
use ui_generic::traits::UiPageInterface;

pub struct Scoreboard {
    stream_handle: GraphicsStreamHandle,
    canvas_handle: GraphicsCanvasHandle,
    skin_container: SkinContainer,
    render_tee: RenderTee,
    flags_container: FlagsContainer,
}

impl Scoreboard {
    pub fn new(graphics: &Graphics) -> Self {
        Self {
            stream_handle: graphics.stream_handle.clone(),
            canvas_handle: graphics.canvas_handle.clone(),
            skin_container: create_skin_container(),
            render_tee: RenderTee::new(graphics),
            flags_container: create_flags_container(),
        }
    }

    fn render_impl(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut UiRenderPipe<()>,
        ui_state: &mut UiState,
    ) {
        let mut red_stages = PoolFxLinkedHashMap::new_without_pool();
        let mut blue_stages = PoolFxLinkedHashMap::new_without_pool();

        let mut red_players = PoolVec::new_without_pool();
        let mut character_infos: FxLinkedHashMap<CharacterId, CharacterInfo> = Default::default();
        let g = IdGenerator::new();
        for i in 0..64 {
            let id = g.next_id();
            character_infos.insert(
                id,
                CharacterInfo {
                    info: PoolRc::from_item_without_pool({
                        let mut info = NetworkCharacterInfo::explicit_default();

                        info.skin = "WWWWWWWWWWWWWWW".try_into().unwrap();
                        info.skin_info = NetworkSkinInfo::Custom {
                            body_color: ubvec4::new(255, 255, 255, 255),
                            feet_color: ubvec4::new(0, 255, 255, 255),
                        };
                        info.name = NetworkString::new("WWWWWWWWWWWWWWW").unwrap();
                        info.clan = NetworkString::new("MWWWWWWWWWWW").unwrap();
                        info.flag = NetworkString::new("CH").unwrap();

                        info
                    }),
                    skin_info: NetworkSkinInfo::Custom {
                        body_color: ubvec4::new(255, 255, 255, 255),
                        feet_color: ubvec4::new(0, 255, 255, 255),
                    },
                    laser_info: Default::default(),
                    stage_id: None,
                    side: None,
                    player_info: Some(CharacterPlayerInfo {
                        cam_mode: PlayerCameraMode::Default,
                        force_scoreboard_visible: false,
                        ingame_mode: PlayerIngameMode::InGame {
                            in_custom_stage: false,
                        },
                    }),
                    browser_score: PoolNetworkString::from_without_pool("999".try_into().unwrap()),
                    browser_eye: TeeEye::Normal,
                    account_name: Some(PoolNetworkString::from_without_pool(
                        "testname".try_into().unwrap(),
                    )),
                },
            );

            red_players.push(ScoreboardCharacterInfo {
                id,
                score: ScoreboardScoreType::Points(999),
                ping: ScoreboardConnectionType::Network(PlayerNetworkStats {
                    ping: Duration::from_millis(999),
                    ..Default::default()
                }),
            });

            if i % 3 == 0 {
                red_stages.insert(
                    g.next_id(),
                    ScoreboardStageInfo {
                        characters: std::mem::replace(
                            &mut red_players,
                            PoolVec::new_without_pool(),
                        ),
                        max_size: 0,
                        name: PoolNetworkString::from_without_pool("TEST".try_into().unwrap()),
                        color: ubvec4::new(
                            (i % 256) as u8,
                            255 - (i % 256) as u8,
                            255 * (i % 2) as u8,
                            20,
                        ),
                        score: ScoreboardScoreType::Points(999),
                    },
                );
            }
        }
        let mut blue_players = PoolVec::new_without_pool();
        for i in 0..12 {
            let id = g.next_id();
            character_infos.insert(
                id,
                CharacterInfo {
                    info: PoolRc::from_item_without_pool({
                        let mut info = NetworkCharacterInfo::explicit_default();

                        info.skin = "WWWWWWWWWWWWWWW".try_into().unwrap();
                        info.skin_info = NetworkSkinInfo::Original;
                        info.name = NetworkString::new("WWWWWWWWWWWWWWW").unwrap();
                        info.clan = NetworkString::new("MWWWWWWWWWWW").unwrap();
                        info.flag = NetworkString::new("GB").unwrap();

                        info
                    }),
                    skin_info: NetworkSkinInfo::Original,
                    laser_info: Default::default(),
                    stage_id: None,
                    side: None,
                    player_info: Some(CharacterPlayerInfo {
                        cam_mode: PlayerCameraMode::Default,
                        force_scoreboard_visible: false,
                        ingame_mode: PlayerIngameMode::InGame {
                            in_custom_stage: false,
                        },
                    }),
                    browser_score: PoolNetworkString::from_without_pool("999".try_into().unwrap()),
                    browser_eye: TeeEye::Normal,
                    account_name: Some(PoolNetworkString::from_without_pool(
                        "testname".try_into().unwrap(),
                    )),
                },
            );
            blue_players.push(ScoreboardCharacterInfo {
                id,
                score: ScoreboardScoreType::Points(999),
                ping: ScoreboardConnectionType::Network(PlayerNetworkStats {
                    ping: Duration::from_millis(999),
                    ..Default::default()
                }),
            });
            if i % 3 == 0 {
                blue_stages.insert(
                    g.next_id(),
                    ScoreboardStageInfo {
                        characters: std::mem::replace(
                            &mut blue_players,
                            PoolVec::new_without_pool(),
                        ),
                        max_size: 0,
                        name: PoolNetworkString::from_without_pool("TEST".try_into().unwrap()),
                        color: ubvec4::new(
                            (i % 256) as u8,
                            255 - (i % 256) as u8,
                            255 * (i % 2) as u8,
                            20,
                        ),
                        score: ScoreboardScoreType::Points(999),
                    },
                );
            }
        }
        let mut spectator_players = PoolVec::new_without_pool();
        for _ in 0..12 {
            let id = g.next_id();
            character_infos.insert(
                id,
                CharacterInfo {
                    info: PoolRc::from_item_without_pool({
                        let mut info = NetworkCharacterInfo::explicit_default();

                        info.skin = "WWWWWWWWWWWWWWW".try_into().unwrap();
                        info.skin_info = NetworkSkinInfo::Original;
                        info.name = NetworkString::new("WWWWWWWWWWWWWWW").unwrap();
                        info.clan = NetworkString::new("MWWWWWWWWWWW").unwrap();
                        info.flag = NetworkString::new("DE").unwrap();

                        info
                    }),
                    skin_info: NetworkSkinInfo::Original,
                    laser_info: Default::default(),
                    stage_id: None,
                    side: None,
                    player_info: Some(CharacterPlayerInfo {
                        cam_mode: PlayerCameraMode::Default,
                        force_scoreboard_visible: false,
                        ingame_mode: PlayerIngameMode::Spectator,
                    }),
                    browser_score: PoolNetworkString::from_without_pool("999".try_into().unwrap()),
                    browser_eye: TeeEye::Angry,
                    account_name: Some(PoolNetworkString::from_without_pool(
                        "testname".try_into().unwrap(),
                    )),
                },
            );
            spectator_players.push(ScoreboardCharacterInfo {
                id,
                score: ScoreboardScoreType::Points(999),
                ping: ScoreboardConnectionType::Network(PlayerNetworkStats {
                    ping: Duration::from_millis(999),
                    ..Default::default()
                }),
            });
        }
        client_ui::scoreboard::main_frame::render(
            ui,
            &mut UiRenderPipe::new(
                pipe.cur_time,
                &mut client_ui::scoreboard::user_data::UserData {
                    scoreboard: &game_interface::types::render::scoreboard::Scoreboard {
                        game: ScoreboardGameType::SidedPlay {
                            ignore_stage: *red_stages.front().unwrap().0,
                            red_stages,
                            blue_stages,
                            spectator_players,
                            red_side_name: PoolNetworkString::from_without_pool(
                                "Red Team".try_into().unwrap(),
                            ),
                            blue_side_name: PoolNetworkString::from_without_pool(
                                "Blue Team".try_into().unwrap(),
                            ),
                        },
                        options: ScoreboardGameOptions {
                            map_name: PoolNetworkString::from_without_pool(
                                "A_Map".try_into().unwrap(),
                            ),
                            ty: ScoreboardGameTypeOptions::Match {
                                score_limit: 50,
                                time_limit: Some(Duration::from_secs(60 * 60)),
                            },
                        },
                    },
                    character_infos: &character_infos,
                    canvas_handle: &self.canvas_handle,
                    stream_handle: &self.stream_handle,
                    skin_container: &mut self.skin_container,
                    render_tee: &self.render_tee,
                    flags_container: &mut self.flags_container,

                    own_character_id: character_infos.front().unwrap().0,
                },
            ),
            ui_state,
        );
        /*let mut players = Vec::new();
        for _ in 0..128 {
            players.push(());
        }
        let mut spectator_players = Vec::new();
        for _ in 0..12 {
            spectator_players.push(());
        }
        client_ui::scoreboard::main_frame::render(
            ui,
            &mut UiRenderPipe::new(
                pipe.cur_time,
                pipe.config,
                client_ui::scoreboard::user_data::UserData {
                    game_data: &ScoreboardGameType::SoloPlay {
                        players,
                        spectator_players,
                    },
                },
            ),
            ui_state,
            graphics,
        );*/
    }
}

impl UiPageInterface<()> for Scoreboard {
    fn render(&mut self, ui: &mut egui::Ui, pipe: &mut UiRenderPipe<()>, ui_state: &mut UiState) {
        self.render_impl(ui, pipe, ui_state)
    }
}
