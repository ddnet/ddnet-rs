use std::time::Duration;

use api_ui_game::render::{create_ninja_container, create_skin_container, create_weapon_container};
use client_containers::{ninja::NinjaContainer, skins::SkinContainer, weapons::WeaponContainer};
use client_render_base::render::{tee::RenderTee, toolkit::ToolkitRender};
use client_types::actionfeed::{Action, ActionInFeed, ActionKill, ActionPlayer};

use game_interface::{
    events::{GameWorldActionKillWeapon, KillFlags},
    types::{character_info::NetworkSkinInfo, weapons::WeaponType},
};
use graphics::{
    graphics::graphics::Graphics,
    handles::{canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle},
};
use math::math::vector::ubvec4;
use ui_base::types::{UiRenderPipe, UiState};
use ui_generic::traits::UiPageInterface;

pub struct ActionfeedPage {
    canvas_handle: GraphicsCanvasHandle,
    stream_handle: GraphicsStreamHandle,
    skin_container: SkinContainer,
    render_tee: RenderTee,
    weapon_container: WeaponContainer,
    toolkit_render: ToolkitRender,
    ninja_container: NinjaContainer,
}

impl ActionfeedPage {
    pub fn new(graphics: &Graphics) -> Self {
        Self {
            canvas_handle: graphics.canvas_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),
            skin_container: create_skin_container(),
            render_tee: RenderTee::new(graphics),
            weapon_container: create_weapon_container(),
            toolkit_render: ToolkitRender::new(graphics),
            ninja_container: create_ninja_container(),
        }
    }

    fn render_impl(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut UiRenderPipe<()>,
        ui_state: &mut UiState,
    ) {
        use num_traits::cast::FromPrimitive;
        let mut entries = vec![];
        for i in 0..7 {
            entries.push(ActionInFeed {
                action: Action::Kill(ActionKill {
                    killer: Some(ActionPlayer {
                        name: if i % 2 == 0 {
                            "k".into()
                        } else {
                            "WWWWWWWWWWWWWWWWWWWWWWWW".into()
                        },
                        skin: Default::default(),
                        skin_info: NetworkSkinInfo::Original,
                        weapon: Default::default(),
                    }),
                    assists: vec![],
                    victims: vec![ActionPlayer {
                        name: if i % 2 == 0 {
                            "v".into()
                        } else {
                            "WWWWWWWWWWWWWWWWWWWWWWWW".into()
                        },
                        skin: Default::default(),
                        skin_info: NetworkSkinInfo::Custom {
                            body_color: ubvec4::new(255, 255, 255, 255),
                            feet_color: ubvec4::new(255, 255, 255, 255),
                        },
                        weapon: Default::default(),
                    }],
                    weapon: WeaponType::from_usize(i)
                        .map(|w| GameWorldActionKillWeapon::Weapon { weapon: w })
                        .unwrap_or(if i % 6 == 0 {
                            GameWorldActionKillWeapon::Ninja
                        } else {
                            GameWorldActionKillWeapon::World
                        }),
                    flags: KillFlags::empty(),
                }),
                add_time: Duration::MAX,
            });
        }
        for i in 0..3 {
            entries.push(ActionInFeed {
                action: Action::RaceFinish {
                    player: ActionPlayer {
                        name: if i % 2 == 0 {
                            "k".into()
                        } else {
                            "WWWWWWWWWWWWWWWWWWWWWWWW".into()
                        },
                        skin: Default::default(),
                        skin_info: NetworkSkinInfo::Original,
                        weapon: Default::default(),
                    },
                    finish_time: if i % 2 == 0 {
                        Duration::from_secs(561)
                    } else {
                        Duration::from_nanos(51265489489464896)
                    },
                },
                add_time: Duration::MAX,
            });
        }

        for i in 0..3 {
            entries.push(ActionInFeed {
                action: Action::RaceTeamFinish {
                    players: vec![
                        ActionPlayer {
                            name: if i % 2 == 0 {
                                "k".into()
                            } else {
                                "WWWWWWWWWWWWWWWWWWWWWWWW".into()
                            },
                            skin: Default::default(),
                            skin_info: NetworkSkinInfo::Original,
                            weapon: Default::default(),
                        },
                        ActionPlayer {
                            name: if i % 2 == 1 {
                                "k".into()
                            } else {
                                "WWWWWWWWWWWWWWWWWWWWWWWW".into()
                            },
                            skin: Default::default(),
                            skin_info: NetworkSkinInfo::Original,
                            weapon: Default::default(),
                        },
                    ],
                    team_name: "new_team".to_string(),
                    finish_time: if i % 2 == 0 {
                        Duration::from_secs(561)
                    } else {
                        Duration::from_nanos(51265489489464896)
                    },
                },
                add_time: Duration::MAX,
            });
        }

        client_ui::actionfeed::main_frame::render(
            ui,
            &mut UiRenderPipe::new(
                pipe.cur_time,
                &mut client_ui::actionfeed::user_data::UserData {
                    canvas_handle: &self.canvas_handle,
                    stream_handle: &self.stream_handle,
                    entries: &entries.into(),
                    skin_container: &mut self.skin_container,
                    render_tee: &self.render_tee,
                    weapon_container: &mut self.weapon_container,
                    toolkit_render: &self.toolkit_render,
                    ninja_container: &mut self.ninja_container,
                    render_tee_helper: &mut Default::default(),
                },
            ),
            ui_state,
        );
    }
}

impl UiPageInterface<()> for ActionfeedPage {
    fn render(&mut self, ui: &mut egui::Ui, pipe: &mut UiRenderPipe<()>, ui_state: &mut UiState) {
        self.render_impl(ui, pipe, ui_state)
    }
}
