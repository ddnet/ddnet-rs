use std::sync::Arc;

use base_io::io::Io;
use config::types::ConfRgb;
use egui::{Button, Grid, Layout, Window};
use egui_extras::{Size, StripBuilder};
use game_config::config::ConfigTeeEye;
use game_interface::types::{
    character_info::NetworkSkinInfo, render::character::TeeEye, resource_key::ResourceKey,
};
use math::math::vector::{ubvec4, vec2};
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    main_menu::{
        profiles_interface::{LinkedCredential, ProfilesInterface},
        settings::player::tee::main_frame::{eye_to_render_eye, render_skin},
        user_data::{PROFILE_SKIN_PREVIEW, ProfileSkin, ProfileState, UserData},
    },
    utils::render_tee_for_ui,
};

use super::back_bar::back_bar;

fn render_eye_to_eye(eye: TeeEye) -> ConfigTeeEye {
    match eye {
        TeeEye::Normal => ConfigTeeEye::Normal,
        TeeEye::Pain => ConfigTeeEye::Pain,
        TeeEye::Happy => ConfigTeeEye::Happy,
        TeeEye::Surprised => ConfigTeeEye::Surprised,
        TeeEye::Angry => ConfigTeeEye::Angry,
        TeeEye::Blink => ConfigTeeEye::Blink,
    }
}

