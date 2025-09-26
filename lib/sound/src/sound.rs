use std::rc::Rc;

use tracing::instrument;

use crate::{
    backend_handle::SoundBackendHandle,
    backend_types::SoundBackendInterface,
    commands::{SoundCommand, SoundCommandState},
    scene_handle::SoundSceneHandle,
    sound_mt::SoundMultiThreaded,
};

#[derive(Debug, Clone)]
pub struct SoundManager {
    pub backend_handle: SoundBackendHandle,
    pub scene_handle: SoundSceneHandle,
}

impl SoundManager {
    pub fn new(backend: Rc<dyn SoundBackendInterface>) -> anyhow::Result<Self> {
        let backend_handle = SoundBackendHandle::new(backend);

        Ok(Self {
            scene_handle: SoundSceneHandle::new(backend_handle.clone()),
            backend_handle,
        })
    }

    #[instrument(level = "trace", skip_all)]
    pub fn swap(&self) {
        self.backend_handle
            .add_cmd(SoundCommand::State(SoundCommandState::Swap));
        self.backend_handle.run_cmds();
    }

    pub fn get_sound_mt(&self) -> SoundMultiThreaded {
        SoundMultiThreaded(self.backend_handle.get_sound_mt())
    }
}
