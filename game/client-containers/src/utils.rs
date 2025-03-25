use std::sync::Arc;

use base_io::io::Io;
use graphics::graphics::graphics::Graphics;
use sound::{scene_object::SceneObject, sound::SoundManager};
use url::Url;

use crate::{
    container::ContainerLoadOptions,
    ctf::{CtfContainer, CTF_CONTAINER_PATH},
    emoticons::{EmoticonsContainer, EMOTICONS_CONTAINER_PATH},
    entities::{EntitiesContainer, ENTITIES_CONTAINER_PATH},
    flags::{FlagsContainer, FLAGS_CONTAINER_PATH},
    freezes::{FreezeContainer, FREEZE_CONTAINER_PATH},
    game::{GameContainer, GAME_CONTAINER_PATH},
    hooks::{HookContainer, HOOK_CONTAINER_PATH},
    hud::{HudContainer, HUD_CONTAINER_PATH},
    ninja::{NinjaContainer, NINJA_CONTAINER_PATH},
    particles::{ParticlesContainer, PARTICLES_CONTAINER_PATH},
    skins::{SkinContainer, SKIN_CONTAINER_PATH},
    weapons::{WeaponContainer, WEAPON_CONTAINER_PATH},
};

#[derive(Debug)]
pub struct RenderGameContainers {
    pub skin_container: SkinContainer,
    pub weapon_container: WeaponContainer,
    pub hook_container: HookContainer,
    pub ctf_container: CtfContainer,
    pub ninja_container: NinjaContainer,
    pub freeze_container: FreezeContainer,
    pub entities_container: EntitiesContainer,
    pub hud_container: HudContainer,
    pub emoticons_container: EmoticonsContainer,
    pub particles_container: ParticlesContainer,
    pub game_container: GameContainer,
    pub flags_container: FlagsContainer,
}

