use egui::DragValue;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    map::{EditorLayerUnionRefMut, EditorMapGroupsInterface, EditorPhysicsLayer},
    ui::user_data::UserDataWithTab,
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    let map = &mut pipe.user_data.editor_tab.map;
    let Some(EditorLayerUnionRefMut::Physics {
        layer: EditorPhysicsLayer::Speedup(layer),
        ..
    }) = map.groups.active_layer_mut()
    else {
        return;
    };
    let style = ui.style();
    let height = style.spacing.interact_size.y + style.spacing.item_spacing.y;

    let res = egui::TopBottomPanel::top("top_toolbar_speedup_extra")
        .resizable(false)
        .default_height(height)
        .height_range(height..=height)
        .show_inside(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        DragValue::new(&mut layer.user.speedup_force)
                            .update_while_editing(false)
                            .prefix("Force: "),
                    );
                    ui.add(
                        DragValue::new(&mut layer.user.speedup_angle)
                            .range(0..=359)
                            .update_while_editing(false)
                            .prefix("Angle: "),
                    );
                    ui.add(
                        DragValue::new(&mut layer.user.speedup_max_speed)
                            .update_while_editing(false)
                            .prefix("Max speed: "),
                    );
                });
            });
        });

    ui_state.add_blur_rect(res.response.rect, 0.0);
}
