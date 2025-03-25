use api::{GRAPHICS, IO, RUNTIME_THREAD_POOL, SOUND};
use client_containers::container::ContainerLoadOptions;
use client_ui::main_menu::theme_container::{ThemeContainer, THEME_CONTAINER_PATH};

pub mod page;
pub mod profiles;

/// made to be easy to use for API stuff
pub fn create_theme_container() -> ThemeContainer {
    let default_item =
        ThemeContainer::load_default(&IO.with(|g| (*g).clone()), THEME_CONTAINER_PATH.as_ref());
    let scene = SOUND.with(|g| g.scene_handle.create(Default::default()));
    ThemeContainer::new(
        IO.with(|g| (*g).clone()),
        RUNTIME_THREAD_POOL.clone(),
        default_item,
        None,
        None,
        "theme-container",
        &GRAPHICS.with(|g| (*g).clone()),
        &SOUND.with(|g| (*g).clone()),
        &scene,
        THEME_CONTAINER_PATH.as_ref(),
        ContainerLoadOptions {
            assume_unused: true,
            ..Default::default()
        },
    )
}
