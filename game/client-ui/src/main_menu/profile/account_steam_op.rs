use std::sync::Arc;

use base_io::io::Io;
use config::config::ConfigPath;

use crate::main_menu::{
    profiles_interface::ProfilesInterface,
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
    if let ProfileState::SteamAccountTokenObtained { op, token } = &tasks.state {
        let op = op.clone();
        let token = token.clone().trim().into();
        back_bar(
            ui,
            match &op {
                AccountOperation::LogoutAll { .. } => "Logout all by steam",
                AccountOperation::LinkCredential { .. } => "Verify new link by steam",
                AccountOperation::Delete { .. } => "Delete account by steam",
            },
            tasks,
        );

        let accounts = accounts.clone();

        path.query.remove("token");
        path.query.remove("email");
        path.query.remove("veri-token");
        match op {
            AccountOperation::LogoutAll { profile_name } => {
                tasks.state = ProfileState::SteamLogoutAll(
                    io.rt
                        .spawn(async move { accounts.logout_all(token, &profile_name).await })
                        .abortable(),
                );
            }
            AccountOperation::LinkCredential {
                credential_auth_token,
                profile_name,
            } => {
                tasks.state = ProfileState::SteamLinkCredential(
                    io.rt
                        .spawn(async move {
                            accounts
                                .link_credential(token, credential_auth_token, &profile_name)
                                .await
                        })
                        .abortable(),
                );
            }
            AccountOperation::Delete { profile_name } => {
                tasks.state = ProfileState::SteamDelete(
                    io.rt
                        .spawn(async move { accounts.delete(token, &profile_name).await })
                        .abortable(),
                );
            }
        }
    }
}
