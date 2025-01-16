use std::time::Duration;

use base::duration_ext::DurationToRaceStr;
use egui::{DragValue, Grid, Window};

use crate::tab::EditorTab;

pub fn render(
    cur_time: Duration,
    editor_tab: &mut EditorTab,
    pointer_is_used: &mut bool,
    ui: &mut egui::Ui,
) {
    let auto_saver = &mut editor_tab.auto_saver;

    let window_res = Window::new("Auto-saver-overview").show(ui.ctx(), |ui| {
        ui.label(
            "The auto-saver is a tool independent \
            to be used for multi people mapping.",
        );
        ui.label(
            "Instead of manually saving, you can define \
            various actions that should trigger a save.",
        );
        ui.add_space(10.0);

        Grid::new("auto-saver-overview-grid")
            .num_columns(2)
            .show(ui, |ui| {
                let mut use_interval = auto_saver.interval.is_some();
                ui.label("Use interval");
                ui.checkbox(&mut use_interval, "");

                if auto_saver.interval.is_some() != use_interval {
                    auto_saver.interval = match auto_saver.interval {
                        Some(_) => None,
                        None => Some(Duration::from_secs(10)),
                    }
                }

                if let Some(interval) = &mut auto_saver.interval {
                    ui.label("Interval in seconds:");
                    let mut interval_secs = interval.as_secs();
                    ui.add(DragValue::new(&mut interval_secs));
                    *interval = Duration::from_secs(interval_secs);
                }
            });

        ui.add_space(10.0);
        if let Some(path) = &auto_saver.path {
            ui.label("The auto-saver is now active.");
            ui.label(format!("Saving to: {}", path.to_string_lossy()));
            if let (Some(interval), Some(last_time)) = (auto_saver.interval, auto_saver.last_time) {
                ui.label(format!(
                    "Next save in: {}",
                    interval
                        .saturating_sub(cur_time.saturating_sub(last_time))
                        .to_race_string()
                ));
            }
        } else {
            ui.label("Map was never saved, please save it before the auto saver works.");
        }
    });

    *pointer_is_used |= if let Some(window_res) = &window_res {
        let intersected = ui.input(|i| {
            if i.pointer.primary_down() {
                Some((
                    !window_res.response.rect.intersects({
                        let min = i.pointer.interact_pos().unwrap_or_default();
                        let max = min;
                        [min, max].into()
                    }),
                    i.pointer.primary_pressed(),
                ))
            } else {
                None
            }
        });
        intersected.is_some_and(|(outside, _)| !outside)
    } else {
        false
    };
}
