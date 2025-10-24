use std::collections::BTreeMap;

use client_containers::skins::SkinContainer;
use client_render_base::render::tee::RenderTee;
use config::{config::ConfigEngine, types::ConfRgb};
use egui::{Button, Color32, Grid, Layout};
use egui_extras::{Size, StripBuilder};
use game_config::config::ConfigTeeEye;
use game_interface::types::{
    character_info::{MAX_ASSET_NAME_LEN, NetworkSkinInfo},
    render::character::TeeEye,
    resource_key::{NetworkResourceKey, ResourceKey},
};
use graphics::handles::canvas::canvas::GraphicsCanvasHandle;
use math::math::vector::vec2;
use ui_base::{
    components::clearable_edit_field::clearable_edit_field,
    types::{UiRenderPipe, UiState},
};

use crate::{
    main_menu::{settings::player::profile_selector::profile_selector, user_data::UserData},
    utils::render_tee_for_ui,
};

pub fn eye_to_render_eye(eye: ConfigTeeEye) -> TeeEye {
    match eye {
        ConfigTeeEye::Normal => TeeEye::Normal,
        ConfigTeeEye::Pain => TeeEye::Pain,
        ConfigTeeEye::Happy => TeeEye::Happy,
        ConfigTeeEye::Surprised => TeeEye::Surprised,
        ConfigTeeEye::Angry => TeeEye::Angry,
        ConfigTeeEye::Blink => TeeEye::Blink,
    }
}

