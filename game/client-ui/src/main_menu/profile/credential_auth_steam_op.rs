use std::sync::Arc;

use base_io::io::Io;
use config::config::ConfigPath;

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
    if let ProfileState::SteamCredentialAuthTokenObtained { op, token } = &tasks.state {
        let op = op.clone();
        let token = token.clone().trim().into();
        back_bar(
            ui,
            match &op {
                CredentialAuthOperation::Login => "Login by steam",
                CredentialAuthOperation::LinkCredential { .. } => "Link new steam",
                CredentialAuthOperation::UnlinkCredential { .. } => "Unlink steam",
            },
            tasks,
        );

        let accounts = accounts.clone();

        path.query.remove("token");
        path.query.remove("email");
        path.query.remove("veri-token");
        match op {
            CredentialAuthOperation::Login => {
                tasks.state = ProfileState::SteamLoggingIn(
                    io.rt
                        .spawn(async move { accounts.login_steam(token).await })
                        .abortable(),
                );
            }
            CredentialAuthOperation::LinkCredential {
                profile_name,
                account_credential,
            } => match account_credential {
                AccountCredential::Email => {
                    tasks.state =
                        ProfileState::EmailAccountTokenPrepare(AccountOperation::LinkCredential {
                            profile_name,
                            credential_auth_token: token,
                        });
                }
                AccountCredential::Steam => {
                    tasks.state =
                        ProfileState::SteamAccountTokenPrepare(AccountOperation::LinkCredential {
                            profile_name,
                            credential_auth_token: token,
                        });
                }
            },
            CredentialAuthOperation::UnlinkCredential { profile_name } => {
                tasks.state = ProfileState::SteamUnlinkCredential(
                    io.rt
                        .spawn(
                            async move { accounts.unlink_credential(token, &profile_name).await },
                        )
                        .abortable(),
                );
            }
        }
    }
}
