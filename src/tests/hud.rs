use std::time::Duration;

use base::{linked_hash_map_view::FxLinkedHashMap, network_string::PoolNetworkString};
use client_containers::utils::RenderGameContainers;
use client_render_base::render::tee::RenderTee;
use client_render_game::components::hud::{RenderHud, RenderHudPipe};
use game_interface::types::{
    character_info::{NetworkCharacterInfo, NetworkSkinInfo},
    emoticons::IntoEnumIterator,
    id_gen::IdGenerator,
    id_types::CharacterId,
    render::{
        character::{
            CharacterInfo, LocalCharacterDdrace, LocalCharacterRenderInfo, LocalCharacterVanilla,
            TeeEye,
        },
        game::{
            GameRenderInfo, MatchRoundTimeType,
            game_match::{LeadingCharacter, MatchStandings},
        },
    },
    weapons::{EnumCount, WeaponType},
};
use graphics::graphics::graphics::Graphics;
use pool::{datatypes::PoolFxLinkedHashSet, rc::PoolRc};
use ui_base::ui::UiCreator;

use crate::tests::utils::render_helper;

pub fn test_hud(
    graphics: &Graphics,
    creator: &UiCreator,
    containers: &mut RenderGameContainers,
    render_tee: &RenderTee,
    save_screenshot: impl Fn(&str),
) {
    let mut hud = RenderHud::new(graphics, creator);

    let mut time_offset = Duration::ZERO;
    let mut render = |local_player_render_info: &LocalCharacterRenderInfo,
                      cur_weapon: WeaponType,
                      game: Option<&'_ GameRenderInfo>,
                      character_infos: &FxLinkedHashMap<CharacterId, CharacterInfo>,
                      base_name: &str| {
        let render_internal = |i: u64, time_offset: Duration| {
            hud.render(&mut RenderHudPipe {
                hud_container: &mut containers.hud_container,
                hud_key: None,
                weapon_container: &mut containers.weapon_container,
                weapon_key: None,
                local_player_render_info,
                cur_weapon,
                race_timer_counter: &(50 * i.pow(10)),
                ticks_per_second: &50.try_into().unwrap(),
                cur_time: &time_offset,
                game,
                skin_container: &mut containers.skin_container,
                skin_renderer: render_tee,
                ctf_container: &mut containers.ctf_container,
                character_infos,
            })
        };
        render_helper(
            graphics,
            render_internal,
            &mut time_offset,
            base_name,
            &save_screenshot,
        );
    };

    render(
        &LocalCharacterRenderInfo::Unavailable,
        WeaponType::Hammer,
        None,
        &Default::default(),
        "hud_none",
    );
    graphics.swap();

    render(
        &LocalCharacterRenderInfo::Unavailable,
        WeaponType::Hammer,
        None,
        &Default::default(),
        "hud_none",
    );

    let mut p = 0;
    for all in (0..16).step_by(5) {
        for w in WeaponType::iter() {
            let local_player_info = LocalCharacterRenderInfo::Vanilla(LocalCharacterVanilla {
                health: all,
                armor: all,
                ammo_of_weapon: if all > 10 { None } else { Some(all) },
            });
            render(
                &local_player_info,
                w,
                None,
                &Default::default(),
                &format!("hud_vanilla_{:0>6}", p),
            );
            p += 1;
        }
    }
    let local_player_info = LocalCharacterRenderInfo::Vanilla(LocalCharacterVanilla {
        health: u32::MAX,
        armor: u32::MAX,
        ammo_of_weapon: Some(u32::MAX),
    });
    render(
        &local_player_info,
        WeaponType::Grenade,
        None,
        &Default::default(),
        "hud_vanilla_max",
    );

    let mut p = 0;
    for counter in 0..12 {
        let local_player_info = LocalCharacterRenderInfo::Ddrace(LocalCharacterDdrace {
            jumps: counter,
            max_jumps: if (counter + 6) % 12 == 0 {
                None
            } else {
                Some(((counter + 6) % 12).try_into().unwrap())
            },
            endless_hook: (counter + 1) % 3 == 0,
            can_hook_others: (counter + 2) % 3 == 0,
            jetpack: (counter) % 3 == 0,
            deep_frozen: (counter + 1) % 3 == 0,
            live_frozen: (counter + 2) % 3 == 0,
            can_finish: (counter) % 3 == 0,
            owned_weapons: PoolFxLinkedHashSet::from_without_pool(
                WeaponType::iter()
                    .take(counter as usize % WeaponType::COUNT)
                    .collect(),
            ),
            disabled_weapons: PoolFxLinkedHashSet::from_without_pool(
                WeaponType::iter()
                    .take(counter as usize % WeaponType::COUNT)
                    .collect(),
            ),
            tele_weapons: PoolFxLinkedHashSet::from_without_pool(
                WeaponType::iter()
                    .take(counter as usize % WeaponType::COUNT)
                    .collect(),
            ),
            solo: (counter + 1) % 3 == 0,
            invincible: (counter + 2) % 3 == 0,
            dummy_hammer: (counter) % 3 == 0,
            dummy_copy: (counter + 1) % 3 == 0,
            stage_locked: (counter + 2) % 3 == 0,
            team0_mode: (counter) % 3 == 0,
            can_collide: (counter + 1) % 3 == 0,
            checkpoint: if counter % 5 == 0 {
                None
            } else {
                Some((counter % u8::MAX as u32) as u8)
            },
        });
        for w in WeaponType::iter() {
            render(
                &local_player_info,
                w,
                None,
                &Default::default(),
                &format!("hud_ddrace_{:0>6}", p),
            );
            p += 1;
        }
    }
    let local_player_info = LocalCharacterRenderInfo::Ddrace(LocalCharacterDdrace {
        jumps: u32::MAX,
        max_jumps: Some(u32::MAX.try_into().unwrap()),
        endless_hook: true,
        can_hook_others: false,
        jetpack: true,
        deep_frozen: true,
        live_frozen: true,
        can_finish: false,
        owned_weapons: PoolFxLinkedHashSet::from_without_pool(WeaponType::iter().collect()),
        disabled_weapons: PoolFxLinkedHashSet::from_without_pool(WeaponType::iter().collect()),
        tele_weapons: PoolFxLinkedHashSet::from_without_pool(WeaponType::iter().collect()),
        solo: true,
        invincible: true,
        dummy_hammer: true,
        dummy_copy: true,
        stage_locked: true,
        team0_mode: true,
        can_collide: false,
        checkpoint: Some(u8::MAX),
    });
    render(
        &local_player_info,
        WeaponType::Grenade,
        None,
        &Default::default(),
        "hud_ddrace_max",
    );

    for (p, score) in (-100000..100000).step_by(20000).enumerate() {
        render(
            &LocalCharacterRenderInfo::Unavailable,
            WeaponType::Hammer,
            Some(&GameRenderInfo::Match {
                standings: MatchStandings::Sided {
                    score_red: score,
                    score_blue: score,
                    flag_carrier_red: None,
                    flag_carrier_blue: None,
                },
                round_time_type: MatchRoundTimeType::Normal,
                unbalanced: false,
            }),
            &Default::default(),
            &format!("hud_game_sided{:0>6}", p),
        );
    }
    for (p, score) in (-100000..100000).step_by(20000).enumerate() {
        let id_gen = IdGenerator::new();
        let character_id = id_gen.next_id();
        render(
            &LocalCharacterRenderInfo::Unavailable,
            WeaponType::Hammer,
            Some(&GameRenderInfo::Match {
                standings: MatchStandings::Solo {
                    leading_characters: [
                        (p % 3 != 0).then_some(LeadingCharacter {
                            character_id,
                            score,
                        }),
                        (p % 4 != 0).then_some(LeadingCharacter {
                            character_id,
                            score,
                        }),
                    ],
                },
                round_time_type: MatchRoundTimeType::Normal,
                unbalanced: false,
            }),
            &[(
                character_id,
                CharacterInfo {
                    info: PoolRc::from_item_without_pool({
                        let mut info = NetworkCharacterInfo::explicit_default();
                        if p % 3 == 0 {
                            info.name = "WWWWWWWWWWWWWWWW".try_into().unwrap();
                        }
                        info
                    }),
                    skin_info: NetworkSkinInfo::Original,
                    laser_info: Default::default(),
                    stage_id: None,
                    side: None,
                    player_info: None,
                    browser_score: PoolNetworkString::new_without_pool(),
                    browser_eye: TeeEye::Normal,
                    account_name: Some(PoolNetworkString::from_without_pool(
                        "testname".try_into().unwrap(),
                    )),
                },
            )]
            .into_iter()
            .collect(),
            &format!("hud_game_solo_{:0>6}", p),
        );
    }
}
