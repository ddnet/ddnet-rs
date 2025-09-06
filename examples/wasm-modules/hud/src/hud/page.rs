use api_ui_game::render::{create_ctf_container, create_skin_container};
use base::{linked_hash_map_view::FxLinkedHashMap, network_string::PoolNetworkString};
use client_containers::{ctf::CtfContainer, skins::SkinContainer};
use client_render_base::render::tee::RenderTee;
use game_interface::types::{
    character_info::{NetworkCharacterInfo, NetworkSkinInfo},
    id_gen::IdGenerator,
    id_types::CharacterId,
    render::{
        character::{CharacterInfo, TeeEye},
        game::{
            GameRenderInfo, MatchRoundTimeType,
            game_match::{FlagCarrierCharacter, MatchStandings},
        },
    },
};
use graphics::{
    graphics::graphics::Graphics,
    handles::{canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle},
};
use pool::rc::PoolRc;
use ui_base::types::{UiRenderPipe, UiState};
use ui_generic::traits::UiPageInterface;

pub struct HudPage {
    canvas_handle: GraphicsCanvasHandle,
    stream_handle: GraphicsStreamHandle,
    skin_container: SkinContainer,
    render_tee: RenderTee,
    ctf_container: CtfContainer,
    character_infos: FxLinkedHashMap<CharacterId, CharacterInfo>,
}

impl HudPage {
    pub fn new(graphics: &Graphics) -> Self {
        let mut character_infos: FxLinkedHashMap<CharacterId, CharacterInfo> = Default::default();
        let id_gen = IdGenerator::new();
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
        Self {
            canvas_handle: graphics.canvas_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),
            skin_container: create_skin_container(),
            render_tee: RenderTee::new(graphics),
            ctf_container: create_ctf_container(),
            character_infos,
        }
    }

    fn render_impl(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut UiRenderPipe<()>,
        ui_state: &mut UiState,
    ) {
        client_ui::hud::main_frame::render(
            ui,
            &mut UiRenderPipe::new(
                pipe.cur_time,
                &mut client_ui::hud::user_data::UserData {
                    race_round_timer_counter: &456156,
                    ticks_per_second: &50.try_into().unwrap(),
                    /*game: Some(&GameRenderInfo::Match {
                        standings: MatchStandings::Solo {
                            leading_characters: [
                                Some(LeadingCharacter {
                                    character_id: *self.character_infos.front().unwrap().0,
                                    score: 999,
                                }),
                                None,
                            ],
                        },
                    }),*/
                    game: Some(&GameRenderInfo::Match {
                        standings: MatchStandings::Sided {
                            score_red: 999,
                            score_blue: -999,
                            flag_carrier_red: Some(FlagCarrierCharacter {
                                character_id: *self.character_infos.front().unwrap().0,
                                score: 999,
                            }),
                            flag_carrier_blue: Some(FlagCarrierCharacter {
                                character_id: *self.character_infos.front().unwrap().0,
                                score: 999,
                            }),
                        },
                        round_time_type: MatchRoundTimeType::Normal,
                        unbalanced: false,
                    }),
                    /*game: Some(&GameRenderInfo::Race {}),*/
                    skin_container: &mut self.skin_container,
                    skin_renderer: &self.render_tee,
                    ctf_container: &mut self.ctf_container,
                    character_infos: &self.character_infos,
                    canvas_handle: &self.canvas_handle,
                    stream_handle: &self.stream_handle,
                },
            ),
            ui_state,
        );
    }
}

impl UiPageInterface<()> for HudPage {
    fn render(&mut self, ui: &mut egui::Ui, pipe: &mut UiRenderPipe<()>, ui_state: &mut UiState) {
        self.render_impl(ui, pipe, ui_state)
    }
}
