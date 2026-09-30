use anyhow::{Context, ensure};
use game_interface::types::{
    character_info::NetworkCharacterInfo, id_types::PlayerId, weapons::WeaponType,
};
use libtw2_gamenet_ddnet::{
    enums,
    snap_obj::{self, SnapObj},
};
use libtw2_snapshot::snap::{Builder, Snap};
use std::collections::{BTreeMap, HashMap};
use vanilla::{
    entities::character::hook::character_hook::Hook,
    snapshot::snapshot::{Snapshot, SnapshotCharacterPhasedState},
};

pub const MAX_SNAPSHOT: usize = 4 * 1024 * 1024;
#[derive(Default)]
pub struct Snapshots {
    pub race: bool,
    pub game_config: vanilla::config::config::ConfigVanilla,
    bases: BTreeMap<u64, (u64, Vec<u8>)>,
    pub players: HashMap<PlayerId, u16>,
    objects: HashMap<Vec<u8>, (u16, u64)>,
    player_seen: HashMap<PlayerId, u64>,
    pub latest_tick: u64,
    pub latest_id: Option<u64>,
}

fn add(builder: &mut Builder, id: u16, obj: impl Into<SnapObj>) -> anyhow::Result<()> {
    let obj = obj.into();
    builder
        .add_item(obj.obj_type_id(), id, obj.encode())
        .map_err(|e| anyhow::anyhow!("legacy snapshot item: {e:?}"))
}

pub fn weapon(w: WeaponType) -> i32 {
    match w {
        WeaponType::Hammer => 0,
        WeaponType::Gun => 1,
        WeaponType::Shotgun | WeaponType::Puller => 2,
        WeaponType::Grenade => 3,
        WeaponType::Laser => 4,
    }
}

fn string<const N: usize>(text: &str) -> [i32; N] {
    let mut bytes = vec![0u8; N * 4];
    let len = text.len().min(bytes.len() - 1);
    bytes[..len].copy_from_slice(&text.as_bytes()[..len]);
    std::array::from_fn(|i| {
        i32::from_be_bytes(std::array::from_fn(|j| bytes[i * 4 + j].wrapping_add(128)))
    })
}

fn info(
    builder: &mut Builder,
    id: u16,
    value: &NetworkCharacterInfo,
    local: bool,
    team: enums::Team,
    score: i64,
) -> anyhow::Result<()> {
    let (custom, body, feet) = match value.skin_info {
        game_interface::types::character_info::NetworkSkinInfo::Original => (0, 0, 0),
        game_interface::types::character_info::NetworkSkinInfo::Custom {
            body_color,
            feet_color,
        } => (
            1,
            math::colors::rgba_to_legacy_color(body_color, true, true),
            math::colors::rgba_to_legacy_color(feet_color, true, true),
        ),
    };
    add(builder, id, snap_obj::ClientInfo { name: string(value.name.as_str()), clan: string(value.clan.as_str()), country: -1, skin: string(std::borrow::Borrow::<game_interface::types::resource_key::ResourceKeyBase>::borrow(&value.skin).name.as_str()), use_custom_color: custom, color_body: body, color_feet: feet })?;
    add(
        builder,
        id,
        snap_obj::PlayerInfo {
            local: local.into(),
            client_id: id.into(),
            team,
            score: score.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            latency: 0,
        },
    )
}

impl Snapshots {
    pub fn decode(
        &mut self,
        data: &[u8],
        diff: Option<u64>,
        id: u64,
        tick: u64,
        keep: bool,
    ) -> anyhow::Result<Option<(u64, Snapshot)>> {
        let (id, tick, bytes) = if let Some(base_id) = diff {
            let Some((base_tick, base)) = self.bases.get(&base_id) else {
                return Ok(None);
            };
            let mut bytes = Vec::new();
            use std::io::Read;
            let dict = zstd::dict::DecoderDictionary::copy(base);
            zstd::Decoder::with_prepared_dictionary(data, &dict)?
                .take((MAX_SNAPSHOT + 1) as u64)
                .read_to_end(&mut bytes)?;
            (
                base_id.checked_add(id).context("snapshot id overflow")?,
                base_tick
                    .checked_add(tick)
                    .context("snapshot tick overflow")?,
                bytes,
            )
        } else {
            (id, tick, data.to_vec())
        };
        ensure!(bytes.len() <= MAX_SNAPSHOT, "snapshot too large");
        let (snapshot, consumed) = bincode::serde::decode_from_slice::<Snapshot, _>(
            &bytes,
            bincode::config::standard().with_limit::<MAX_SNAPSHOT>(),
        )?;
        ensure!(consumed == bytes.len(), "trailing snapshot bytes");
        if keep {
            self.bases.insert(id, (tick, bytes));
            while self.bases.len() > 32 {
                self.bases.pop_first();
            }
        }
        if self.latest_id.is_some_and(|latest| id <= latest) {
            return Ok(None);
        }
        self.latest_tick = tick;
        self.latest_id = Some(id);
        Ok(Some((tick, snapshot)))
    }

