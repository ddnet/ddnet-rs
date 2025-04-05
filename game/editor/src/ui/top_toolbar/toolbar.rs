use egui::Button;
use map::map::groups::layers::design::{Quad, Sound, SoundShape};
use math::math::vector::{ffixed, nffixed, uffixed, vec2, vec2_base, vec4_base};
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
    hotkeys::{
        EditorHotkeyEvent, EditorHotkeyEventSharedTool, EditorHotkeyEventTileBrush,
        EditorHotkeyEventTileTool, EditorHotkeyEventTools,
    },
    map::{EditorLayer, EditorLayerUnionRef, EditorMapInterface},
    tools::tool::{ActiveTool, ActiveToolQuads, ActiveToolSounds, ActiveToolTiles},
    ui::user_data::UserDataWithTab,
    utils::ui_pos_to_world_pos,
};

use super::tile_mirror::{
    mirror_layer_tiles_x, mirror_layer_tiles_y, mirror_tiles_x, mirror_tiles_y,
    rotate_layer_tiles_plus_90, rotate_tile_flags_plus_90, rotate_tiles_plus_90,
};

fn render_toolbar_tiles(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserDataWithTab>) {
    let tools = &mut pipe.user_data.tools;
    let ActiveTool::Tiles(tool) = tools.active_tool else {
        return;
    };

    let binds = &*pipe.user_data.hotkeys;
    let per_ev = &mut *pipe.user_data.cached_binds_per_event;
    // mirror y
    let btn = Button::new("\u{f07d}");
    let by_hotkey = pipe
        .user_data
        .cur_hotkey_events
        .remove(&EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
            EditorHotkeyEventTileTool::Brush(EditorHotkeyEventTileBrush::FlipY),
        )));
    if ui
        .add(btn)
        .on_hover_ui(|ui| {
            let mut cache = egui_commonmark::CommonMarkCache::default();
            egui_commonmark::CommonMarkViewer::new().show(
                ui,
                &mut cache,
                &format!(
                    "{}\n\nHotkey: `{}`",
                    TEXT_TILE_BRUSH_MIRROR,
                    binds.fmt_ev_bind(
                        per_ev,
                        &EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                            EditorHotkeyEventTileTool::Brush(EditorHotkeyEventTileBrush::FlipY)
                        )),
                    )
                ),
            );
        })
        .clicked()
        || by_hotkey
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
    let by_hotkey = pipe
        .user_data
        .cur_hotkey_events
        .remove(&EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
            EditorHotkeyEventTileTool::Brush(EditorHotkeyEventTileBrush::FlipX),
        )));
    if ui
        .add(btn)
        .on_hover_ui(|ui| {
            let mut cache = egui_commonmark::CommonMarkCache::default();
            egui_commonmark::CommonMarkViewer::new().show(
                ui,
                &mut cache,
                &format!(
                    "{}\n\nHotkey: `{}`",
                    TEXT_TILE_BRUSH_MIRROR,
                    binds.fmt_ev_bind(
                        per_ev,
                        &EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                            EditorHotkeyEventTileTool::Brush(EditorHotkeyEventTileBrush::FlipX)
                        )),
                    )
                ),
            );
        })
        .clicked()
        || by_hotkey
    {
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
            let by_hotkey = pipe
                .user_data
                .cur_hotkey_events
                .remove(&EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                    EditorHotkeyEventTileTool::Brush(EditorHotkeyEventTileBrush::RotMinus90),
                )));
            if ui
                .add(btn)
                .on_hover_ui(|ui| {
                    let mut cache = egui_commonmark::CommonMarkCache::default();
                    egui_commonmark::CommonMarkViewer::new().show(
                        ui,
                        &mut cache,
                        &format!(
                            "Hotkey: `{}`",
                            binds.fmt_ev_bind(
                                per_ev,
                                &EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                                    EditorHotkeyEventTileTool::Brush(
                                        EditorHotkeyEventTileBrush::RotMinus90
                                    )
                                )),
                            )
                        ),
                    );
                })
                .clicked()
                || by_hotkey
            {
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
            let by_hotkey = pipe
                .user_data
                .cur_hotkey_events
                .remove(&EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                    EditorHotkeyEventTileTool::Brush(EditorHotkeyEventTileBrush::RotPlus90),
                )));
            if ui
                .add(btn)
                .on_hover_ui(|ui| {
                    let mut cache = egui_commonmark::CommonMarkCache::default();
                    egui_commonmark::CommonMarkViewer::new().show(
                        ui,
                        &mut cache,
                        &format!(
                            "Hotkey: `{}`",
                            binds.fmt_ev_bind(
                                per_ev,
                                &EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                                    EditorHotkeyEventTileTool::Brush(
                                        EditorHotkeyEventTileBrush::RotPlus90
                                    )
                                )),
                            )
                        ),
                    );
                })
                .clicked()
                || by_hotkey
            {
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
            let by_hotkey = pipe
                .user_data
                .cur_hotkey_events
                .remove(&EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                    EditorHotkeyEventTileTool::Brush(
                        EditorHotkeyEventTileBrush::RotIndividualTilePlus90,
                    ),
                )));
            if ui
                .add(btn)
                .on_hover_ui(|ui| {
                    let mut cache = egui_commonmark::CommonMarkCache::default();
                    egui_commonmark::CommonMarkViewer::new().show(
                        ui,
                        &mut cache,
                        &format!(
                            "Hotkey: `{}`",
                            binds.fmt_ev_bind(
                                per_ev,
                                &EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                                    EditorHotkeyEventTileTool::Brush(
                                        EditorHotkeyEventTileBrush::RotIndividualTilePlus90
                                    )
                                )),
                            )
                        ),
                    );
                })
                .clicked()
                || by_hotkey
            {
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
            if let Some(layer) = pipe.user_data.editor_tab.map.active_layer() {
                // rotate inner tiles (flags) by 90°
                let btn = Button::new("\u{e4f6}");
                let by_hotkey = pipe
                    .user_data
                    .cur_hotkey_events
                    .remove(&EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Tile(
                        EditorHotkeyEventTileTool::Brush(
                            EditorHotkeyEventTileBrush::RotIndividualTilePlus90,
                        ),
                    )));
                if ui.add(btn).clicked() || by_hotkey {
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
}

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

    let ui_pos = ui.ctx().screen_rect().center();

    let tools = &mut pipe.user_data.tools;
    let res =
        match &tools.active_tool {
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
                                    render_toolbar_tiles(ui, pipe);
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
                                let binds = &*pipe.user_data.hotkeys;
                                let per_ev = &mut *pipe.user_data.cached_binds_per_event;
                                // add quad
                                let btn = Button::new("\u{f0fe}");
                                let by_hotkey = pipe.user_data.cur_hotkey_events.remove(
                                    &EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Shared(
                                        EditorHotkeyEventSharedTool::AddQuadOrSound,
                                    )),
                                );
                                if ui
                                    .add(btn)
                                    .on_hover_ui(|ui| {
                                        let mut cache = egui_commonmark::CommonMarkCache::default();
                                        egui_commonmark::CommonMarkViewer::new().show(
                                            ui,
                                            &mut cache,
                                            &format!(
                                            "{}\n\nHotkey: `{}`",
                                            TEXT_ADD_QUAD,
                                            binds.fmt_ev_bind(
                                                per_ev,
                                                &EditorHotkeyEvent::Tools(
                                                    EditorHotkeyEventTools::Shared(
                                                        EditorHotkeyEventSharedTool::AddQuadOrSound,
                                                    )
                                                ),
                                            )
                                        ),
                                        );
                                    })
                                    .clicked()
                                    || by_hotkey
                                {
                                    let map = &pipe.user_data.editor_tab.map;
                                    if let Some(EditorLayerUnionRef::Design {
                                        group_index,
                                        layer_index,
                                        is_background,
                                        layer: EditorLayer::Quad(layer),
                                        group,
                                    }) = map.active_layer()
                                    {
                                        let pos = ui_pos_to_world_pos(
                                            pipe.user_data.canvas_handle,
                                            &ui.ctx().screen_rect(),
                                            map.groups.user.zoom,
                                            vec2::new(ui_pos.x, ui_pos.y),
                                            map.groups.user.pos.x,
                                            map.groups.user.pos.y,
                                            group.attr.offset.x.to_num(),
                                            group.attr.offset.y.to_num(),
                                            group.attr.parallax.x.to_num(),
                                            group.attr.parallax.y.to_num(),
                                            map.groups.user.parallax_aware_zoom,
                                        );
                                        let index = layer.layer.quads.len();
                                        pipe.user_data.editor_tab.client.execute(
                                            EditorAction::QuadLayerAddQuads(ActQuadLayerAddQuads {
                                                base: ActQuadLayerAddRemQuads {
                                                    is_background,
                                                    group_index,
                                                    layer_index,
                                                    index,
                                                    quads: vec![Quad {
                                                        points: [
                                                            vec2_base::new(
                                                                ffixed::from_num(pos.x - 10.0),
                                                                ffixed::from_num(pos.y - 10.0),
                                                            ),
                                                            vec2_base::new(
                                                                ffixed::from_num(pos.x + 10.0),
                                                                ffixed::from_num(pos.y - 10.0),
                                                            ),
                                                            vec2_base::new(
                                                                ffixed::from_num(pos.x - 10.0),
                                                                ffixed::from_num(pos.y + 10.0),
                                                            ),
                                                            vec2_base::new(
                                                                ffixed::from_num(pos.x + 10.0),
                                                                ffixed::from_num(pos.y + 10.0),
                                                            ),
                                                            vec2_base::new(
                                                                ffixed::from_num(pos.x),
                                                                ffixed::from_num(pos.y),
                                                            ),
                                                        ],
                                                        colors: [
                                                            vec4_base::new(
                                                                nffixed::from_num(1.0),
                                                                nffixed::from_num(1.0),
                                                                nffixed::from_num(1.0),
                                                                nffixed::from_num(1.0),
                                                            ),
                                                            vec4_base::new(
                                                                nffixed::from_num(1.0),
                                                                nffixed::from_num(1.0),
                                                                nffixed::from_num(1.0),
                                                                nffixed::from_num(1.0),
                                                            ),
                                                            vec4_base::new(
                                                                nffixed::from_num(1.0),
                                                                nffixed::from_num(1.0),
                                                                nffixed::from_num(1.0),
                                                                nffixed::from_num(1.0),
                                                            ),
                                                            vec4_base::new(
                                                                nffixed::from_num(1.0),
                                                                nffixed::from_num(1.0),
                                                                nffixed::from_num(1.0),
                                                                nffixed::from_num(1.0),
                                                            ),
                                                        ],
                                                        tex_coords: [
                                                            vec2_base::new(
                                                                ffixed::from_num(0.0),
                                                                ffixed::from_num(0.0),
                                                            ),
                                                            vec2_base::new(
                                                                ffixed::from_num(1.0),
                                                                ffixed::from_num(0.0),
                                                            ),
                                                            vec2_base::new(
                                                                ffixed::from_num(0.0),
                                                                ffixed::from_num(1.0),
                                                            ),
                                                            vec2_base::new(
                                                                ffixed::from_num(1.0),
                                                                ffixed::from_num(1.0),
                                                            ),
                                                        ],
                                                        ..Default::default()
                                                    }],
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
                                let binds = &*pipe.user_data.hotkeys;
                                let per_ev = &mut *pipe.user_data.cached_binds_per_event;
                                // add sound
                                let btn = Button::new("\u{f0fe}");
                                let by_hotkey = pipe.user_data.cur_hotkey_events.remove(
                                    &EditorHotkeyEvent::Tools(EditorHotkeyEventTools::Shared(
                                        EditorHotkeyEventSharedTool::AddQuadOrSound,
                                    )),
                                );
                                if ui
                                    .add(btn)
                                    .on_hover_ui(|ui| {
                                        let mut cache = egui_commonmark::CommonMarkCache::default();
                                        egui_commonmark::CommonMarkViewer::new().show(
                                            ui,
                                            &mut cache,
                                            &format!(
                                            "{}\n\nHotkey: `{}`",
                                            TEXT_ADD_SOUND,
                                            binds.fmt_ev_bind(
                                                per_ev,
                                                &EditorHotkeyEvent::Tools(
                                                    EditorHotkeyEventTools::Shared(
                                                        EditorHotkeyEventSharedTool::AddQuadOrSound,
                                                    )
                                                ),
                                            )
                                        ),
                                        );
                                    })
                                    .clicked()
                                    || by_hotkey
                                {
                                    let map = &pipe.user_data.editor_tab.map;
                                    if let Some(EditorLayerUnionRef::Design {
                                        group_index,
                                        layer_index,
                                        is_background,
                                        layer: EditorLayer::Sound(layer),
                                        group,
                                    }) = map.active_layer()
                                    {
                                        let pos = ui_pos_to_world_pos(
                                            pipe.user_data.canvas_handle,
                                            &ui.ctx().screen_rect(),
                                            map.groups.user.zoom,
                                            vec2::new(ui_pos.x, ui_pos.y),
                                            map.groups.user.pos.x,
                                            map.groups.user.pos.y,
                                            group.attr.offset.x.to_num(),
                                            group.attr.offset.y.to_num(),
                                            group.attr.parallax.x.to_num(),
                                            group.attr.parallax.y.to_num(),
                                            map.groups.user.parallax_aware_zoom,
                                        );
                                        let index = layer.layer.sounds.len();
                                        pipe.user_data.editor_tab.client.execute(
                                            EditorAction::SoundLayerAddSounds(
                                                ActSoundLayerAddSounds {
                                                    base: ActSoundLayerAddRemSounds {
                                                        is_background,
                                                        group_index,
                                                        layer_index,
                                                        index,
                                                        sounds: vec![Sound {
                                                            pos: vec2_base::new(
                                                                ffixed::from_num(pos.x),
                                                                ffixed::from_num(pos.y),
                                                            ),
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
                                                },
                                            ),
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
