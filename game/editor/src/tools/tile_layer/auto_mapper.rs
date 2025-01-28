use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::anyhow;
use base::hash::{fmt_hash, name_and_hash, Hash};
use base_io::{io::IoFileSys, runtime::IoRuntimeTask};
use editor_interface::auto_mapper::{
    AutoMapperInputModes, AutoMapperInterface, AutoMapperModes, AutoMapperOutputModes,
};
use egui::{vec2, Rect};
use egui_file_dialog::FileDialog;
use graphics::{
    graphics::graphics::Graphics,
    graphics_mt::GraphicsMultiThreaded,
    handles::texture::texture::{GraphicsTextureHandle, TextureContainer},
};
use graphics_types::{
    commands::TexFlags,
    types::{GraphicsBackendMemory, GraphicsMemoryAllocationType},
};
use image_utils::utils::texture_2d_to_3d;
use map::map::groups::layers::tiles::{Tile, TileBase, TileFlags};
use math::math::vector::ivec2;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};

use crate::{
    actions::actions::{ActTileLayerReplTilesBase, ActTileLayerReplaceTiles, EditorAction},
    client::EditorClient,
    fs::read_file_editor,
    map::{EditorLayer, EditorLayerTile, EditorLayerUnionRef, EditorMap, EditorMapInterface},
    notifications::{EditorNotification, EditorNotifications},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum TileLayerAutoMapperTileType {
    /// can _only_ overwrite existing tiles
    Default,
    /// can spawn new tiles (even if there was none before)
    Spawnable,
    /// Only spawns if there was no tile before, else does nothing.
    SpawnOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TileLayerAutoMapperOperator {
    /// Logical OR
    Or,
    /// Logical AND
    And,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TileLayerAutoMapperTileExpr {
    /// tile index
    pub tile_index: u8,
    /// tile flag
    pub tile_flags: Option<TileFlags>,
}

/// Never zero in both components
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct TileOffsetNonZero {
    x: i32,
    y: i32,
}

impl Serialize for TileOffsetNonZero {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if serializer.is_human_readable() {
            let v = self.get();
            format!("{},{}", v.x, v.y).serialize(serializer)
        } else {
            self.get().serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for TileOffsetNonZero {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            String::deserialize(deserializer)?
                .split_once(",")
                .ok_or_else(|| serde::de::Error::custom("couldn't split coordinates"))
                .and_then(|(x, y)| {
                    x.parse()
                        .map_err(|_| serde::de::Error::custom("couldn't parse x"))
                        .and_then(|x| {
                            y.parse()
                                .map_err(|_| serde::de::Error::custom("couldn't parse x"))
                                .and_then(|y| {
                                    Self::new(x, y).ok_or_else(|| {
                                        serde::de::Error::custom("Both components were 0")
                                    })
                                })
                        })
                })
        } else {
            let res = ivec2::deserialize(deserializer)?;
            Self::new(res.x, res.y)
                .ok_or_else(|| serde::de::Error::custom("Both components were 0"))
        }
    }
}

impl TileOffsetNonZero {
    pub fn new(x: i32, y: i32) -> Option<Self> {
        if x == 0 && y == 0 {
            return None;
        }
        Some(Self { x, y })
    }

    pub fn get(&self) -> ivec2 {
        ivec2::new(self.x, self.y)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileLayerAutoMapperCheckGroup {
    /// Negate the whole group
    /// !(cond1 && cond2)
    pub negate: bool,

    /// The tile expression for the automapper.
    pub tile: TileLayerAutoMapperTileExpr,

    /// Optional expression evaluated by the given boolean operator.
    pub operation: Option<(
        TileLayerAutoMapperOperator,
        Box<TileLayerAutoMapperCheckGroup>,
    )>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileLayerAutoMapperTile {
    pub tile_index: u8,
    pub tile_flags: TileFlags,

    pub tile_type: TileLayerAutoMapperTileType,
    /// how often should this tile appear
    pub randomness: Option<NonZeroU32>, // None = always

    /// Groups are __always__ logically evaluated with an AND operator.
    ///
    /// The key is the relative offset towards the current tile
    /// (0, 0) = cur tile.
    pub check_groups: HashMap<TileOffsetNonZero, TileLayerAutoMapperCheckGroup>,

    #[serde(skip)]
    pub grid_size: usize,
    #[serde(skip)]
    pub check_tile_offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileLayerAutoMapperRun {
    pub tiles: Vec<TileLayerAutoMapperTile>,

    #[serde(skip)]
    pub active_tile: Option<usize>,
}

pub trait EditorAutoMapperInterface {
    fn run_layer(
        &mut self,
        layer: &EditorLayerTile,
        is_background: bool,
        group_index: usize,
        layer_index: usize,
        client: &mut EditorClient,
    );
}

impl<T: AutoMapperInterface> EditorAutoMapperInterface for T {
    fn run_layer(
        &mut self,
        layer: &EditorLayerTile,
        is_background: bool,
        group_index: usize,
        layer_index: usize,
        client: &mut EditorClient,
    ) {
        let width = layer.layer.attr.width;
        let height = layer.layer.attr.height;

        let deleted_tiles: Vec<TileBase> = layer.layer.tiles.clone();

        let Ok(AutoMapperOutputModes::DesignTileLayer { tiles }) =
            self.run(AutoMapperInputModes::DesignTileLayer {
                tiles: deleted_tiles.clone(),
                width,
                height,
            })
        else {
            log::error!("wanted design tile layer auto mapper, got different output instead.");
            return;
        };

        if tiles.len() != deleted_tiles.len() {
            log::error!("auto mapper changed number of tiles.");
            return;
        }

        // replace tiles as action (deleted_tiles vs tile_list)
        client.execute(
            EditorAction::TileLayerReplaceTiles(ActTileLayerReplaceTiles {
                base: ActTileLayerReplTilesBase {
                    is_background,
                    group_index,
                    layer_index,
                    old_tiles: deleted_tiles,
                    new_tiles: tiles,
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
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileLayerAutoMapperRule {
    pub runs: Vec<TileLayerAutoMapperRun>,

    #[serde(skip)]
    pub active_run: usize,
}

impl TileLayerAutoMapperRule {
    pub fn run_active_layer(
        &mut self,
        map: &EditorMap,
        client: &mut EditorClient,
    ) -> anyhow::Result<()> {
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

        self.run_layer(layer, is_background, group_index, layer_index, client);

        Ok(())
    }
}

impl Default for TileLayerAutoMapperRule {
    fn default() -> Self {
        Self {
            runs: vec![TileLayerAutoMapperRun {
                tiles: Default::default(),

                active_tile: Default::default(),
            }],
            active_run: 0,
        }
    }
}

impl AutoMapperInterface for TileLayerAutoMapperRule {
    fn supported_modes() -> AutoMapperModes {
        AutoMapperModes::DesignTileLayer
    }

    fn run(&mut self, input: AutoMapperInputModes) -> anyhow::Result<AutoMapperOutputModes> {
        let AutoMapperInputModes::DesignTileLayer {
            mut tiles,
            width,
            height,
        } = input;

        let seed = 0;

        for run in &self.runs {
            for y in 0..height.get() as usize {
                for x in 0..width.get() as usize {
                    for run_tile in &run.tiles {
                        let check_groups = &run_tile.check_groups;

                        fn eval_expression(
                            x: usize,
                            y: usize,
                            width: usize,
                            height: usize,
                            tiles: &[Tile],
                            group_grid: &TileOffsetNonZero,
                            expr: &TileLayerAutoMapperCheckGroup,
                        ) -> bool {
                            let grid_offset = group_grid.get();
                            let real_x = x as i32 + grid_offset.x;
                            let real_y = y as i32 + grid_offset.y;
                            let result = if real_x >= 0
                                && real_y >= 0
                                && real_x < width as i32
                                && real_y < height as i32
                            {
                                let new_tile = &tiles[real_y as usize * width + real_x as usize];
                                expr.tile.tile_index == new_tile.index
                                    && expr
                                        .tile
                                        .tile_flags
                                        .is_none_or(|flags| flags == new_tile.flags)
                            } else {
                                false
                            };

                            let result = if expr.negate { !result } else { result };

                            if let Some((op, tile)) = &expr.operation {
                                let right_result =
                                    eval_expression(x, y, width, height, tiles, group_grid, tile);

                                match op {
                                    TileLayerAutoMapperOperator::Or => result || right_result,
                                    TileLayerAutoMapperOperator::And => result && right_result,
                                }
                            } else {
                                result
                            }
                        }

                        let result = check_groups.iter().all(|(offset, group)| {
                            eval_expression(
                                x,
                                y,
                                width.get() as usize,
                                height.get() as usize,
                                &tiles,
                                offset,
                                group,
                            )
                        });

                        let can_spawn =
                            run_tile.tile_type == TileLayerAutoMapperTileType::Spawnable;

                        let new_tile = &mut tiles[y * width.get() as usize + x];
                        if result && (can_spawn || new_tile.index != 0) {
                            let mut r = rand::rngs::StdRng::seed_from_u64(seed);
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

        Ok(AutoMapperOutputModes::DesignTileLayer { tiles })
    }
}

#[derive(Debug)]
pub enum TileLayerAutoMapperRuleType {
    EditorRule(TileLayerAutoMapperRule),
}

impl EditorAutoMapperInterface for TileLayerAutoMapperRuleType {
    fn run_layer(
        &mut self,
        layer: &EditorLayerTile,
        is_background: bool,
        group_index: usize,
        layer_index: usize,
        client: &mut EditorClient,
    ) {
        let rule: Box<&mut dyn EditorAutoMapperInterface> = match self {
            Self::EditorRule(rule) => Box::new(rule),
        };
        rule.run_layer(layer, is_background, group_index, layer_index, client);
    }
}

#[derive(Debug, Clone)]
pub struct TileLayerAutoMapperVisuals {
    pub tile_textures_pngs: Vec<TextureContainer>,
}

struct LoadResourceTask {
    name: String,
    image: Vec<u8>,
    hash: Hash,
}

struct LoadTask {
    rules: HashMap<String, TileLayerAutoMapperRule>,
    texture_mems: Vec<GraphicsBackendMemory>,
}

pub struct TileLayerAutoMapperRules {
    pub rules: HashMap<String, TileLayerAutoMapperRuleType>,
    pub visuals: TileLayerAutoMapperVisuals,
}

/// this is a tool that allows to automatically map a tile layer based on
/// certain rules (e.g. tiles that soround the current tile)
pub struct TileLayerAutoMapper {
    pub resources: HashMap<String, TileLayerAutoMapperRules>,

    pub active_resource: Option<String>,
    pub active_rule: Option<String>,

    pub selected_tile: Option<u8>,
    pub selected_grid: Option<ivec2>,
    pub new_rule_name: String,

    // ui shown
    pub active: bool,
    pub window_rect: Rect,
    pub file_dialog: FileDialog,

    load_resource_tasks: Vec<IoRuntimeTask<LoadResourceTask>>,
    load_tasks: HashMap<String, IoRuntimeTask<LoadTask>>,
    failed_tasks: HashSet<String>,
    pub errors: Vec<anyhow::Error>,

    pub io: IoFileSys,
    pub tp: Arc<rayon::ThreadPool>,
    pub graphics_mt: GraphicsMultiThreaded,
    pub texture_handle: GraphicsTextureHandle,
}

impl TileLayerAutoMapper {
    pub fn new(graphics: &Graphics, io: IoFileSys, tp: Arc<rayon::ThreadPool>) -> Self {
        Self {
            resources: Default::default(),
            active_resource: None,
            active_rule: None,
            active: false,

            window_rect: Rect::from_min_size(Default::default(), vec2(50.0, 50.0)),

            file_dialog: FileDialog::new(),

            selected_tile: None,
            selected_grid: None,
            new_rule_name: Default::default(),

            load_tasks: Default::default(),
            load_resource_tasks: Default::default(),
            errors: Default::default(),
            failed_tasks: Default::default(),

            io,
            tp,
            graphics_mt: graphics.get_graphics_mt(),
            texture_handle: graphics.texture_handle.clone(),
        }
    }

    pub fn load_resource_then_rule(&mut self, path: &Path) {
        let Some(file_name) = path
            .file_stem()
            .and_then(|s| s.to_str().map(|s| s.to_string()))
        else {
            self.errors
                .push(anyhow!("Resource file stem invalid: {:?}", path));
            return;
        };
        let path = path.to_path_buf();
        // try to load the rule from editor dir
        // else simply create a new one
        let fs = self.io.fs.clone();
        self.load_resource_tasks.push(self.io.rt.spawn(async move {
            let image = read_file_editor(&fs, path.as_ref()).await?;

            let (name, hash) = name_and_hash(&file_name, &image);
            Ok(LoadResourceTask { image, name, hash })
        }));
    }

    pub fn load_from_res(&mut self, name: String, hash: Hash, image: Vec<u8>) {
        let fs = self.io.fs.clone();
        let graphics_mt = self.graphics_mt.clone();
        let tp = self.tp.clone();
        self.load_tasks.insert(
            format!("{name}_{}", fmt_hash(&hash)),
            self.io.rt.spawn(async move {
                let editor_path: PathBuf =
                    format!("editor/rules/{name}_{}", fmt_hash(&hash)).into();

                let files = fs
                    .files_in_dir_recursive(&editor_path)
                    .await
                    .unwrap_or_default();

                let mut img_mem = Vec::new();
                let img = image_utils::png::load_png_image_as_rgba(
                    &image,
                    |w, h, color_channel_count| {
                        img_mem.resize(w * h * color_channel_count, 0);
                        img_mem.as_mut()
                    },
                )?;

                let mut tex_3d = vec![0; img.width as usize * img.height as usize * 4];
                let mut image_3d_width = 0;
                let mut image_3d_height = 0;
                if !texture_2d_to_3d(
                    &tp,
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
                    Err(anyhow!(
                        "Given resource is not a tile set that is divisible by 16"
                    ))
                } else {
                    let texture_mems: Vec<_> = tex_3d
                        .chunks_exact(image_3d_width * image_3d_height * 4)
                        .map(|chunk| {
                            let mut mem = graphics_mt.mem_alloc(
                                GraphicsMemoryAllocationType::TextureRgbaU8 {
                                    width: image_3d_width.try_into().unwrap(),
                                    height: image_3d_height.try_into().unwrap(),
                                    flags: TexFlags::empty(),
                                },
                            );
                            mem.as_mut_slice().copy_from_slice(chunk);
                            let _ = graphics_mt.try_flush_mem(&mut mem, true);
                            mem
                        })
                        .collect::<Vec<_>>();

                    Ok(LoadTask {
                        texture_mems,
                        rules: files
                            .into_iter()
                            .filter_map(|(f, file)| {
                                f.file_stem().and_then(|s| s.to_str()).and_then(|s| {
                                    f.extension()
                                        .is_some_and(|e| e == "editorrule")
                                        .then_some(file)
                                        .and_then(|file| {
                                            serde_json::from_slice::<TileLayerAutoMapperRule>(&file)
                                                .ok()
                                                .map(|f| (s.to_string(), f))
                                        })
                                })
                            })
                            .collect(),
                    })
                }
            }),
        );
    }

    pub fn save(
        io: &IoFileSys,
        name: String,
        resource_name_and_hash: String,
        rule: TileLayerAutoMapperRule,
    ) {
        let fs = io.fs.clone();
        io.rt.spawn_without_lifetime(async move {
            let editor_path: PathBuf =
                format!("editor/rules/{resource_name_and_hash}/{name}.editorrule").into();

            if let Some(parent) = editor_path.parent() {
                fs.create_dir(parent).await?;
            }

            let file = serde_json::to_vec_pretty(&rule)?;

            fs.write_file(&editor_path, file).await?;

            Ok(())
        });
    }

    pub fn update(&mut self, notifications: &EditorNotifications) {
        for err in self.errors.drain(..) {
            notifications.push(EditorNotification::Error(err.to_string()));
        }

        let mut to_load_res: Vec<_> = Default::default();
        let res_tasks: Vec<_> = self
            .load_resource_tasks
            .drain(..)
            .flat_map(|task| {
                if task.is_finished() {
                    match task.get_storage() {
                        Ok(task) => {
                            to_load_res.push((task.name, task.hash, task.image));
                        }
                        Err(err) => {
                            self.errors.push(err);
                        }
                    }
                    None
                } else {
                    Some(task)
                }
            })
            .collect();
        self.load_resource_tasks = res_tasks;

        for (name, hash, image) in to_load_res {
            self.load_from_res(name, hash, image);
        }

        let load_tasks: HashMap<_, _> = self
            .load_tasks
            .drain()
            .filter_map(|(name, task)| {
                if task.is_finished() {
                    let load_task = task.get_storage();
                    match load_task {
                        Ok(load_task) => {
                            let textures = load_task
                                .texture_mems
                                .into_iter()
                                .map(|mem| {
                                    self.texture_handle.load_texture_rgba_u8(mem, "auto-mapper")
                                })
                                .collect::<anyhow::Result<Vec<_>>>();
                            match textures {
                                Ok(textures) => {
                                    let entry = self.resources.entry(name).or_insert_with(|| {
                                        TileLayerAutoMapperRules {
                                            rules: Default::default(),
                                            visuals: TileLayerAutoMapperVisuals {
                                                tile_textures_pngs: textures,
                                            },
                                        }
                                    });
                                    for (rule_name, rule) in load_task.rules {
                                        entry.rules.insert(
                                            rule_name,
                                            TileLayerAutoMapperRuleType::EditorRule(
                                                TileLayerAutoMapperRule {
                                                    runs: rule.runs,
                                                    active_run: rule.active_run,
                                                },
                                            ),
                                        );
                                    }
                                }
                                Err(err) => {
                                    self.failed_tasks.insert(name);
                                    self.errors.push(err);
                                }
                            }
                        }
                        Err(err) => {
                            self.failed_tasks.insert(name);
                            self.errors.push(err);
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

    pub fn try_load(
        &mut self,
        resource_name_and_hash: &str,
        name: &str,
        hash: &Hash,
        image: &[u8],
    ) {
        if !self.resources.contains_key(resource_name_and_hash)
            && !self.failed_tasks.contains(resource_name_and_hash)
            && !self.load_tasks.contains_key(resource_name_and_hash)
        {
            self.load_from_res(name.to_string(), *hash, image.to_vec());
        }
    }
}
