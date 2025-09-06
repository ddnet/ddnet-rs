use hiarc::{Hiarc, hiarc_safer_rc_refcell};

#[derive(Debug, Hiarc)]
pub enum EditorNotification {
    Error(String),
    Warning(String),
    Info(String),
}

#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc, Default)]
pub struct EditorNotifications {
    notifications: Vec<EditorNotification>,
}

#[hiarc_safer_rc_refcell]
impl EditorNotifications {
    pub fn push(&mut self, nfy: EditorNotification) {
        self.notifications.push(nfy);
    }

    pub fn take(&mut self) -> Vec<EditorNotification> {
        std::mem::take(&mut self.notifications)
    }
}
