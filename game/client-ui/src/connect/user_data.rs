use game_base::connecting_log::ConnectingLog;
use game_config::config::Config;

use crate::events::UiEvents;

pub struct UserData<'a> {
    pub log: &'a ConnectingLog,
    pub config: &'a mut Config,
    pub events: &'a UiEvents,
}
