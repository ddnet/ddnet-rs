use hiarc::{Hiarc, hiarc_safer_rc_refcell};

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
    pub fn set_raw_input(&mut self, raw_input: RawInput) {
        self.raw_input = raw_input;
    }

    pub fn raw_input(&self) -> RawInput {
        self.raw_input.clone()
    }

    pub fn request_raw_input(&mut self) {
        self.needs_raw_input = true;
    }

    pub fn wants_raw_input(&mut self) -> bool {
        std::mem::take(&mut self.needs_raw_input)
    }
}
