use std::{
    collections::HashMap, fmt::Debug, future::Future, ops::Deref, path::PathBuf, pin::Pin,
    str::FromStr, sync::Arc,
};

use anyhow::anyhow;
use async_trait::async_trait;
use base_io::{io::Io, runtime::IoRuntimeTask};
use client_ui::main_menu::profiles_interface::{
    self, CredentialAuthTokenError, LinkedCredential, ProfileData, ProfilesInterface,
};
use ddnet_account_client_http_fs::{
    client::ClientHttpTokioFs,
    fs::Fs,
    http::Http,
    profiles::{
        AccountTokenResult, Certificate, CredentialAuthTokenResult, Profiles, ProfilesLoading,
    },
};
use ddnet_accounts_shared::{
    account_server::{
        account_info::CredentialType, account_token, credential_auth_token,
        errors::AccountServerRequestError,
    },
    client::{
        account_data::AccountDataForClient, account_token::AccountTokenOperation,
        credential_auth_token::CredentialAuthTokenOperation,
    },
};
use steam::traits::SteamClient;
use url::Url;

use http_accounts::http::AccountHttp;

pub type FactoryTy = Arc<
    dyn Fn(
            PathBuf,
        )
            -> Pin<Box<dyn Future<Output = anyhow::Result<ClientHttpTokioFs>> + Sync + Send>>
        + Sync
        + Send,
>;

pub struct Factory(FactoryTy);
impl Debug for Factory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("F").finish()
    }
}

impl Deref for Factory {
    type Target = dyn Fn(
        PathBuf,
    ) -> Pin<
        Box<dyn Future<Output = anyhow::Result<ClientHttpTokioFs>> + Sync + Send>,
    >;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

pub struct Accounts {
    profiles: Profiles<ClientHttpTokioFs, Factory>,
    steam: Arc<dyn SteamClient>,
}

impl Accounts {
    pub fn new(loading: AccountsLoading, steam: Arc<dyn SteamClient>) -> Self {
        let loading = loading.task.get().unwrap();

        Self {
            profiles: Profiles::new(loading),
            steam,
        }
    }

    /// Tries to refresh the account certificate by the account server.
    /// This is only done if the cert is about to expire.
    pub async fn try_refresh_account_cert(&self) -> anyhow::Result<()> {
        self.profiles.signed_cert_and_key_pair().await;
        Ok(())
    }

    pub async fn connect_to_game_server(
        &self,
    ) -> (AccountDataForClient, Certificate, Option<anyhow::Error>) {
        self.profiles.signed_cert_and_key_pair().await
    }
}

#[async_trait]
impl ProfilesInterface for Accounts {
    fn supports_steam(&self) -> bool {
        !self.steam.is_stub()
    }

    fn steam_id64(&self) -> i64 {
        self.steam.steam_id64()
    }

    async fn credential_auth_email_token(
        &self,
        op: profiles_interface::CredentialAuthTokenOperation,
        email: email_address::EmailAddress,
        secret_token: Option<String>,
    ) -> anyhow::Result<(), CredentialAuthTokenError> {
        let op = match op {
            profiles_interface::CredentialAuthTokenOperation::Login => {
                CredentialAuthTokenOperation::Login
            }
            profiles_interface::CredentialAuthTokenOperation::LinkCredential => {
                CredentialAuthTokenOperation::LinkCredential
            }
            profiles_interface::CredentialAuthTokenOperation::UnlinkCredential => {
                CredentialAuthTokenOperation::UnlinkCredential
            }
        };
        let res = self
            .profiles
            .credential_auth_email_token(email, op, secret_token)
            .await;
        match res {
            Ok(_) => Ok(()),
            Err(err) => match err {
                CredentialAuthTokenResult::AccountServerRequstError(err) => match err {
                    AccountServerRequestError::LogicError(err) => match err {
                        credential_auth_token::CredentialAuthTokenError::WebValidationProcessNeeded { url } => {
                            Err(CredentialAuthTokenError::WebValidationProcessNeeded { url })
                        }
                    },
                    _ => Err(CredentialAuthTokenError::Other(err.into())),
                },
                _ => Err(CredentialAuthTokenError::Other(err.into())),
            },
        }
    }

