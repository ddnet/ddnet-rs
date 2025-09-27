use hiarc::{Hiarc, hiarc_safer_rc_refcell};
use tracing::instrument;

#[derive(Debug, Hiarc, Default, Clone)]
pub struct RawInput {
    #[cfg(feature = "binds")]
    pub keys: std::collections::HashSet<binds::binds::BindKey>,
}

#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc, Default)]
pub struct RawInputInfo {
    raw_input: RawInput,
    needs_raw_input: bool,
}

#[hiarc_safer_rc_refcell]
impl RawInputInfo {
    #[instrument(level = "trace", skip_all)]
    pub fn set_raw_input(&mut self, raw_input: RawInput) {
        self.raw_input = raw_input;
    }

    #[instrument(level = "trace", skip_all)]
    pub fn raw_input(&self) -> RawInput {
        self.raw_input.clone()
    }

    #[instrument(level = "trace", skip_all)]
    pub fn request_raw_input(&mut self) {
        self.needs_raw_input = true;
    }

    #[instrument(level = "trace", skip_all)]
    pub fn wants_raw_input(&mut self) -> bool {
        std::mem::take(&mut self.needs_raw_input)
    }
}
