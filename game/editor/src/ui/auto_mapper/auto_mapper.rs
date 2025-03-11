use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Add,
};

use base::hash::fmt_hash;
use client_render_base::map::map_buffered::graphic_tile::tile_flags_to_uv;
use client_ui::utils::render_texture_for_ui;
use egui::{
    vec2, Color32, ComboBox, DragValue, Frame, Rect, ScrollArea, Sense, Spacing, Stroke, Window,
};
use egui_file_dialog::{DialogMode, DialogState};
use graphics::handles::{
    canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
};
use map::map::groups::layers::tiles::{rotate_by_plus_90, TileFlags};
use math::math::vector::{ivec2, vec2};
use ui_base::{
    components::clearable_edit_field::clearable_edit_field,
    types::{UiRenderPipe, UiState},
};

use crate::{
    explain::{AUTO_MAPPER_CREATOR_EXPLAIN, AUTO_MAPPER_CREATOR_EXPRESSION_LIST_EXPLAIN},
    tools::{
        tile_layer::auto_mapper::{
            FileDialogTy, TileLayerAutoMapper, TileLayerAutoMapperCheckGroup,
            TileLayerAutoMapperEditorRule, TileLayerAutoMapperOperator,
            TileLayerAutoMapperRuleType, TileLayerAutoMapperRun, TileLayerAutoMapperTile,
            TileLayerAutoMapperTileExpr, TileLayerAutoMapperTileType, TileLayerAutoMapperVisuals,
            TileOffsetNonZero,
        },
        utils::render_checkerboard_ui,
    },
    ui::user_data::UserData,
};

const MIN_TILE_SIZE: f32 = 40.0;

fn render_tile_picker(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    stream_handle: &GraphicsStreamHandle,
    canvas_handle: &GraphicsCanvasHandle,
    tile_size: f32,
    rule_textures: &TileLayerAutoMapperVisuals,
    auto_mapper_selected_tile: &mut Option<u8>,
) {
    const MARGIN: f32 = 2.0;
    ui.label("Tile picker");

    let available_rect = ui.available_rect_before_wrap();
    render_checkerboard_ui(
        ui,
        egui::Rect::from_min_size(
            available_rect.min,
            egui::vec2(
                tile_size * 16.0 + MARGIN * 15.0,
                tile_size * 16.0 + MARGIN * 15.0,
            ),
        ),
        tile_size / 3.0,
    );
    ui.vertical(|ui| {
        for y in 0..16 {
            ui.horizontal(|ui| {
                for x in 0..16 {
                    let tile_index = y * 16 + x;
                    let tile_texture = &rule_textures.tile_textures_pngs[tile_index];

                    let pos_min = ui.available_rect_before_wrap().min;
                    render_texture_for_ui(
                        stream_handle,
                        canvas_handle,
                        tile_texture,
                        ui,
                        ui_state,
                        ui.ctx().screen_rect(),
                        Some(ui.clip_rect()),
                        vec2::new(pos_min.x, pos_min.y) + vec2::new(tile_size, tile_size) / 2.0,
                        vec2::new(tile_size, tile_size),
                        None,
                    );

                    if ui
                        .allocate_exact_size((tile_size, tile_size).into(), Sense::click())
                        .1
                        .clicked()
                    {
                        let new_tile = tile_index as u8;
                        *auto_mapper_selected_tile = match *auto_mapper_selected_tile {
                            Some(tile) if tile == new_tile => None,
                            Some(_) | None => Some(new_tile),
                        };
                    }

                    if auto_mapper_selected_tile.is_some_and(|i| i as usize == tile_index) {
                        const STROKE_SIZE: f32 = 2.0;
                        ui.painter().rect_stroke(
                            egui::Rect::from_min_size(
                                available_rect.min
                                    + egui::vec2(
                                        tile_size * x as f32 + MARGIN * x as f32,
                                        tile_size * y as f32 + MARGIN * y as f32,
                                    ),
                                egui::vec2(tile_size, tile_size),
                            ),
                            0.0,
                            Stroke::new(STROKE_SIZE, Color32::RED),
                            egui::StrokeKind::Inside,
                        );
                    }

                    ui.add_space(MARGIN);
                }
            });
            ui.add_space(MARGIN);
        }
    });
}

