use std::{path::PathBuf, time::Duration};

#[derive(Debug)]
pub struct AutoSaver {
    pub active: bool,
    pub interval: Option<Duration>,

    pub last_time: Option<Duration>,

    pub path: Option<PathBuf>,
}