    async fn credential_auth_steam_token(
        &self,
        op: profiles_interface::CredentialAuthTokenOperation,
        secret_token: Option<String>,
    ) -> anyhow::Result<String, CredentialAuthTokenError> {
        if !self.steam.is_stub() {
            let ticket = self
                .steam
                .session_ticket_for_webapi()
                .await
                .map_err(CredentialAuthTokenError::Other)?;

            match self
                .profiles
                .credential_auth_steam_token(
                    ticket,
                    match op {
                        profiles_interface::CredentialAuthTokenOperation::Login => {
                            CredentialAuthTokenOperation::Login
                        }
                        profiles_interface::CredentialAuthTokenOperation::LinkCredential => {
                            CredentialAuthTokenOperation::LinkCredential
                        }
                        profiles_interface::CredentialAuthTokenOperation::UnlinkCredential => {
                            CredentialAuthTokenOperation::UnlinkCredential
                        }
                    },
                    secret_token,
                )
                .await
            {
                Ok(token) => Ok(token),
                Err(err) => match err {
                    CredentialAuthTokenResult::AccountServerRequstError(err) => match err {
                        AccountServerRequestError::LogicError(err) => {
                            use credential_auth_token::CredentialAuthTokenError as AccCredentialAuthTokenError;
                            match err {
                                AccCredentialAuthTokenError::WebValidationProcessNeeded { url } => {
                                    Err(CredentialAuthTokenError::WebValidationProcessNeeded {
                                        url,
                                    })
                                }
                            }
                        }
                        _ => Err(CredentialAuthTokenError::Other(err.into())),
                    },
                    _ => Err(CredentialAuthTokenError::Other(err.into())),
                },
            }
        } else {
            Err(CredentialAuthTokenError::Other(anyhow!(
                "Steam was not initialized."
            )))
        }
    }

    async fn account_email_token(
        &self,
        op: profiles_interface::AccountTokenOperation,
        email: email_address::EmailAddress,
        secret_token: Option<String>,
    ) -> anyhow::Result<(), profiles_interface::AccountTokenError> {
        let op = match op {
            profiles_interface::AccountTokenOperation::LogoutAll => {
                AccountTokenOperation::LogoutAll
            }
            profiles_interface::AccountTokenOperation::LinkCredential => {
                AccountTokenOperation::LinkCredential
            }
            profiles_interface::AccountTokenOperation::Delete => AccountTokenOperation::Delete,
        };
        let res = self
            .profiles
            .account_email_token(email, op, secret_token)
            .await;
        match res {
            Ok(_) => Ok(()),
            Err(err) => match err {
                AccountTokenResult::AccountServerRequstError(err) => match err {
                    AccountServerRequestError::LogicError(err) => match err {
                        account_token::AccountTokenError::WebValidationProcessNeeded { url } => {
                            Err(
                                profiles_interface::AccountTokenError::WebValidationProcessNeeded {
                                    url,
                                },
                            )
                        }
                    },
                    _ => Err(profiles_interface::AccountTokenError::Other(err.into())),
                },
                _ => Err(profiles_interface::AccountTokenError::Other(err.into())),
            },
        }
    }

    async fn account_steam_token(
        &self,
        op: profiles_interface::AccountTokenOperation,
        secret_token: Option<String>,
    ) -> anyhow::Result<String, profiles_interface::AccountTokenError> {
        if !self.steam.is_stub() {
            let ticket = self
                .steam
                .session_ticket_for_webapi()
                .await
                .map_err(profiles_interface::AccountTokenError::Other)?;

            match self
                .profiles
                .account_steam_token(
                    ticket,
                    match op {
                        profiles_interface::AccountTokenOperation::LogoutAll => {
                            AccountTokenOperation::LogoutAll
                        }
                        profiles_interface::AccountTokenOperation::LinkCredential => {
                            AccountTokenOperation::LinkCredential
                        }
                        profiles_interface::AccountTokenOperation::Delete => {
                            AccountTokenOperation::Delete
                        }
                    },
                    secret_token,
                )
                .await
            {
                Ok(token) => Ok(token),
                Err(err) => match err {
                    AccountTokenResult::AccountServerRequstError(err) => match err {
                        AccountServerRequestError::LogicError(err) => match err {
                            account_token::AccountTokenError::WebValidationProcessNeeded {
                                url,
                            } => Err(
                                profiles_interface::AccountTokenError::WebValidationProcessNeeded {
                                    url,
                                },
                            ),
                        },
                        _ => Err(profiles_interface::AccountTokenError::Other(err.into())),
                    },
                    _ => Err(profiles_interface::AccountTokenError::Other(err.into())),
                },
            }
        } else {
            Err(profiles_interface::AccountTokenError::Other(anyhow!(
                "Steam was not initialized."
            )))
        }
    }

