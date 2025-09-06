use std::{
    sync::{
        mpsc::{channel, Sender},
        Arc,
    },
    time::Duration,
};

use anyhow::anyhow;
use async_trait::async_trait;
use base::join_thread::JoinThread;
use steamworks::{Client, TicketForWebApiResponse};
use tokio::sync::Mutex;

use crate::traits::{SteamClient, SteamRaii};

/// The single threaded steam implementation.
///
/// This is a RAII object, it should be kept alive
/// over the runtime of the app.
pub struct SteamSt {
    // RAII objects
    _client_sender: Sender<()>,
    _client_thread: JoinThread<()>,
}

impl SteamSt {
    pub fn new(steam: Client, steam_mutex: Arc<Mutex<()>>) -> Self {
        let steam_mutex_thread = steam_mutex.clone();
        let (client_sender, client_recv) = channel();
        let client_thread = std::thread::Builder::new()
            .name("steam-loop".to_string())
            .spawn(move || loop {
                let g = steam_mutex_thread.blocking_lock();
                steam.run_callbacks();
                drop(g);
                if client_recv
                    .recv_timeout(Duration::from_millis(150))
                    .is_err_and(|err| {
                        matches!(err, std::sync::mpsc::RecvTimeoutError::Disconnected)
                    })
                {
                    break;
                }
            })
            .unwrap();

        Self {
            _client_sender: client_sender,
            _client_thread: JoinThread::new(client_thread),
        }
    }
}

impl SteamRaii for SteamSt {}

/// The multi threaded steam part.
///
/// This can be used to call steam specific things
pub struct SteamMt {
    steam: steamworks::Client,
    steam_mutex: Arc<Mutex<()>>,
}

impl SteamMt {
    pub fn new(steam: steamworks::Client, steam_mutex: Arc<Mutex<()>>) -> Self {
        Self { steam, steam_mutex }
    }
}

#[async_trait]
impl SteamClient for SteamMt {
    fn is_stub(&self) -> bool {
        false
    }

    fn steam_id64(&self) -> i64 {
        self.steam.user().steam_id().raw() as i64
    }

    fn steam_user_name(&self) -> String {
        self.steam.friends().name()
    }

    async fn session_ticket_for_webapi(&self) -> anyhow::Result<Vec<u8>> {
        let notifier = Arc::new(tokio::sync::Notify::new());
        let notifier_cb = notifier.clone();

        let g = self.steam_mutex.lock().await;
        let ticket = {
            let user = self.steam.user();
            user.authentication_session_ticket_for_webapi("account")
        };
        let token = Arc::new(Mutex::new(None));

        let token_cb = token.clone();
        let cb = self
            .steam
            .register_callback(move |mut e: TicketForWebApiResponse| {
                if e.ticket_handle == ticket {
                    e.ticket.truncate(e.ticket_len as usize);
                    *token_cb.blocking_lock() = Some(e.ticket);
                    notifier_cb.notify_one();
                }
            });
        drop(g);

        notifier.notified().await;
        drop(cb);

        let token = token
            .lock()
            .await
            .take()
            .ok_or_else(|| anyhow!("Steam session token was invalid."))?;

        Ok(token)
    }
}
