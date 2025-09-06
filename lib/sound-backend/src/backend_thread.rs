use std::sync::{
    Arc,
    mpsc::{Receiver, Sender},
};

use anyhow::anyhow;
use base::join_thread::JoinThread;
use config::config::{ConfigSound, ConfigSoundLimits};
use hiarc::Hiarc;

use crate::backend::{
    kira::kira::{SoundBackendKira, SoundBackendMtKira},
    null::null::SoundBackendNull,
};
use sound::{
    backend_types::{SoundBackendDriverInterface, SoundBackendMtDriverInterface},
    commands::SoundCommand,
    frame_fetcher_plugin::BackendFrameFetcher,
};

#[derive(Debug, Hiarc)]
enum SoundBackendType {
    Kira(Box<SoundBackendKira>),
    Null(SoundBackendNull),
}

impl SoundBackendType {
    pub fn as_mut(&mut self) -> &mut dyn SoundBackendDriverInterface {
        match self {
            SoundBackendType::Kira(backend) => backend.as_mut(),
            SoundBackendType::Null(backend) => backend,
        }
    }
}

#[derive(Debug, Hiarc)]
pub(crate) enum SoundBackendMtType {
    Kira(SoundBackendMtKira),
    Null(SoundBackendNull),
}

impl SoundBackendMtType {
    pub(crate) fn as_ref(&self) -> &dyn SoundBackendMtDriverInterface {
        match self {
            SoundBackendMtType::Kira(backend) => backend,
            SoundBackendMtType::Null(backend) => backend,
        }
    }
}

#[derive(Debug, Hiarc)]
pub enum BackendThreadBackendEvent {
    Init {
        backend: String,
        limits: ConfigSoundLimits,
    },
    RunCmds {
        cmds: Vec<SoundCommand>,
    },
    AttachFrameFetcher {
        name: String,
        #[hiarc_skip_unsafe]
        fetcher: Arc<dyn BackendFrameFetcher>,
    },
    DetachFrameFetcher {
        name: String,
    },
}

#[derive(Debug, Hiarc)]
enum BackendThreadFrontendEvent {
    InitRes {
        backend_mt: SoundBackendMtType,
    },
    /// send every time commands were handled. Useful to make sure the backend doesn't get overloaded
    Sync,
}

#[derive(Debug, Hiarc)]
pub struct SoundBackendThread {
    events: Sender<BackendThreadBackendEvent>,
    recv_events: Receiver<BackendThreadFrontendEvent>,

    // custom drop, must stay last element
    _thread: JoinThread<anyhow::Result<()>>,
}

impl SoundBackendThread {
    pub(crate) fn new(config: &ConfigSound) -> anyhow::Result<(Self, SoundBackendMtType)> {
        let (events, recv) = std::sync::mpsc::channel();
        let (sender, frontent_events) = std::sync::mpsc::channel();

        events.send(BackendThreadBackendEvent::Init {
            backend: config.backend.clone(),
            limits: config.limits,
        })?;

        let thread = std::thread::Builder::new()
            .name("sound-backend-thread".into())
            .spawn(move || match SoundBackendThread::run(recv, sender) {
                Ok(_) => Ok(()),
                Err(err) => {
                    log::info!("sounds backend thread exited: {err} {}", err.backtrace());
                    Err(err)
                }
            })?;

        let BackendThreadFrontendEvent::InitRes { backend_mt } = frontent_events.recv()? else {
            return Err(anyhow!("missing init event response"));
        };

        Ok((
            Self {
                events,
                recv_events: frontent_events,
                _thread: JoinThread::new(thread),
            },
            backend_mt,
        ))
    }

    pub fn run_cmds(&self, cmds: Vec<SoundCommand>) -> anyhow::Result<()> {
        let BackendThreadFrontendEvent::Sync = self.recv_events.recv()? else {
            return Err(anyhow!("Sync event not found"));
        };

        self.events
            .send(BackendThreadBackendEvent::RunCmds { cmds })?;

        Ok(())
    }

    pub fn attach_frame_fetcher(
        &self,
        name: String,
        fetcher: Arc<dyn BackendFrameFetcher>,
    ) -> anyhow::Result<()> {
        self.events
            .send(BackendThreadBackendEvent::AttachFrameFetcher { name, fetcher })?;
        Ok(())
    }

    pub fn detach_frame_fetcher(&self, name: String) -> anyhow::Result<()> {
        self.events
            .send(BackendThreadBackendEvent::DetachFrameFetcher { name })?;
        Ok(())
    }

    fn run(
        events: Receiver<BackendThreadBackendEvent>,
        sender: Sender<BackendThreadFrontendEvent>,
    ) -> anyhow::Result<()> {
        // handle loading
        let load_ev = events.recv()?;
        let BackendThreadBackendEvent::Init { backend, limits } = load_ev else {
            return Err(anyhow!("first event is always the load event"));
        };
        let mut backend = match backend.to_ascii_lowercase().as_str() {
            "null" => SoundBackendType::Null(SoundBackendNull {}),
            // "kira"
            _ => match SoundBackendKira::new(limits) {
                Ok(backend) => SoundBackendType::Kira(backend),
                _ => SoundBackendType::Null(SoundBackendNull {}),
            },
        };
        sender.send(BackendThreadFrontendEvent::InitRes {
            backend_mt: match &backend {
                SoundBackendType::Kira(backend) => {
                    SoundBackendMtType::Kira(backend.get_backend_mt())
                }
                SoundBackendType::Null(_) => SoundBackendMtType::Null(SoundBackendNull {}),
            },
        })?;
        sender.send(BackendThreadFrontendEvent::Sync)?;

        while let Ok(event) = events.recv() {
            match event {
                BackendThreadBackendEvent::Init { .. } => {
                    panic!("backend was already initialized")
                }
                BackendThreadBackendEvent::RunCmds { cmds } => {
                    backend.as_mut().run_cmds(cmds)?;
                    sender.send(BackendThreadFrontendEvent::Sync)?;
                }
                BackendThreadBackendEvent::AttachFrameFetcher { name, fetcher } => {
                    backend.as_mut().attach_frame_fetcher(name, fetcher)
                }
                BackendThreadBackendEvent::DetachFrameFetcher { name } => {
                    backend.as_mut().detach_frame_fetcher(name)
                }
            }
        }

        Ok(())
    }
}