fn uv_from_tile_flags(flags: TileFlags) -> (vec2, vec2, vec2, vec2) {
    let (x0, y0, x1, y1, x2, y2, x3, y3) = tile_flags_to_uv(flags);
    (
        vec2::new(x0 as f32, y0 as f32),
        vec2::new(x1 as f32, y1 as f32),
        vec2::new(x2 as f32, y2 as f32),
        vec2::new(x3 as f32, y3 as f32),
    )
}

fn render_grid(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    stream_handle: &GraphicsStreamHandle,
    canvas_handle: &GraphicsCanvasHandle,
    tile_size: f32,
    run_tile: &mut TileLayerAutoMapperTile,
    rule_textures: &TileLayerAutoMapperVisuals,
    auto_mapper_selected_tile: Option<u8>,
    auto_mapper_selected_grid: &mut Option<ivec2>,
) {
    ui.label("Grid");

    const MARGIN: f32 = 1.0;
    let available_rect = ui.available_rect_before_wrap();
    render_checkerboard_ui(
        ui,
        egui::Rect::from_min_size(
            available_rect.min,
            egui::vec2(
                (tile_size + MARGIN) * (run_tile.grid_size * 2 + 1) as f32,
                (tile_size + MARGIN) * (run_tile.grid_size * 2 + 1) as f32,
            ),
        ),
        tile_size / 3.0,
    );
    ui.vertical(|ui| {
        for y in 0..(run_tile.grid_size * 2 + 1) {
            ui.horizontal(|ui| {
                for x in 0..(run_tile.grid_size * 2 + 1) {
                    let grid_x = x as i32 - run_tile.grid_size as i32;
                    let grid_y = y as i32 - run_tile.grid_size as i32;
                    let is_main_tile = grid_x == 0 && grid_y == 0;

                    fn find_tile_in_group_mut(
                        check_tiles: &mut TileLayerAutoMapperCheckGroup,
                        offset: usize,
                        counter: usize,
                    ) -> Option<&mut TileLayerAutoMapperCheckGroup> {
                        if offset == counter {
                            Some(check_tiles)
                        } else if check_tiles.operation.is_some() {
                            find_tile_in_group_mut(
                                check_tiles.operation.as_mut().unwrap().1.as_mut(),
                                offset,
                                counter + 1,
                            )
                        } else {
                            Some(check_tiles)
                        }
                    }
                    fn find_tile_mut(
                        check_tiles: &mut BTreeMap<
                            TileOffsetNonZero,
                            TileLayerAutoMapperCheckGroup,
                        >,
                        x: i32,
                        y: i32,
                        offset: usize,
                    ) -> Option<&mut TileLayerAutoMapperCheckGroup> {
                        let grid_offset = TileOffsetNonZero::new(x, y)?;
                        check_tiles
                            .get_mut(&grid_offset)
                            .and_then(|group| find_tile_in_group_mut(group, offset, 0))
                    }
                    fn find_tile_index_and_flags(
                        check_tiles: &mut BTreeMap<
                            TileOffsetNonZero,
                            TileLayerAutoMapperCheckGroup,
                        >,
                        x: i32,
                        y: i32,
                        offset: usize,
                    ) -> Option<(u8, Option<TileFlags>)> {
                        find_tile_mut(check_tiles, x, y, offset)
                            .map(|t| (t.tile.tile_index, t.tile.tile_flags))
                    }

                    let ((tile_index, tile_flags), found_item) = if is_main_tile {
                        ((run_tile.tile_index, Some(run_tile.tile_flags)), true)
                    } else {
                        let res = find_tile_index_and_flags(
                            &mut run_tile.check_groups,
                            grid_x,
                            grid_y,
                            run_tile.check_tile_offset,
                        );
                        (res.unwrap_or((0, Default::default())), res.is_some())
                    };
                    let tile_flags = tile_flags.unwrap_or_default();
                    let tile_texture = &rule_textures.tile_textures_pngs[tile_index as usize];

                    let pos_min = ui.available_rect_before_wrap().min;
                    render_texture_for_ui(
                        stream_handle,
                        canvas_handle,
                        tile_texture,
                        ui,
                        ui_state,
                        ui.ctx().screen_rect(),
                        Some(ui.clip_rect()),
                        vec2::new(pos_min.x, pos_min.y) + vec2::new(tile_size, tile_size) / 2.0,
                        vec2::new(tile_size, tile_size),
                        Some(uv_from_tile_flags(tile_flags)),
                    );

                    if ui
                        .allocate_exact_size((tile_size, tile_size).into(), Sense::click())
                        .1
                        .clicked()
                    {
                        if let Some(tile) = auto_mapper_selected_tile {
                            if is_main_tile {
                                run_tile.tile_index = tile;
                            } else if let Some(expr_tile) = find_tile_mut(
                                &mut run_tile.check_groups,
                                grid_x,
                                grid_y,
                                run_tile.check_tile_offset,
                            ) {
                                expr_tile.tile.tile_index = tile;
                            } else {
                                let group = run_tile
                                    .check_groups
                                    .entry(TileOffsetNonZero::new(grid_x, grid_y).unwrap())
                                    .or_insert_with(|| TileLayerAutoMapperCheckGroup {
                                        negate: false,
                                        tile: TileLayerAutoMapperTileExpr {
                                            tile_index: tile,
                                            tile_flags: Default::default(),
                                        },
                                        operation: None,
                                    });

                                group.tile.tile_index = tile;
                            }
                        }
                        *auto_mapper_selected_grid = Some(ivec2::new(grid_x, grid_y));
                    }
                    if let Some((color, stroke_width)) =
                        if *auto_mapper_selected_grid == Some(ivec2::new(grid_x, grid_y)) {
                            Some((Color32::RED, MARGIN))
                        } else if found_item {
                            Some((
                                if grid_x == 0 && grid_y == 0 {
                                    Color32::BLUE
                                } else {
                                    Color32::GREEN
                                },
                                MARGIN / 2.0,
                            ))
                        } else {
                            None
                        }
                    {
                        ui.painter().rect_stroke(
                            egui::Rect::from_min_size(
                                available_rect.min
                                    + egui::vec2(
                                        (tile_size + MARGIN) * x as f32,
                                        (tile_size + MARGIN) * y as f32,
                                    ),
                                egui::vec2(tile_size, tile_size),
                            ),
                            0.0,
                            Stroke::new(stroke_width, color),
                            egui::StrokeKind::Inside,
                        );
                    }

                    ui.add_space(MARGIN);
                }
            });
            ui.add_space(MARGIN);
        }
    });
}

