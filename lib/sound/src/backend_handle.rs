use std::{rc::Rc, sync::Arc};

use hiarc::{Hiarc, hiarc_safer_rc_refcell};

use crate::{
    backend_types::{SoundBackendInterface, SoundManagerMtInterface},
    commands::SoundCommand,
};

/// wrapper around the actual backend
#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc)]
pub struct SoundBackendHandle {
    #[hiarc_skip_unsafe]
    backend: Rc<dyn SoundBackendInterface>,

    cmds: Vec<SoundCommand>,
}

#[hiarc_safer_rc_refcell]
impl SoundBackendHandle {
    pub fn new(backend: Rc<dyn SoundBackendInterface>) -> Self {
        Self {
            backend,
            cmds: Vec::new(),
        }
    }

    pub fn add_cmd(&mut self, cmd: SoundCommand) {
        self.cmds.push(cmd);
    }

    pub fn add_cmds(&mut self, cmds: &mut Vec<SoundCommand>) {
        self.cmds.append(cmds);
    }

    pub fn run_cmds(&mut self) {
        if !self.cmds.is_empty() {
            self.backend.run_cmds(std::mem::take(&mut self.cmds));
        }
    }

    pub(crate) fn get_sound_mt(&self) -> Arc<dyn SoundManagerMtInterface> {
        self.backend.get_backend_mt()
    }
}

#[hiarc_safer_rc_refcell]
impl Drop for SoundBackendHandle {
    fn drop(&mut self) {
        // last cleanup
        self.run_cmds();
    }
}