/// Loads all game containers at once.
///
/// `assume_unused` should only be `true` if
/// the containers are most likely not used,
/// e.g. for UI.
pub fn load_containers(
    io: &Io,
    thread_pool: &Arc<rayon::ThreadPool>,
    resource_http_download_url: Option<Url>,
    resource_server_download_url: Option<Url>,
    assume_unused: bool,
    graphics: &Graphics,
    sound: &SoundManager,
    scene: &SceneObject,
) -> RenderGameContainers {
    let default_skin = SkinContainer::load_default(io, SKIN_CONTAINER_PATH.as_ref());
    let default_weapon = WeaponContainer::load_default(io, WEAPON_CONTAINER_PATH.as_ref());
    let default_hook = HookContainer::load_default(io, HOOK_CONTAINER_PATH.as_ref());
    let default_ctf = CtfContainer::load_default(io, CTF_CONTAINER_PATH.as_ref());
    let default_ninja = NinjaContainer::load_default(io, NINJA_CONTAINER_PATH.as_ref());
    let default_freeze = FreezeContainer::load_default(io, FREEZE_CONTAINER_PATH.as_ref());
    let default_entities = EntitiesContainer::load_default(io, ENTITIES_CONTAINER_PATH.as_ref());
    let default_hud = HudContainer::load_default(io, HUD_CONTAINER_PATH.as_ref());
    let default_emoticons = EmoticonsContainer::load_default(io, EMOTICONS_CONTAINER_PATH.as_ref());
    let default_particles = ParticlesContainer::load_default(io, PARTICLES_CONTAINER_PATH.as_ref());
    let default_games = GameContainer::load_default(io, GAME_CONTAINER_PATH.as_ref());
    let default_flags = FlagsContainer::load_default(io, FLAGS_CONTAINER_PATH.as_ref());

    let skin_container = SkinContainer::new(
        io.clone(),
        thread_pool.clone(),
        default_skin,
        resource_http_download_url.clone(),
        resource_server_download_url.clone(),
        "skin-container",
        graphics,
        sound,
        scene,
        SKIN_CONTAINER_PATH.as_ref(),
        ContainerLoadOptions {
            assume_unused,
            ..Default::default()
        },
    );
    let weapon_container = WeaponContainer::new(
        io.clone(),
        thread_pool.clone(),
        default_weapon,
        resource_http_download_url.clone(),
        resource_server_download_url.clone(),
        "weapon-container",
        graphics,
        sound,
        scene,
        WEAPON_CONTAINER_PATH.as_ref(),
        ContainerLoadOptions {
            assume_unused,
            ..Default::default()
        },
    );
    let hook_container = HookContainer::new(
        io.clone(),
        thread_pool.clone(),
        default_hook,
        resource_http_download_url.clone(),
        resource_server_download_url.clone(),
        "hook-container",
        graphics,
        sound,
        scene,
        HOOK_CONTAINER_PATH.as_ref(),
        ContainerLoadOptions {
            assume_unused,
            ..Default::default()
        },
    );
    let ctf_container = CtfContainer::new(
        io.clone(),
        thread_pool.clone(),
        default_ctf,
        resource_http_download_url.clone(),
        resource_server_download_url.clone(),
        "ctf-container",
        graphics,
        sound,
        scene,
        CTF_CONTAINER_PATH.as_ref(),
        ContainerLoadOptions {
            assume_unused,
            ..Default::default()
        },
    );
    let ninja_container = NinjaContainer::new(
        io.clone(),
        thread_pool.clone(),
        default_ninja,
        resource_http_download_url.clone(),
        resource_server_download_url.clone(),
        "ninja-container",
        graphics,
        sound,
        scene,
        NINJA_CONTAINER_PATH.as_ref(),
        ContainerLoadOptions {
            assume_unused,
            ..Default::default()
        },
    );
    let freeze_container = FreezeContainer::new(
        io.clone(),
        thread_pool.clone(),
        default_freeze,
        resource_http_download_url.clone(),
        resource_server_download_url.clone(),
        "freeze-container",
        graphics,
        sound,
        scene,
        FREEZE_CONTAINER_PATH.as_ref(),
        ContainerLoadOptions {
            assume_unused,
            ..Default::default()
        },
    );
    let entities_container = EntitiesContainer::new(
        io.clone(),
        thread_pool.clone(),
        default_entities,
        resource_http_download_url.clone(),
        resource_server_download_url.clone(),
        "entities-container",
        graphics,
        sound,
        scene,
        ENTITIES_CONTAINER_PATH.as_ref(),
        ContainerLoadOptions {
            assume_unused,
            ..Default::default()
        },
    );
    let hud_container = HudContainer::new(
        io.clone(),
        thread_pool.clone(),
        default_hud,
        resource_http_download_url.clone(),
        resource_server_download_url.clone(),
        "hud-container",
        graphics,
        sound,
        scene,
        HUD_CONTAINER_PATH.as_ref(),
        ContainerLoadOptions {
            assume_unused,
            ..Default::default()
        },
    );
    let emoticons_container = EmoticonsContainer::new(
        io.clone(),
        thread_pool.clone(),
        default_emoticons,
        resource_http_download_url.clone(),
        resource_server_download_url.clone(),
        "emoticons-container",
        graphics,
        sound,
        scene,
        EMOTICONS_CONTAINER_PATH.as_ref(),
        ContainerLoadOptions {
            assume_unused,
            ..Default::default()
        },
    );
    let particles_container = ParticlesContainer::new(
        io.clone(),
        thread_pool.clone(),
        default_particles,
        resource_http_download_url.clone(),
        resource_server_download_url.clone(),
        "particles-container",
        graphics,
        sound,
        scene,
        PARTICLES_CONTAINER_PATH.as_ref(),
        ContainerLoadOptions {
            assume_unused,
            ..Default::default()
        },
    );
    let game_container = GameContainer::new(
        io.clone(),
        thread_pool.clone(),
        default_games,
        resource_http_download_url.clone(),
        resource_server_download_url.clone(),
        "games-container",
        graphics,
        sound,
        scene,
        GAME_CONTAINER_PATH.as_ref(),
        ContainerLoadOptions {
            assume_unused,
            ..Default::default()
        },
    );
    let flags_container = FlagsContainer::new(
        io.clone(),
        thread_pool.clone(),
        default_flags,
        resource_http_download_url.clone(),
        resource_server_download_url.clone(),
        "flags-container",
        graphics,
        sound,
        scene,
        FLAGS_CONTAINER_PATH.as_ref(),
        ContainerLoadOptions {
            assume_unused,
            ..Default::default()
        },
    );

    RenderGameContainers {
        skin_container,
        weapon_container,
        hook_container,
        ctf_container,
        ninja_container,
        freeze_container,
        entities_container,
        hud_container,
        emoticons_container,
        particles_container,
        game_container,
        flags_container,
    }
}

impl RenderGameContainers {
    pub fn clear_except_default(&mut self) {
        self.skin_container.clear_except_default();
        self.weapon_container.clear_except_default();
        self.hook_container.clear_except_default();
        self.ctf_container.clear_except_default();
        self.ninja_container.clear_except_default();
        self.freeze_container.clear_except_default();
        self.entities_container.clear_except_default();
        self.hud_container.clear_except_default();
        self.emoticons_container.clear_except_default();
        self.particles_container.clear_except_default();
        self.game_container.clear_except_default();
        self.flags_container.clear_except_default();
    }
}
