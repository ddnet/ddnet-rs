use api::{GRAPHICS, IO, RUNTIME_THREAD_POOL, SOUND};
use client_containers::container::ContainerLoadOptions;
use client_ui::{
    main_menu::theme_container::{THEME_CONTAINER_PATH, ThemeContainer},
    thumbnail_container::ThumbnailContainer,
};

pub mod page;
pub mod profiles;

/// made to be easy to use for API stuff
pub fn create_thumbnail_container(path: &str, container_name: &str) -> ThumbnailContainer {
    let default_item = ThumbnailContainer::load_default(&IO.with(|g| (*g).clone()), path.as_ref());
    let scene = SOUND.with(|g| g.scene_handle.create(Default::default()));
    ThumbnailContainer::new(
        IO.with(|g| (*g).clone()),
        RUNTIME_THREAD_POOL.clone(),
        default_item,
        None,
        None,
        container_name,
        &GRAPHICS.with(|g| (*g).clone()),
        &SOUND.with(|g| (*g).clone()),
        &scene,
        path.as_ref(),
        ContainerLoadOptions {
            assume_unused: true,
            ..Default::default()
        },
    )
}

/// made to be easy to use for API stuff
pub fn create_theme_container() -> ThemeContainer {
    create_thumbnail_container(THEME_CONTAINER_PATH, "theme-container")
}
