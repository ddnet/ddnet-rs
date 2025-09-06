use graphics_types::commands::AllCommands;
use hiarc::{Hiarc, hiarc_safer_rc_refcell};

#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc)]
pub struct BackendCommands {
    cmds: Vec<AllCommands>,
}

#[hiarc_safer_rc_refcell]
impl Default for BackendCommands {
    fn default() -> Self {
        Self {
            cmds: Vec::with_capacity(200),
        }
    }
}

#[hiarc_safer_rc_refcell]
impl BackendCommands {
    pub fn add_cmd(&mut self, cmd: AllCommands) {
        self.cmds.push(cmd);
    }

    pub fn add_cmds(&mut self, cmds: &mut Vec<AllCommands>) {
        self.cmds.append(cmds);
    }

    pub fn take(&mut self) -> Vec<AllCommands> {
        std::mem::take(&mut self.cmds)
    }

    pub fn replace(&mut self, swap: &mut Vec<AllCommands>) {
        std::mem::swap(&mut self.cmds, swap);
    }
}
