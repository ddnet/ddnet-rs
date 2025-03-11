use egui::Button;
use map::map::groups::layers::design::{Quad, Sound, SoundShape};
use math::math::vector::uffixed;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{
    actions::actions::{
        ActQuadLayerAddQuads, ActQuadLayerAddRemQuads, ActSoundLayerAddRemSounds,
        ActSoundLayerAddSounds, EditorAction,
    },
    explain::{
        TEXT_ADD_QUAD, TEXT_ADD_SOUND, TEXT_QUAD_BRUSH, TEXT_QUAD_SELECTION, TEXT_SOUND_BRUSH,
        TEXT_TILE_BRUSH, TEXT_TILE_BRUSH_MIRROR, TEXT_TILE_SELECT,
    },
    map::{EditorLayer, EditorLayerUnionRef, EditorMapInterface},
    tools::tool::{ActiveTool, ActiveToolQuads, ActiveToolSounds, ActiveToolTiles},
    ui::user_data::UserDataWithTab,
};

use super::tile_mirror::{
    mirror_layer_tiles_x, mirror_layer_tiles_y, mirror_tiles_x, mirror_tiles_y,
    rotate_layer_tiles_plus_90, rotate_tile_flags_plus_90, rotate_tiles_plus_90,
};

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>, ui_state: &mut UiState) {
    let style = ui.style();
    // 4.0 is some margin for strokes
    let height = style.spacing.interact_size.y + style.spacing.item_spacing.y + 4.0;
    let res = egui::TopBottomPanel::top("top_toolbar")
        .resizable(false)
        .default_height(height)
        .height_range(height..=height)
        .show_inside(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    match &mut pipe.user_data.tools.active_tool {
                        ActiveTool::Tiles(tool) => {
                            // brush
                            let mut btn = Button::new("\u{f55d}");
                            if matches!(tool, ActiveToolTiles::Brush) {
                                btn = btn.selected(true);
                            }
                            if ui
                                .add(btn)
                                .on_hover_ui(|ui| {
                                    let mut cache = egui_commonmark::CommonMarkCache::default();
                                    egui_commonmark::CommonMarkViewer::new().show(
                                        ui,
                                        &mut cache,
                                        TEXT_TILE_BRUSH,
                                    );
                                })
                                .clicked()
                            {
                                *tool = ActiveToolTiles::Brush;
                            }
                            // select
                            let mut btn = Button::new("\u{f247}");
                            if matches!(tool, ActiveToolTiles::Selection) {
                                btn = btn.selected(true);
                            }
                            if ui
                                .add(btn)
                                .on_hover_ui(|ui| {
                                    let mut cache = egui_commonmark::CommonMarkCache::default();
                                    egui_commonmark::CommonMarkViewer::new().show(
                                        ui,
                                        &mut cache,
                                        TEXT_TILE_SELECT,
                                    );
                                })
                                .clicked()
                            {
                                *tool = ActiveToolTiles::Selection;
                            }
                        }
                        ActiveTool::Quads(tool) => {
                            // brush
                            let mut btn = Button::new("\u{f55d}");
                            if matches!(tool, ActiveToolQuads::Brush) {
                                btn = btn.selected(true);
                            }
                            if ui
                                .add(btn)
                                .on_hover_ui(|ui| {
                                    let mut cache = egui_commonmark::CommonMarkCache::default();
                                    egui_commonmark::CommonMarkViewer::new().show(
                                        ui,
                                        &mut cache,
                                        TEXT_QUAD_BRUSH,
                                    );
                                })
                                .clicked()
                            {
                                *tool = ActiveToolQuads::Brush;
                            }
                            // select
                            let mut btn = Button::new("\u{f247}");
                            if matches!(tool, ActiveToolQuads::Selection) {
                                btn = btn.selected(true);
                            }
                            if ui
                                .add(btn)
                                .on_hover_ui(|ui| {
                                    let mut cache = egui_commonmark::CommonMarkCache::default();
                                    egui_commonmark::CommonMarkViewer::new().show(
                                        ui,
                                        &mut cache,
                                        TEXT_QUAD_SELECTION,
                                    );
                                })
                                .clicked()
                            {
                                *tool = ActiveToolQuads::Selection;
                            }
                        }
                        ActiveTool::Sounds(tool) => {
                            // brush
                            let mut btn = Button::new("\u{f55d}");
                            if matches!(tool, ActiveToolSounds::Brush) {
                                btn = btn.selected(true);
                            }
                            if ui
                                .add(btn)
                                .on_hover_ui(|ui| {
                                    let mut cache = egui_commonmark::CommonMarkCache::default();
                                    egui_commonmark::CommonMarkViewer::new().show(
                                        ui,
                                        &mut cache,
                                        TEXT_SOUND_BRUSH,
                                    );
                                })
                                .clicked()
                            {
                                *tool = ActiveToolSounds::Brush;
                            }
                        }
                    }
                });
            });
        });

    ui_state.add_blur_rect(res.response.rect, 0.0);

    let tools = &mut pipe.user_data.tools;
    let res = match &tools.active_tool {
        ActiveTool::Tiles(tool) => {
            let is_active = (matches!(tool, ActiveToolTiles::Brush)
                && tools.tiles.brush.brush.is_some())
                || (matches!(tool, ActiveToolTiles::Selection)
                    && tools.tiles.selection.range.is_some());
            egui::TopBottomPanel::top("top_toolbar_tiles_extra")
                .resizable(false)
                .default_height(height)
                .height_range(height..=height)
                .show_inside(ui, |ui| {
                    egui::ScrollArea::horizontal().show(ui, |ui| {
                        ui.add_enabled_ui(is_active, |ui| {
                            ui.horizontal(|ui| {
                                // mirror y
                                let btn = Button::new("\u{f07d}");
                                if ui
                                    .add(btn)
                                    .on_hover_ui(|ui| {
                                        let mut cache = egui_commonmark::CommonMarkCache::default();
                                        egui_commonmark::CommonMarkViewer::new().show(
                                            ui,
                                            &mut cache,
                                            TEXT_TILE_BRUSH_MIRROR,
                                        );
                                    })
                                    .clicked()
                                {
                                    match tool {
                                        ActiveToolTiles::Brush => {
                                            if let Some(brush) = &mut tools.tiles.brush.brush {
                                                mirror_tiles_y(
                                                    pipe.user_data.tp,
                                                    pipe.user_data.graphics_mt,
                                                    pipe.user_data.buffer_object_handle,
                                                    pipe.user_data.backend_handle,
                                                    brush,
                                                    true,
                                                );
                                            }
                                        }
                                        ActiveToolTiles::Selection => {
                                            if let (Some(layer), Some(range)) = (
                                                pipe.user_data.editor_tab.map.active_layer(),
                                                &tools.tiles.selection.range,
                                            ) {
                                                mirror_layer_tiles_y(
                                                    pipe.user_data.tp,
                                                    layer,
                                                    range,
                                                    &mut pipe.user_data.editor_tab.client,
                                                );
                                            }
                                        }
                                    }
                                }
                                // mirror x
                                let btn = Button::new("\u{f07e}");
                                if ui.add(btn).clicked() {
                                    match tool {
                                        ActiveToolTiles::Brush => {
                                            if let Some(brush) = &mut tools.tiles.brush.brush {
                                                mirror_tiles_x(
                                                    pipe.user_data.tp,
                                                    pipe.user_data.graphics_mt,
                                                    pipe.user_data.buffer_object_handle,
                                                    pipe.user_data.backend_handle,
                                                    brush,
                                                    true,
                                                );
                                            }
                                        }
                                        ActiveToolTiles::Selection => {
                                            if let (Some(layer), Some(range)) = (
                                                pipe.user_data.editor_tab.map.active_layer(),
                                                &tools.tiles.selection.range,
                                            ) {
                                                mirror_layer_tiles_x(
                                                    pipe.user_data.tp,
                                                    layer,
                                                    range,
                                                    &mut pipe.user_data.editor_tab.client,
                                                );
                                            }
                                        }
                                    }
                                }
                                match tool {
                                    ActiveToolTiles::Brush => {
                                        // rotate -90°
                                        let btn = Button::new("\u{f2ea}");
                                        if ui.add(btn).clicked() {
                                            if let Some(brush) = &mut tools.tiles.brush.brush {
                                                // use 3 times 90° here, bcs the 90° logic also "fixes" the cursor
                                                // x,y mirror does not
                                                rotate_tiles_plus_90(
                                                    pipe.user_data.tp,
                                                    pipe.user_data.graphics_mt,
                                                    pipe.user_data.buffer_object_handle,
                                                    pipe.user_data.backend_handle,
                                                    brush,
                                                    false,
                                                );
                                                rotate_tiles_plus_90(
                                                    pipe.user_data.tp,
                                                    pipe.user_data.graphics_mt,
                                                    pipe.user_data.buffer_object_handle,
                                                    pipe.user_data.backend_handle,
                                                    brush,
                                                    false,
                                                );
                                                rotate_tiles_plus_90(
                                                    pipe.user_data.tp,
                                                    pipe.user_data.graphics_mt,
                                                    pipe.user_data.buffer_object_handle,
                                                    pipe.user_data.backend_handle,
                                                    brush,
                                                    true,
                                                );
                                            }
                                        }
                                        // rotate +90°
                                        let btn = Button::new("\u{f2f9}");
                                        if ui.add(btn).clicked() {
                                            if let Some(brush) = &mut tools.tiles.brush.brush {
                                                rotate_tiles_plus_90(
                                                    pipe.user_data.tp,
                                                    pipe.user_data.graphics_mt,
                                                    pipe.user_data.buffer_object_handle,
                                                    pipe.user_data.backend_handle,
                                                    brush,
                                                    true,
                                                );
                                            }
                                        }
                                        // rotate tiles (only by flags) +90°
                                        let btn = Button::new("\u{e4f6}");
                                        if ui.add(btn).clicked() {
                                            if let Some(brush) = &mut tools.tiles.brush.brush {
                                                rotate_tile_flags_plus_90(
                                                    pipe.user_data.tp,
                                                    pipe.user_data.graphics_mt,
                                                    pipe.user_data.buffer_object_handle,
                                                    pipe.user_data.backend_handle,
                                                    brush,
                                                    true,
                                                );
                                            }
                                        }
                                    }
                                    ActiveToolTiles::Selection => {
                                        if let Some(layer) =
                                            pipe.user_data.editor_tab.map.active_layer()
                                        {
                                            // rotate inner tiles (flags) by 90°
                                            let btn = Button::new("\u{e4f6}");
                                            if ui.add(btn).clicked() {
                                                if let Some(range) = &tools.tiles.selection.range {
                                                    rotate_layer_tiles_plus_90(
                                                        pipe.user_data.tp,
                                                        layer,
                                                        range,
                                                        &mut pipe.user_data.editor_tab.client,
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            });
                        });
                    });
                })
        }
        ActiveTool::Quads(_) => {
            egui::TopBottomPanel::top("top_toolbar_quads_extra")
                .resizable(false)
                .default_height(height)
                .height_range(height..=height)
                .show_inside(ui, |ui| {
                    egui::ScrollArea::horizontal().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // add quad
                            let btn = Button::new("\u{f0fe}");
                            if ui
                                .add(btn)
                                .on_hover_ui(|ui| {
                                    let mut cache = egui_commonmark::CommonMarkCache::default();
                                    egui_commonmark::CommonMarkViewer::new().show(
                                        ui,
                                        &mut cache,
                                        TEXT_ADD_QUAD,
                                    );
                                })
                                .clicked()
                            {
                                if let Some(EditorLayerUnionRef::Design {
                                    group_index,
                                    layer_index,
                                    is_background,
                                    layer: EditorLayer::Quad(layer),
                                    ..
                                }) = pipe.user_data.editor_tab.map.active_layer()
                                {
                                    let index = layer.layer.quads.len();
                                    pipe.user_data.editor_tab.client.execute(
                                        EditorAction::QuadLayerAddQuads(ActQuadLayerAddQuads {
                                            base: ActQuadLayerAddRemQuads {
                                                is_background,
                                                group_index,
                                                layer_index,
                                                index,
                                                quads: vec![Quad::default()],
                                            },
                                        }),
                                        Some(&format!("quad-add design {}", layer_index)),
                                    );
                                }
                            }
                        });
                    });
                })
        }
        ActiveTool::Sounds(_) => {
            egui::TopBottomPanel::top("top_toolbar_sound_extra")
                .resizable(false)
                .default_height(height)
                .height_range(height..=height)
                .show_inside(ui, |ui| {
                    egui::ScrollArea::horizontal().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // add sound
                            let btn = Button::new("\u{f0fe}");
                            if ui
                                .add(btn)
                                .on_hover_ui(|ui| {
                                    let mut cache = egui_commonmark::CommonMarkCache::default();
                                    egui_commonmark::CommonMarkViewer::new().show(
                                        ui,
                                        &mut cache,
                                        TEXT_ADD_SOUND,
                                    );
                                })
                                .clicked()
                            {
                                if let Some(EditorLayerUnionRef::Design {
                                    group_index,
                                    layer_index,
                                    is_background,
                                    layer: EditorLayer::Sound(layer),
                                    ..
                                }) = pipe.user_data.editor_tab.map.active_layer()
                                {
                                    let index = layer.layer.sounds.len();
                                    pipe.user_data.editor_tab.client.execute(
                                        EditorAction::SoundLayerAddSounds(ActSoundLayerAddSounds {
                                            base: ActSoundLayerAddRemSounds {
                                                is_background,
                                                group_index,
                                                layer_index,
                                                index,
                                                sounds: vec![Sound {
                                                    pos: Default::default(),
                                                    looped: true,
                                                    panning: true,
                                                    time_delay: Default::default(),
                                                    falloff: Default::default(),
                                                    pos_anim: Default::default(),
                                                    pos_anim_offset: Default::default(),
                                                    sound_anim: Default::default(),
                                                    sound_anim_offset: Default::default(),
                                                    shape: SoundShape::Circle {
                                                        radius: uffixed::from_num(10.0),
                                                    },
                                                }],
                                            },
                                        }),
                                        Some(&format!("sound-add design {}", layer_index)),
                                    );
                                }
                            }
                        });
                    });
                })
        }
    };

    ui_state.add_blur_rect(res.response.rect, 0.0);

    super::tune::render(ui, pipe, ui_state);
    super::switch::render(ui, pipe, ui_state);
    super::speedup::render(ui, pipe, ui_state);
    super::tele::render(ui, pipe, ui_state);
}