pub fn render_skin(
    ui: &mut egui::Ui,
    canvas_handle: &GraphicsCanvasHandle,
    skin_container: &mut SkinContainer,
    render_tee: &RenderTee,
    ui_state: &mut UiState,
    config_engine: &mut ConfigEngine,

    on_set: impl FnOnce(),

    skin_name: &str,
    mut set_name: impl FnMut(String),
    skin_eye: ConfigTeeEye,
    mut set_eye: impl FnMut(ConfigTeeEye),
    skin_info: NetworkSkinInfo,
    mut custom_colors: bool,
    mut body_color: ConfRgb,
    mut feet_color: ConfRgb,
    mut set_colors: impl FnMut(bool, ConfRgb, ConfRgb),
) {
    StripBuilder::new(ui)
        .size(Size::exact(180.0))
        .size(Size::remainder())
        .vertical(|mut strip| {
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                let render_eye = eye_to_render_eye(skin_eye);
                StripBuilder::new(ui)
                    .size(Size::exact(200.0))
                    .size(Size::remainder())
                    .horizontal(|mut strip| {
                        strip.cell(|ui| {
                            ui.style_mut().wrap_mode = None;
                            ui.label("Preview:");
                            let skin_size = 100.0;
                            let rect = ui.available_rect_before_wrap();
                            let pos = vec2::new(
                                rect.min.x + skin_size / 2.0,
                                rect.min.y + skin_size / 2.0,
                            );
                            render_tee_for_ui(
                                canvas_handle,
                                skin_container,
                                render_tee,
                                ui,
                                ui_state,
                                ui.ctx().screen_rect(),
                                Some(ui.clip_rect()),
                                &ResourceKey::from_str_lossy(skin_name),
                                Some(&skin_info),
                                pos,
                                skin_size,
                                render_eye,
                            );
                            ui.add_space(skin_size);
                            ui.horizontal(|ui| {
                                let mut name = skin_name.to_string();
                                clearable_edit_field(
                                    ui,
                                    &mut name,
                                    Some(skin_size + 20.0),
                                    Some(24),
                                );
                                set_name(name);
                            });
                            let resource_key: Result<NetworkResourceKey<MAX_ASSET_NAME_LEN>, _> =
                                skin_name.try_into();
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                            ui.colored_label(
                                Color32::RED,
                                if let Err(err) = &resource_key {
                                    format!(
                                        "Error: A valid skin name \
                                    must only contain [0-9,a-z,A-Z], \
                                    \"_\" or \"-\" characters \
                                    in their name and \
                                    must not exceed the 24 \
                                    character limit. Actual error: {err}"
                                    )
                                } else {
                                    "".to_string()
                                },
                            );
                        });
                        strip.cell(|ui| {
                            ui.style_mut().wrap_mode = None;

                            Grid::new("player-skin-options")
                                .num_columns(2)
                                .show(ui, |ui| {
                                    ui.label("Custom colors:");
                                    ui.checkbox(&mut custom_colors, "");
                                    ui.end_row();

                                    if custom_colors {
                                        ui.label("Color body:");
                                        let mut rgb = [body_color.r, body_color.g, body_color.b];
                                        ui.color_edit_button_srgb(&mut rgb);
                                        body_color = ConfRgb {
                                            r: rgb[0],
                                            g: rgb[1],
                                            b: rgb[2],
                                        };
                                        ui.end_row();

                                        ui.label("Color feet:");
                                        let mut rgb = [feet_color.r, feet_color.g, feet_color.b];
                                        ui.color_edit_button_srgb(&mut rgb);
                                        feet_color = ConfRgb {
                                            r: rgb[0],
                                            g: rgb[1],
                                            b: rgb[2],
                                        };
                                        ui.end_row();
                                    }
                                    set_colors(custom_colors, body_color, feet_color);
                                });

                            let mut add_eye_btn =
                                |ui: &mut egui::Ui, eye: TeeEye, config_eye: ConfigTeeEye| {
                                    let res = ui.add_sized(
                                        egui::vec2(50.0, 50.0),
                                        Button::new("").selected(skin_eye == config_eye),
                                    );
                                    let pos = res.rect.center();
                                    let skin_size = res.rect.height();
                                    let pos = vec2::new(pos.x, pos.y);
                                    render_tee_for_ui(
                                        canvas_handle,
                                        skin_container,
                                        render_tee,
                                        ui,
                                        ui_state,
                                        ui.ctx().screen_rect(),
                                        Some(ui.clip_rect()),
                                        &ResourceKey::from_str_lossy(skin_name),
                                        Some(&skin_info),
                                        pos,
                                        skin_size,
                                        eye,
                                    );
                                    if res.clicked() {
                                        set_eye(config_eye);
                                    }
                                };

                            let spacing = ui.style_mut().spacing.item_spacing;
                            ui.style_mut().spacing.item_spacing = egui::vec2(4.0, 4.0);
                            ui.horizontal(|ui| {
                                add_eye_btn(ui, TeeEye::Normal, ConfigTeeEye::Normal);
                                add_eye_btn(ui, TeeEye::Pain, ConfigTeeEye::Pain);
                                add_eye_btn(ui, TeeEye::Happy, ConfigTeeEye::Happy);
                            });
                            ui.horizontal(|ui| {
                                add_eye_btn(ui, TeeEye::Surprised, ConfigTeeEye::Surprised);
                                add_eye_btn(ui, TeeEye::Angry, ConfigTeeEye::Angry);
                                add_eye_btn(ui, TeeEye::Blink, ConfigTeeEye::Blink);
                            });
                            ui.style_mut().spacing.item_spacing = spacing;
                        });
                    });
            });
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                let entries = skin_container.entries_index();
                let entries_sorted = entries.into_iter().collect::<BTreeMap<_, _>>();
                let render_eye = eye_to_render_eye(skin_eye);
                let skin_search = config_engine
                    .ui
                    .path
                    .query
                    .entry("skin-search".to_string())
                    .or_default();
                let mut next_name = None;
                super::super::super::list::list::render(
                    ui,
                    entries_sorted.iter().map(|(name, &ty)| (name.as_str(), ty)),
                    100.0,
                    |_, name| {
                        let skin_valid: Result<NetworkResourceKey<MAX_ASSET_NAME_LEN>, _> =
                            name.try_into();
                        skin_valid.map(|_| ()).map_err(|err| err.into())
                    },
                    |_, name| skin_name == name,
                    |s| s,
                    |ui, _, name, pos, skin_size| {
                        render_tee_for_ui(
                            canvas_handle,
                            skin_container,
                            render_tee,
                            ui,
                            ui_state,
                            ui.ctx().screen_rect(),
                            Some(ui.clip_rect()),
                            &name.try_into().unwrap_or_default(),
                            Some(&skin_info),
                            pos,
                            skin_size,
                            render_eye,
                        );
                    },
                    |_, name| {
                        next_name = Some(name.to_string());
                    },
                    |_, _| None,
                    skin_search,
                    |_| {},
                );
                if let Some(next_name) = next_name.take() {
                    set_name(next_name);
                    on_set();
                }
            });
        });
}

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    ui.with_layout(Layout::top_down(egui::Align::Min), |ui| {
        let config = &mut pipe.user_data.config.game;

        let profile_index = profile_selector(
            ui,
            "skin-profile-selection",
            config,
            &mut pipe.user_data.config.engine,
        );
        ui.add_space(5.0);

        let player = &mut config.players[profile_index as usize];
        let name = player.skin.name.clone();
        let eye = player.eyes;
        let skin_info: NetworkSkinInfo = (&player.skin).into();
        render_skin(
            ui,
            pipe.user_data.canvas_handle,
            pipe.user_data.skin_container,
            pipe.user_data.render_tee,
            ui_state,
            &mut pipe.user_data.config.engine,
            || {
                pipe.user_data
                    .player_settings_sync
                    .set_player_info_changed();
            },
            &name,
            |name| {
                if player.skin.name != name {
                    pipe.user_data
                        .player_settings_sync
                        .set_player_info_changed();
                }
                player.skin.name = name;
            },
            eye,
            |eye| {
                if player.eyes != eye {
                    pipe.user_data
                        .player_settings_sync
                        .set_player_info_changed();
                }
                player.eyes = eye;
            },
            skin_info,
            player.skin.custom_colors,
            player.skin.body_color,
            player.skin.feet_color,
            |custom_colors, body_color, feet_color| {
                if player.skin.custom_colors != custom_colors
                    || player.skin.body_color != body_color
                    || player.skin.feet_color != feet_color
                {
                    pipe.user_data
                        .player_settings_sync
                        .set_player_info_changed();
                }
                player.skin.custom_colors = custom_colors;
                player.skin.body_color = body_color;
                player.skin.feet_color = feet_color;
            },
        );
    });
}