    fn object(&mut self, kind: u8, id: &impl serde::Serialize) -> anyhow::Result<u16> {
        let key = bincode::serde::encode_to_vec((kind, id), bincode::config::standard())?;
        if let Some((id, seen)) = self.objects.get_mut(&key) {
            *seen = self.latest_tick;
            return Ok(*id);
        }
        let id = (0..u16::MAX)
            .find(|id| !self.objects.values().any(|(v, _)| v == id))
            .context("legacy entity id space exhausted")?;
        self.objects.insert(key, (id, self.latest_tick));
        Ok(id)
    }

    pub fn build(
        &mut self,
        snapshot: &Snapshot,
        local: PlayerId,
        tick: i32,
        effects: &[SnapObj],
    ) -> anyhow::Result<Snap> {
        let mut builder = Builder::new();
        self.objects
            .retain(|_, (_, seen)| self.latest_tick.saturating_sub(*seen) < 150);
        self.players.retain(|id, _| {
            *id == local
                || self
                    .player_seen
                    .get(id)
                    .is_some_and(|seen| self.latest_tick.saturating_sub(*seen) < 150)
        });
        self.player_seen
            .retain(|id, _| self.players.contains_key(id));
        // Keep IDs stable across snapshots; the local player always gets slot zero.
        if self.players.is_empty() {
            self.players.insert(local, 0);
        }
        let ids = snapshot
            .stages
            .values()
            .flat_map(|s| s.world.characters.keys())
            .chain(snapshot.spectator_players.keys());
        for id in ids {
            self.player_seen.insert(*id, self.latest_tick);
            if !self.players.contains_key(id) {
                let slot = (0..64)
                    .find(|slot| !self.players.values().any(|v| v == slot))
                    .context("legacy client supports at most 64 player IDs per map")?;
                self.players.insert(*id, slot);
            }
        }
        let own_stage = snapshot
            .stages
            .values()
            .find(|s| {
                s.world.characters.contains_key(&local)
                    || snapshot.spectator_players.get(&local).is_some_and(|p| {
                        p.player
                            .spectated_characters
                            .iter()
                            .any(|id| s.world.characters.contains_key(id))
                    })
            })
            .or_else(|| snapshot.stages.values().next());
        use vanilla::match_state::match_state::{MatchState, MatchType};
        let game = own_stage.map(|s| &s.match_manager.game_match);
        let sided = game.is_some_and(|g| matches!(g.ty, MatchType::Sided { .. }));
        let ctf = own_stage
            .is_some_and(|s| !s.world.red_flags.is_empty() || !s.world.blue_flags.is_empty());
        let game_state_flags = game.map_or(0, |g| match g.state {
            MatchState::Running { .. } => 0,
            MatchState::Paused { .. } => 4,
            MatchState::SuddenDeath { .. } => 2,
            MatchState::PausedSuddenDeath { .. } => 6,
            MatchState::GameOver { .. } => 1,
        });
        add(
            &mut builder,
            0,
            snap_obj::GameInfo {
                game_flags: i32::from(sided) | (i32::from(ctf) * 2),
                game_state_flags,
                round_start_tick: snap_obj::Tick(game.map_or(0, |g| {
                    tick.saturating_sub(g.state.passed_ticks().min(i32::MAX as u64) as i32)
                })),
                warmup_timer: 0,
                score_limit: self.game_config.score_limit.min(i32::MAX as u64) as i32,
                time_limit: self
                    .game_config
                    .time_limit_secs
                    .div_ceil(60)
                    .min(i32::MAX as u64) as i32,
                round_num: 0,
                round_current: 1,
            },
        )?;
        add(
            &mut builder,
            0,
            snap_obj::GameInfoEx {
                flags: if self.race {
                    snap_obj::GAMEINFOFLAG_GAMETYPE_DDRACE
                        | snap_obj::GAMEINFOFLAG_GAMETYPE_DDNET
                        | snap_obj::GAMEINFOFLAG_PREDICT_DDRACE
                        | snap_obj::GAMEINFOFLAG_PREDICT_DDRACE_TILES
                        | snap_obj::GAMEINFOFLAG_ENTITIES_DDNET
                } else {
                    snap_obj::GAMEINFOFLAG_GAMETYPE_VANILLA | snap_obj::GAMEINFOFLAG_PREDICT_VANILLA
                },
                version: 6,
                flags2: 0,
            },
        )?;
        if let Some(stage) = own_stage {
            let scores = match stage.match_manager.game_match.ty {
                MatchType::Sided { scores } => scores,
                _ => [0, 0],
            };
            let carrier = |flags: &vanilla::snapshot::snapshot::SnapshotFlags| {
                flags.values().next().map_or(-3, |f| {
                    f.core
                        .carrier
                        .and_then(|id| self.players.get(&id))
                        .map_or(if f.core.drop_ticks.is_some() { -1 } else { -2 }, |v| {
                            *v as i32
                        })
                })
            };
            add(
                &mut builder,
                0,
                snap_obj::GameData {
                    teamscore_red: scores[0].clamp(i32::MIN as i64, i32::MAX as i64) as i32,
                    teamscore_blue: scores[1].clamp(i32::MIN as i64, i32::MAX as i64) as i32,
                    flag_carrier_red: carrier(&stage.world.red_flags),
                    flag_carrier_blue: carrier(&stage.world.blue_flags),
                },
            )?;
        }
        for stage in snapshot.stages.values() {
            for (strong_weak, (pid, c)) in stage.world.characters.iter().enumerate() {
                let id = self.players[pid];
                info(
                    &mut builder,
                    id,
                    &c.player_info.player_info,
                    *pid == local,
                    match c.core.side {
                        Some(game_interface::types::render::game::game_match::MatchSide::Blue) => {
                            enums::Team::Blue
                        }
                        _ => enums::Team::Red,
                    },
                    c.score,
                )?;
                if !own_stage.is_some_and(|s| s.game_el_id == stage.game_el_id) {
                    continue;
                }
                use vanilla::snapshot::snapshot::SnapshotCharacterSpectateMode as Camera;
                let (camera, flags) = match &c.phased {
                    SnapshotCharacterPhasedState::Normal {
                        ingame_spectate, ..
                    } => (
                        ingame_spectate.as_ref(),
                        if ingame_spectate.is_some() { 2 } else { 0 },
                    ),
                    SnapshotCharacterPhasedState::PhasedSpectate(mode) => (Some(mode), 4),
                    _ => (None, 0),
                };
                add(
                    &mut builder,
                    id,
                    snap_obj::DdnetPlayer {
                        flags,
                        auth_level: 0,
                    },
                )?;
                if *pid == local
                    && let Some(camera) = camera
                {
                    let (spectator_id, pos) = match camera {
                        Camera::Free(pos) => (-1, *pos),
                        Camera::Follows { ids, .. } => ids
                            .iter()
                            .find_map(|follow| {
                                snapshot.stages.values().find_map(|stage| {
                                    stage.world.characters.get(follow).map(|c| {
                                        (
                                            self.players.get(follow).map_or(-1, |id| *id as i32),
                                            c.pos,
                                        )
                                    })
                                })
                            })
                            .unwrap_or((-1, c.pos)),
                    };
                    add(
                        &mut builder,
                        id,
                        snap_obj::SpectatorInfo {
                            spectator_id,
                            x: pos.x as i32,
                            y: pos.y as i32,
                        },
                    )?;
                }
                let SnapshotCharacterPhasedState::Normal {
                    hook: (hook, hooked),
                    ..
                } = &c.phased
                else {
                    continue;
                };
                let (hs, ht, hx, hy, hdx, hdy) = match hook {
                    Hook::None => (0, 0, c.pos.x as i32, c.pos.y as i32, 0, 0),
                    Hook::WaitsForRelease => (-1, 0, c.pos.x as i32, c.pos.y as i32, 0, 0),
                    Hook::Active {
                        hook_pos,
                        hook_dir,
                        hook_tick,
                        hook_state,
                        ..
                    } => (
                        *hook_state as i32 + 1,
                        *hook_tick,
                        hook_pos.x.round() as i32,
                        hook_pos.y.round() as i32,
                        (hook_dir.x * 256.0).round() as i32,
                        (hook_dir.y * 256.0).round() as i32,
                    ),
                };
                let cursor = c.core.input.cursor.to_vec2();
                use game_interface::types::render::character::{
                    CharacterBuff, CharacterDebuff, TeeEye,
                };
                let frozen = c.reusable_core.debuffs.get(&CharacterDebuff::Freeze);
                let mut flags = 0;
                for (enabled, flag) in [
                    (c.core.core.solo, snap_obj::CHARACTERFLAG_SOLO),
                    (
                        c.core.core.collision_disabled,
                        snap_obj::CHARACTERFLAG_COLLISION_DISABLED,
                    ),
                    (
                        c.core.core.hook_hit_disabled,
                        snap_obj::CHARACTERFLAG_HOOK_HIT_DISABLED,
                    ),
                    (
                        c.core.core.has_endless,
                        snap_obj::CHARACTERFLAG_ENDLESS_HOOK,
                    ),
                    (
                        c.core.core.jumps.endless,
                        snap_obj::CHARACTERFLAG_ENDLESS_JUMP,
                    ),
                    (c.core.core.is_super, snap_obj::CHARACTERFLAG_SUPER),
                    (frozen.is_some(), snap_obj::CHARACTERFLAG_IN_FREEZE),
                ] {
                    if enabled {
                        flags |= flag;
                    }
                }
                for weapon in c.reusable_core.weapons.keys() {
                    flags |= 1 << (14 + self::weapon(*weapon));
                }
                let ninja = c.reusable_core.buffs.contains_key(&CharacterBuff::Ninja);
                if ninja {
                    flags |= snap_obj::CHARACTERFLAG_WEAPON_NINJA;
                }
                add(
                    &mut builder,
                    id,
                    snap_obj::DdnetCharacter {
                        flags,
                        freeze_end: snap_obj::Tick(frozen.map_or(0, |f| {
                            tick.saturating_add(
                                f.remaining_tick
                                    .get()
                                    .map_or(0, |t| t.get().min(i32::MAX as u64) as i32),
                            )
                        })),
                        jumps: c.core.core.jumps.max,
                        tele_checkpoint: c.core.tele_checkpoint as i32,
                        strong_weak_id: strong_weak as i32,
                        jumped_total: c.core.core.jumps.count,
                        ninja_activation_tick: snap_obj::Tick(0),
                        freeze_start: snap_obj::Tick(0),
                        target_x: (cursor.x * 32.0) as i32,
                        target_y: (cursor.y * 32.0) as i32,
                        tune_zone_override: 0,
                    },
                )?;

                add(
                    &mut builder,
                    id,
                    snap_obj::Character {
                        character_core: snap_obj::CharacterCore {
                            tick,
                            x: c.pos.x.round() as i32,
                            y: c.pos.y.round() as i32,
                            vel_x: (c.core.core.vel.x * 256.0).round() as i32,
                            vel_y: (c.core.core.vel.y * 256.0).round() as i32,
                            angle: (cursor.y.atan2(cursor.x) * 256.0) as i32,
                            direction: c.core.core.direction,
                            jumped: c.core.core.jumps.flag,
                            hooked_player: hooked
                                .and_then(|p| self.players.get(&p))
                                .map_or(-1, |v| *v as i32),
                            hook_state: hs,
                            hook_tick: ht,
                            hook_x: hx,
                            hook_y: hy,
                            hook_dx: hdx,
                            hook_dy: hdy,
                        },
                        player_flags: 0,
                        health: if *pid == local {
                            c.core.health.min(10) as i32
                        } else {
                            0
                        },
                        armor: if *pid == local {
                            c.core.armor.min(10) as i32
                        } else {
                            0
                        },
                        ammo_count: if *pid == local {
                            c.reusable_core
                                .weapons
                                .get(&c.core.active_weapon)
                                .and_then(|w| w.cur_ammo)
                                .unwrap_or(10)
                                .min(10) as i32
                        } else {
                            0
                        },
                        weapon: if ninja {
                            5
                        } else {
                            weapon(c.core.active_weapon)
                        },
                        emote: match c.core.eye {
                            TeeEye::Pain => enums::Emote::Pain,
                            TeeEye::Happy => enums::Emote::Happy,
                            TeeEye::Surprised => enums::Emote::Surprise,
                            TeeEye::Angry => enums::Emote::Angry,
                            TeeEye::Blink => enums::Emote::Blink,
                            _ => enums::Emote::Normal,
                        },
                        attack_tick: c.core.attack_recoil.action_ticks().map_or(0, |age| {
                            tick.saturating_sub(age.min(i32::MAX as u64) as i32)
                        }),
                    },
                )?;
            }
        }
        for (pid, s) in snapshot.spectator_players.iter() {
            info(
                &mut builder,
                self.players[pid],
                &s.player.player_info.player_info,
                *pid == local,
                enums::Team::Spectators,
                0,
            )?;
            if *pid == local {
                let pos = spectator_pos(snapshot, s);
                add(
                    &mut builder,
                    self.players[pid],
                    snap_obj::SpectatorInfo {
                        spectator_id: s
                            .player
                            .spectated_characters
                            .iter()
                            .find_map(|id| self.players.get(id))
                            .map_or(-1, |id| *id as i32),
                        x: pos.x.round() as i32,
                        y: pos.y.round() as i32,
                    },
                )?;
            }
        }
        if let Some(stage) = own_stage {
            for (pid, p) in stage.world.projectiles.iter() {
                let id = self.object(1, pid)?;
                add(
                    &mut builder,
                    id,
                    projectile(&p.core, &snapshot.global_tune_zone, tick),
                )?;
            }
            for (pid, p) in stage.world.ddrace_projectiles.iter() {
                let id = self.object(3, pid)?;
                let age = p.core.life_span.length().map_or(0, |len| {
                    len.get()
                        .saturating_sub(p.core.life_span.get().map_or(0, |v| v.get()))
                });
                add(
                    &mut builder,
                    id,
                    snap_obj::Projectile {
                        x: p.core.start_pos.x as i32,
                        y: p.core.start_pos.y as i32,
                        vel_x: (p.core.vel.x * 100.0) as i32,
                        vel_y: (p.core.vel.y * 100.0) as i32,
                        type_: projectile_weapon(p.core.ty),
                        start_tick: snap_obj::Tick(
                            tick.saturating_sub(age.min(i32::MAX as u64) as i32),
                        ),
                    },
                )?;
            }
            for (pid, p) in stage.world.pickups.iter() {
                use game_interface::types::pickup::PickupType::*;
                let (type_, subtype) = match p.core.ty {
                    PowerupHealth => (0, 0),
                    PowerupArmor => (1, 0),
                    PowerupNinja => (3, 0),
                    PowerupWeapon(w) => (2, weapon(w)),
                    PowerupWeaponShield(_) | PowerupNinjaShield => (1, 0),
                };
                let id = self.object(4, pid)?;
                add(
                    &mut builder,
                    id,
                    snap_obj::Pickup {
                        x: p.core.pos.x as i32,
                        y: p.core.pos.y as i32,
                        type_,
                        subtype,
                    },
                )?;
            }
            for (team, flags) in [(0, &stage.world.red_flags), (1, &stage.world.blue_flags)] {
                if let Some(f) = flags.values().next() {
                    add(
                        &mut builder,
                        team as u16,
                        snap_obj::Flag {
                            x: f.core.pos.x as i32,
                            y: f.core.pos.y as i32,
                            team,
                        },
                    )?;
                }
            }
            for (lid, l) in stage.world.lasers.iter() {
                let id = self.object(2, lid)?;
                add(
                    &mut builder,
                    id,
                    snap_obj::Laser {
                        x: l.core.pos.x as i32,
                        y: l.core.pos.y as i32,
                        from_x: l.core.from.x as i32,
                        from_y: l.core.from.y as i32,
                        start_tick: snap_obj::Tick(
                            l.core.next_eval_in.action_ticks().map_or(tick, |age| {
                                tick.saturating_sub(age.min(i32::MAX as u64) as i32)
                            }),
                        ),
                    },
                )?;
            }
        }
        for (id, event) in effects.iter().enumerate() {
            add(&mut builder, id as u16, *event)?;
        }
        Ok(builder.finish())
    }
}

