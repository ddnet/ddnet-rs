use std::{collections::HashMap, ops::Range};

use anyhow::anyhow;
use client_types::console::{entries_to_parser, ConsoleEntry};
use command_parser::parser::{self, Command, CommandType, ParserCache, Syn};
use game_interface::types::weapons::WeaponType;
use hiarc::Hiarc;
pub use input_binds::binds::{BindKey, KeyCode, PhysicalKey};

#[derive(Debug, Hiarc, Clone, Copy, Hash, PartialEq, Eq)]
pub enum BindActionsCharacter {
    MoveLeft,
    MoveRight,
    Jump,
    Fire,
    Hook,
    NextWeapon,
    PrevWeapon,
    Weapon(WeaponType),
    ResetInput,
}

#[derive(Debug, Hiarc, Clone, Copy, Hash, PartialEq, Eq)]
pub enum BindActionsLocalPlayer {
    Character(BindActionsCharacter),
    Dummy(BindActionsCharacter),
    /// Dummy aims to the character
    DummyAimCharacter,
    ShowHookCollision,
    OpenMenu,
    ActivateChatInput,
    ActivateSideOrStageChatInput,
    ActivateWhisperChatInput,
    ShowScoreboard,
    ShowChatHistory,
    ShowEmoteWheel,
    ShowSpectatorSelection,
    Kill,
    FreeCam,
    PhasedFreeCam,
    ToggleDummyCopyMoves,
    ToggleDummyHammerFly,
    VoteYes,
    VoteNo,
    ZoomOut,
    ZoomIn,
    ZoomReset,
}

const LOCAL_PLAYER_ACTIONS: [(&str, BindActionsLocalPlayer); 46] = [
    (
        "+left",
        BindActionsLocalPlayer::Character(BindActionsCharacter::MoveLeft),
    ),
    (
        "+right",
        BindActionsLocalPlayer::Character(BindActionsCharacter::MoveRight),
    ),
    (
        "+jump",
        BindActionsLocalPlayer::Character(BindActionsCharacter::Jump),
    ),
    (
        "+fire",
        BindActionsLocalPlayer::Character(BindActionsCharacter::Fire),
    ),
    (
        "+hook",
        BindActionsLocalPlayer::Character(BindActionsCharacter::Hook),
    ),
    (
        "+nextweapon",
        BindActionsLocalPlayer::Character(BindActionsCharacter::NextWeapon),
    ),
    (
        "+prevweapon",
        BindActionsLocalPlayer::Character(BindActionsCharacter::PrevWeapon),
    ),
    (
        "+reset_input",
        BindActionsLocalPlayer::Character(BindActionsCharacter::ResetInput),
    ),
    // weapons
    (
        "+weapon1",
        BindActionsLocalPlayer::Character(BindActionsCharacter::Weapon(WeaponType::Hammer)),
    ),
    (
        "+weapon2",
        BindActionsLocalPlayer::Character(BindActionsCharacter::Weapon(WeaponType::Gun)),
    ),
    (
        "+weapon3",
        BindActionsLocalPlayer::Character(BindActionsCharacter::Weapon(WeaponType::Shotgun)),
    ),
    (
        "+weapon4",
        BindActionsLocalPlayer::Character(BindActionsCharacter::Weapon(WeaponType::Grenade)),
    ),
    (
        "+weapon5",
        BindActionsLocalPlayer::Character(BindActionsCharacter::Weapon(WeaponType::Laser)),
    ),
    (
        "+dummy.left",
        BindActionsLocalPlayer::Dummy(BindActionsCharacter::MoveLeft),
    ),
    (
        "+dummy.right",
        BindActionsLocalPlayer::Dummy(BindActionsCharacter::MoveRight),
    ),
    (
        "+dummy.jump",
        BindActionsLocalPlayer::Dummy(BindActionsCharacter::Jump),
    ),
    (
        "+dummy.fire",
        BindActionsLocalPlayer::Dummy(BindActionsCharacter::Fire),
    ),
    (
        "+dummy.hook",
        BindActionsLocalPlayer::Dummy(BindActionsCharacter::Hook),
    ),
    (
        "+dummy.nextweapon",
        BindActionsLocalPlayer::Dummy(BindActionsCharacter::NextWeapon),
    ),
    (
        "+dummy.prevweapon",
        BindActionsLocalPlayer::Dummy(BindActionsCharacter::PrevWeapon),
    ),
    (
        "+dummy.reset_input",
        BindActionsLocalPlayer::Dummy(BindActionsCharacter::ResetInput),
    ),
    // weapons
    (
        "+dummy.weapon1",
        BindActionsLocalPlayer::Dummy(BindActionsCharacter::Weapon(WeaponType::Hammer)),
    ),
    (
        "+dummy.weapon2",
        BindActionsLocalPlayer::Dummy(BindActionsCharacter::Weapon(WeaponType::Gun)),
    ),
    (
        "+dummy.weapon3",
        BindActionsLocalPlayer::Dummy(BindActionsCharacter::Weapon(WeaponType::Shotgun)),
    ),
    (
        "+dummy.weapon4",
        BindActionsLocalPlayer::Dummy(BindActionsCharacter::Weapon(WeaponType::Grenade)),
    ),
    (
        "+dummy.weapon5",
        BindActionsLocalPlayer::Dummy(BindActionsCharacter::Weapon(WeaponType::Laser)),
    ),
    // weapons end
    (
        "+dummy.aim_character",
        BindActionsLocalPlayer::DummyAimCharacter,
    ),
    (
        "+show_hook_collision",
        BindActionsLocalPlayer::ShowHookCollision,
    ),
    ("ingame_menu", BindActionsLocalPlayer::OpenMenu),
    ("chat_all", BindActionsLocalPlayer::ActivateChatInput),
    (
        "chat_team",
        BindActionsLocalPlayer::ActivateSideOrStageChatInput,
    ),
    (
        "chat_whisper",
        BindActionsLocalPlayer::ActivateWhisperChatInput,
    ),
    ("+scoreboard", BindActionsLocalPlayer::ShowScoreboard),
    ("+chat_history", BindActionsLocalPlayer::ShowChatHistory),
    ("+emote_wheel", BindActionsLocalPlayer::ShowEmoteWheel),
    (
        "+spectator_selection",
        BindActionsLocalPlayer::ShowSpectatorSelection,
    ),
    ("vote_yes", BindActionsLocalPlayer::VoteYes),
    ("vote_no", BindActionsLocalPlayer::VoteNo),
    ("kill", BindActionsLocalPlayer::Kill),
    ("free_camera", BindActionsLocalPlayer::FreeCam),
    ("phased_free_camera", BindActionsLocalPlayer::PhasedFreeCam),
    (
        "dummy_copy_moves",
        BindActionsLocalPlayer::ToggleDummyCopyMoves,
    ),
    (
        "dummy_hammer_fly",
        BindActionsLocalPlayer::ToggleDummyHammerFly,
    ),
    ("zoom-", BindActionsLocalPlayer::ZoomOut),
    ("zoom+", BindActionsLocalPlayer::ZoomIn),
    ("zoom", BindActionsLocalPlayer::ZoomReset),
];

