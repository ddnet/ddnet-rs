use std::time::Duration;

use api_ui_game::render::create_skin_container;
use client_containers::skins::SkinContainer;
use client_render_base::render::tee::RenderTee;
use client_ui::{
    thumbnail_container::{DEFAULT_THUMBNAIL_CONTAINER_PATH, ThumbnailContainer},
    vote::user_data::{VoteRenderData, VoteRenderType},
};
use game_interface::votes::{
    MapCategoryVoteKey, MapVote, MapVoteDetails, MapVoteKey, VoteState, VoteType, Voted,
};
use graphics::{
    graphics::graphics::Graphics,
    handles::{canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle},
};
use ui_base::types::{UiRenderPipe, UiState};
use ui_generic::traits::UiPageInterface;

use super::create_thumbnail_container;

pub struct VotePage {
    canvas_handle: GraphicsCanvasHandle,
    stream_handle: GraphicsStreamHandle,
    skin_container: SkinContainer,
    map_vote_thumbnail_container: ThumbnailContainer,
    render_tee: RenderTee,
}

impl VotePage {
    pub fn new(graphics: &Graphics) -> Self {
        Self {
            canvas_handle: graphics.canvas_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),
            skin_container: create_skin_container(),
            map_vote_thumbnail_container: create_thumbnail_container(
                DEFAULT_THUMBNAIL_CONTAINER_PATH,
                "map-vote-thumbnails",
            ),
            render_tee: RenderTee::new(graphics),
        }
    }

    fn render_impl(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut UiRenderPipe<()>,
        ui_state: &mut UiState,
    ) {
        client_ui::vote::main_frame::render(
            ui,
            &mut UiRenderPipe::new(
                pipe.cur_time,
                &mut client_ui::vote::user_data::UserData {
                    canvas_handle: &self.canvas_handle,
                    stream_handle: &self.stream_handle,
                    skin_container: &mut self.skin_container,
                    map_vote_thumbnail_container: &mut self.map_vote_thumbnail_container,
                    render_tee: &self.render_tee,

                    vote_data: VoteRenderData {
                        ty: VoteRenderType::PlayerVoteKick(
                            client_ui::vote::user_data::VoteRenderPlayer {
                                name: "nameless tee",
                                skin: &Default::default(),
                                skin_info: &Default::default(),
                                reason: "a reason why the player has been voted.",
                            },
                        ),
                        /*ty: VoteRenderType::Map {
                            key: &MapCategoryVoteKey {
                                category: "Auto".try_into().unwrap(),
                                map: MapVoteKey {
                                    name: "A_Map".try_into().unwrap(),
                                    hash: Default::default(),
                                },
                            },
                            map: &MapVote {
                                thumbnail_resource: None,
                                details: MapVoteDetails::None,
                                is_default_map: true,
                            },
                        },*/
                        data: &VoteState {
                            vote: VoteType::Map {
                                key: MapCategoryVoteKey {
                                    category: "Auto".try_into().unwrap(),
                                    map: MapVoteKey {
                                        name: "A_Map".try_into().unwrap(),
                                        hash: Default::default(),
                                    },
                                },
                                map: MapVote {
                                    thumbnail_resource: None,
                                    details: MapVoteDetails::None,
                                    is_default_map: true,
                                },
                            },
                            remaining_time: Duration::ZERO,
                            yes_votes: 5,
                            no_votes: 4,
                            allowed_to_vote_count: 10,
                        },
                        remaining_time: &Duration::from_secs(1),
                        voted: Some(Voted::Yes),
                    },
                    player_vote_miniscreen: false,
                    player_vote_rect: &mut None,
                },
            ),
            ui_state,
        );
    }
}

impl UiPageInterface<()> for VotePage {
    fn render(&mut self, ui: &mut egui::Ui, pipe: &mut UiRenderPipe<()>, ui_state: &mut UiState) {
        self.render_impl(ui, pipe, ui_state)
    }
}