fn projectile_weapon(
    w: game_interface::types::render::projectiles::WeaponWithProjectile,
) -> enums::Weapon {
    use game_interface::types::render::projectiles::WeaponWithProjectile::*;
    match w {
        Gun => enums::Weapon::Pistol,
        Shotgun => enums::Weapon::Shotgun,
        Grenade => enums::Weapon::Grenade,
    }
}

fn spectator_pos(
    snapshot: &Snapshot,
    spectator: &vanilla::snapshot::snapshot::SnapshotSpectatorPlayer,
) -> math::math::vector::vec2 {
    spectator
        .player
        .spectated_characters
        .iter()
        .find_map(|id| {
            snapshot
                .stages
                .values()
                .find_map(|stage| stage.world.characters.get(id).map(|c| c.pos))
        })
        .unwrap_or_else(|| {
            let pos = spectator.player.player_input.cursor.to_vec2();
            math::math::vector::vec2::new(pos.x as f32 * 32.0, pos.y as f32 * 32.0)
        })
}

fn projectile(
    core: &vanilla::entities::projectile::projectile::ProjectileCore,
    tuning: &vanilla::collision::Tunings,
    tick: i32,
) -> snap_obj::Projectile {
    use game_interface::types::render::projectiles::WeaponWithProjectile::*;
    let (speed, curvature) = match core.ty {
        Gun => (tuning.gun_speed, tuning.gun_curvature),
        Shotgun => (tuning.shotgun_speed, tuning.shotgun_curvature),
        Grenade => (tuning.grenade_speed, tuning.grenade_curvature),
    };
    // Legacy rendering interpolates behind the current tick. Rebase one tick
    // backwards so short-lived pellets are already visible there.
    let distance = speed / 50.0;
    let curvature = curvature / 10000.0;
    let mut dir = core.vel;
    dir.y -= 2.0 * curvature * distance;
    snap_obj::Projectile {
        x: (core.pos.x - dir.x * distance).round() as i32,
        y: (core.pos.y - dir.y * distance - curvature * distance * distance).round() as i32,
        vel_x: (dir.x * 100.0).round() as i32,
        vel_y: (dir.y * 100.0).round() as i32,
        type_: projectile_weapon(core.ty),
        start_tick: snap_obj::Tick(tick.saturating_sub(1)),
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use base::network_string::NetworkString;
    use pool::traits::Recyclable;
    use vanilla::snapshot::snapshot::*;

    pub fn fixture() -> (Snapshot, PlayerId) {
        let id = game_interface::types::id_types::CharacterId::from(
            "1".parse::<game_interface::types::id_gen::IdGeneratorIdType>()
                .unwrap(),
        );
        let stage_id = game_interface::types::id_types::StageId::from(
            "2".parse::<game_interface::types::id_gen::IdGeneratorIdType>()
                .unwrap(),
        );
        let pool = SnapshotPool::new(16, 1);
        let mut snapshot = Snapshot::new(&pool, "100".parse().unwrap(), None, Default::default());
        let world_pool = SnapshotWorldPool::new(16);
        let mut world = SnapshotWorld::new(&world_pool);
        let mut core = vanilla::entities::character::character::CharacterCore {
            health: 10,
            ..Default::default()
        };
        core.core.vel = math::math::vector::vec2::new(2.0, -3.0);
        world.characters.insert(id,SnapshotCharacter{
            core,reusable_core:pool::recycle::Recycle::from_without_pool(vanilla::entities::character::character::CharacterReusableCore::new()),
            player_info:vanilla::entities::character::player::player::PlayerInfo{player_info:pool::rc::PoolRc::from_item_without_pool(NetworkCharacterInfo::explicit_default()),version:1,unique_identifier:game_interface::types::player_info::PlayerUniqueId::CertFingerprint([0;32]),account_name:None,id:0},
            ty:SnapshotCharacterPlayerTy::Player(Default::default()),pos:math::math::vector::vec2::new(320.0,640.0),phased:SnapshotCharacterPhasedState::Normal{hook:(Hook::None,None),ingame_spectate:None},score:4,game_el_id:id,
        });
        snapshot.stages.insert(
            stage_id,
            SnapshotStage {
                world,
                match_manager: SnapshotMatchManager::new(
                    vanilla::match_state::match_state::Match {
                        ty: vanilla::match_state::match_state::MatchType::Solo,
                        state: vanilla::match_state::match_state::MatchState::Running {
                            round_ticks_passed: 10,
                            round_ticks_left: Default::default(),
                        },
                        balance_tick: Default::default(),
                    },
                ),
                game_el_id: stage_id,
                stage_name: base::network_string::PoolNetworkString::from_without_pool(
                    NetworkString::new_lossy("test"),
                ),
                stage_color: Default::default(),
            },
        );
        (snapshot, id)
    }

    #[test]
    fn projectiles_preserve_direction_and_reach_current_position() {
        use game_interface::types::render::projectiles::WeaponWithProjectile::*;
        use math::math::vector::vec2;
        let tuning = vanilla::collision::Tunings::default();
        for (ty, speed, curvature) in [
            (Gun, tuning.gun_speed, tuning.gun_curvature),
            (Shotgun, tuning.shotgun_speed, tuning.shotgun_curvature),
            (Grenade, tuning.grenade_speed, tuning.grenade_curvature),
        ] {
            let core = vanilla::entities::projectile::projectile::ProjectileCore {
                pos: vec2::new(320.0, 640.0),
                vel: vec2::new(0.8, -0.4),
                life_span: 20,
                damage: 1,
                force: 0.0,
                is_explosive: false,
                ty,
                side: None,
                can_hit_others: true,
            };
            let legacy = projectile(&core, &tuning, 100);
            assert_eq!(legacy.start_tick.0, 99);
            let mut pos = vec2::new(legacy.x as f32, legacy.y as f32);
            let mut vel = vec2::new(legacy.vel_x as f32 / 100.0, legacy.vel_y as f32 / 100.0);
            vanilla::entities::entity::entity::calc_pos_and_vel(
                &mut pos,
                &mut vel,
                curvature,
                speed,
                1.0 / 50.0,
            );
            assert!((pos.x - core.pos.x).abs() < 1.0);
            assert!((pos.y - core.pos.y).abs() < 1.0);
            assert!((vel.x - core.vel.x).abs() < 0.006);
            assert!((vel.y - core.vel.y).abs() < 0.006);
            assert_eq!(legacy.type_, projectile_weapon(ty));
        }
    }

    #[test]
    fn two_players_keep_distinct_ids_for_both_clients() {
        let (mut snapshot, first) = fixture();
        let (mut other_snapshot, _) = fixture();
        let second = PlayerId::from(
            "3".parse::<game_interface::types::id_gen::IdGeneratorIdType>()
                .unwrap(),
        );
        let mut other = other_snapshot
            .stages
            .values_mut()
            .next()
            .unwrap()
            .world
            .characters
            .remove(&first)
            .unwrap();
        other.game_el_id = second;
        snapshot
            .stages
            .values_mut()
            .next()
            .unwrap()
            .world
            .characters
            .insert(second, other);
        for local in [first, second] {
            let mut translator = Snapshots::default();
            let old = translator.build(&snapshot, local, 100, &[]).unwrap();
            assert_eq!(translator.players[&local], 0);
            assert_ne!(translator.players[&first], translator.players[&second]);
            for id in [0, 1] {
                assert!(
                    old.item(snap_obj::TypeId::Ordinal(snap_obj::CHARACTER), id)
                        .is_some()
                );
            }
        }
    }

    #[test]
    fn game_info_transfers_match_limits() {
        let (snapshot, local) = fixture();
        let mut snapshots = Snapshots::default();
        snapshots.game_config.score_limit = 25;
        snapshots.game_config.time_limit_secs = 300;
        let old = snapshots.build(&snapshot, local, 100, &[]).unwrap();
        let info = old
            .item(snap_obj::TypeId::Ordinal(snap_obj::GAME_INFO), 0)
            .unwrap();
        assert_eq!(info[4], 25);
        assert_eq!(info[5], 5);
    }

    #[test]
    fn shotgun_snapshot_keeps_weapon_while_firing() {
        let (mut snapshot, local) = fixture();
        let character = snapshot
            .stages
            .values_mut()
            .next()
            .unwrap()
            .world
            .characters
            .get_mut(&local)
            .unwrap();
        character.core.active_weapon = WeaponType::Shotgun;
        character.core.attack_recoil = 25.into();
        character.reusable_core.weapons.insert(
            WeaponType::Shotgun,
            vanilla::weapons::definitions::weapon_def::Weapon {
                cur_ammo: Some(9),
                next_ammo_regeneration_tick: Default::default(),
                upgrades: pool::datatypes::PoolFxHashSet::new_without_pool(),
            },
        );
        let old = Snapshots::default()
            .build(&snapshot, local, 100, &[])
            .unwrap();
        let character = old
            .item(snap_obj::TypeId::Ordinal(snap_obj::CHARACTER), 0)
            .unwrap();
        assert_eq!(character[18], 9);
        assert_eq!(character[19], 2);
    }

    #[test]
    fn spectator_snapshot_uses_followed_character_position() {
        let (mut snapshot, target) = fixture();
        let spectator_id = PlayerId::from(
            "2".parse::<game_interface::types::id_gen::IdGeneratorIdType>()
                .unwrap(),
        );
        let target_info = snapshot
            .stages
            .values()
            .next()
            .unwrap()
            .world
            .characters
            .get(&target)
            .unwrap()
            .player_info
            .clone();
        let mut followed = pool::datatypes::PoolFxHashSet::new_without_pool();
        followed.insert(target);
        snapshot.spectator_players.insert(
            spectator_id,
            SnapshotSpectatorPlayer {
                player: vanilla::entities::character::player::player::SpectatorPlayer::new(
                    target_info,
                    Default::default(),
                    &spectator_id,
                    followed,
                    Default::default(),
                    Default::default(),
                    Default::default(),
                ),
            },
        );
        let mut converter = Snapshots::default();
        let old = converter.build(&snapshot, spectator_id, 100, &[]).unwrap();
        let raw = old
            .item(snap_obj::TypeId::Ordinal(snap_obj::SPECTATOR_INFO), 0)
            .unwrap();
        assert_eq!(raw, &[converter.players[&target] as i32, 320, 640]);
    }

    #[test]
    fn reconstructs_new_deltas_and_emits_legacy_character() {
        let (snapshot, local) = fixture();
        let bytes = bincode::serde::encode_to_vec(&snapshot, bincode::config::standard()).unwrap();
        let mut converter = Snapshots::default();
        let (_, snapshot) = converter
            .decode(&bytes, None, 10, 100, true)
            .unwrap()
            .unwrap();
        let old = converter.build(&snapshot, local, 100, &[]).unwrap();
        let raw = old
            .item(
                libtw2_gamenet_ddnet::snap_obj::TypeId::Ordinal(snap_obj::CHARACTER),
                0,
            )
            .unwrap();
        assert_eq!(&raw[..5], &[100, 320, 640, 512, -768]);
        let mut patch = Vec::new();
        bin_patch::diff(&bytes, &bytes, &mut patch).unwrap();
        assert_eq!(
            converter
                .decode(&patch, Some(10), 2, 5, true)
                .unwrap()
                .unwrap()
                .0,
            105
        );
        assert_eq!(converter.latest_id, Some(12));
        assert!(
            converter
                .decode(&patch, Some(999), 1, 1, true)
                .unwrap()
                .is_none()
        );
        assert!(
            converter
                .decode(&bytes, None, 9, 99, true)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn legacy_delta_roundtrip() {
        let (snapshot, local) = fixture();
        let mut converter = Snapshots::default();
        let first = converter.build(&snapshot, local, 100, &[]).unwrap();
        let second = converter.build(&snapshot, local, 101, &[]).unwrap();
        let mut delta = libtw2_snapshot::snap::Delta::new();
        delta.create(&first, &second);
        let mut data = Vec::with_capacity(65536);
        libtw2_packer::with_packer(&mut data, |p| delta.write(snap_obj::obj_size, p)).unwrap();
        let mut decoded = libtw2_snapshot::snap::Delta::new();
        decoded
            .read(
                &mut crate::legacy::Ignore,
                snap_obj::obj_size,
                &mut libtw2_packer::Unpacker::new(&data),
            )
            .unwrap();
        let mut restored = libtw2_snapshot::snap::Snap::empty();
        restored
            .read_with_delta(&mut crate::legacy::Ignore, &first, &decoded)
            .unwrap();
        assert_eq!(restored.crc(), second.crc());
        assert_ne!(first.crc(), second.crc());
    }

    #[test]
    fn string_encoding_matches_legacy_termination() {
        assert_eq!(
            string::<1>("ab"),
            [i32::from_be_bytes([225, 226, 128, 128])]
        );
        assert_eq!(
            string::<1>("abcd"),
            [i32::from_be_bytes([225, 226, 227, 128])]
        );
    }
}
