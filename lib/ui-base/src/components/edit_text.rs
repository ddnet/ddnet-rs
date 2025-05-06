use egui::{Align, Layout, WidgetText};

pub fn edit_text<A>(
    ui: &mut egui::Ui,
    user: &mut A,
    edit_text: impl Fn(&mut A) -> Option<(WidgetText, &mut String)>,
    enter_edit_mode: impl FnOnce(&mut A),
    cancel_edit: impl FnOnce(&mut A),
    commit_edit: impl FnOnce(&mut A),
    non_edit_text: impl FnOnce() -> WidgetText,
) {
    let in_edit_mode = edit_text(user).is_some();

    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
        if ui
            .button(if in_edit_mode { "\u{f00c}" } else { "\u{f304}" })
            .clicked()
        {
            if in_edit_mode {
                commit_edit(user);
            } else {
                enter_edit_mode(user);
            }
        }
        if in_edit_mode && ui.button("\u{f00d}").clicked() {
            cancel_edit(user);
        }

        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            if let Some((prefix, edit_text)) = edit_text(user) {
                if !prefix.is_empty() {
                    ui.label(prefix);
                }
                ui.text_edit_singleline(edit_text);
            } else {
                ui.label(non_edit_text());
            }
        });
    });
}