fn render_op_list(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    stream_handle: &GraphicsStreamHandle,
    canvas_handle: &GraphicsCanvasHandle,
    rule_textures: &TileLayerAutoMapperVisuals,
    tile_index: u8,
    tile_flags: TileFlags,
    negate: &mut bool,
    operation: &mut Option<(
        TileLayerAutoMapperOperator,
        Box<TileLayerAutoMapperCheckGroup>,
    )>,
    selected_operation_index: &mut usize,
    counter: usize,
) {
    Frame::default()
        .fill(Color32::from_black_alpha(50))
        .inner_margin(5.0)
        .corner_radius(5.0)
        .show(ui, |ui| {
            ui.label("Negate the check");
            ui.checkbox(negate, "");

            ui.add_space(3.0);

            const ROW_HEIGHT: f32 = 20.0;
            let pos_min = ui.available_rect_before_wrap().min;
            render_texture_for_ui(
                stream_handle,
                canvas_handle,
                &rule_textures.tile_textures_pngs[tile_index as usize],
                ui,
                ui_state,
                ui.ctx().screen_rect(),
                Some(ui.clip_rect()),
                vec2::new(pos_min.x, pos_min.y) + vec2::new(ROW_HEIGHT, ROW_HEIGHT) / 2.0,
                vec2::new(ROW_HEIGHT, ROW_HEIGHT),
                Some(uv_from_tile_flags(tile_flags)),
            );

            if ui
                .allocate_exact_size((ROW_HEIGHT, ROW_HEIGHT).into(), Sense::click())
                .1
                .clicked()
            {
                *selected_operation_index = counter;
            }

            if *selected_operation_index == counter {
                ui.painter().rect_stroke(
                    Rect::from_min_size(pos_min, egui::vec2(ROW_HEIGHT, ROW_HEIGHT)),
                    0.0,
                    Stroke::new(2.0, Color32::RED),
                    egui::StrokeKind::Inside,
                );
            }
        });

    if let Some((operator, next)) = operation {
        ui.add_space(3.0);
        ComboBox::new(
            format!("tile-auto-mapper-creator-operator-ty-{}", counter),
            "",
        )
        .selected_text(match operator {
            TileLayerAutoMapperOperator::Or => "OR",
            TileLayerAutoMapperOperator::And => "AND",
        })
        .show_ui(ui, |ui| {
            if ui.button("OR").clicked() {
                *operator = TileLayerAutoMapperOperator::Or;
            }
            if ui.button("AND").clicked() {
                *operator = TileLayerAutoMapperOperator::And;
            }
        });
        ui.add_space(3.0);

        render_op_list(
            ui,
            ui_state,
            stream_handle,
            canvas_handle,
            rule_textures,
            next.tile.tile_index,
            next.tile.tile_flags.unwrap_or_default(),
            &mut next.negate,
            &mut next.operation,
            selected_operation_index,
            counter + 1,
        );
    }
}