pub fn gen_local_player_action_hash_map() -> HashMap<&'static str, BindActionsLocalPlayer> {
    LOCAL_PLAYER_ACTIONS.into_iter().collect()
}

pub fn gen_local_player_action_hash_map_rev() -> HashMap<BindActionsLocalPlayer, &'static str> {
    LOCAL_PLAYER_ACTIONS
        .into_iter()
        .map(|(v1, v2)| (v2, v1))
        .collect()
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum BindActionsHotkey {
    Screenshot,
    LocalConsole,
    RemoteConsole,
    ConsoleClose,
    DebugHud,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum BindAction {
    LocalPlayer(BindActionsLocalPlayer),
    Command(Command),
    /// A command that is triggered directly
    /// (on key press down)
    TriggerCommand(Command),
}

fn action_str_to_action(
    action_str: &str,
    map: &HashMap<&'static str, BindActionsLocalPlayer>,
) -> anyhow::Result<BindActionsLocalPlayer> {
    map.get(action_str)
        .cloned()
        .ok_or_else(|| anyhow!("not a valid action"))
}

fn action_to_action_str(
    action: BindActionsLocalPlayer,
    map: &HashMap<BindActionsLocalPlayer, &'static str>,
) -> anyhow::Result<&'static str> {
    map.get(&action)
        .cloned()
        .ok_or_else(|| anyhow!("{:?} is not a valid action", action))
}

