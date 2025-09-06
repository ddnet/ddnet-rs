use egui::{Frame, Sense, Shadow};
use egui_extras::{Column, Size, StripBuilder, TableBuilder};
use game_interface::votes::{MAX_VOTE_REASON_LEN, PlayerVoteKey};
use ui_base::{
    components::clearable_edit_field::clearable_edit_field,
    style::bg_frame_color,
    types::UiRenderPipe,
    utils::{add_margins, get_margin},
};

use crate::{events::UiEvent, ingame_menu::user_data::UserData};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>) {
    pipe.user_data.server_players.request_player_infos();
    let server_players: Vec<_> = pipe
        .user_data
        .server_players
        .collect()
        .into_iter()
        .collect();

    const VOTE_PLAYER_INDEX: &str = "vote-player-index";
    let mut index_entry = pipe
        .user_data
        .browser_menu
        .config
        .engine
        .ui
        .path
        .query
        .entry(VOTE_PLAYER_INDEX.to_string())
        .or_default()
        .clone();
    let index: usize = index_entry.parse().unwrap_or_default();

    Frame::default()
        .fill(bg_frame_color())
        .inner_margin(get_margin(ui))
        .shadow(Shadow::NONE)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                StripBuilder::new(ui)
                    .size(Size::remainder())
                    .size(Size::exact(20.0))
                    .vertical(|mut strip| {
                        strip.cell(|ui| {
                            ui.style_mut().wrap_mode = None;
                            ui.painter().rect_filled(
                                ui.available_rect_before_wrap(),
                                0.0,
                                bg_frame_color(),
                            );
                            ui.set_clip_rect(ui.available_rect_before_wrap());
                            add_margins(ui, |ui| {
                                TableBuilder::new(ui)
                                    .auto_shrink([false, false])
                                    .columns(Column::remainder(), 1)
                                    .sense(Sense::click())
                                    .header(30.0, |mut row| {
                                        row.col(|ui| {
                                            ui.label("Name");
                                        });
                                    })
                                    .body(|body| {
                                        body.rows(25.0, server_players.len(), |mut row| {
                                            row.set_selected(index == row.index());
                                            let (_, char) = &server_players[row.index()];
                                            row.col(|ui| {
                                                ui.label(char.name.as_str());
                                            });
                                            if row.response().clicked() {
                                                index_entry = row.index().to_string();
                                            }
                                        })
                                    });
                            });
                        });
                        strip.cell(|ui| {
                            ui.style_mut().wrap_mode = None;
                            ui.horizontal(|ui| {
                                let reason = pipe
                                    .user_data
                                    .browser_menu
                                    .config
                                    .engine
                                    .ui
                                    .path
                                    .query
                                    .entry("player-vote-reason-str".to_string())
                                    .or_default();

                                ui.label("Reason:");
                                clearable_edit_field(
                                    ui,
                                    reason,
                                    Some(100.0),
                                    Some(MAX_VOTE_REASON_LEN),
                                );
                                let reason = reason.to_string();

                                if ui.button("Kick").clicked()
                                    && let Some((id, _)) = server_players.get(index)
                                {
                                    pipe.user_data.browser_menu.events.push(
                                        UiEvent::VoteKickPlayer(PlayerVoteKey {
                                            voted_player_id: *id,
                                            reason: reason.as_str().try_into().unwrap(),
                                        }),
                                    );
                                }
                                if ui.button("Move to spec").clicked()
                                    && let Some((id, _)) = server_players.get(index)
                                {
                                    pipe.user_data.browser_menu.events.push(
                                        UiEvent::VoteSpecPlayer(PlayerVoteKey {
                                            voted_player_id: *id,
                                            reason: reason.try_into().unwrap(),
                                        }),
                                    );
                                }
                            });
                        });
                    });
            });
        });

    pipe.user_data
        .browser_menu
        .config
        .engine
        .ui
        .path
        .query
        .insert(VOTE_PLAYER_INDEX.to_string(), index_entry);
}
