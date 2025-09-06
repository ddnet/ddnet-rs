use std::{str::FromStr, sync::Arc};

use base_io::io::Io;
use config::config::ConfigPath;

use crate::main_menu::{
    profiles_interface::{AccountTokenOperation, ProfilesInterface},
    user_data::{AccountOperation, ProfileState, ProfileTasks},
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
    if let ProfileState::EmailAccountTokenWebValidation { op, url } = &tasks.state {
        let op = op.clone();
        let url = url.clone();
        back_bar(
            ui,
            match &op {
                AccountOperation::LogoutAll { .. } => "Logout all by email",
                AccountOperation::LinkCredential { .. } => "Verify new link by email",
                AccountOperation::Delete { .. } => "Delete account by email",
            },
            tasks,
        );
        ui.vertical_centered(|ui| {
            egui::Grid::new("email-token")
                .spacing([2.0, 4.0])
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Account's Email:");

                    let email = path.query.entry("email".into()).or_default();
                    ui.text_edit_singleline(email);
                    ui.end_row();
                });
            ui.label("A verification on this web page is needed:");
            ui.hyperlink(url);
            ui.label("Afterwards add the code from\nthe web page to this field:");
            egui::Grid::new("email-token-secret-key")
                .spacing([2.0, 4.0])
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Token:");
                    let veri_token = path.query.entry("veri-token".into()).or_default();
                    ui.text_edit_singleline(veri_token);
                    ui.end_row();
                });

            if ui.button("\u{f2f6} Request code by email").clicked()
                && let (Some(email), veri_token) = (
                    path.query
                        .get("email")
                        .and_then(|email| email_address::EmailAddress::from_str(email).ok()),
                    path.query.get("veri-token"),
                )
            {
                let accounts = accounts.clone();
                let veri_token = veri_token.cloned();
                tasks.state = ProfileState::EmailAccountToken {
                    op: op.clone(),
                    task: io
                        .rt
                        .spawn(async move {
                            Ok(accounts
                                .account_email_token(
                                    match op {
                                        AccountOperation::LogoutAll { .. } => {
                                            AccountTokenOperation::LogoutAll
                                        }
                                        AccountOperation::LinkCredential { .. } => {
                                            AccountTokenOperation::LinkCredential
                                        }
                                        AccountOperation::Delete { .. } => {
                                            AccountTokenOperation::Delete
                                        }
                                    },
                                    email,
                                    veri_token,
                                )
                                .await)
                        })
                        .abortable(),
                };
            }
        });
    }
}
