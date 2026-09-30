use super::*;
use game_interface::events::*;
use libtw2_gamenet_ddnet::{
    enums::Sound,
    snap_obj::{self, SnapObj},
};

// Keep event bursts small to avoid handling libtw2's 64 KiB snapshot limit here.
// Other snapshot objects also consume space, so this is not a total-size guarantee.
const MAX_PENDING_EFFECTS: usize = 256;

fn sound(event: GameWorldEntitySoundEvent) -> Option<Sound> {
    use GameCharacterEventSound as C;
    Some(match event {
        GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Sound(event)) => {
            match event {
                C::WeaponSwitch { .. } => Sound::WeaponSwitch,
                C::NoAmmo { .. } => Sound::WeaponNoammo,
                C::HammerFire => Sound::HammerFire,
                C::GunFire => Sound::GunFire,
                C::GrenadeFire => Sound::GrenadeFire,
                C::LaserFire => Sound::RifleFire,
                C::ShotgunFire | C::PullerFire => Sound::ShotgunFire,
                C::GroundJump => Sound::PlayerJump,
                C::AirJump => Sound::PlayerAirjump,
                C::HookHitPlayer { .. } => Sound::HookAttachPlayer,
                C::HookHitHookable { .. } => Sound::HookAttachGround,
                C::HookHitUnhookable { .. } => Sound::HookNoattach,
                C::Spawn => Sound::PlayerSpawn,
                C::Death => Sound::PlayerDie,
                C::Pain { long: true } => Sound::PlayerPainLong,
                C::Pain { long: false } => Sound::PlayerPainShort,
                C::Hit { .. } => Sound::Hit,
                C::HammerHit => Sound::HammerHit,
            }
        }
        GameWorldEntitySoundEvent::Grenade(e) => match e {
            GameGrenadeEventSound::Spawn => Sound::WeaponSpawn,
            GameGrenadeEventSound::Collect => Sound::PickupGrenade,
            GameGrenadeEventSound::Explosion => Sound::GrenadeExplode,
        },
        GameWorldEntitySoundEvent::Laser(e) => match e {
            GameLaserEventSound::Spawn => Sound::WeaponSpawn,
            GameLaserEventSound::Collect => Sound::PickupShotgun,
            GameLaserEventSound::Bounce => Sound::RifleBounce,
        },
        GameWorldEntitySoundEvent::Shotgun(e) => match e {
            GameShotgunEventSound::Spawn => Sound::WeaponSpawn,
            GameShotgunEventSound::Collect => Sound::PickupShotgun,
        },
        GameWorldEntitySoundEvent::Puller(e) => match e {
            GamePullerEventSound::Spawn => Sound::WeaponSpawn,
            GamePullerEventSound::Collect => Sound::PickupShotgun,
        },
        GameWorldEntitySoundEvent::Flag(e) => match e {
            GameFlagEventSound::Capture => Sound::CtfCapture,
            GameFlagEventSound::Drop => Sound::CtfDrop,
            GameFlagEventSound::Return => Sound::CtfReturn,
            GameFlagEventSound::Collect { .. } => Sound::CtfGrabPl,
        },
        GameWorldEntitySoundEvent::Pickup(GamePickupSoundEvent::Heart(e)) => match e {
            GamePickupHeartEventSound::Spawn => Sound::WeaponSpawn,
            GamePickupHeartEventSound::Collect => Sound::PickupHealth,
        },
        GameWorldEntitySoundEvent::Pickup(GamePickupSoundEvent::Armor(e)) => match e {
            GamePickupArmorEventSound::Spawn => Sound::WeaponSpawn,
            GamePickupArmorEventSound::Collect => Sound::PickupArmor,
        },
        _ => return None,
    })
}

fn chat(
    net: &mut Net<SocketAddr>,
    socket: &mut Socket,
    pid: PeerId,
    message: &str,
) -> anyhow::Result<()> {
    let msg = text::<512>(message.as_bytes());
    send_game_ref(
        net,
        socket,
        pid,
        game::SvChat {
            team: 0,
            client_id: -1,
            message: msg.as_str().as_bytes(),
        }
        .into(),
    )
}

