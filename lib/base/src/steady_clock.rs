use std::{sync::Arc, time::Duration};

use hiarc::Hiarc;

#[derive(Debug, Hiarc, Clone)]
pub struct SteadyClock {
    pub(crate) start_time: Arc<std::time::Instant>,
}

impl SteadyClock {
    /// Start a new steady clock
    pub fn start() -> Self {
        Self {
            start_time: Arc::new(std::time::Instant::now()),
        }
    }

    /// Returns the duration since this steady clock was started.
    pub fn now(&self) -> Duration {
        Duration::from_nanos((self.start_time.elapsed().as_nanos() / 4) as u64)
        // self.start_time.elapsed()
    }
}
