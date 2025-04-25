use egui::Button;
use egui_extras::TableRow;
use game_base::server_browser::ServerBrowserServer;

/// Single server list entry
///
/// Returns if the item was clicked, and if restart was clicked (local server only)
pub fn render(
    mut row: TableRow<'_, '_>,
    server: &ServerBrowserServer,
    local_server: bool,
) -> (bool, bool) {
    let mut clicked_restart = false;
    let mut clicked = false;
    clicked |= row
        .col(|ui| {
            clicked |= if server.info.passworded {
                ui.label("\u{f023}")
            } else {
                ui.label("")
            }
            .clicked();
            clicked |= if server.legacy_server {
                ui.label("\u{4c}").on_hover_text("Legacy server")
            } else {
                ui.label("")
            }
            .clicked();
        })
        .1
        .clicked();
    clicked |= row
        .col(|ui| {
            clicked |= ui.label(server.info.name.as_str()).clicked();
        })
        .1
        .clicked();
    clicked |= row
        .col(|ui| {
            clicked |= ui.label(server.info.game_type.as_str()).clicked();
        })
        .1
        .clicked();
    clicked |= row
        .col(|ui| {
            clicked |= ui.label(server.info.map.name.as_str()).clicked();
        })
        .1
        .clicked();
    clicked |= row
        .col(|ui| {
            clicked |= ui.label(server.info.players.len().to_string()).clicked();
        })
        .1
        .clicked();
    clicked |= row
        .col(|ui| {
            if local_server {
                clicked_restart |= ui
                    .add(Button::new("\u{f2f1}"))
                    .on_hover_text("Restart local server")
                    .clicked();
            } else {
                clicked |= ui.label(server.location.as_str()).clicked();
            }
        })
        .1
        .clicked();
    (clicked, clicked_restart)
}
