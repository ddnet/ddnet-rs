use game_interface::account_info;
use hiarc::{Hiarc, hiarc_safer_rc_refcell};

#[hiarc_safer_rc_refcell]
#[derive(Debug, Default, Hiarc)]
pub struct AccountInfo {
    account_info: Option<account_info::AccountInfo>,
    creation_date_system_time: Option<String>,

    edit_data: String,

    last_action_response: Option<Option<String>>,
}

#[hiarc_safer_rc_refcell]
impl AccountInfo {
    pub fn fill_account_info(&mut self, account_info: Option<(account_info::AccountInfo, String)>) {
        let (account_info, creation_date_system_time) = account_info.unzip();
        self.account_info = account_info;
        self.creation_date_system_time = creation_date_system_time;
    }

    pub fn account_info(&self) -> Option<(account_info::AccountInfo, String)> {
        self.account_info
            .clone()
            .zip(self.creation_date_system_time.clone())
    }

    pub fn fill_edit_data(&mut self, edit_data: String) {
        self.edit_data = edit_data;
    }

    pub fn edit_data(&self) -> String {
        self.edit_data.clone()
    }

    pub fn fill_last_action_response(&mut self, last_action_response: Option<Option<String>>) {
        self.last_action_response = last_action_response;
    }

    /// If first Option is `Some`, then there was an action response.
    pub fn last_action_response(&self) -> Option<Option<String>> {
        self.last_action_response.clone()
    }
}
