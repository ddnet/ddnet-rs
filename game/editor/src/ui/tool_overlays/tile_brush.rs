use egui::{Color32, FontId, Shape};
use ui_base::types::UiRenderPipe;

use crate::{
    map::EditorMapInterface,
    tools::{
        quad_layer::{self, selection::QuadPointerDownState},
        sound_layer,
        tile_layer::brush::{TileBrush, TileBrushDown, TileBrushDownPos},
        tool::{ActiveTool, ActiveToolQuads, ActiveToolSounds, ActiveToolTiles},
    },
    ui::user_data::UserDataWithTab,
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>) {
    let tools = &mut *pipe.user_data.tools;

    let pos = |ui_pos: egui::Pos2| {
        ui.input(|i| {
            i.pointer
                .latest_pos()
                .or(i.pointer.hover_pos())
                .or(i.pointer.interact_pos())
        })
        .unwrap_or(ui_pos)
    };
    match tools.active_tool {
        ActiveTool::Tiles(ty) => match ty {
            ActiveToolTiles::Brush => {
                let brush = &tools.tiles.brush;
                let draw = |text: String, pointer_pos: &TileBrushDownPos| {
                    let pos = pos(pointer_pos.ui) + egui::vec2(20.0, 20.0);
                    // draw amount of tiles selected/drawn whatever
                    let bg = ui.painter().add(Shape::Noop);
                    let rect = ui.painter().text(
                        pos,
                        egui::Align2::LEFT_TOP,
                        text,
                        FontId::monospace(24.0),
                        Color32::WHITE,
                    );
                    ui.painter().set(
                        bg,
                        Shape::rect_filled(rect.expand(5.0), 5.0, Color32::from_black_alpha(125)),
                    );
                };
                if let Some(brush_tiles) = brush.brush.as_ref() {
                    if let Some(TileBrushDown {
                        pos: pointer_pos, ..
                    }) = brush.pointer_down_world_pos
                    {
                        draw(
                            format!("{}x{}", brush_tiles.w.get(), brush_tiles.h.get()),
                            &pointer_pos,
                        );
                    } else if let Some(pointer_pos) = brush.shift_pointer_down_world_pos {
                        let layer = pipe.user_data.editor_tab.map.active_layer();
                        let (offset, parallax) = if let Some(layer) = &layer {
                            layer.get_offset_and_parallax()
                        } else {
                            Default::default()
                        };
                        let pos = pos(pointer_pos.ui);
                        let cur = TileBrush::pos_on_map(
                            &pipe.user_data.editor_tab.map,
                            &ui.ctx().screen_rect(),
                            pipe.user_data.canvas_handle,
                            &pos,
                            &offset,
                            &parallax,
                        );
                        let (_, _, _, width, height) =
                            TileBrush::selection_size(&pointer_pos.world, &cur);
                        draw(
                            format!(
                                "{}x{} - {:.2}x{:.2}",
                                width,
                                height,
                                (width as f64) / (brush_tiles.w.get() as f64),
                                (height as f64) / (brush_tiles.h.get() as f64),
                            ),
                            &pointer_pos,
                        );
                    }
                }
            }
            ActiveToolTiles::Selection => {
                let selection = &tools.tiles.selection;
                if let Some(range) = selection.range.as_ref()
                    && let Some(pointer_pos) = &selection.pointer_down_state
                {
                    let pos = pos(pointer_pos.ui) + egui::vec2(20.0, 20.0);
                    // draw amount of tiles selected/drawn whatever
                    let bg = ui.painter().add(Shape::Noop);
                    let rect = ui.painter().text(
                        pos,
                        egui::Align2::LEFT_TOP,
                        format!("{}x{}", range.w.get(), range.h.get()),
                        FontId::monospace(24.0),
                        Color32::WHITE,
                    );
                    ui.painter().set(
                        bg,
                        Shape::rect_filled(rect.expand(5.0), 5.0, Color32::from_black_alpha(125)),
                    );
                }
            }
        },
        ActiveTool::Quads(ty) => {
            let count = match ty {
                ActiveToolQuads::Brush => {
                    let selection = &tools.quads.brush;
                    if let Some(range) = selection.brush.as_ref() {
                        if let quad_layer::brush::QuadPointerDownState::Selection(pointer_pos) =
                            &selection.pointer_down_state
                        {
                            Some((range.quads.len(), *pointer_pos))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                ActiveToolQuads::Selection => {
                    let selection = &tools.quads.selection;
                    if let Some(range) = selection.range.as_ref() {
                        if let QuadPointerDownState::Selection(pointer_pos) =
                            &selection.pointer_down_state
                        {
                            Some((range.quads.len(), *pointer_pos))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
            };
            if let Some((count, pointer_pos)) = count {
                let pos = pos(egui::pos2(pointer_pos.x, pointer_pos.y)) + egui::vec2(20.0, 20.0);
                // draw amount of tiles selected/drawn whatever
                let bg = ui.painter().add(Shape::Noop);
                let rect = ui.painter().text(
                    pos,
                    egui::Align2::LEFT_TOP,
                    format!("{count}x"),
                    FontId::monospace(24.0),
                    Color32::WHITE,
                );
                ui.painter().set(
                    bg,
                    Shape::rect_filled(rect.expand(5.0), 5.0, Color32::from_black_alpha(125)),
                );
            }
        }
        ActiveTool::Sounds(ty) => {
            let count = match ty {
                ActiveToolSounds::Brush => {
                    let selection = &tools.sounds.brush;
                    if let Some(range) = selection.brush.as_ref() {
                        if let sound_layer::brush::SoundPointerDownState::Selection(pointer_pos) =
                            &selection.pointer_down_state
                        {
                            Some((range.sounds.len(), *pointer_pos))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
            };
            if let Some((count, pointer_pos)) = count {
                let pos = pos(egui::pos2(pointer_pos.x, pointer_pos.y)) + egui::vec2(20.0, 20.0);
                // draw amount of tiles selected/drawn whatever
                let bg = ui.painter().add(Shape::Noop);
                let rect = ui.painter().text(
                    pos,
                    egui::Align2::LEFT_TOP,
                    format!("{count}x"),
                    FontId::monospace(24.0),
                    Color32::WHITE,
                );
                ui.painter().set(
                    bg,
                    Shape::rect_filled(rect.expand(5.0), 5.0, Color32::from_black_alpha(125)),
                );
            }
        }
    }
}
