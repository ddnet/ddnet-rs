use game_base::connecting_log::ConnectingLog;
use game_config::config::Config;
use ui_base::types::{UiRenderPipe, UiState};
use ui_generic::traits::UiPageInterface;

use crate::events::UiEvents;

use super::{main_frame, user_data::UserData};

pub struct ConnectingUi {
    log: ConnectingLog,
    events: UiEvents,
}

impl ConnectingUi {
    pub fn new(log: ConnectingLog, events: UiEvents) -> Self {
        Self { log, events }
    }

    fn render_impl(
        &mut self,
        ui: &mut egui::Ui,
        ui_state: &mut UiState,
        pipe: &mut UiRenderPipe<Config>,
    ) {
        main_frame::render(
            ui,
            ui_state,
            &mut UiRenderPipe {
                cur_time: pipe.cur_time,
                user_data: &mut UserData {
                    log: &self.log,
                    config: pipe.user_data,
                    events: &self.events,
                },
            },
        );
    }
}

impl UiPageInterface<Config> for ConnectingUi {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut UiRenderPipe<Config>,
        ui_state: &mut UiState,
    ) {
        self.render_impl(ui, ui_state, pipe)
    }

    fn unmount(&mut self) {
        self.log.clear();
    }
}