fn bind_keys_str_to_bind_keys(bind_keys_str: &str) -> anyhow::Result<Vec<BindKey>> {
    let mut bind_keys: Vec<BindKey> = Vec::new();
    for bind_key_str in bind_keys_str.split('+') {
        let mut cap_bind_key_str = bind_key_str.to_string();
        cap_bind_key_str.make_ascii_lowercase();
        cap_bind_key_str = {
            let str_len = cap_bind_key_str.chars().count();
            let mut last_was_upper = false;
            let mut res: Vec<_> = cap_bind_key_str
                .chars()
                .enumerate()
                .collect::<Vec<(usize, char)>>()
                .windows(2)
                .flat_map(|arg| {
                    let [(_, c1), (c2_index, c2)] = [arg[0], arg[1]];
                    if last_was_upper {
                        last_was_upper = false;
                        if str_len - 1 == c2_index {
                            vec![c2]
                        } else {
                            vec![]
                        }
                    } else if c1 == '_' {
                        last_was_upper = true;
                        vec![c2.to_ascii_uppercase()]
                    } else if str_len - 1 == c2_index {
                        vec![c1, c2]
                    } else {
                        vec![c1]
                    }
                })
                .collect();
            if res.is_empty() {
                cap_bind_key_str.to_ascii_uppercase()
            } else {
                res[0] = res[0].to_ascii_uppercase();
                res.into_iter().collect()
            }
        };
        let bind_key_str = format!("\"{cap_bind_key_str}\"");
        if let Ok(key_code) = serde_json::from_str::<KeyCode>(&bind_key_str) {
            bind_keys.push(BindKey::Key(PhysicalKey::Code(key_code)));
        } else if let Ok(key_code) =
            serde_json::from_str::<_>(&bind_key_str.replacen("\"Mouse", "\"", 1))
        {
            bind_keys.push(BindKey::Mouse(key_code));
        } else if let Ok(key_code) = serde_json::from_str::<_>(&bind_key_str) {
            bind_keys.push(BindKey::Extra(key_code));
        } else {
            let bind_key_str = format!("\"Key{cap_bind_key_str}\"");
            if let Ok(key_code) = serde_json::from_str::<KeyCode>(&bind_key_str) {
                bind_keys.push(BindKey::Key(PhysicalKey::Code(key_code)));
            }
        }
    }
    anyhow::ensure!(
        !bind_keys.is_empty(),
        "no keys in bind found: {bind_keys_str}"
    );
    Ok(bind_keys)
}

pub fn syn_to_bind_keys(
    path: &mut dyn Iterator<Item = &(Syn, Range<usize>)>,
) -> anyhow::Result<Vec<BindKey>> {
    let (keys_str, _) = path.next().ok_or_else(|| anyhow!("no keys text found"))?;
    let bind_keys = match keys_str {
        Syn::Text(keys_str) => bind_keys_str_to_bind_keys(keys_str)?,
        _ => anyhow::bail!("keys_str must be of type Text"),
    };
    Ok(bind_keys)
}

/// This is for a parsed console syntax
pub fn syn_to_bind(
    path: &[(Syn, Range<usize>)],
    map: &HashMap<&'static str, BindActionsLocalPlayer>,
) -> anyhow::Result<(Vec<BindKey>, Vec<BindAction>)> {
    let mut path = path.iter();

    let bind_keys = syn_to_bind_keys(&mut path)?;

    let (action, _) = path.next().ok_or_else(|| anyhow!("no action text found"))?;

    let actions = match action {
        Syn::Commands(actions) => actions
            .iter()
            .map(|action| match action_str_to_action(&action.cmd_text, map) {
                Ok(action) => BindAction::LocalPlayer(action),
                Err(_) => {
                    if action.cmd_text.starts_with("+") {
                        BindAction::TriggerCommand(action.clone())
                    } else {
                        BindAction::Command(action.clone())
                    }
                }
            })
            .collect(),
        act => anyhow::bail!("action must be of type \"Commands\", but was {:?}", act),
    };

    Ok((bind_keys, actions))
}

pub fn bind_keys_to_str(bind_keys: &[BindKey]) -> String {
    fn replace_inner_upper_with_underscore(s: &str) -> String {
        s.chars()
            .enumerate()
            .flat_map(|(index, c)| {
                if index != 0 && c.is_ascii_uppercase() {
                    vec!['_', c]
                } else {
                    vec![c]
                }
            })
            .collect()
    }

    let mut res = String::default();
    let key_chain_len = bind_keys.len();
    for (index, bind_key) in bind_keys.iter().enumerate() {
        match bind_key {
            BindKey::Key(key) => match key {
                PhysicalKey::Code(key) => {
                    res.push_str(
                        replace_inner_upper_with_underscore(
                            &serde_json::to_string(key)
                                .unwrap()
                                .replace("Key", "")
                                .replace('"', ""),
                        )
                        .to_lowercase()
                        .as_str(),
                    );
                }
                PhysicalKey::Unidentified(_) => {
                    // ignore
                }
            },
            BindKey::Mouse(btn) => {
                res.push_str(&format!(
                    "mouse_{}",
                    replace_inner_upper_with_underscore(
                        &serde_json::to_string(btn).unwrap().replace('"', ""),
                    )
                    .to_lowercase()
                ));
            }
            BindKey::Extra(ext) => {
                res.push_str(
                    replace_inner_upper_with_underscore(
                        &serde_json::to_string(ext).unwrap().replace('"', ""),
                    )
                    .to_lowercase()
                    .as_str(),
                );
            }
        }

        if index + 1 != key_chain_len {
            res.push('+');
        }
    }
    res
}

