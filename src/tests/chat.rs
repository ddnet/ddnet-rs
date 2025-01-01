use std::{collections::VecDeque, time::Duration};

use client_containers::utils::RenderGameContainers;
use client_render::chat::render::{ChatRender, ChatRenderOptions, ChatRenderPipe};
use client_render_base::render::tee::RenderTee;
use client_types::chat::{ChatMsg, ServerMsg};
use client_ui::chat::user_data::{ChatMode, MsgInChat};
use game_base::network::types::chat::NetChatMsgPlayerChannel;
use game_interface::types::character_info::NetworkSkinInfo;
use graphics::graphics::graphics::Graphics;
use math::math::vector::ubvec4;
use ui_base::{remember_mut::RememberMut, ui::UiCreator};

use super::utils::render_helper;

pub fn test_chat(
    graphics: &Graphics,
    creator: &UiCreator,
    containers: &mut RenderGameContainers,
    render_tee: &RenderTee,
    save_screenshot: impl Fn(&str),
) {
    let mut chat = ChatRender::new(graphics, creator);

    let mut time_offset = Duration::ZERO;
    let mut render = |base_name: &str, msgs: &VecDeque<MsgInChat>| {
        let render_internal = |_i: u64, time_offset: Duration| {
            chat.msgs = RememberMut::new(msgs.clone());
            chat.last_render_options = None;
            chat.render(&mut ChatRenderPipe {
                cur_time: &time_offset,
                options: ChatRenderOptions {
                    is_chat_input_active: false,
                    show_chat_history: false,
                },
                msg: &mut "".to_string(),
                input: &mut None,
                skin_container: &mut containers.skin_container,
                tee_render: render_tee,
                mode: ChatMode::Global,
                character_infos: &Default::default(),
                local_character_ids: &Default::default(),
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

    render("chat_empty", &Default::default());
    let entries: VecDeque<MsgInChat> = vec![
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
                channel: NetChatMsgPlayerChannel::Global,
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
                channel: NetChatMsgPlayerChannel::Global,
            }),
            add_time: Duration::MAX,
        },
    ]
    .into();
    render("chat_short_long", &entries);
}
