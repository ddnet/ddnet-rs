use egui::{Grid, Layout, Slider};
use tracing::instrument;
use ui_base::types::UiRenderPipe;

use crate::main_menu::{settings::constants::SETTINGS_SUB_UI_PAGE_QUERY, user_data::UserData};

#[instrument(level = "trace", skip_all)]
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>) {
    let cur_sub = pipe
        .user_data
        .config
        .engine
        .ui
        .path
        .query
        .get(SETTINGS_SUB_UI_PAGE_QUERY)
        .map(|path| path.as_ref())
        .unwrap_or("")
        .to_string();

    match cur_sub.as_str() {
        "Spatial Chat" => {
            super::spatial_chat::main_frame::render(ui, pipe);
        }
        // Sound itself is selected
        _ => {
            ui.with_layout(Layout::top_down(egui::Align::Min), |ui| {
                let config = &mut pipe.user_data.config.game.snd;

                Grid::new("ingame-sound-volume")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Global sound volume:");
                        ui.add(Slider::new(&mut config.global_volume, 0.0..=1.0).max_decimals(2));
                        ui.end_row();

                        ui.label("Ingame sound volume:");
                        ui.add(
                            Slider::new(&mut config.render.ingame_sound_volume, 0.0..=1.0)
                                .max_decimals(2),
                        );
                        ui.end_row();

                        ui.label("Map sound volume:");
                        ui.add(
                            Slider::new(&mut config.render.map_sound_volume, 0.0..=1.0)
                                .max_decimals(2),
                        );
                        ui.end_row();
                    });
            });
        }
    }
}