pub fn bind_to_str(
    bind_keys: &[BindKey],
    actions: Vec<BindAction>,
    map: &HashMap<BindActionsLocalPlayer, &'static str>,
) -> String {
    let mut res = "bind ".to_string();

    res.push_str(&bind_keys_to_str(bind_keys));

    res.push(' ');

    let actions_str = actions
        .into_iter()
        .map(|action| match action {
            BindAction::LocalPlayer(action) => {
                action_to_action_str(action, map).unwrap().to_string()
            }
            BindAction::Command(cmd) | BindAction::TriggerCommand(cmd) => cmd.to_string(),
        })
        .collect::<Vec<_>>()
        .join(";");

    res.push_str(&actions_str);

    res
}

pub fn str_to_bind_lossy(
    bind: &str,
    entries: &[ConsoleEntry],
    map: &HashMap<&'static str, BindActionsLocalPlayer>,
    cache: &ParserCache,
) -> Vec<(Vec<BindKey>, Vec<BindAction>)> {
    let cmds = parser::parse(bind, &entries_to_parser(entries), cache);

    let mut res: Vec<_> = Default::default();
    for cmd in &cmds {
        match cmd {
            CommandType::Full(cmd) => match syn_to_bind(&cmd.args, map) {
                Ok((keys, actions)) => {
                    res.push((keys, actions));
                }
                Err(err) => {
                    log::info!("ignored invalid bind (syntax error): {bind}, err: {err}");
                }
            },
            CommandType::Partial(err) => {
                log::info!("ignored invalid bind: {bind}, err: {err}");
            }
        }
    }
    res
}

pub fn str_list_to_binds_lossy(
    binds: &[String],
    entries: &[ConsoleEntry],
    map: &HashMap<&'static str, BindActionsLocalPlayer>,
    cache: &ParserCache,
) -> Vec<(Vec<BindKey>, Vec<BindAction>)> {
    binds
        .iter()
        .flat_map(|bind| str_to_bind_lossy(bind, entries, map, cache))
        .collect()
}

#[cfg(test)]
mod test {
    use command_parser::parser::{Command, Syn};
    use input_binds::binds::{BindKey, KeyCode, MouseButton, MouseExtra, PhysicalKey};

    use crate::binds::{
        bind_to_str, gen_local_player_action_hash_map, gen_local_player_action_hash_map_rev,
        syn_to_bind, BindAction, BindActionsCharacter, BindActionsLocalPlayer,
    };

    #[test]
    fn bind_json_abuses() {
        let map = gen_local_player_action_hash_map_rev();
        assert!(bind_to_str(
            &[BindKey::Key(PhysicalKey::Code(KeyCode::KeyT))],
            vec![BindAction::LocalPlayer(
                BindActionsLocalPlayer::ActivateChatInput
            )],
            &map
        )
        .contains("bind t "));
        assert!(bind_to_str(
            &[
                BindKey::Key(PhysicalKey::Code(KeyCode::ControlLeft)),
                BindKey::Key(PhysicalKey::Code(KeyCode::KeyT))
            ],
            vec![BindAction::LocalPlayer(
                BindActionsLocalPlayer::ActivateChatInput
            )],
            &map
        )
        .contains("bind control_left+t "));
        assert!(bind_to_str(
            &[BindKey::Mouse(MouseButton::Left)],
            vec![BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                BindActionsCharacter::Fire
            ))],
            &map
        )
        .contains("bind left "));
        assert!(bind_to_str(
            &[BindKey::Extra(MouseExtra::WheelDown)],
            vec![BindAction::LocalPlayer(BindActionsLocalPlayer::Character(
                BindActionsCharacter::PrevWeapon
            ))],
            &map
        )
        .contains("bind wheel_down "));

        let map = gen_local_player_action_hash_map();
        let res = syn_to_bind(
            &[
                (Syn::Text("wheel_down".to_string()), 0..0),
                (
                    Syn::Commands(vec![Command {
                        ident: "+fire".to_string(),
                        cmd_text: "+fire".to_string(),
                        cmd_range: 0..0,
                        args: vec![],
                    }]),
                    0..0,
                ),
            ],
            &map,
        );
        assert!(res.is_ok(), "{:?}", res);
        let res = syn_to_bind(
            &[
                (Syn::Text("wheel_up".to_string()), 0..0),
                (
                    Syn::Commands(vec![Command {
                        ident: "+fire".to_string(),
                        cmd_text: "+fire".to_string(),
                        cmd_range: 0..0,
                        args: vec![],
                    }]),
                    0..0,
                ),
            ],
            &map,
        );
        assert!(res.is_ok(), "{:?}", res);
    }
}
