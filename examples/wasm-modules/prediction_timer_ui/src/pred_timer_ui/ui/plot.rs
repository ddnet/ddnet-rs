use std::collections::VecDeque;

use egui_plot::{Line, Plot, PlotPoints};
use prediction_timer::prediction_timing::PredictionTiming;

pub fn render(ui: &mut egui::Ui, history: &mut VecDeque<PredictionTiming>) {
    let mut plot = |name: &str, val: &dyn Fn(&PredictionTiming) -> f64| {
        ui.label(name);
        let sin: PlotPoints = history
            .iter()
            .enumerate()
            .map(|(i, timing)| {
                let x = i as f64 * 0.01;
                [x, val(timing)]
            })
            .collect();
        let max = history.iter().map(val).max_by(|v1, v2| v1.total_cmp(v2));
        let min = history.iter().map(val).min_by(|v1, v2| v1.total_cmp(v2));

        ui.label(format!("min {:?}, max {:?}", min, max));

        let line = Line::new("", sin);
        Plot::new(name)
            .height(150.0)
            .show(ui, |plot_ui| plot_ui.line(line));
    };

    plot("max_ping", &|timing| {
        timing.ping_max().as_secs_f64() * 1000.0
    });
    plot("min_ping", &|timing| {
        timing.ping_min().as_secs_f64() * 1000.0
    });
    plot("avg_ping", &|timing| {
        timing.ping_average().as_secs_f64() * 1000.0
    });
    plot("smooth_adjustment_time", &|timing| {
        timing.smooth_adjustment_time * 1000.0
    });
    plot("last_lag", &|timing| timing.last_forced_weight * 1000.0);
}
