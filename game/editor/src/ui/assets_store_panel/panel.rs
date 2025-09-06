use std::collections::{BTreeMap, HashSet};

use base::hash::{Hash, generate_hash_for};
use client_containers::container::ContainerItemIndexType;
use client_ui::{main_menu::settings::list::list, utils::render_texture_for_ui};
use egui::{Button, Rect, UiBuilder};
use game_interface::types::resource_key::ResourceKey;
use map::{
    map::resources::{MapResourceMetaData, MapResourceRef},
    skeleton::resources::MapResourceRefSkeleton,
};
use math::math::vector::vec2;
use sound::types::SoundPlayProps;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    actions::actions::{
        ActAddImage, ActAddImage2dArray, ActAddRemImage, ActAddRemSound, ActAddSound, EditorAction,
    },
    client::EditorClient,
    notifications::EditorNotification,
    tab::AssetsStoreTab,
    ui::user_data::UserDataWithTab,
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    let tab = &mut *pipe.user_data.editor_tab;

    if !tab.assets_store_open {
        return;
    };

    let res = {
        let mut panel = egui::SidePanel::right("assets_store_panel")
            .resizable(true)
            .width_range(600.0..=1200.0);
        panel = panel.default_width(800.0);

        let res = panel.show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add(
                        Button::new("Tilesets")
                            .selected(matches!(tab.assets_store.tab, AssetsStoreTab::Tileset)),
                    )
                    .clicked()
                {
                    tab.assets_store.tab = AssetsStoreTab::Tileset;
                }
                if ui
                    .add(
                        Button::new("Images")
                            .selected(matches!(tab.assets_store.tab, AssetsStoreTab::Image)),
                    )
                    .clicked()
                {
                    tab.assets_store.tab = AssetsStoreTab::Image;
                }
                if ui
                    .add(
                        Button::new("Sounds")
                            .selected(matches!(tab.assets_store.tab, AssetsStoreTab::Sound)),
                    )
                    .clicked()
                {
                    tab.assets_store.tab = AssetsStoreTab::Sound;
                }
            });

            if matches!(tab.assets_store.tab, AssetsStoreTab::Sound) {
                let entries: BTreeMap<_, _> = pipe
                    .user_data
                    .sound_images_container
                    .entries_index()
                    .into_iter()
                    .collect();

                let mut next_selected_asset = None;
                let mut add_selected_asset = None;

                let key: ResourceKey = tab
                    .assets_store
                    .selected_entry
                    .as_str()
                    .try_into()
                    .unwrap_or_default();
                let selected_fully_loaded = pipe.user_data.sound_images_container.is_loaded(&key)
                    && key.name.as_str() != "default";

                list::render(
                    ui,
                    entries
                        .iter()
                        .filter(|(_, v)| matches!(v, ContainerItemIndexType::Http))
                        .map(|(s, v)| (s.as_str(), *v)),
                    200.0,
                    |_, name| {
                        let valid: Result<ResourceKey, _> = name.try_into();
                        valid.map(|_| ()).map_err(|err| err.into())
                    },
                    |_, name| tab.assets_store.selected_entry == name,
                    |ui, _, name, pos, size| {
                        let key: ResourceKey = name.try_into().unwrap_or_default();
                        let is_fully_loaded = pipe.user_data.sound_images_container.is_loaded(&key);
                        ui.scope_builder(
                            UiBuilder::new().max_rect(Rect::from_center_size(
                                egui::pos2(pos.x, pos.y),
                                (size, size).into(),
                            )),
                            |ui| {
                                ui.centered_and_justified(|ui| {
                                    if is_fully_loaded {
                                        let snd = pipe
                                            .user_data
                                            .sound_images_container
                                            .get_or_default(&key);
                                        let is_playing = tab
                                            .assets_store
                                            .cur_play
                                            .as_ref()
                                            .is_some_and(|(s, _, _)| s == name);
                                        if is_playing {
                                            // pause
                                            if ui.button("\u{f04c}").clicked() {
                                                tab.assets_store.cur_play = None;
                                            }
                                        } else {
                                            // play
                                            if ui.button("\u{f04b}").clicked() {
                                                tab.assets_store.cur_play = Some((
                                                    name.to_string(),
                                                    snd.snd.play(
                                                        SoundPlayProps::new_with_pos_opt(None)
                                                            .with_looped(true),
                                                    ),
                                                    pipe.user_data
                                                        .container_scene
                                                        .sound_listener_handle
                                                        .create(Default::default()),
                                                ));
                                            }
                                        }
                                    } else {
                                        ui.label("...");
                                    }
                                });
                            },
                        );
                    },
                    |_, name| {
                        next_selected_asset = Some(name.to_string());
                    },
                    |_, _| None,
                    &mut tab.assets_store.search,
                    |ui| {
                        if ui
                            .add_enabled(selected_fully_loaded, Button::new("Add to map"))
                            .clicked()
                        {
                            add_selected_asset = Some(key);
                        }
                    },
                );

                if let Some(next_selected_asset) = next_selected_asset {
                    tab.assets_store.selected_entry = next_selected_asset;
                }
                if let Some(key) = add_selected_asset {
                    let img = pipe.user_data.sound_images_container.get_or_default(&key);

                    tab.client.execute(
                        EditorAction::AddSound(ActAddSound {
                            base: ActAddRemSound {
                                res: MapResourceRef {
                                    name: img.name.as_str().try_into().unwrap_or_default(),
                                    meta: MapResourceMetaData {
                                        blake3_hash: generate_hash_for(&img.file),
                                        ty: "ogg".try_into().unwrap(),
                                    },
                                    hq_meta: None,
                                },
                                file: img.file.clone(),
                                index: tab.map.resources.sounds.len(),
                            },
                        }),
                        None,
                    );
                }

                pipe.user_data.container_scene.stay_active();
            } else {
                let entries: BTreeMap<_, _> = pipe
                    .user_data
                    .quad_tile_images_container
                    .entries_index()
                    .into_iter()
                    .collect();
                let meta = pipe
                    .user_data
                    .quad_tile_images_container
                    .entries_meta_data();

                let mut next_selected_asset = None;
                let mut add_selected_asset = None;

                let key: ResourceKey = tab
                    .assets_store
                    .selected_entry
                    .as_str()
                    .try_into()
                    .unwrap_or_default();
                let selected_fully_loaded =
                    pipe.user_data.quad_tile_images_container.is_loaded(&key)
                        && key.name.as_str() != "default";

                list::render(
                    ui,
                    entries
                        .iter()
                        .filter(|(name, v)| {
                            meta.get(*name).is_some_and(|m| {
                                m.category.as_deref()
                                    == Some(
                                        if matches!(tab.assets_store.tab, AssetsStoreTab::Tileset) {
                                            "tileset"
                                        } else {
                                            "img"
                                        },
                                    )
                            }) && matches!(v, ContainerItemIndexType::Http)
                        })
                        .map(|(s, v)| (s.as_str(), *v)),
                    200.0,
                    |_, name| {
                        let valid: Result<ResourceKey, _> = name.try_into();
                        valid.map(|_| ()).map_err(|err| err.into())
                    },
                    |_, name| tab.assets_store.selected_entry == name,
                    |ui, _, name, pos, asset_size| {
                        let key: ResourceKey = name.try_into().unwrap_or_default();
                        let img = pipe
                            .user_data
                            .quad_tile_images_container
                            .get_or_default(&key);
                        let width = img.width as f32;
                        let height = img.height as f32;
                        let draw_width = asset_size;
                        let draw_height = asset_size - 30.0 * 2.0;
                        let w_scale = draw_width / width;
                        let h_scale = draw_height / height;
                        let scale = w_scale.min(h_scale).min(1.0);
                        render_texture_for_ui(
                            pipe.user_data.stream_handle,
                            pipe.user_data.canvas_handle,
                            &img.tex,
                            ui,
                            ui_state,
                            ui.ctx().screen_rect(),
                            Some(ui.clip_rect()),
                            pos,
                            vec2::new(width * scale, height * scale),
                            None,
                        );
                    },
                    |_, name| {
                        next_selected_asset = Some(name.to_string());
                    },
                    |_, _| None,
                    &mut tab.assets_store.search,
                    |ui| {
                        if ui
                            .add_enabled(selected_fully_loaded, Button::new("Add to map"))
                            .clicked()
                        {
                            add_selected_asset = Some(key);
                        }
                    },
                );

                if let Some(next_selected_asset) = next_selected_asset {
                    tab.assets_store.selected_entry = next_selected_asset;
                }

                if let Some(key) = add_selected_asset {
                    let img = pipe
                        .user_data
                        .quad_tile_images_container
                        .get_or_default(&key);

                    let res = MapResourceRef {
                        name: img.name.as_str().try_into().unwrap_or_default(),
                        meta: MapResourceMetaData {
                            blake3_hash: generate_hash_for(&img.file),
                            ty: "png".try_into().unwrap(),
                        },
                        hq_meta: None,
                    };
                    let imgs: HashSet<_> = tab
                        .map
                        .resources
                        .images
                        .iter()
                        .map(|i| i.def.meta.blake3_hash)
                        .chain(
                            tab.map
                                .resources
                                .image_arrays
                                .iter()
                                .map(|i| i.def.meta.blake3_hash),
                        )
                        .collect();
                    // ddnet limitation
                    if imgs.len() >= 64 {
                        tab.client.notifications.push(EditorNotification::Warning(
                            "Adding more than 64 images makes \
                            this map incompatible to (old) ddnet"
                                .to_string(),
                        ));
                    }
                    // true if no duplicate was found
                    fn check_res_duplicate<U>(
                        client: &EditorClient,
                        hash: &Hash,
                        resources: &[MapResourceRefSkeleton<U>],
                    ) -> bool {
                        if resources.iter().any(|r| r.def.meta.blake3_hash == *hash) {
                            client.notifications.push(EditorNotification::Warning(
                                "A resource with identical file \
                            hash already exists."
                                    .to_string(),
                            ));
                            false
                        } else {
                            true
                        }
                    }
                    if matches!(tab.assets_store.tab, AssetsStoreTab::Tileset) {
                        if check_res_duplicate(
                            &tab.client,
                            &res.meta.blake3_hash,
                            &tab.map.resources.image_arrays,
                        ) {
                            tab.client.execute(
                                EditorAction::AddImage2dArray(ActAddImage2dArray {
                                    base: ActAddRemImage {
                                        res,
                                        file: img.file.clone(),
                                        index: tab.map.resources.image_arrays.len(),
                                    },
                                }),
                                None,
                            );
                        }
                    } else if check_res_duplicate(
                        &tab.client,
                        &res.meta.blake3_hash,
                        &tab.map.resources.images,
                    ) {
                        tab.client.execute(
                            EditorAction::AddImage(ActAddImage {
                                base: ActAddRemImage {
                                    res,
                                    file: img.file.clone(),
                                    index: tab.map.resources.images.len(),
                                },
                            }),
                            None,
                        );
                    }
                }
            }
        });

        Some(res)
    };

    if let Some(res) = res {
        ui_state.add_blur_rect(res.response.rect, 0.0);
    }
}
