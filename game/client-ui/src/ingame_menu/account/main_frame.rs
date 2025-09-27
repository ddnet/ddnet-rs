use base::network_string::NetworkReducedAsciiString;
use egui::{Color32, Frame, TextEdit};

use tracing::instrument;
use ui_base::{
    style::bg_frame_color,
    types::{UiRenderPipe, UiState},
    utils::get_margin,
};

use crate::{
    events::{UiEvent, UiEvents},
    ingame_menu::{account_info::AccountInfo, user_data::UserData},
};

fn render_name_change(ui: &mut egui::Ui, events: &UiEvents, account_info: &AccountInfo) {
    let mut account_name = account_info.edit_data();
    ui.add(TextEdit::singleline(&mut account_name).char_limit(24));
    let account_name_res = NetworkReducedAsciiString::new(account_name.as_str());
    account_info.fill_edit_data(account_name);
    match account_name_res {
        Ok(new_account_name) if new_account_name.len() >= 3 => {
            if ui.button("Change name").clicked() {
                events.push(UiEvent::ChangeAccountName {
                    name: new_account_name,
                });
            }
        }
        Ok(_) => {
            ui.colored_label(
                Color32::RED,
                "Name must be at least \
                3 characters long.",
            );
        }
        Err(err) => {
            ui.colored_label(Color32::RED, err.to_string());
        }
    }

    let action_response = account_info.last_action_response();
    if let Some(action_response) = action_response {
        match action_response {
            Some(err) => {
                ui.colored_label(Color32::RED, err);
            }
            None => {
                ui.colored_label(Color32::GREEN, "Your request was successful.");
            }
        }
    }

    ui.add_space(10.0);

    let account_info = account_info.account_info();
    ui.label("Account information on the server");

    match account_info {
        Some((account_info, creation_date)) => {
            ui.label(format!("Name: {}", account_info.name.as_str()));
            ui.label(format!("Creation date: {creation_date}"));
        }
        None => {
            ui.label("Loading...");
            events.push(UiEvent::RequestAccountInfo);
        }
    }
}

/// main frame. full width
#[instrument(level = "trace", skip_all)]
pub fn render(ui: &mut egui::Ui, ui_state: &mut UiState, pipe: &mut UiRenderPipe<UserData>) {
    let res = Frame::default()
        .fill(bg_frame_color())
        .corner_radius(5.0)
        .inner_margin(get_margin(ui))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(ui.available_height());
            ui.label(
                "Here you can modify your account name \
                        for all game servers this server belongs to.",
            );

            render_name_change(
                ui,
                pipe.user_data.browser_menu.events,
                pipe.user_data.account_info,
            );
        });
    ui_state.add_blur_rect(res.response.rect, 5.0);
}
