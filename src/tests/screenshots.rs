use std::{rc::Rc, time::Duration};

use base::benchmark::Benchmark;
use client_containers::{
    container::ContainerKey,
    utils::{load_containers, RenderGameContainers},
};
use client_render_base::render::{tee::RenderTee, toolkit::ToolkitRender};
use client_ui::thumbnail_container::{
    load_thumbnail_container, ThumbnailContainer, DEFAULT_THUMBNAIL_CONTAINER_PATH,
};
use game_interface::types::character_info::NetworkSkinInfo;
use graphics::graphics::graphics::Graphics;
use graphics_backend::backend::GraphicsBackend;
use math::math::vector::ubvec4;
use ui_base::{
    font_data::{UiFontData, UiFontDataLoading},
    ui::UiCreator,
};

use super::{
    actionfeed::test_actionfeed,
    base::{get_base, Options},
    chat::test_chat,
    emote_wheel::test_emote_wheel,
    hud::test_hud,
    ingame::{test_ingame, test_ingame_skins},
    motd::test_motd,
    scoreboard::test_scoreboard,
    screenshot::save_screenshot,
    spectator_selection::test_spectator_selection,
    vote::test_vote,
};

fn prepare(
    backend_validation: bool,
    options: Option<Options>,
) -> (
    Graphics,
    Rc<GraphicsBackend>,
    UiCreator,
    RenderGameContainers,
    ThumbnailContainer,
    RenderTee,
    ToolkitRender,
) {
    let (io, tp, graphics, graphics_backend, sound) = get_base(backend_validation, options);

    let font_loading = UiFontDataLoading::new(&io);
    let font_data = UiFontData::new(font_loading)
        .unwrap()
        .into_font_definitions();

    let mut creator = UiCreator::default();
    creator.load_font(&font_data);

    let scene = sound.scene_handle.create(Default::default());
    let containers = load_containers(&io, &tp, None, None, true, &graphics, &sound, &scene);

    let render_tee = RenderTee::new(&graphics);
    let toolkit_render = ToolkitRender::new(&graphics);

    let map_vote_thumbnail_container = load_thumbnail_container(
        io,
        tp,
        DEFAULT_THUMBNAIL_CONTAINER_PATH,
        "map-vote-thumbnail",
        &graphics,
        &sound,
        scene,
        None,
    );
    (
        graphics,
        graphics_backend,
        creator,
        containers,
        map_vote_thumbnail_container,
        render_tee,
        toolkit_render,
    )
}

fn test_screenshots(
    backend_validation: bool,
    save_screenshot: impl Fn(&Graphics, &Rc<GraphicsBackend>, &str),
) {
    let (
        graphics,
        graphics_backend,
        creator,
        mut containers,
        mut map_vote_thumbnail_container,
        render_tee,
        toolkit_render,
    ) = prepare(backend_validation, None);

    test_hud(&graphics, &creator, &mut containers, &render_tee, |name| {
        save_screenshot(&graphics, &graphics_backend, name)
    });
    test_scoreboard(&graphics, &creator, &mut containers, &render_tee, |name| {
        save_screenshot(&graphics, &graphics_backend, name)
    });
    test_chat(&graphics, &creator, &mut containers, &render_tee, |name| {
        save_screenshot(&graphics, &graphics_backend, name)
    });
    test_actionfeed(
        &graphics,
        &creator,
        &mut containers,
        &render_tee,
        &toolkit_render,
        |name| save_screenshot(&graphics, &graphics_backend, name),
    );
    test_emote_wheel(&graphics, &creator, &mut containers, &render_tee, |name| {
        save_screenshot(&graphics, &graphics_backend, name)
    });
    test_motd(&graphics, &creator, |name| {
        save_screenshot(&graphics, &graphics_backend, name)
    });
    test_vote(
        &graphics,
        &creator,
        &mut containers,
        &mut map_vote_thumbnail_container,
        &render_tee,
        |name| save_screenshot(&graphics, &graphics_backend, name),
    );
    test_spectator_selection(&graphics, &creator, &mut containers, &render_tee, |name| {
        save_screenshot(&graphics, &graphics_backend, name)
    });
    test_ingame(
        &graphics,
        &creator,
        &mut containers,
        |name| save_screenshot(&graphics, &graphics_backend, name),
        100,
        1,
    );
}

