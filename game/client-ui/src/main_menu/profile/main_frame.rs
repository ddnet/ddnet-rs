use egui::Frame;
use egui_extras::{Size, StripBuilder};

use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
    utils::add_margins,
};

use crate::main_menu::{
    constants::MENU_PROFILE_NAME,
    user_data::{ProfileState, UserData},
};

pub fn render_profile(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    is_ingame_ui: bool,
) {
    let path = &mut pipe.user_data.config.engine.ui.path;
    let tasks = &mut *pipe.user_data.profile_tasks;
    let profiles = pipe.user_data.profiles;
    let io = pipe.user_data.io;
    match &tasks.state {
        ProfileState::Overview => {
            super::overview::render(ui, profiles, tasks, io, is_ingame_ui);
        }
        ProfileState::EmailCredentialAuthTokenPrepare(_) => {
            super::credential_auth_email_token::render(ui, profiles, tasks, io, path);
        }
        ProfileState::EmailCredentialAuthTokenObtained(_) => {
            super::credential_auth_email_op::render(ui, profiles, tasks, io, path);
        }
        ProfileState::EmailCredentialAuthTokenWebValidation { .. } => {
            super::credential_auth_email_token_web_veri::render(ui, profiles, tasks, io, path);
        }
        ProfileState::EmailLoggingIn(_)
        | ProfileState::EmailLinkCredential(_)
        | ProfileState::EmailUnlinkCredential(_)
        | ProfileState::EmailLogoutAll(_)
        | ProfileState::EmailDelete(_)
        | ProfileState::EmailAccountToken { .. }
        | ProfileState::EmailCredentialAuthToken { .. } => {
            super::email_loading::render(ui, tasks);
        }
        ProfileState::SteamCredentialAuthTokenPrepare(_) => {
            super::credential_auth_steam_token::render(ui, profiles, tasks, io);
        }
        ProfileState::SteamCredentialAuthTokenObtained { .. } => {
            super::credential_auth_steam_op::render(ui, profiles, tasks, io, path);
        }
        ProfileState::SteamCredentialAuthTokenWebValidation { .. } => {
            super::credential_auth_steam_token_web_veri::render(ui, profiles, tasks, io, path);
        }
        ProfileState::SteamLoggingIn(_)
        | ProfileState::SteamLinkCredential(_)
        | ProfileState::SteamUnlinkCredential(_)
        | ProfileState::SteamLogoutAll(_)
        | ProfileState::SteamDelete(_)
        | ProfileState::SteamAccountToken { .. }
        | ProfileState::SteamCredentialAuthToken { .. } => {
            super::steam_loading::render(ui, tasks);
        }
        ProfileState::AccountInfoFetch { .. } => {
            super::account_info_loading::render(ui, tasks);
        }
        ProfileState::AccountInfo { .. } => {
            super::account_info::render(ui, pipe, ui_state, profiles, io);
        }
        ProfileState::Logout(_) => {
            super::logout_loading::render(ui, tasks);
        }
        ProfileState::LogoutAllPrepare { .. } => {
            super::logout_all_prepare::render(ui, profiles, tasks);
        }
        ProfileState::DeleteConfirm { .. } => {
            super::delete_confirm::render(ui, tasks);
        }
        ProfileState::DeletePrepare { .. } => {
            super::delete_prepare::render(ui, profiles, tasks);
        }
        ProfileState::LinkEmailPrepare { .. } => {
            super::link_email_prepare::render(ui, profiles, tasks);
        }
        ProfileState::LinkSteamPrepare { .. } => {
            super::link_steam_prepare::render(ui, profiles, tasks);
        }
        ProfileState::UnlinkEmailPrepare { .. } => {
            super::unlink_email_prepare::render(ui, tasks);
        }
        ProfileState::UnlinkSteamPrepare { .. } => {
            super::unlink_steam_prepare::render(ui, tasks);
        }
        ProfileState::Err(_) => {
            super::general_error::render(ui, tasks);
        }
        ProfileState::EmailAccountTokenPrepare(_) => {
            super::account_email_token::render(ui, profiles, tasks, io, path);
        }
        ProfileState::EmailAccountTokenObtained(_) => {
            super::account_email_op::render(ui, profiles, tasks, io, path);
        }
        ProfileState::EmailAccountTokenWebValidation { .. } => {
            super::account_email_token_web_veri::render(ui, profiles, tasks, io, path);
        }
        ProfileState::SteamAccountTokenPrepare(_) => {
            super::account_steam_token::render(ui, profiles, tasks, io);
        }
        ProfileState::SteamAccountTokenObtained { .. } => {
            super::account_steam_op::render(ui, profiles, tasks, io, path);
        }
        ProfileState::SteamAccountTokenWebValidation { .. } => {
            super::account_steam_token_web_veri::render(ui, profiles, tasks, io, path);
        }
    }
}

/// big box, rounded edges
pub fn render(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    cur_page: &str,
    is_ingame_ui: bool,
) {
    let tasks = &mut *pipe.user_data.profile_tasks;
    tasks.update(
        pipe.user_data.io,
        pipe.user_data.profiles,
        &pipe.user_data.config.game,
    );
    if cur_page == MENU_PROFILE_NAME {
        StripBuilder::new(ui)
            .size(Size::remainder())
            .size(Size::exact(
                if matches!(tasks.state, ProfileState::Overview) {
                    550.0
                } else {
                    400.0
                },
            ))
            .size(Size::remainder())
            .horizontal(|mut strip| {
                strip.empty();
                strip.cell(|ui| {
                    ui.style_mut().wrap_mode = None;
                    StripBuilder::new(ui)
                        .size(Size::remainder())
                        .size(Size::exact(200.0))
                        .size(Size::remainder())
                        .clip(true)
                        .vertical(|mut strip| {
                            strip.empty();
                            strip.cell(|ui| {
                                ui.style_mut().wrap_mode = None;
                                let res = Frame::default()
                                    .fill(bg_frame_color())
                                    .corner_radius(5.0)
                                    .show(ui, |ui| {
                                        add_margins(ui, |ui| {
                                            render_profile(ui, pipe, ui_state, is_ingame_ui);
                                        });
                                    });
                                ui_state.add_blur_rect(res.response.rect, 5.0);
                            });
                            strip.empty();
                        });
                });
                strip.empty();
            });
    }
}
