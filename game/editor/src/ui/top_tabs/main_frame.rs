use egui::Button;
use ui_base::types::UiRenderPipe;

use crate::ui::user_data::UserData;

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>) {
    let style = ui.style();
    let height = style.spacing.interact_size.y + style.spacing.item_spacing.y;
    egui::TopBottomPanel::top("top_tabs")
        .resizable(false)
        .default_height(height)
        .height_range(height..=height)
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.style_mut().spacing.item_spacing.x = 0.0;
                let mut remove_tab = None;
                for (tab_name, tab) in pipe.user_data.editor_tabs.tabs.iter() {
                    let tab_display_name = if tab.client.clients.len() > 1 {
                        format!("\u{f0c0} {tab_name}")
                    } else {
                        tab_name.clone()
                    };
                    let mut btn = ui.add(
                        Button::new(tab_display_name)
                            .selected(pipe.user_data.editor_tabs.active_tab == tab_name),
                    );
                    if tab.client.clients.len() > 1 {
                        btn = btn.on_hover_ui(|ui| {
                            ui.vertical(|ui| {
                                for client in tab.client.clients.iter() {
                                    ui.label(&client.mapper_name);
                                }
                            });
                        })
                    }
                    if btn.clicked() {
                        *pipe.user_data.editor_tabs.active_tab = tab_name.clone();
                    }
                    if ui.add(Button::new("\u{f00d}")).clicked() {
                        remove_tab = Some(tab_name.clone());
                    }
                    ui.add_space(10.0);
                }
                if let Some(tab) = remove_tab {
                    pipe.user_data.editor_tabs.tabs.remove(&tab);
                }
            })
        });
}