    async fn login_email(
        &self,
        email: email_address::EmailAddress,
        token_hex: String,
    ) -> anyhow::Result<String> {
        self.profiles.login_email(email, token_hex).await
    }

    async fn login_steam(&self, token_hex: String) -> anyhow::Result<String> {
        self.profiles
            .login_steam(self.steam.steam_user_name(), token_hex)
            .await
    }

    async fn link_credential(
        &self,
        account_token_hex: String,
        credential_auth_token_hex: String,
        name: &str,
    ) -> anyhow::Result<()> {
        self.profiles
            .link_credential(account_token_hex, credential_auth_token_hex, name)
            .await
    }
    async fn unlink_credential(
        &self,
        credential_auth_token_hex: String,
        name: &str,
    ) -> anyhow::Result<()> {
        self.profiles
            .unlink_credential(credential_auth_token_hex, name)
            .await
    }

    async fn logout(&self, name: &str) -> anyhow::Result<()> {
        self.profiles.logout(name).await
    }

    async fn logout_all(&self, account_token_hex: String, name: &str) -> anyhow::Result<()> {
        self.profiles.logout_all(account_token_hex, name).await
    }

    async fn delete(&self, account_token_hex: String, name: &str) -> anyhow::Result<()> {
        self.profiles.delete(account_token_hex, name).await
    }

    async fn user_interaction(&self) -> anyhow::Result<()> {
        self.try_refresh_account_cert().await
    }

    async fn account_info(&self, name: &str) -> anyhow::Result<profiles_interface::AccountInfo> {
        self.profiles
            .account_info(name)
            .await
            .map(|account_info| profiles_interface::AccountInfo {
                account_id: account_info.account_id,
                creation_date: <chrono::DateTime<chrono::Local>>::from(account_info.creation_date)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
                credentials: account_info
                    .credentials
                    .into_iter()
                    .map(|c| match c {
                        CredentialType::Email(mail) => LinkedCredential::Email(mail),
                        CredentialType::Steam(id) => LinkedCredential::Steam(id),
                    })
                    .collect(),
            })
    }

    fn profiles(&self) -> (HashMap<String, ProfileData>, String) {
        let (profiles, cur_profile) = self.profiles.profiles();
        (
            profiles
                .into_iter()
                .map(|(key, val)| {
                    (
                        key,
                        ProfileData {
                            name: val.name,
                            user: val.user,
                        },
                    )
                })
                .collect(),
            cur_profile,
        )
    }

    fn cur_profile(&self) -> Option<ProfileData> {
        self.profiles.current_profile().map(|val| ProfileData {
            name: val.name,
            user: val.user,
        })
    }

    async fn set_profile(&self, name: &str) {
        self.profiles.set_profile(name).await
    }

    async fn set_profile_display_name(&self, profile_name: &str, display_name: String) {
        self.profiles
            .set_profile_display_name(profile_name, display_name)
            .await
    }

    async fn set_profile_user_data(
        &self,
        profile_name: &str,
        user: HashMap<String, serde_json::Value>,
    ) {
        self.profiles
            .set_profile_user_data(profile_name, user)
            .await
    }
}

pub struct AccountsLoading {
    task: IoRuntimeTask<ProfilesLoading<ClientHttpTokioFs, Factory>>,
}

impl AccountsLoading {
    pub fn new(io: &Io) -> Self {
        let secure_base_path = io.fs.get_secure_path();
        let http = io.http.clone();
        let secure_base_path_factory = secure_base_path.clone();
        let factory = Arc::new(Factory(Arc::new(move |path| {
            let http = http.clone();
            let secure_base_path_factory = secure_base_path_factory.clone();
            Box::pin(async move {
                let http: Vec<Arc<dyn Http>> = vec![Arc::new(AccountHttp::new_with_url(
                    Url::from_str("https://pg.ddnet.org:5555/").unwrap(),
                    http,
                ))];
                let fastest_http_fs = Fs::new(secure_base_path_factory).await?;
                let fs = Fs::new(path).await?;
                Ok(ClientHttpTokioFs {
                    cur_http: ClientHttpTokioFs::get_fastest_http(&fastest_http_fs, &http)
                        .await
                        .into(),
                    http,
                    fs,
                })
            })
        })));
        Self {
            task: io
                .rt
                .spawn(async move { ProfilesLoading::new(secure_base_path, factory).await }),
        }
    }
}
