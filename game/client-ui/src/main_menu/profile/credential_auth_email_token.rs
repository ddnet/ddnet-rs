use std::sync::Arc;

use base_io::io::Io;
use config::config::ConfigPath;

use crate::main_menu::{
    profiles_interface::{CredentialAuthTokenOperation, ProfilesInterface},
    user_data::{CredentialAuthOperation, ProfileState, ProfileTasks},
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
    if let ProfileState::EmailCredentialAuthTokenPrepare(op) = &tasks.state {
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
            egui::Grid::new("login-email-token")
                .spacing([2.0, 4.0])
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Email:");

                    let email = path.query.entry("email".into()).or_default();
                    ui.text_edit_singleline(email);
                    ui.end_row();
                });

            if ui.button("\u{f2f6} Request code by email").clicked()
                && let Some(email) = path.query.get("email").and_then(|email| {
                    email_address::EmailAddress::parse_with_options(
                        email,
                        email_address::Options::default()
                            .without_display_text()
                            .without_domain_literal()
                            .with_required_tld(),
                    )
                    .ok()
                })
            {
                let accounts = accounts.clone();
                tasks.state = ProfileState::EmailCredentialAuthToken {
                    op: op.clone(),
                    task: io
                        .rt
                        .spawn(async move {
                            Ok(accounts
                                .credential_auth_email_token(
                                    match op {
                                        CredentialAuthOperation::Login => {
                                            CredentialAuthTokenOperation::Login
                                        }
                                        CredentialAuthOperation::LinkCredential { .. } => {
                                            CredentialAuthTokenOperation::LinkCredential
                                        }
                                        CredentialAuthOperation::UnlinkCredential { .. } => {
                                            CredentialAuthTokenOperation::UnlinkCredential
                                        }
                                    },
                                    email,
                                    None,
                                )
                                .await)
                        })
                        .abortable(),
                };
            }
        });
    }
}
