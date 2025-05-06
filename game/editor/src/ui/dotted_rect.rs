use egui::{Color32, Pos2, Rect, Stroke, Ui};

pub fn draw_dotted_rect(ui: &mut Ui, rect: Rect, dot_spacing: f32, color: Color32) {
    let painter = ui.painter();

    let stroke = Stroke::new(1.0, color);

    let min = rect.min;
    let max = rect.max;

    // Top edge
    let mut x = min.x;
    while x < max.x {
        let end_x = (x + dot_spacing).min(max.x);
        painter.line_segment([Pos2::new(x, min.y), Pos2::new(end_x, min.y)], stroke);
        x += dot_spacing * 2.0;
    }

    // Bottom edge
    x = min.x;
    while x < max.x {
        let end_x = (x + dot_spacing).min(max.x);
        painter.line_segment([Pos2::new(x, max.y), Pos2::new(end_x, max.y)], stroke);
        x += dot_spacing * 2.0;
    }

    // Left edge
    let mut y = min.y;
    while y < max.y {
        let end_y = (y + dot_spacing).min(max.y);
        painter.line_segment([Pos2::new(min.x, y), Pos2::new(min.x, end_y)], stroke);
        y += dot_spacing * 2.0;
    }

    // Right edge
    y = min.y;
    while y < max.y {
        let end_y = (y + dot_spacing).min(max.y);
        painter.line_segment([Pos2::new(max.x, y), Pos2::new(max.x, end_y)], stroke);
        y += dot_spacing * 2.0;
    }
}