pub(super) fn translate(
    client: &mut Client,
    net: &mut Net<SocketAddr>,
    socket: &mut Socket,
    pid: PeerId,
    events: game_interface::events::GameEvents,
) -> anyhow::Result<()> {
    for world in events.worlds.values() {
        for event in world.events.values() {
            match event {
                GameWorldEvent::Sound(event) => {
                    if let Some(sound_id) = sound(event.ev) {
                        if let Some(pos) = event.pos {
                            client.effects.push(SnapObj::from(snap_obj::SoundWorld {
                                common: snap_obj::Common {
                                    x: (pos.x * 32.0) as i32,
                                    y: (pos.y * 32.0) as i32,
                                },
                                sound_id,
                            }));
                        } else {
                            send_game(net, socket, pid, game::SvSoundGlobal { sound_id })?;
                        }
                    }
                }
                GameWorldEvent::Effect(event) => {
                    let common = snap_obj::Common {
                        x: (event.pos.x * 32.0) as i32,
                        y: (event.pos.y * 32.0) as i32,
                    };
                    let obj: Option<SnapObj> = match event.ev {
                        GameWorldEntityEffectEvent::Grenade(GameGrenadeEventEffect::Explosion) => {
                            Some(snap_obj::Explosion { common }.into())
                        }
                        GameWorldEntityEffectEvent::Character(
                            GameCharacterEffectEvent::Effect(effect),
                        ) => match effect {
                            GameCharacterEventEffect::Spawn => {
                                Some(snap_obj::Spawn { common }.into())
                            }
                            GameCharacterEventEffect::Death => Some(
                                snap_obj::Death {
                                    common,
                                    client_id: event
                                        .owner_id
                                        .and_then(|id| client.snapshots.players.get(&id))
                                        .map_or(0, |v| *v as i32),
                                }
                                .into(),
                            ),
                            GameCharacterEventEffect::HammerHit => {
                                Some(snap_obj::HammerHit { common }.into())
                            }
                            GameCharacterEventEffect::DamageIndicator { vel } => Some(
                                snap_obj::DamageInd {
                                    common,
                                    // DDNet negates the direction when creating the indicator.
                                    angle: ((-vel.y).atan2(-vel.x) * 256.0) as i32,
                                }
                                .into(),
                            ),
                            _ => None,
                        },
                        _ => None,
                    };
                    if let Some(obj) = obj {
                        client.effects.push(obj);
                    }
                }
                GameWorldEvent::Notification(GameWorldNotificationEvent::Motd { msg }) => {
                    send_game_ref(
                        net,
                        socket,
                        pid,
                        game::SvMotd {
                            message: msg.as_str().as_bytes(),
                        }
                        .into(),
                    )?
                }
                GameWorldEvent::Notification(GameWorldNotificationEvent::System(msg)) => {
                    match msg {
                        GameWorldSystemMessage::Custom(msg) => {
                            chat(net, socket, pid, msg.as_str())?
                        }
                        GameWorldSystemMessage::PlayerJoined { name, .. } => chat(
                            net,
                            socket,
                            pid,
                            &format!("{} entered the game", name.as_str()),
                        )?,
                        GameWorldSystemMessage::PlayerLeft { name, .. } => chat(
                            net,
                            socket,
                            pid,
                            &format!("{} left the game", name.as_str()),
                        )?,
                        GameWorldSystemMessage::CharacterInfoChanged {
                            old_name, new_name, ..
                        } => {
                            if old_name.as_str() != new_name.as_str() {
                                chat(
                                    net,
                                    socket,
                                    pid,
                                    &format!(
                                        "{} is now known as {}",
                                        old_name.as_str(),
                                        new_name.as_str()
                                    ),
                                )?;
                            }
                        }
                    }
                }
                GameWorldEvent::Notification(GameWorldNotificationEvent::Action(action)) => {
                    match action {
                        GameWorldAction::Kill {
                            killer,
                            victims,
                            weapon,
                            ..
                        } => {
                            for victim in victims.iter() {
                                if let Some(&victim) = client.snapshots.players.get(victim) {
                                    let killer = killer
                                        .and_then(|id| client.snapshots.players.get(&id))
                                        .copied()
                                        .unwrap_or(victim);
                                    let weapon = match weapon {
                                        GameWorldActionKillWeapon::Weapon { weapon } => {
                                            snapshot::weapon(*weapon)
                                        }
                                        GameWorldActionKillWeapon::Ninja => 5,
                                        GameWorldActionKillWeapon::World => -1,
                                    };
                                    send_game(
                                        net,
                                        socket,
                                        pid,
                                        game::SvKillMsg {
                                            killer: killer as i32,
                                            victim: victim as i32,
                                            weapon,
                                            mode_special: 0,
                                        },
                                    )?;
                                }
                            }
                        }
                        GameWorldAction::Custom(msg) => chat(net, socket, pid, msg.as_str())?,
                        GameWorldAction::RaceFinish { finish_time, .. }
                        | GameWorldAction::RaceTeamFinish { finish_time, .. } => chat(
                            net,
                            socket,
                            pid,
                            &format!("Finished in {:.2} seconds", finish_time.as_secs_f64()),
                        )?,
                    }
                }
            }
            if client.effects.len() > MAX_PENDING_EFFECTS {
                client
                    .effects
                    .drain(..client.effects.len() - MAX_PENDING_EFFECTS);
            }
        }
    }
    Ok(())
}