pub fn render(pipe: &mut UiRenderPipe<UserData>, ui: &mut egui::Ui, ui_state: &mut UiState) {
    let auto_mapper = &mut *pipe.user_data.auto_mapper;

    for tab in pipe.user_data.editor_tabs.tabs.values() {
        for image_array in &tab.map.resources.image_arrays {
            auto_mapper.try_load(
                &format!(
                    "{}_{}",
                    image_array.def.name.as_str(),
                    fmt_hash(&image_array.def.meta.blake3_hash)
                ),
                image_array.def.name.as_str(),
                &image_array.def.meta.blake3_hash,
                &image_array.user.file,
            );
        }
    }

    let window_res = Window::new("Auto-mapper-rule-creator").show(ui.ctx(), |ui| {
        ui.set_min_width(MIN_TILE_SIZE * 9.0 + MIN_TILE_SIZE * 16.0 + 450.0);
        ui.set_min_height(MIN_TILE_SIZE * 16.0 + 250.0);

        ui.label("Allows you to create new rules for tile sets \u{f05a}")
            .on_hover_ui(|ui| {
                let mut cache = egui_commonmark::CommonMarkCache::default();
                egui_commonmark::CommonMarkViewer::new().show(
                    ui,
                    &mut cache,
                    AUTO_MAPPER_CREATOR_EXPLAIN,
                );
            });

        ui.horizontal(|ui| {
            egui::ComboBox::new(
                "auto-mapper-rules-selector",
                "Select resource to edit rules for",
            )
            .selected_text(match &auto_mapper.active_resource {
                Some(rule) => auto_mapper
                    .resources
                    .get(rule)
                    .map(|_| rule.as_str())
                    .unwrap_or("Resource not found."),
                None => "None...",
            })
            .show_ui(ui, |ui| {
                ui.vertical(|ui| {
                    let rules: BTreeMap<_, _> = auto_mapper.resources.iter().collect();
                    for (r, _) in rules {
                        if ui.add(egui::Button::new(r)).clicked() {
                            auto_mapper.active_resource = Some(r.clone());
                        }
                    }
                })
            });

            if ui.button("\u{f07c}").clicked() {
                auto_mapper.file_dialog.pick_file();
                auto_mapper.file_dialog_ty = FileDialogTy::LoadResource;
            }
            if auto_mapper.file_dialog.state() == DialogState::Open {
                let mode = auto_mapper.file_dialog.mode();
                if let Some(selected) = auto_mapper
                    .file_dialog
                    .update(ui.ctx())
                    .picked()
                    .map(|path| path.to_path_buf())
                {
                    match mode {
                        DialogMode::PickFile => {
                            match auto_mapper.file_dialog_ty {
                                FileDialogTy::LoadResource => {
                                    // add rule to loading tasks
                                    auto_mapper.load_resource_then_rule(selected.as_ref());
                                }
                                FileDialogTy::ImportRule => {
                                    // load and import the given rule file
                                    auto_mapper.import_rule_for_resource(
                                        &auto_mapper.active_resource.clone().unwrap_or_default(),
                                        selected.as_ref(),
                                    );
                                }
                            }
                        }
                        _ => panic!("this was not implemented."),
                    }
                }
            }
        });

        // render rule
        if let Some((resource_name, resource)) = auto_mapper
            .active_resource
            .as_ref()
            .and_then(|i| auto_mapper.resources.get_mut(i).map(|res| (i, res)))
        {
            ui.horizontal(|ui| {
                clearable_edit_field(ui, &mut auto_mapper.new_rule_name, Some(150.0), None);
                if ui.button("\u{f055} Add new rule").clicked() {
                    resource.rules.insert(
                        auto_mapper.new_rule_name.clone(),
                        TileLayerAutoMapperRuleType::EditorRule(
                            TileLayerAutoMapperEditorRule::default(),
                        ),
                    );
                }

                ui.add_space(10.0);

                if ui.button("\u{f07c} Import rule").clicked() {
                    auto_mapper.file_dialog.pick_file();
                    auto_mapper.file_dialog_ty = FileDialogTy::ImportRule;
                }
            });

            egui::ComboBox::new("auto-mapper-a-rule-selector", "Select rule to edit")
                .selected_text(match &auto_mapper.active_rule {
                    Some(rule) => resource
                        .rules
                        .get(rule)
                        .map(|_| rule.as_str())
                        .unwrap_or("Rule not found."),
                    None => "None...",
                })
                .show_ui(ui, |ui| {
                    ui.vertical(|ui| {
                        let rules: BTreeSet<_> = resource
                            .rules
                            .iter()
                            .filter_map(|(name, rule)| {
                                matches!(rule, TileLayerAutoMapperRuleType::EditorRule(_))
                                    .then_some(name)
                            })
                            .collect();
                        for r in rules {
                            if ui.add(egui::Button::new(r)).clicked() {
                                auto_mapper.active_rule = Some(r.clone());
                            }
                        }
                    })
                });

            let Some((rule_name, TileLayerAutoMapperRuleType::EditorRule(rule))) = auto_mapper
                .active_rule
                .as_ref()
                .and_then(|r| resource.rules.get_mut(r).map(|rule| (r, rule)))
            else {
                ui.label("Select a rule to continue..");
                return;
            };

            rule.runs.iter_mut().for_each(|run| {
                run.tiles.iter_mut().for_each(|tile| {
                    let min = tile
                        .check_groups
                        .keys()
                        .map(|g| {
                            let g = g.get();
                            g.x.min(g.y)
                        })
                        .min()
                        .unwrap_or(0);
                    let max = tile
                        .check_groups
                        .keys()
                        .map(|g| {
                            let g = g.get();
                            g.x.max(g.y)
                        })
                        .max()
                        .unwrap_or(0)
                        .max(min.abs());
                    tile.grid_size = tile.grid_size.max(max as usize + 1).max(3);
                });
            });

            let mut remove_rule = false;
            ui.horizontal(|ui| {
                if ui.button("\u{f0c7} Save rule").clicked() {
                    TileLayerAutoMapper::save(
                        &auto_mapper.io,
                        rule_name.clone(),
                        resource_name.to_string(),
                        rule.clone(),
                    );
                }

                if ui.button("\u{f1f8} Delete rule").clicked() {
                    remove_rule = true;
                }
            });
            if remove_rule {
                resource.rules.remove(rule_name);
                return;
            }

            ui.horizontal(|ui| {
                ui.label("Run:")
                    .on_hover_text("The run, that runs the rules for all tiles");

                // prev run
                if ui.button("\u{f060}").clicked() {
                    rule.active_run = rule.active_run.saturating_sub(1);
                }

                ui.label(format!("{} / {}", rule.active_run + 1, rule.runs.len()));

                // next run
                if ui.button("\u{f061}").clicked() {
                    rule.active_run = rule.active_run.add(1).clamp(0, rule.runs.len() - 1);
                }

                ui.add_space(10.0);

                // new run
                if ui.button("\u{f0fe}").clicked() {
                    rule.runs.push(TileLayerAutoMapperRun {
                        tiles: Default::default(),
                        active_tile: Default::default(),
                    });
                }
                // remove cur run
                if ui.button("\u{f2ed}").clicked() && rule.runs.len() > 1 {
                    rule.runs.remove(rule.active_run);
                    rule.active_run = rule.active_run.saturating_sub(1);
                }
            });
            if let Some(run) = rule.runs.get_mut(rule.active_run) {
                ui.horizontal(|ui| {
                    ui.label("Tile:").on_hover_text("The tile to spawn/change");

                    // prev tile
                    if ui.button("\u{f060}").clicked() {
                        run.active_tile = run.active_tile.map(|t| t.saturating_sub(1));
                    }

                    ui.label(format!(
                        "{} / {}",
                        run.active_tile
                            .map(|t| format!("{}", t + 1))
                            .unwrap_or_else(|| "None".to_string()),
                        run.tiles.len()
                    ));

                    // next tile
                    if ui.button("\u{f061}").clicked() {
                        run.active_tile = match run.active_tile {
                            Some(active_tile) => {
                                Some((active_tile + 1).clamp(0, run.tiles.len().saturating_sub(1)))
                            }
                            None => (!run.tiles.is_empty()).then_some(0),
                        };
                    }

                    ui.add_space(10.0);

                    // new tile
                    if ui.button("\u{f0fe}").clicked() {
                        run.tiles.push(TileLayerAutoMapperTile {
                            tile_index: 0,
                            tile_flags: Default::default(),
                            tile_type: TileLayerAutoMapperTileType::Default,
                            randomness: None,
                            check_groups: Default::default(),

                            grid_size: 3,
                            check_tile_offset: 0,
                        });
                    }
                    // remove cur tile
                    if ui.button("\u{f2ed}").clicked() && run.tiles.len() > 1 {
                        if let Some(t) = run.active_tile {
                            run.tiles.remove(t);
                        }
                        run.active_tile = run.active_tile.and_then(|t| t.checked_sub(1));
                    }
                });

                if let Some(run_tile) = run.active_tile.and_then(|t| run.tiles.get_mut(t)) {
                    // render current run & tile
                    let available_rect = ui.available_rect_before_wrap();
                    let size = available_rect.height().min(available_rect.width()) / 2.0 - 10.0;

                    let tile_size = (size / 16.0).max(MIN_TILE_SIZE);

                    let spacing = ui.spacing_mut();
                    spacing.item_spacing = vec2(0.0, 0.0);
                    spacing.interact_size = vec2(0.0, 0.0);

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            render_tile_picker(
                                ui,
                                ui_state,
                                pipe.user_data.stream_handle,
                                pipe.user_data.canvas_handle,
                                tile_size,
                                &resource.visuals,
                                &mut auto_mapper.selected_tile,
                            );
                        });

                        ui.add_space(10.0);

                        ui.vertical(|ui| {
                            render_grid(
                                ui,
                                ui_state,
                                pipe.user_data.stream_handle,
                                pipe.user_data.canvas_handle,
                                tile_size,
                                run_tile,
                                &resource.visuals,
                                auto_mapper.selected_tile,
                                &mut auto_mapper.selected_grid,
                            );
                        });

                        ui.add_space(10.0);

                        ui.vertical(|ui| {
                            ui.style_mut().spacing.item_spacing = Spacing::default().item_spacing;

                            #[derive(Debug)]
                            enum TileFlagsMode<'a> {
                                Flags(&'a mut TileFlags),
                                Optional(&'a mut Option<TileFlags>),
                            }
                            if let Some((
                                tile_index,
                                mut tile_flag,
                                spawn_mode,
                                randomness,
                                negate,
                                operation,
                            )) = auto_mapper.selected_grid.and_then(|selected_grid| {
                                if selected_grid.x == 0 && selected_grid.y == 0 {
                                    Some((
                                        run_tile.tile_index,
                                        TileFlagsMode::Flags(&mut run_tile.tile_flags),
                                        Some(&mut run_tile.tile_type),
                                        Some(&mut run_tile.randomness),
                                        None,
                                        None,
                                    ))
                                } else if let Some(check_group) = run_tile.check_groups.get_mut(
                                    &TileOffsetNonZero::new(selected_grid.x, selected_grid.y)
                                        .unwrap(),
                                ) {
                                    Some((
                                        check_group.tile.tile_index,
                                        TileFlagsMode::Optional(&mut check_group.tile.tile_flags),
                                        None,
                                        None,
                                        Some(&mut check_group.negate),
                                        Some(&mut check_group.operation),
                                    ))
                                } else {
                                    None
                                }
                            }) {
                                let tile_flags_or_default = match &tile_flag {
                                    TileFlagsMode::Flags(flags) => **flags,
                                    TileFlagsMode::Optional(tile_flags) => {
                                        tile_flags.unwrap_or_default()
                                    }
                                };
                                if let TileFlagsMode::Optional(flags) = &mut tile_flag {
                                    let mut ignore_flags = flags.is_none();
                                    ui.label("Ignore tile flags");
                                    if ui.checkbox(&mut ignore_flags, "").changed() {
                                        if ignore_flags {
                                            **flags = None;
                                        } else {
                                            **flags = Some(TileFlags::empty());
                                        }
                                    }
                                }
                                match tile_flag {
                                    TileFlagsMode::Flags(flags)
                                    | TileFlagsMode::Optional(Some(flags)) => {
                                        ui.horizontal(|ui| {
                                            // mirror y
                                            if ui.button("\u{f07d}").clicked() {
                                                flags.toggle(TileFlags::YFLIP);
                                            }
                                            // mirror x
                                            if ui.button("\u{f07e}").clicked() {
                                                flags.toggle(TileFlags::XFLIP);
                                            }
                                            // rotate -90°
                                            if ui.button("\u{f2ea}").clicked() {
                                                rotate_by_plus_90(flags);
                                                rotate_by_plus_90(flags);
                                                rotate_by_plus_90(flags);
                                            }
                                            // rotate +90°
                                            if ui.button("\u{f2f9}").clicked() {
                                                rotate_by_plus_90(flags);
                                            }
                                        });
                                    }
                                    TileFlagsMode::Optional(_) => {
                                        // ignore
                                    }
                                }

                                if let Some(spawn_mode) = spawn_mode {
                                    ui.label("Spawn mode");
                                    ComboBox::new("auto-mapper-creator-tile-spawn-mode", "")
                                        .selected_text(match spawn_mode {
                                            TileLayerAutoMapperTileType::Default => "default",
                                            TileLayerAutoMapperTileType::Spawnable => "spawnable",
                                            TileLayerAutoMapperTileType::SpawnOnly => "spawn only",
                                        })
                                        .show_ui(ui, |ui| {
                                            if ui.button("default").clicked() {
                                                *spawn_mode = TileLayerAutoMapperTileType::Default;
                                            }
                                            if ui.button("spawnable").clicked() {
                                                *spawn_mode =
                                                    TileLayerAutoMapperTileType::Spawnable;
                                            }
                                            if ui.button("spawn only").clicked() {
                                                *spawn_mode =
                                                    TileLayerAutoMapperTileType::SpawnOnly;
                                            }
                                        });
                                }

                                if let Some(randomness) = randomness {
                                    ui.label("Probability (0 = off)");
                                    let mut rn = randomness.map(|r| r.get()).unwrap_or_default()
                                        as f64
                                        / u32::MAX as f64;
                                    ui.add(DragValue::new(&mut rn).range(0.0..=1.0).speed(0.1));

                                    let rn = (rn * u32::MAX as f64) as u32;
                                    *randomness = (rn != 0).then(|| rn.try_into().unwrap());
                                }

                                if let Some((operation, negate)) = operation.zip(negate) {
                                    ui.label("Current operation list \u{f05a}")
                                        .on_hover_ui(|ui| {
                                            let mut cache =
                                                egui_commonmark::CommonMarkCache::default();
                                            egui_commonmark::CommonMarkViewer::new().show(
                                                ui,
                                                &mut cache,
                                                AUTO_MAPPER_CREATOR_EXPRESSION_LIST_EXPLAIN,
                                            );
                                        });
                                    ui.style_mut().spacing.item_spacing = Default::default();

                                    ScrollArea::vertical().show(ui, |ui| {
                                        render_op_list(
                                            ui,
                                            ui_state,
                                            pipe.user_data.stream_handle,
                                            pipe.user_data.canvas_handle,
                                            &resource.visuals,
                                            tile_index,
                                            tile_flags_or_default,
                                            negate,
                                            operation,
                                            &mut run_tile.check_tile_offset,
                                            0,
                                        );
                                    });

                                    ui.add_space(3.0);
                                    if ui.button("Add operation").clicked() {
                                        fn add_op(
                                            operation: &mut Option<(
                                                TileLayerAutoMapperOperator,
                                                Box<TileLayerAutoMapperCheckGroup>,
                                            )>,
                                        ) {
                                            match operation {
                                                Some((_, op)) => add_op(&mut op.operation),
                                                None => {
                                                    *operation = Some((
                                                        TileLayerAutoMapperOperator::Or,
                                                        Box::new(TileLayerAutoMapperCheckGroup {
                                                            negate: false,
                                                            tile: TileLayerAutoMapperTileExpr {
                                                                tile_index: 1,
                                                                tile_flags: None,
                                                            },
                                                            operation: None,
                                                        }),
                                                    ))
                                                }
                                            }
                                        }

                                        add_op(operation);
                                    }

                                    if let Some(selected_grid) = auto_mapper
                                        .selected_grid
                                        .and_then(|g| TileOffsetNonZero::new(g.x, g.y))
                                    {
                                        ui.add_space(10.0);
                                        if ui.button("Remove this grid entry").clicked() {
                                            run_tile.check_groups.remove(&selected_grid);
                                        }
                                    }
                                }
                            }
                        });
                    });
                }
            }
        } else {
            ui.label("Select a resource to continue...");
        }
    });

    if let Some(window_res) = &window_res {
        auto_mapper.window_rect = window_res.response.rect;
    }

    *pipe.user_data.pointer_is_used |= if let Some(window_res) = &window_res {
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
