use egui::{Button, Color32, DragValue, Grid, Window};

use crate::tab::EditorTab;

use super::user_data::EditorUiEvent;

pub fn render(
    ui_events: &mut Vec<EditorUiEvent>,
    editor_tab: &mut EditorTab,
    pointer_is_used: &mut bool,
    ui: &mut egui::Ui,
) {
    let window_res = Window::new("Debug panel").show(ui.ctx(), |ui| {
        ui.colored_label(
            Color32::RED,
            "The debug panel is only intended for testing. It will break your map massively!",
        );
        ui.add_space(10.0);

        Grid::new("auto-saver-overview-grid")
            .num_columns(2)
            .show(ui, |ui| {
                let dbg_panel = &mut editor_tab.dbg_panel;
                ui.label("Generate actions count:");
                ui.add(DragValue::new(&mut dbg_panel.props.num_actions));
                ui.end_row();

                ui.label("Shuffle action probability:");
                let mut v = dbg_panel.props.action_shuffle_probability as f64 / 255.0;
                ui.add(DragValue::new(&mut v).speed(1.0 / 255.0).range(0.0..=1.0));
                dbg_panel.props.action_shuffle_probability = (v * 255.0) as u8;
                ui.end_row();

                ui.label("Undo/redo probability:");
                let mut v = dbg_panel.props.undo_redo_probability as f64 / 255.0;
                ui.add(DragValue::new(&mut v).speed(1.0 / 255.0).range(0.0..=1.0));
                dbg_panel.props.undo_redo_probability = (v * 255.0) as u8;
                ui.end_row();

                ui.label("Don't group actions (increases undo/redo history per generated action):");
                ui.checkbox(&mut dbg_panel.props.no_actions_identifier, "");
                ui.end_row();

                if ui.button("Step").clicked() {
                    ui_events.push(EditorUiEvent::DbgAction(dbg_panel.props));
                }

                if ui.add(Button::new("Run").selected(dbg_panel.run)).clicked() {
                    dbg_panel.run = !dbg_panel.run;
                }
                if dbg_panel.run {
                    ui_events.push(EditorUiEvent::DbgAction(dbg_panel.props));
                }
                ui.end_row();
            });

        ui.add_space(10.0);
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
