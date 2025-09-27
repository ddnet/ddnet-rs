use egui::Frame;
use egui_extras::{Column, TableBuilder};
use tracing::instrument;
use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
    utils::{add_margins, get_margin},
};

use crate::ingame_menu::user_data::UserData;

#[instrument(level = "trace", skip_all)]
pub fn render(ui: &mut egui::Ui, ui_state: &mut UiState, pipe: &mut UiRenderPipe<UserData>) {
    pipe.user_data.server_players.request_player_infos();
    let server_players: Vec<_> = pipe
        .user_data
        .server_players
        .collect()
        .into_iter()
        .collect();
    let res = Frame::default()
        .fill(bg_frame_color())
        .corner_radius(5.0)
        .inner_margin(get_margin(ui))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(ui.available_height());
            ui.painter()
                .rect_filled(ui.available_rect_before_wrap(), 0.0, bg_frame_color());
            ui.set_clip_rect(ui.available_rect_before_wrap());
            add_margins(ui, |ui| {
                TableBuilder::new(ui)
                    .auto_shrink([false, false])
                    .columns(Column::remainder(), 2)
                    .header(30.0, |mut row| {
                        row.col(|ui| {
                            ui.label("Name");
                        });
                        row.col(|ui| {
                            ui.label("Flag");
                        });
                    })
                    .body(|body| {
                        body.rows(25.0, server_players.len(), |mut row| {
                            let (_, char) = &server_players[row.index()];
                            row.col(|ui| {
                                ui.label(char.name.as_str());
                            });
                            row.col(|ui| {
                                ui.label("TODO:");
                            });
                        })
                    });
            });
        });
    ui_state.add_blur_rect(res.response.rect, 5.0);
}
