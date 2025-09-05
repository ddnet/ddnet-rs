use std::{str::FromStr, sync::Arc};

use base_io::io::Io;
use config::config::ConfigPath;
use egui::Label;

use crate::main_menu::{
    profiles_interface::ProfilesInterface,
    user_data::{
        AccountCredential, AccountOperation, CredentialAuthOperation, ProfileState, ProfileTasks,
    },
};

use super::back_bar::back_bar;

/// overview
pub fn render(
    ui: &mut egui::Ui,
    accounts: &Arc<dyn ProfilesInterface>,
    tasks: &mut ProfileTasks,
    io: &Io,
    path: &mut ConfigPath,
) {
    if let ProfileState::EmailCredentialAuthTokenObtained(op) = &tasks.state {
        let op = op.clone();
        back_bar(
            ui,
            match &op {
                CredentialAuthOperation::Login => "Login by email",
                CredentialAuthOperation::LinkCredential { .. } => "Link new email",
                CredentialAuthOperation::UnlinkCredential { .. } => "Unlink email",
            },
            tasks,
        );

        ui.vertical_centered(|ui| {
            if path
                .query
                .get("email")
                .and_then(|email| email_address::EmailAddress::from_str(email).ok())
                .is_some()
            {
                egui::Grid::new("login-by-email")
                    .spacing([2.0, 4.0])
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Email:");

                        let email = path.query.entry("email".into()).or_default();
                        ui.add_enabled(false, Label::new(email.as_str()));
                        ui.end_row();

                        ui.label("Code:");

                        let token = path.query.entry("token".into()).or_default();
                        ui.text_edit_singleline(token);
                        ui.end_row();
                    });

                if ui
                    .button(format!(
                        "\u{f2f6} {}",
                        match &op {
                            CredentialAuthOperation::Login => "Login",
                            CredentialAuthOperation::LinkCredential { .. } => "Link this email",
                            CredentialAuthOperation::UnlinkCredential { .. } => "Unlink this email",
                        },
                    ))
                    .clicked()
                {
                    if let Some((email, token)) = path
                        .query
                        .get("email")
                        .and_then(|email| email_address::EmailAddress::from_str(email).ok())
                        .zip(path.query.get("token").cloned())
                    {
                        let token = token.trim().into();
                        path.query.remove("token");
                        path.query.remove("email");
                        path.query.remove("veri-token");
                        let accounts = accounts.clone();
                        match op {
                            CredentialAuthOperation::Login => {
                                tasks.state = ProfileState::EmailLoggingIn(
                                    io.rt
                                        .spawn(
                                            async move { accounts.login_email(email, token).await },
                                        )
                                        .abortable(),
                                );
                            }
                            CredentialAuthOperation::LinkCredential {
                                profile_name,
                                account_credential,
                            } => match account_credential {
                                AccountCredential::Email => {
                                    tasks.state = ProfileState::EmailAccountTokenPrepare(
                                        AccountOperation::LinkCredential {
                                            profile_name,
                                            credential_auth_token: token,
                                        },
                                    );
                                }
                                AccountCredential::Steam => {
                                    tasks.state = ProfileState::SteamAccountTokenPrepare(
                                        AccountOperation::LinkCredential {
                                            profile_name,
                                            credential_auth_token: token,
                                        },
                                    );
                                }
                            },
                            CredentialAuthOperation::UnlinkCredential { profile_name } => {
                                tasks.state = ProfileState::EmailUnlinkCredential(
                                    io.rt
                                        .spawn(async move {
                                            accounts.unlink_credential(token, &profile_name).await
                                        })
                                        .abortable(),
                                );
                            }
                        }
                    }
                }
            }
        });
    }
}
