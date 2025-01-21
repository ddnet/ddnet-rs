use std::collections::BTreeSet;

use binds::binds::{
    bind_keys_to_str, bind_to_str, gen_local_player_action_hash_map,
    gen_local_player_action_hash_map_rev, str_list_to_binds_lossy, BindAction,
    BindActionsCharacter, BindActionsLocalPlayer, BindKey, KeyCode, PhysicalKey,
};
use egui::{Button, Color32, DragValue, Grid, Layout, ScrollArea};
use egui_extras::{Size, StripBuilder};
use game_interface::types::weapons::WeaponType;
use serde::{Deserialize, Serialize};
use ui_base::types::UiRenderPipe;

use crate::main_menu::{settings::player::profile_selector::profile_selector, user_data::UserData};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetBind {
    keys: BTreeSet<BindKey>,
    bind_name: String,
}

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>) {
    ui.with_layout(Layout::top_down(egui::Align::Min), |ui| {
        let entry = pipe
            .user_data
            .config
            .path()
            .query
            .entry("control-input-active".to_string())
            .or_default();
        let mut control_inp_active =
            serde_json::from_str::<Option<SetBind>>(entry).unwrap_or_default();

        let config = &mut pipe.user_data.config.game;

        let profile_index = profile_selector(
            ui,
            "controls-profile-selection",
            config,
            &mut pipe.user_data.config.engine,
        );

        let player = &mut config.players[profile_index as usize];

        let mut binds_changed = false;

        let map = gen_local_player_action_hash_map();
        let map_rev = gen_local_player_action_hash_map_rev();
        let mut binds: Vec<_> = str_list_to_binds_lossy(
            &player.binds,
            pipe.user_data.console_entries,
            &map,
            pipe.user_data.parser_cache,
        );

        ScrollArea::vertical().show(ui, |ui| {
            Grid::new("controls-grid").num_columns(2).show(ui, |ui| {
                // Mouse input
                if config.inp.use_dyncam {
                    ui.label("Mouse sensitivity");
                    ui.add(
                        DragValue::new(&mut config.inp.dyncam_mouse.sensitivity)
                            .update_while_editing(false)
                            .range(1..=100000),
                    );
                    ui.end_row();
                } else {
                    ui.label("Mouse sensitivity");
                    ui.add(
                        DragValue::new(&mut config.inp.mouse.sensitivity)
                            .update_while_editing(false)
                            .range(1..=100000),
                    );
                    ui.end_row();
                }

                // Dyncam mouse
                ui.label("Dynamic camera, follows the mouse");
                ui.checkbox(&mut config.inp.use_dyncam, "");
                ui.end_row();

                // Movement controls
                let mut inp = |label: &str, bind_action: BindAction| {
                    let mut keys = binds
                        .iter()
                        .filter_map(|(keys, actions)| {
                            actions
                                .iter()
                                .any(|action| action.eq(&bind_action))
                                .then_some((keys, actions))
                        })
                        .collect::<Vec<_>>();
                    keys.sort_by_key(|(_, actions)| actions.len());

                    ui.label(label);
                    let (text, hover_text, multibind_text, info_text) = if !keys.is_empty() {
                        let info_text =
                            if keys.get(1).is_some_and(|(_, actions)| actions.len() == 1) {
                                let binds = keys
                                    .iter()
                                    .filter_map(|(keys, actions)| {
                                        if actions.len() == 1 {
                                            Some(bind_to_str(keys, (*actions).clone(), &map_rev))
                                        } else {
                                            None
                                        }
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                Some(format!(
                                    "This control/command has multiple binds:\n{}",
                                    binds
                                ))
                            } else {
                                None
                            };
                        let multibind_text = if keys.iter().any(|(_, actions)| actions.len() > 1) {
                            let binds = keys
                                .iter()
                                .filter_map(|(keys, actions)| {
                                    if actions.len() > 1 {
                                        Some(bind_to_str(keys, (*actions).clone(), &map_rev))
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            Some(format!(
                                "This control/command is part of binds \
                                with other controls/commands:\n{}",
                                binds
                            ))
                        } else {
                            None
                        };
                        let (keys, actions) = keys.first().unwrap();
                        let bind_keys_str = bind_keys_to_str(keys);
                        if actions.len() == 1 {
                            (
                                Some(bind_keys_str.clone()),
                                format!("Bound on {}", bind_keys_str),
                                multibind_text,
                                info_text,
                            )
                        } else {
                            (
                                None,
                                "This command/control is only bound \
                                to key binds with other commands/controls."
                                    .to_string(),
                                multibind_text,
                                info_text,
                            )
                        }
                    } else {
                        (
                            None,
                            "This command/control is not bound \
                            to any key bind yet."
                                .to_string(),
                            None,
                            None,
                        )
                    };
                    ui.horizontal(|ui| {
                        StripBuilder::new(ui)
                            .size(Size::exact(300.0))
                            .size(Size::exact(30.0))
                            .size(Size::exact(30.0))
                            .cell_layout(
                                Layout::left_to_right(egui::Align::Center).with_main_justify(true),
                            )
                            .horizontal(|mut strip| {
                                strip.cell(|ui| {
                                    ui.style_mut().wrap_mode = None;
                                    if text.is_none() {
                                        ui.style_mut().visuals.widgets.inactive.bg_fill =
                                            Color32::BLACK;
                                    }
                                    if let Some(bind) = control_inp_active
                                        .as_ref()
                                        .is_some_and(|bind| bind.bind_name == label)
                                        .then_some(control_inp_active.as_mut())
                                        .flatten()
                                    {
                                        let keys: Vec<_> = bind.keys.iter().copied().collect();

                                        pipe.user_data.raw_input.request_raw_input();
                                        let raw_input = pipe.user_data.raw_input.raw_input();
                                        let escape_pressed =
                                            ui.input(|i| i.key_pressed(egui::Key::Escape));

                                        let btn = ui
                                            .add(
                                                Button::new(bind_keys_to_str(&keys)).selected(true),
                                            )
                                            .on_hover_text(hover_text);
                                        if btn.clicked() || escape_pressed {
                                            if let Some(bind) = control_inp_active.take() {
                                                binds.retain(|(keys, actions)| {
                                                    (actions.len() != 1
                                                        || actions[0].ne(&bind_action))
                                                        && bind.keys.ne(&{
                                                            let keys: BTreeSet<BindKey> =
                                                                keys.iter().copied().collect();
                                                            keys
                                                        })
                                                });
                                                if !bind.keys.is_empty() {
                                                    binds.push((
                                                        bind.keys.into_iter().collect(),
                                                        vec![bind_action],
                                                    ));
                                                }
                                                binds_changed = true;
                                            }
                                        } else if !btn.is_pointer_button_down_on() {
                                            for key_down in raw_input.keys {
                                                // generally ignore escape
                                                if matches!(
                                                    key_down,
                                                    BindKey::Key(PhysicalKey::Code(
                                                        KeyCode::Escape
                                                    ))
                                                ) {
                                                    continue;
                                                }
                                                bind.keys.insert(key_down);
                                            }
                                        }
                                    } else if ui
                                        .add(Button::new(text.unwrap_or_default()))
                                        .on_hover_text(hover_text)
                                        .clicked()
                                    {
                                        control_inp_active = Some(SetBind {
                                            keys: Default::default(),
                                            bind_name: label.to_string(),
                                        });
                                    }
                                });
                                strip.cell(|ui| {
                                    ui.style_mut().wrap_mode = None;
                                    if let Some(hover_text) = multibind_text {
                                        ui.label("\u{f0c1}").on_hover_text(hover_text);
                                    }
                                });
                                strip.cell(|ui| {
                                    ui.style_mut().wrap_mode = None;
                                    if let Some(hover_text) = info_text {
                                        ui.label("\u{f071}").on_hover_text(hover_text);
                                    }
                                });
                            });
                    });
                    ui.end_row();
                };

                inp(
                    "Move left:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                        BindActionsCharacter::MoveLeft,
                    )),
                );
                inp(
                    "Move right:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                        BindActionsCharacter::MoveRight,
                    )),
                );
                inp(
                    "Jump:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                        BindActionsCharacter::Jump,
                    )),
                );
                inp(
                    "Fire:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                        BindActionsCharacter::Fire,
                    )),
                );
                inp(
                    "Hook:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                        BindActionsCharacter::Hook,
                    )),
                );
                inp(
                    "Hook collisions:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::ShowHookCollision),
                );
                //inp("Pause:", BindActions::LocalPlayer(BindActionsLocalPlayer::Pause));
                inp(
                    "Kill:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::Kill),
                );
                inp(
                    "Zoom in:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::ZoomIn),
                );
                inp(
                    "Zoom out:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::ZoomOut),
                );
                inp(
                    "Default zoom:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::ZoomReset),
                );
                //inp("Show others:", BindActions::LocalPlayer(BindActionsLocalPlayer::ShowOthers));
                //inp("Show all:", BindActions::LocalPlayer(BindActionsLocalPlayer::ShowAll));
                //inp("Toggle dyncam:", BindActions::LocalPlayer(BindActionsLocalPlayer::ToggleDynCam));
                //inp("Toggle ghost:", BindActions::LocalPlayer(BindActionsLocalPlayer::ToggleGhost));

                // Weapon
                inp(
                    "Hammer:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                        BindActionsCharacter::Weapon(WeaponType::Hammer),
                    )),
                );
                inp(
                    "Pistol:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                        BindActionsCharacter::Weapon(WeaponType::Gun),
                    )),
                );
                inp(
                    "Shotgun:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                        BindActionsCharacter::Weapon(WeaponType::Shotgun),
                    )),
                );
                inp(
                    "Grenade:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                        BindActionsCharacter::Weapon(WeaponType::Grenade),
                    )),
                );
                inp(
                    "Laser:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                        BindActionsCharacter::Weapon(WeaponType::Laser),
                    )),
                );

                inp(
                    "Next weapon:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                        BindActionsCharacter::NextWeapon,
                    )),
                );
                inp(
                    "Previous weapon:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                        BindActionsCharacter::PrevWeapon,
                    )),
                );

                // Voting
                inp(
                    "Vote yes:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::VoteYes),
                );
                inp(
                    "Vote no:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::VoteNo),
                );

                // Chat
                inp(
                    "Chat:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::ActivateChatInput),
                );
                //inp("Team chat:", BindActions::LocalPlayer(BindActionsLocalPlayer::ActivateTeamChatInput));
                //inp("Converse:",BindActions::LocalPlayer(BindActionsLocalPlayer::ActivateConverseChatInput));
                //inp("Chat command:",BindActions::LocalPlayer(BindActionsLocalPlayer::ActivateChatCommandInput));
                inp(
                    "Show chat history:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::ShowChatHistory),
                );

                // Dummy
                //inp("Toggle dummy:", BindActions::LocalPlayer(BindActionsLocalPlayer::ToggleDummy));
                inp(
                    "Dummy copy:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::ToggleDummyCopyMoves),
                );
                inp(
                    "Hammerfly dummy:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::ToggleDummyHammerFly),
                );

                // Misc
                inp(
                    "Emoticon:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::ShowEmoteWheel),
                );
                inp(
                    "Spectate list:",
                    BindAction::LocalPlayer(BindActionsLocalPlayer::ShowSpectatorSelection),
                );
                /*inp("Spectate next:", BindActions::LocalPlayer(BindActionsLocalPlayer::SpectateNext));
                inp("Spectate previous:", BindActions::LocalPlayer(BindActionsLocalPlayer::SpectatePrev));
                inp("Client console:", BindActions::LocalPlayer(BindActionsLocalPlayer::LocalConsole));
                inp("Remote console:", BindActions::LocalPlayer(BindActionsLocalPlayer::RemoteConsole));
                inp("Screenshot:", BindActions::LocalPlayer(BindActionsLocalPlayer::Screenshot));
                inp("Scoreboard:", BindActions::LocalPlayer(BindActionsLocalPlayer::ShowScoreboard));
                inp("Statboard:", BindActions::LocalPlayer(BindActionsLocalPlayer::ShowStatboard));
                inp("Lock team:", BindActions::LocalPlayer(BindActionsLocalPlayer::LockTeam));
                inp("Show entities:", BindActions::LocalPlayer(BindActionsLocalPlayer::ShowEntities));
                inp("Show HUD:", BindActions::LocalPlayer(BindActionsLocalPlayer::ShowHUD));*/
            });
        });

        if binds_changed {
            player.binds = binds
                .into_iter()
                .map(|(keys, actions)| bind_to_str(&keys, actions, &map_rev))
                .collect();
            pipe.user_data.player_settings_sync.set_controls_changed();
        }

        *pipe
            .user_data
            .config
            .path()
            .query
            .entry("control-input-active".to_string())
            .or_default() = serde_json::to_string(&control_inp_active).unwrap_or_default();
    });
}