#[test]
fn create_screenshots() {
    test_screenshots(true, |graphics, graphics_backend, name| {
        save_screenshot(graphics, graphics_backend, name)
    });
}

#[test]
fn benchmark_screenshots() {
    let (graphics, _, creator, mut containers, _, _, _) = prepare(false, None);
    let b = Benchmark::new(true);
    test_ingame(
        &graphics,
        &creator,
        &mut containers,
        |_| {
            // ignore
        },
        10000,
        5,
    );
    b.bench("ingame 10000 players, 5 run(s)");
}

#[test]
fn test_all_skins() {
    let (graphics, graphics_backend, creator, mut containers, _, _, _) = prepare(
        false,
        Some(Options {
            width: 8000,
            height: 8000,
        }),
    );
    let entries = loop {
        let entries = containers.skin_container.entries_index();
        if !entries.is_empty() {
            break entries;
        }

        std::thread::sleep(Duration::from_secs(1));
    };
    for entry in entries.keys() {
        let key: Result<ContainerKey, _> = entry.as_str().try_into();
        if let Ok(key) = key {
            containers.skin_container.get_or_default(&key);
        }
    }
    for entry in entries.keys() {
        let key: Result<ContainerKey, _> = entry.as_str().try_into();
        if let Ok(key) = key {
            containers.skin_container.blocking_wait_loaded(&key);
        }
    }
    test_ingame_skins(
        &graphics,
        &creator,
        &mut containers,
        NetworkSkinInfo::Original,
        true,
        |name| save_screenshot(&graphics, &graphics_backend, &format!("original_{}", name)),
        1,
    );
    let mut custom = |prefix: &str, body_color: ubvec4, feet_color: ubvec4| {
        test_ingame_skins(
            &graphics,
            &creator,
            &mut containers,
            NetworkSkinInfo::Custom {
                body_color,
                feet_color,
            },
            true,
            |name| save_screenshot(&graphics, &graphics_backend, &format!("{prefix}_{name}")),
            1,
        )
    };
    // grey scales
    custom(
        "white",
        ubvec4::new(255, 255, 255, 255),
        ubvec4::new(255, 255, 255, 255),
    );
    custom(
        "dark_grey",
        ubvec4::new(50, 50, 50, 255),
        ubvec4::new(50, 50, 50, 255),
    );
    let mut colors = |prefix: &str, div_factor: u8, zero_offset: u8| {
        custom(
            &format!("{prefix}yellow_red"),
            ubvec4::new(255 / div_factor, 255 / div_factor, zero_offset, 255),
            ubvec4::new(255 / div_factor, zero_offset, zero_offset, 255),
        );
        custom(
            &format!("{prefix}red_yellow"),
            ubvec4::new(255 / div_factor, zero_offset, zero_offset, 255),
            ubvec4::new(255 / div_factor, 255 / div_factor, zero_offset, 255),
        );
        custom(
            &format!("{prefix}purple_blue"),
            ubvec4::new(255 / div_factor, zero_offset, 255 / div_factor, 255),
            ubvec4::new(zero_offset, zero_offset, 255 / div_factor, 255),
        );
        custom(
            &format!("{prefix}blue_purple"),
            ubvec4::new(zero_offset, zero_offset, 255 / div_factor, 255),
            ubvec4::new(255 / div_factor, zero_offset, 255 / div_factor, 255),
        );
        custom(
            &format!("{prefix}cyan_green"),
            ubvec4::new(zero_offset, 255 / div_factor, 255 / div_factor, 255),
            ubvec4::new(zero_offset, 255 / div_factor, zero_offset, 255),
        );
        custom(
            &format!("{prefix}green_cyan"),
            ubvec4::new(zero_offset, 255 / div_factor, zero_offset, 255),
            ubvec4::new(zero_offset, 255 / div_factor, 255 / div_factor, 255),
        );
    };
    // normal
    colors("", 1, 0);
    // dark
    colors("dark_", 4, 0);
    // light
    colors("light_", 1, 128);
}
