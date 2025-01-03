use std::{collections::VecDeque, time::Duration};

use api_ui_game::render::create_skin_container;
use client_containers::skins::SkinContainer;
use client_render_base::render::tee::RenderTee;
use client_types::chat::{ChatMsg, MsgSystem, ServerMsg, SystemMsgPlayerSkin};
use client_ui::chat::user_data::{ChatMode, MsgInChat};
use game_base::network::types::chat::{ChatPlayerInfo, NetChatMsgPlayerChannel};
use game_interface::types::{character_info::NetworkSkinInfo, id_gen::IdGenerator};
use graphics::{
    graphics::graphics::Graphics,
    handles::{canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle},
};
use math::math::vector::ubvec4;
use ui_base::types::{UiRenderPipe, UiState};
use ui_generic::traits::UiPageInterface;

pub struct ChatPage {
    canvas_handle: GraphicsCanvasHandle,
    stream_handle: GraphicsStreamHandle,
    skin_container: SkinContainer,
    render_tee: RenderTee,
}

impl ChatPage {
    pub fn new(graphics: &Graphics) -> Self {
        Self {
            canvas_handle: graphics.canvas_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),
            skin_container: create_skin_container(),
            render_tee: RenderTee::new(graphics),
        }
    }

    fn render_impl(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut UiRenderPipe<()>,
        ui_state: &mut UiState,
    ) {
        let id_gen = IdGenerator::default();
        let mut entries: VecDeque<MsgInChat> = vec![
            MsgInChat {
                msg: ServerMsg::Chat(ChatMsg {
                    player: "name".into(),
                    clan: "clan".into(),
                    skin_name: "skin".try_into().unwrap(),
                    skin_info: NetworkSkinInfo::Custom {
                        body_color: ubvec4::new(0, 255, 255, 255),
                        feet_color: ubvec4::new(255, 255, 255, 255),
                    },
                    msg: "test".into(),
                    channel: NetChatMsgPlayerChannel::GameTeam,
                }),
                add_time: Duration::MAX,
            },
            MsgInChat {
                msg: ServerMsg::Chat(ChatMsg {
                    player: "ngme2".into(),
                    clan: "clan2".into(),
                    skin_name: "skgn2".try_into().unwrap(),
                    skin_info: NetworkSkinInfo::Custom {
                        body_color: ubvec4::new(255, 255, 255, 255),
                        feet_color: ubvec4::new(255, 0, 255, 255),
                    },
                    msg: "WWW a very long message that should hopefully break or \
                            smth like that bla bla bla bla bla bla bla bla bla bla \
                            bla bla bla bla bla bla"
                        .into(),
                    channel: NetChatMsgPlayerChannel::Whisper(ChatPlayerInfo {
                        id: id_gen.next_id(),
                        name: "other".try_into().unwrap(),
                        skin: "skin".try_into().unwrap(),
                        skin_info: NetworkSkinInfo::Custom {
                            body_color: ubvec4::new(0, 255, 255, 255),
                            feet_color: ubvec4::new(255, 255, 255, 255),
                        },
                    }),
                }),
                add_time: Duration::MAX,
            },
        ]
        .into();
        for _ in 0..3 {
            /*entries.push_back(MsgInChat {
                msg: ServerMsg::Chat(ChatMsg {
                    player: "ngme2".into(),
                    clan: "clan3".into(),
                    skin_name: "skgn2".try_into().unwrap(),
                    skin_info: NetworkSkinInfo::Original,
                    msg: "WWW a very long message that should hopefully break or \
                            smth like that bla bla bla bla bla bla bla bla bla bla \
                            bla bla bla bla bla bla"
                        .into(),
                    channel: ChatMsgPlayerChannel::Global,
                }),
                add_time: Duration::MAX,
            });*/
            entries.push_back(MsgInChat {
                msg: ServerMsg::Chat(ChatMsg {
                    player: "ngme2".into(),
                    clan: "clan3".into(),
                    skin_name: "skgn2".try_into().unwrap(),
                    skin_info: NetworkSkinInfo::Original,
                    msg: "short".into(),
                    channel: NetChatMsgPlayerChannel::Global,
                }),
                add_time: Duration::MAX,
            });
        }
        entries.push_back(MsgInChat {
            msg: ServerMsg::System(MsgSystem {
                msg: "Player test".into(),
                front_skin: Some(SystemMsgPlayerSkin {
                    skin_name: "default".try_into().unwrap(),
                    skin_info: NetworkSkinInfo::Original,
                }),
                end_skin: Some(SystemMsgPlayerSkin {
                    skin_name: "default".try_into().unwrap(),
                    skin_info: NetworkSkinInfo::Original,
                }),
            }),
            add_time: Duration::MAX,
        });
        entries.push_back(MsgInChat {
            msg: ServerMsg::System(MsgSystem {
                msg: "No skin test".into(),
                front_skin: None,
                end_skin: None,
            }),
            add_time: Duration::MAX,
        });
        entries.push_back(MsgInChat {
            msg: ServerMsg::System(MsgSystem {
                msg: "WWW a very long message that should hopefully break or \
                        smth like that bla bla bla bla bla bla bla bla bla bla \
                        bla bla bla bla bla bla"
                    .into(),
                front_skin: None,
                end_skin: Some(SystemMsgPlayerSkin {
                    skin_name: "default".try_into().unwrap(),
                    skin_info: NetworkSkinInfo::Original,
                }),
            }),
            add_time: Duration::MAX,
        });
        client_ui::chat::main_frame::render(
            ui,
            &mut UiRenderPipe::new(
                pipe.cur_time,
                &mut client_ui::chat::user_data::UserData {
                    entries: &entries,
                    show_chat_history: false,
                    is_input_active: false,
                    msg: &mut String::new(),
                    chat_events: &mut Default::default(),
                    canvas_handle: &self.canvas_handle,
                    stream_handle: &self.stream_handle,
                    skin_container: &mut self.skin_container,
                    render_tee: &self.render_tee,
                    mode: ChatMode::Global,

                    character_infos: &Default::default(),
                    local_character_ids: &Default::default(),

                    find_player_prompt: &mut Default::default(),
                    find_player_id: &mut Default::default(),
                    cur_whisper_player_id: &mut Default::default(),
                },
            ),
            ui_state,
        );
    }
}

impl UiPageInterface<()> for ChatPage {
    fn render(&mut self, ui: &mut egui::Ui, pipe: &mut UiRenderPipe<()>, ui_state: &mut UiState) {
        self.render_impl(ui, pipe, ui_state)
    }
}
