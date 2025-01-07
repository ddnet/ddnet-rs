pub mod events {
    use game_interface::{
        events::GameWorldActionKillWeapon,
        types::{
            flag::FlagType, game::GameTickCooldown, id_types::CharacterId, pickup::PickupType,
            render::projectiles::WeaponWithProjectile,
        },
    };
    use hiarc::Hiarc;
    use math::math::vector::vec2;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
    pub enum ProjectileEvent {
        Despawn {
            pos: vec2,
            respawns_in_ticks: GameTickCooldown,
        },
    }

    #[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
    pub enum LaserEvent {
        Despawn {
            pos: vec2,
            respawns_in_ticks: GameTickCooldown,
        },
    }

    #[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
    pub enum PickupEvent {
        Despawn {
            pos: vec2,
            ty: PickupType,
            respawns_in_ticks: GameTickCooldown,
        },
        Pickup {
            pos: vec2,
            by: CharacterId,
            ty: PickupType,
        },
    }

    #[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
    pub enum FlagEvent {
        Despawn {
            pos: vec2,
            ty: FlagType,
            respawns_in_ticks: GameTickCooldown,
        },
        Collect {
            by: CharacterId,
        },
        Capture {
            by: CharacterId,
            pos: vec2,
        },
    }

    #[derive(Debug, Hiarc, Serialize, Deserialize)]
    pub struct CharacterDespawnInfo {
        pub pos: vec2,
        pub respawns_in_ticks: GameTickCooldown,
        pub killer_id: Option<CharacterId>,
        pub weapon: GameWorldActionKillWeapon,
    }

    #[derive(Debug, Hiarc, Default)]
    pub enum CharacterDespawnType {
        #[default]
        DropFromGame,
        JoinsSpectator,
    }

    #[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
    pub enum CharacterEventMod {}

    #[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
    pub enum CharacterEvent {
        Despawn {
            id: CharacterId,
            killer_id: Option<CharacterId>,
            weapon: GameWorldActionKillWeapon,
        },
        Mod(CharacterEventMod),
    }

    #[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
    pub enum CharacterTickEvent {
        Projectile {
            pos: vec2,
            dir: vec2,
            ty: WeaponWithProjectile,
            lifetime: f32,
        },
        Laser {
            pos: vec2,
            dir: vec2,
            energy: f32,
            can_hit_own: bool,
        },
    }
}
