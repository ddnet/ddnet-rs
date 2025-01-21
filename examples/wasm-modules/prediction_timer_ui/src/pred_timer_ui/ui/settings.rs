use std::time::Duration;

use egui::DragValue;

use super::user_data::SimulationProps;

pub fn render(ui: &mut egui::Ui, props: &mut SimulationProps) {
    ui.label("rtt offset(ms)");
    let mut millis = props.rtt_offset.as_millis() as u64;
    ui.add(DragValue::new(&mut millis).update_while_editing(false));
    props.rtt_offset = Duration::from_millis(millis);

    ui.label("rtt half jitter range");
    let mut millis = props.half_rtt_jitter_range.as_millis() as u64;
    ui.add(DragValue::new(&mut millis).update_while_editing(false));
    props.half_rtt_jitter_range = Duration::from_millis(millis);

    ui.label("snaps/s");
    ui.add(DragValue::new(&mut props.snaps_per_sec).update_while_editing(false));

    ui.label("time scale");
    ui.add(DragValue::new(&mut props.time_scale).update_while_editing(false));
}
