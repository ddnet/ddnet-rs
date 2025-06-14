use std::collections::HashMap;

use base_io::io::Io;
use game_interface::{
    rcon_entries::AuthLevel,
    types::player_info::{AccountId, PlayerUniqueId},
};
use rand::Rng;

use crate::client::ServerClient;

/// Everything the server needs for rcon
#[derive(Debug)]
pub struct Rcon {
    pub auths: HashMap<AccountId, AuthLevel>,
    /// gives full access, mostly interesting for internal servers
    pub rcon_secret: [u8; 32],
}

impl Rcon {
    pub fn new(io: &Io) -> Self {
        let fs = io.fs.clone();

        let auths = io
            .rt
            .spawn(async move {
                let file = fs.read_file("auth.json".as_ref()).await?;
                Ok(serde_json::from_slice::<HashMap<AccountId, AuthLevel>>(
                    &file,
                )?)
            })
            .get()
            .unwrap_or_default();

        let mut rcon_secret: [u8; 32] = Default::default();
        rand::rng().fill(&mut rcon_secret);
        Rcon { auths, rcon_secret }
    }

    pub fn try_rcon_auth(
        &self,
        client: &mut ServerClient,
        rcon_secret: Option<&[u8; 32]>,
        unique_identifier: &PlayerUniqueId,
    ) -> bool {
        if let Some(auth) =
            unique_identifier.is_account_then(|account_id| self.auths.get(&account_id))
        {
            client.auth.level = *auth;
            true
        } else if rcon_secret.is_some_and(|rcon_secret| self.rcon_secret.eq(rcon_secret)) {
            client.auth.level = AuthLevel::Admin;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ServerRconCommand {
    BanId,
    KickId,
    Status,
    ConfVariable,
    /// Executes a command line like file
    /// and parses it in the command chain.
    Exec,
    /// Loads server config from a specific path
    Load,
    AddMiscVote,
    RemoveMiscVote,
    RecordDemo,
}
