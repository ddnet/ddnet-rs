use std::{collections::HashMap, num::NonZeroU32, path::Path, sync::Arc};

use anyhow::anyhow;
use base::hash::{fmt_hash, Hash};
use base_io::{io::IoFileSys, runtime::IoRuntimeTask};
use egui::{vec2, ColorImage, Rect, TextBuffer, TextureHandle};
use egui_file_dialog::FileDialog;
use image_utils::utils::texture_2d_to_3d;
use map::map::groups::layers::tiles::{TileBase, TileFlags};
use math::math::vector::{ivec2, vec2_base};
use rand::SeedableRng;
use serde::{Deserialize, Serialize};

use crate::{
    actions::actions::{ActTileLayerReplTilesBase, ActTileLayerReplaceTiles, EditorAction},
    client::EditorClient,
    fs::read_file_editor,
    map::{EditorLayer, EditorLayerUnionRef, EditorMap, EditorMapInterface},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum TileLayerAutoMapperTileType {
    None,
    /// can overwrite existing tiles
    Default,
    /// can spawn new tiles (even if there was none before)
    Spawnable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TileLayerAutoMapperCheckOrTileType {
    /// index in range of 0..255, note that index == 0 will fail on map corners, so disable the check if you want to have no check
    EqualsIndex = 0,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TileLayerAutoMapperCheckAndTileType {
    /// -1 = none allowed(for map corners), 0-255 = check if index does not equal
    NotEqualsIndex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileLayerAutoMapperCheckTile<I, T> {
    pub index: I,
    pub tile_flag: TileFlags,

    pub check_type: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileLayerAutoMapperOrTiles {
    /// relative offset towards the current tile
    /// (0, 0) = cur tile
    pub offset: ivec2,
    /// either of these must fit the required check
    pub tiles: Vec<TileLayerAutoMapperCheckTile<u8, TileLayerAutoMapperCheckOrTileType>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TileLayerAutoMapperAndTile {
    /// relative offset towards the current tile
    /// (0, 0) = cur tile
    pub offset: ivec2,
    /// all of these must fit the required check (usually with negation checks, a.k.a. index is not equal to)
    pub tiles: Vec<TileLayerAutoMapperCheckTile<i32, TileLayerAutoMapperCheckAndTileType>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TileLayerAutoMapperTile {
    pub tile_index: u8,
    pub tile_flags: TileFlags,

    pub auto_map_tile_type: TileLayerAutoMapperTileType,
    /// how often should this tile appear
    pub randomness: Option<NonZeroU32>, // 0 = always

    /// tiles where at least one of the tiles/checks must exist (like a logical `OR`)
    pub or_check_tiles: Vec<TileLayerAutoMapperOrTiles>,
    /// tiles where all tiles/checks must exist (like a logical `AND`)
    pub and_check_tiles: Vec<TileLayerAutoMapperAndTile>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TileLayerAutoMapperRun {
    pub tiles: Vec<TileLayerAutoMapperTile>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TileLayerAutoMapperRuleBase<B> {
    pub runs: Vec<TileLayerAutoMapperRun>,
    pub name: String,

    #[serde(skip)]
    pub active_run: usize,

    pub user: B,
}

pub type TileLayerAutoMapperRule = TileLayerAutoMapperRuleBase<()>;

impl From<TileLayerAutoMapperRuleBase<TileLayerAutoMapperVisuals>> for TileLayerAutoMapperRule {
    fn from(val: TileLayerAutoMapperRuleBase<TileLayerAutoMapperVisuals>) -> Self {
        TileLayerAutoMapperRule {
            runs: val.runs,
            name: val.name,
            active_run: val.active_run,
            user: (),
        }
    }
}

impl<B> TileLayerAutoMapperRuleBase<B> {
    pub fn new(user: B, name: String) -> Self {
        Self {
            runs: vec![TileLayerAutoMapperRun {
                tiles: Default::default(),
            }],
            name,
            active_run: 0,
            user,
        }
    }

    pub fn run(&self, map: &EditorMap, client: &mut EditorClient) -> anyhow::Result<()> {
        let layer = map.active_layer();
        let Some(EditorLayerUnionRef::Design {
            layer: EditorLayer::Tile(layer),
            group_index,
            is_background,
            layer_index,
            ..
        }) = layer
        else {
            return Err(anyhow!(
                "the current active layer is not a design tile layer"
            ));
        };

        let mut tile_list = vec![TileBase::default(); layer.layer.tiles.len()];
        let width = layer.layer.attr.width.get();
        let height = layer.layer.attr.height.get();

        let deleted_tiles: Vec<TileBase> = layer.layer.tiles.clone();

        for run in &self.runs {
            for y in 0..height as usize {
                for x in 0..width as usize {
                    for run_tile in &run.tiles {
                        let mut followed_rules = true;

                        // check the or tiles
                        if run_tile.auto_map_tile_type == TileLayerAutoMapperTileType::None {
                            for or_tiles in &run_tile.or_check_tiles {
                                let check_list = &or_tiles.tiles;
                                let mut tile_found = false;
                                let mut c = 0;
                                while (c < check_list.len()) && followed_rules && !tile_found {
                                    let do_check = &check_list[c];
                                    let real_x = x as i32 + or_tiles.offset.x;
                                    let real_y = y as i32 + or_tiles.offset.y;
                                    if real_x >= 0
                                        && real_y >= 0
                                        && real_x < width as i32
                                        && real_y < height as i32
                                    {
                                        if do_check.check_type
                                            == TileLayerAutoMapperCheckOrTileType::EqualsIndex
                                        {
                                            let new_tile = &tile_list[real_y as usize
                                                * width as usize
                                                + real_x as usize];
                                            if do_check.index == new_tile.index
                                                && do_check.tile_flag == new_tile.flags
                                            {
                                                tile_found = true;
                                            }
                                        }
                                    } else {
                                        // tile cannot be found, it can't fulfill the requirements
                                        break;
                                    }
                                    c += 1;
                                }

                                followed_rules &= tile_found;
                            }
                        }

                        if run_tile.auto_map_tile_type == TileLayerAutoMapperTileType::None {
                            for and_tiles in &run_tile.and_check_tiles {
                                let check_list = &and_tiles.tiles;
                                let mut tile_found = true;
                                let mut c = 0;
                                while c < check_list.len() && followed_rules && tile_found {
                                    let do_check = &check_list[c];
                                    let real_x = x as i32 + and_tiles.offset.x;
                                    let real_y = y as i32 + and_tiles.offset.y;
                                    if real_x >= 0
                                        && real_y >= 0
                                        && real_x < width as i32
                                        && real_y < height as i32
                                    {
                                        if do_check.check_type
                                            == TileLayerAutoMapperCheckAndTileType::NotEqualsIndex
                                        {
                                            let new_tile = &tile_list[real_y as usize
                                                * width as usize
                                                + real_x as usize];
                                            if do_check.index != new_tile.index as i32
                                                || do_check.tile_flag != new_tile.flags
                                            {
                                                tile_found &= true;
                                            }
                                        }
                                    } else if do_check.index != -1 {
                                        tile_found = false;
                                    }
                                    c += 1;
                                }

                                followed_rules &= tile_found;
                            }
                        }

                        let can_spawn =
                            run_tile.auto_map_tile_type == TileLayerAutoMapperTileType::Spawnable;

                        let new_tile = &mut tile_list[y * width as usize + x];
                        if followed_rules && (can_spawn || new_tile.index != 0) {
                            let mut r = rand::rngs::StdRng::seed_from_u64(0);
                            let rand_val: u32 = rand::Rng::gen_range(&mut r, 1..=u32::MAX);
                            if run_tile.randomness.is_none()
                                || run_tile.randomness.is_some_and(|val| rand_val <= val.get())
                            {
                                new_tile.index = run_tile.tile_index;
                                new_tile.flags = run_tile.tile_flags;
                            }
                        }
                    }
                }
            }
        }

        // replace tiles as action (deleted_tiles vs tile_list)
        client.execute(
            EditorAction::TileLayerReplaceTiles(ActTileLayerReplaceTiles {
                base: ActTileLayerReplTilesBase {
                    is_background,
                    group_index,
                    layer_index,
                    old_tiles: deleted_tiles,
                    new_tiles: tile_list,
                    x: 0,
                    y: 0,
                    w: layer.layer.attr.width,
                    h: layer.layer.attr.height,
                },
            }),
            Some(&format!(
                "auto-mapper-{}-{}-{}",
                is_background, group_index, layer_index
            )),
        );
        Ok(())
    }
}

pub struct TileLayerAutoMapperVisuals {
    pub tile_textures_pngs: Vec<TextureHandle>,
}

pub struct TileLayerAutoMapperLoadTask {
    rule: Option<Vec<u8>>,
    image: Option<Vec<u8>>,
    ctx: egui::Context,
}

/// this is a tool that allows to automatically map a tile layer based on
/// certain rules (e.g. tiles that soround the current tile)
pub struct TileLayerAutoMapper {
    pub rules: Vec<TileLayerAutoMapperRuleBase<TileLayerAutoMapperVisuals>>,

    pub active_rule: Option<usize>,

    pub selected_tile: Option<u8>,
    pub selected_grid: Option<vec2_base<usize>>,

    // ui shown
    pub active: bool,
    pub window_rect: Rect,
    pub file_dialog: FileDialog,
    pub load_tasks: HashMap<String, IoRuntimeTask<TileLayerAutoMapperLoadTask>>,
    pub task_needs_image: HashMap<String, TileLayerAutoMapperLoadTask>,
    pub io: IoFileSys,
    pub tp: Arc<rayon::ThreadPool>,
}

impl TileLayerAutoMapper {
    pub fn new(io: IoFileSys, tp: Arc<rayon::ThreadPool>) -> Self {
        Self {
            rules: Vec::new(),
            active_rule: None,
            active: false,

            window_rect: Rect::from_min_size(Default::default(), vec2(50.0, 50.0)),

            file_dialog: FileDialog::new(),

            selected_tile: None,
            selected_grid: None,

            load_tasks: Default::default(),
            task_needs_image: Default::default(),

            io,
            tp,
        }
    }

    pub fn run_rule(
        &self,
        rule_index: usize,
        map: &EditorMap,
        client: &mut EditorClient,
    ) -> anyhow::Result<()> {
        if rule_index < self.rules.len() {
            let rule = &self.rules[rule_index];

            rule.run(map, client)?;
        }
        Ok(())
    }

    pub fn load(&mut self, path: &Path, ctx: egui::Context) {
        let fs = self.io.fs.clone();
        if let (Some(file_name), Some(file_ext)) = (path.file_stem(), path.extension()) {
            let path = path.to_path_buf();
            let file_ext = file_ext.to_string_lossy().to_string();
            let file_name = file_name.to_string_lossy().to_string();
            match file_ext.to_lowercase().as_str() {
                "png" => {
                    // try to load the rule from editor dir
                    // else simply create a new one
                    self.load_tasks.insert(
                        file_name.clone(),
                        self.io.rt.spawn(async move {
                            let hash: Hash = Default::default();
                            let editor_path =
                                format!("editor/rules/{file_name}_{}.rule", fmt_hash(&hash));
                            let file = read_file_editor(&fs, editor_path.as_ref()).await;
                            let image = read_file_editor(&fs, path.as_ref()).await;
                            Ok(TileLayerAutoMapperLoadTask {
                                rule: file.ok(),
                                image: image.ok(),
                                ctx,
                            })
                        }),
                    );
                }
                "rule" => {
                    self.load_tasks.insert(
                        file_name.clone(),
                        self.io.rt.spawn(async move {
                            let hash: Hash = Default::default();
                            let editor_path =
                                format!("map/resources/images/{file_name}_{}.png", fmt_hash(&hash));
                            let file = read_file_editor(&fs, path.as_ref()).await;
                            let image = read_file_editor(&fs, editor_path.as_ref()).await;
                            Ok(TileLayerAutoMapperLoadTask {
                                rule: file.ok(),
                                image: image.ok(),
                                ctx,
                            })
                        }),
                    );
                }
                _ => {
                    // ignore
                }
            }
        }
    }

    pub fn update(&mut self) {
        let load_tasks: HashMap<_, _> = self
            .load_tasks
            .drain()
            .filter_map(|(name, task)| {
                if task.is_finished() {
                    let load_task = task.get_storage().unwrap();
                    if load_task.image.is_none() {
                        self.task_needs_image.insert(name, load_task);
                    } else {
                        let image = load_task.image.unwrap();
                        let mut img_mem = Vec::new();
                        if let Some(tile_textures) = image_utils::png::load_png_image_as_rgba(
                            &image,
                            |w, h, color_channel_count| {
                                img_mem.resize(w * h * color_channel_count, 0);
                                img_mem.as_mut()
                            },
                        )
                        .ok()
                        .and_then(|img| {
                            let mut tex_3d = vec![0; img.width as usize * img.height as usize * 4];
                            let mut image_3d_width = 0;
                            let mut image_3d_height = 0;
                            if !texture_2d_to_3d(
                                &self.tp,
                                img.data,
                                img.width as usize,
                                img.height as usize,
                                4,
                                16,
                                16,
                                tex_3d.as_mut_slice(),
                                &mut image_3d_width,
                                &mut image_3d_height,
                            ) {
                                None
                            } else {
                                let tile_textures: Vec<_> = tex_3d
                                    .chunks_exact(image_3d_width * image_3d_height * 4)
                                    .map(|chunk| {
                                        load_task.ctx.load_texture(
                                            name.clone(),
                                            egui::ImageData::Color(Arc::new(
                                                ColorImage::from_rgba_unmultiplied(
                                                    [image_3d_width, image_3d_height],
                                                    chunk,
                                                ),
                                            )),
                                            Default::default(),
                                        )
                                    })
                                    .collect::<_>();
                                Some(tile_textures)
                            }
                        }) {
                            if let Some(Ok(mut rule_base)) = load_task.rule.map(|rule| {
                                serde_json::from_str::<TileLayerAutoMapperRule>(
                                    String::from_utf8_lossy(&rule).as_str(),
                                )
                            }) {
                                if rule_base.runs.is_empty() {
                                    rule_base.runs.push(TileLayerAutoMapperRun {
                                        tiles: Default::default(),
                                    });
                                }
                                self.rules.push(TileLayerAutoMapperRuleBase::<
                                    TileLayerAutoMapperVisuals,
                                > {
                                    runs: rule_base.runs,
                                    name: rule_base.name,
                                    active_run: rule_base.active_run,
                                    user: TileLayerAutoMapperVisuals {
                                        tile_textures_pngs: tile_textures,
                                    },
                                });
                            } else {
                                self.rules.push(TileLayerAutoMapperRuleBase::<
                                    TileLayerAutoMapperVisuals,
                                > {
                                    runs: vec![TileLayerAutoMapperRun {
                                        tiles: Default::default(),
                                    }],
                                    name,
                                    active_run: Default::default(),
                                    user: TileLayerAutoMapperVisuals {
                                        tile_textures_pngs: tile_textures,
                                    },
                                });
                            }
                        } else {
                            // error
                        }
                    }
                    None
                } else {
                    Some((name, task))
                }
            })
            .collect();
        self.load_tasks = load_tasks;
    }
}