/// overview
pub fn render(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    accounts: &Arc<dyn ProfilesInterface>,
    io: &Io,
) {
    let user_data = &mut *pipe.user_data;
    let tasks = &mut *user_data.profile_tasks;
    back_bar(ui, "Account overview", tasks);

    if let ProfileState::AccountInfo {
        info,
        profile_name,
        profile_data,
    } = &mut tasks.state
    {
        let mut next_state = None;
        Grid::new("account_info").num_columns(2).show(ui, |ui| {
            ui.label("Profile name:");
            StripBuilder::new(ui)
                .size(Size::remainder())
                .size(Size::exact(30.0))
                .horizontal(|mut strip| {
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        ui.text_edit_singleline(&mut profile_data.name);
                    });
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        if ui.button("\u{f00c}").clicked() {
                            let accounts = accounts.clone();
                            let display_name = profile_data.name.clone();
                            let profile_name = profile_name.clone();
                            tasks.user_interactions.push(
                                io.rt
                                    .spawn(async move {
                                        accounts
                                            .set_profile_display_name(&profile_name, display_name)
                                            .await;
                                        Ok(())
                                    })
                                    .abortable(),
                            );
                        }
                    });
                });
            ui.end_row();
            ui.label("Account id:");
            ui.label(info.account_id.to_string());
            ui.end_row();
            ui.label("Creation date:");
            ui.label(&info.creation_date);
            ui.end_row();
            let can_unlink = info.credentials.len() >= 2;
            for credential in info.credentials.iter() {
                match credential {
                    LinkedCredential::Email(mail) => {
                        ui.label("Linked email:");
                        StripBuilder::new(ui)
                            .size(Size::remainder())
                            .size(Size::exact(30.0))
                            .horizontal(|mut strip| {
                                strip.cell(|ui| {
                                    ui.style_mut().wrap_mode = None;
                                    ui.label(mail);
                                });
                                strip.cell(|ui| {
                                    ui.style_mut().wrap_mode = None;
                                    if can_unlink && ui.button("\u{f1f8}").clicked() {
                                        let profile_name = profile_name.clone();
                                        next_state = Some(ProfileState::UnlinkEmailPrepare {
                                            profile_name,
                                            info: info.clone(),
                                        });
                                    }
                                });
                            });
                    }
                    LinkedCredential::Steam(id) => {
                        ui.label("Linked steam id:");
                        StripBuilder::new(ui)
                            .size(Size::remainder())
                            .size(Size::exact(30.0))
                            .horizontal(|mut strip| {
                                strip.cell(|ui| {
                                    ui.style_mut().wrap_mode = None;
                                    ui.label(id.to_string());
                                });
                                strip.cell(|ui| {
                                    ui.style_mut().wrap_mode = None;
                                    if can_unlink
                                        && accounts.steam_id64() == *id
                                        && ui.button("\u{f1f8}").clicked()
                                    {
                                        let profile_name = profile_name.clone();
                                        next_state = Some(ProfileState::UnlinkSteamPrepare {
                                            profile_name,
                                            info: info.clone(),
                                        });
                                    }
                                });
                            });
                    }
                }
                ui.end_row();
            }

            let skin_preview = profile_data
                .user
                .entry(PROFILE_SKIN_PREVIEW.to_string())
                .or_default()
                .clone();
            let mut skin_preview = serde_json::from_value::<ProfileSkin>(skin_preview)
                .unwrap_or_else(|_| ProfileSkin {
                    name: "".try_into().unwrap(),
                    color_body: None,
                    color_feet: None,
                    eye: TeeEye::Happy,
                });
            let prev_skin = skin_preview.clone();
            let colors = skin_preview.color_body.zip(skin_preview.color_feet);
            let eye = render_eye_to_eye(skin_preview.eye);
            let skin_info: NetworkSkinInfo = colors
                .map(|(body_color, feet_color)| NetworkSkinInfo::Custom {
                    body_color,
                    feet_color,
                })
                .unwrap_or_default();
            ui.label("Skin:");
            const SKIN_PREVIEW_WINDOW: &str = "account-skin-preview";
            let active = user_data
                .config
                .engine
                .ui
                .path
                .query
                .entry(SKIN_PREVIEW_WINDOW.to_string())
                .or_default();
            let res = ui.add_sized(egui::vec2(50.0, 50.0), Button::new(""));
            if res.clicked() {
                *active = "1".to_string();
            }
            let pos = res.rect.center();
            let skin_size = res.rect.height();
            let pos = vec2::new(pos.x, pos.y);
            render_tee_for_ui(
                user_data.canvas_handle,
                user_data.skin_container,
                user_data.render_tee,
                ui,
                ui_state,
                ui.ctx().screen_rect(),
                Some(ui.clip_rect()),
                &ResourceKey::from_str_lossy(skin_preview.name.as_str()),
                Some(&skin_info),
                pos,
                skin_size,
                eye_to_render_eye(eye),
            );
            if *active == "1" {
                let name = skin_preview.name.to_string();
                let body_color: ConfRgb = skin_preview
                    .color_body
                    .unwrap_or_else(|| ubvec4::new(255, 255, 255, 255))
                    .into();
                let feet_color: ConfRgb = skin_preview
                    .color_feet
                    .unwrap_or_else(|| ubvec4::new(255, 255, 255, 255))
                    .into();
                let mut window_active = true;
                Window::new("Skin preview")
                    .fixed_size((500.0, 500.0))
                    .open(&mut window_active)
                    .show(ui.ctx(), |ui| {
                        render_skin(
                            ui,
                            user_data.canvas_handle,
                            user_data.skin_container,
                            user_data.render_tee,
                            ui_state,
                            &mut user_data.config.engine,
                            || {
                                // ignore
                            },
                            &name,
                            |name| {
                                if let Ok(name) = name.as_str().try_into() {
                                    skin_preview.name = name;
                                }
                            },
                            eye,
                            |eye| {
                                skin_preview.eye = eye_to_render_eye(eye);
                            },
                            skin_info,
                            colors.is_some(),
                            body_color,
                            feet_color,
                            |custom_color, color_body, feet_color| {
                                if custom_color {
                                    skin_preview.color_body = Some(color_body.into());
                                    skin_preview.color_feet = Some(feet_color.into());
                                } else {
                                    skin_preview.color_body = None;
                                    skin_preview.color_feet = None;
                                }
                            },
                        );
                    });
                if !window_active {
                    user_data
                        .config
                        .engine
                        .ui
                        .path
                        .query
                        .remove(SKIN_PREVIEW_WINDOW);
                }
            }

            if prev_skin != skin_preview {
                profile_data.user.insert(
                    PROFILE_SKIN_PREVIEW.to_string(),
                    serde_json::to_value(&skin_preview).unwrap(),
                );
                let user = profile_data.user.clone();
                let profile_name = profile_name.clone();
                let accounts = accounts.clone();
                tasks.user_interactions.push(
                    io.rt
                        .spawn(async move {
                            accounts.set_profile_user_data(&profile_name, user).await;
                            Ok(())
                        })
                        .abortable(),
                );
            }

            ui.end_row();
        });

        ui.with_layout(
            Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
            |ui| {
                if ui.button("Logout").clicked() {
                    let profile_name = profile_name.clone();
                    let accounts = accounts.clone();
                    next_state = Some(ProfileState::Logout(
                        io.rt
                            .spawn(async move { accounts.logout(&profile_name).await })
                            .abortable(),
                    ));
                }
                if ui.button("Logout other sessions").clicked() {
                    let profile_name = profile_name.clone();
                    next_state = Some(ProfileState::LogoutAllPrepare {
                        profile_name,
                        info: info.clone(),
                    });
                }
                if ui.button("Delete account").clicked() {
                    let profile_name = profile_name.clone();
                    next_state = Some(ProfileState::DeleteConfirm {
                        profile_name,
                        info: info.clone(),
                    });
                }
                if !info
                    .credentials
                    .iter()
                    .any(|c| matches!(c, LinkedCredential::Email(_)))
                    && ui.button("Link email").clicked()
                {
                    let profile_name = profile_name.clone();
                    next_state = Some(ProfileState::LinkEmailPrepare {
                        profile_name,
                        info: info.clone(),
                    });
                }
                if accounts.supports_steam()
                    && !info
                        .credentials
                        .iter()
                        .any(|c| matches!(c, LinkedCredential::Steam(_)))
                    && ui.button("Link steam").clicked()
                {
                    let profile_name = profile_name.clone();
                    next_state = Some(ProfileState::LinkSteamPrepare {
                        profile_name,
                        info: info.clone(),
                    });
                }
            },
        );
        if let Some(next_state) = next_state {
            tasks.state = next_state;
        }
    }
}
