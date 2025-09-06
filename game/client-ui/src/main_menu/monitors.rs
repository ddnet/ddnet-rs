use std::rc::Rc;

use hiarc::{Hiarc, hiarc_safer_rc_refcell};

#[derive(Debug, Hiarc)]
pub struct UiMonitorVideoMode {
    pub width: u32,
    pub height: u32,
    pub refresh_rate_mhz: u32,
}

#[derive(Debug, Hiarc)]
pub struct UiMonitor {
    pub name: String,
    pub video_modes: Vec<UiMonitorVideoMode>,
}

#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc)]
pub struct UiMonitors {
    monitors: Rc<Vec<UiMonitor>>,
}

#[hiarc_safer_rc_refcell]
impl UiMonitors {
    pub fn new(monitors: Vec<UiMonitor>) -> Self {
        Self {
            monitors: Rc::new(monitors),
        }
    }

    pub fn monitors(&self) -> Rc<Vec<UiMonitor>> {
        self.monitors.clone()
    }
}
