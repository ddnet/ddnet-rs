use std::time::Duration;

use base::{linked_hash_map_view::FxLinkedHashMap, network_string::PoolNetworkString};
use client_containers::utils::RenderGameContainers;
use client_render::scoreboard::render::{ScoreboardRender, ScoreboardRenderPipe};
use client_render_base::render::tee::RenderTee;
use game_interface::types::{
    character_info::{NetworkCharacterInfo, NetworkSkinInfo},
    id_gen::IdGenerator,
    id_types::{CharacterId, StageId},
    network_stats::PlayerNetworkStats,
    render::{
        character::{CharacterInfo, TeeEye},
        scoreboard::{
            Scoreboard, ScoreboardCharacterInfo, ScoreboardConnectionType, ScoreboardGameOptions,
            ScoreboardGameType, ScoreboardGameTypeOptions, ScoreboardScoreType,
            ScoreboardStageInfo,
        },
    },
};
use graphics::graphics::graphics::Graphics;
use math::math::vector::ubvec4;
use pool::{
    datatypes::{PoolFxLinkedHashMap, PoolVec},
    rc::PoolRc,
};
use ui_base::ui::UiCreator;

use super::utils::render_helper;

pub fn test_scoreboard(
    graphics: &Graphics,
    creator: &UiCreator,
    containers: &mut RenderGameContainers,
    render_tee: &RenderTee,
    save_screenshot: impl Fn(&str),
) {
    let mut scoreboard = ScoreboardRender::new(graphics, creator);

    let mut character_infos: FxLinkedHashMap<CharacterId, CharacterInfo> = Default::default();
    let id_gen = IdGenerator::default();

    for _ in 0..400 {
        character_infos.insert(
            id_gen.next_id(),
            CharacterInfo {
                info: PoolRc::from_item_without_pool(NetworkCharacterInfo::explicit_default()),
                skin_info: NetworkSkinInfo::Original,
                laser_info: Default::default(),
                stage_id: Some(id_gen.next_id()),
                side: None,
                player_info: None,
                browser_score: PoolNetworkString::new_without_pool(),
                browser_eye: TeeEye::Happy,
                account_name: Some(PoolNetworkString::from_without_pool(
                    "testname".try_into().unwrap(),
                )),
            },
        );
    }

    let mut time_offset = Duration::ZERO;
    let mut render = |base_name: &str, scoreboard_info: &Scoreboard| {
        let render_internal = |_i: u64, time_offset: Duration| {
            scoreboard.render(&mut ScoreboardRenderPipe {
                cur_time: &time_offset,
                scoreboard: scoreboard_info,
                character_infos: &character_infos,
                skin_container: &mut containers.skin_container,
                tee_render: render_tee,
                flags_container: &mut containers.flags_container,

                own_character_id: character_infos.front().unwrap().0,
            });
        };
        render_helper(
            graphics,
            render_internal,
            &mut time_offset,
            base_name,
            &save_screenshot,
        );
    };

    let options = ScoreboardGameOptions {
        map_name: PoolNetworkString::from_without_pool("A_Map".try_into().unwrap()),
        ty: ScoreboardGameTypeOptions::Match {
            score_limit: 50,
            time_limit: Some(Duration::from_secs(60 * 60)),
        },
    };

    let g = IdGenerator::new();
    let gen_players_or_stages =
        |i: usize,
         players: Option<&mut PoolVec<ScoreboardCharacterInfo>>,
         mut stages: Option<&mut PoolFxLinkedHashMap<StageId, ScoreboardStageInfo>>| {
            let mut dummy_players = PoolVec::new_without_pool();
            let players = players.unwrap_or(&mut dummy_players);
            for i in 0..i {
                players.push(ScoreboardCharacterInfo {
                    id: *character_infos.keys().next().unwrap(),
                    score: ScoreboardScoreType::Points(999),
                    ping: ScoreboardConnectionType::Network(PlayerNetworkStats {
                        ping: Duration::from_millis(999),
                        ..Default::default()
                    }),
                });

                if let Some(stages) = (i % 3 == 0).then_some(stages.as_deref_mut()).flatten() {
                    stages.insert(
                        g.next_id(),
                        ScoreboardStageInfo {
                            characters: std::mem::replace(players, PoolVec::new_without_pool()),
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
        };

    let mut gen_sided = |i: usize, base_name: &str| {
        let mut red_stages = PoolFxLinkedHashMap::new_without_pool();
        let mut blue_stages = PoolFxLinkedHashMap::new_without_pool();
        let mut spectator_players = PoolVec::new_without_pool();

        gen_players_or_stages(i, None, Some(&mut red_stages));
        gen_players_or_stages(i, None, Some(&mut blue_stages));
        gen_players_or_stages(i, Some(&mut spectator_players), None);
        let scoreboard = Scoreboard {
            game: ScoreboardGameType::SidedPlay {
                ignore_stage: *red_stages.front().unwrap().0,
                red_stages,
                blue_stages,
                spectator_players,
                red_side_name: PoolNetworkString::from_without_pool("Red Team".try_into().unwrap()),
                blue_side_name: PoolNetworkString::from_without_pool(
                    "Blue Team".try_into().unwrap(),
                ),
            },
            options: options.clone(),
        };
        render(base_name, &scoreboard);
    };
    gen_sided(8, "scoreboard_sided_8-8-8");
    gen_sided(16, "scoreboard_sided_16-16-16");
}
