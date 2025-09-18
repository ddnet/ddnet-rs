use std::{collections::HashMap, io::BufRead, num::NonZeroU16};

use anyhow::anyhow;
use editor_interface::auto_mapper::{
    AutoMapperInputModes, AutoMapperInterface, AutoMapperModes, AutoMapperOutputModes,
};
use legacy_map::mapdef_06::DdraceTileNum;
use map::map::groups::layers::tiles::{Tile, TileFlags};
use scan_fmt::{scan_fmt, scan_fmt_some};

// Based on triple32inc from https://github.com/skeeto/hash-prospector/tree/79a6074062a84907df6e45b756134b74e2956760
fn hash_u32(mut num: u32) -> u32 {
    num = num.wrapping_add(1);
    num ^= num.wrapping_shr(17);
    num = num.wrapping_mul(0xed5ad4bbu32);
    num ^= num.wrapping_shr(11);
    num = num.wrapping_mul(0xac4c1b51u32);
    num ^= num.wrapping_shr(15);
    num = num.wrapping_mul(0x31848babu32);
    num ^= num.wrapping_shr(14);
    num
}

const HASH_MAX: u32 = 65536;

fn hash_location(seed: u32, run: u32, rule: u32, x: u32, y: u32) -> u32 {
    let prime = 31u32;
    let mut hash = 1u32;
    hash = hash.wrapping_mul(prime).wrapping_add(hash_u32(seed));
    hash = hash.wrapping_mul(prime).wrapping_add(hash_u32(run));
    hash = hash.wrapping_mul(prime).wrapping_add(hash_u32(rule));
    hash = hash.wrapping_mul(prime).wrapping_add(hash_u32(x));
    hash = hash.wrapping_mul(prime).wrapping_add(hash_u32(y));
    // Just to double-check that values are well-distributed
    hash = hash_u32(hash.wrapping_mul(prime));
    hash % HASH_MAX
}

#[derive(Debug, Clone, Copy)]
struct IndexInfo {
    id: i32,
    flags: TileFlags,
    test_flags: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndexTy {
    NoRule = 0,
    Index,
    NotIndex,
}

#[derive(Debug)]
struct PosRule {
    x: i32,
    y: i32,
    index_ty: IndexTy,
    indices: Vec<IndexInfo>,
}

#[derive(Debug)]
struct CModuloRule {
    mod_x: i32,
    mod_y: i32,
    offset_x: i32,
    offset_y: i32,
}

#[derive(Debug)]
struct CIndexRule {
    id: i32,
    rules: Vec<PosRule>,
    flags: TileFlags,
    random_probability: f32,
    modulo_rules: Vec<CModuloRule>,
    default_rule: bool,
    skip_empty: bool,
    skip_full: bool,
}

#[derive(Debug)]
struct Run {
    index_rules: Vec<CIndexRule>,
    automap_copy: bool,
}

#[derive(Debug)]
pub struct Configuration {
    runs: Vec<Run>,
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
}

#[derive(Debug)]
pub struct LegacyRulesLoading {
    pub file: Vec<u8>,

    pub configs: HashMap<String, Configuration>,
}

impl LegacyRulesLoading {
    pub fn new(file: &[u8]) -> anyhow::Result<Self> {
        let mut cur_conf_name = None;
        let mut cur_run_index = None;
        let mut cur_index_index = None;

        let mut configs: HashMap<String, Configuration> = Default::default();

        // read each line
        for line in file.lines() {
            let mut line = line?;
            let mut cur_conf = cur_conf_name.as_ref().and_then(|i| configs.get_mut(i));
            // skip blank/empty lines as well as comments
            if !line.is_empty()
                && !line.starts_with('#')
                && !line.starts_with('\t')
                && !line.starts_with(char::is_whitespace)
            {
                if line.starts_with('[') && line.ends_with(']') {
                    let mut name = line.split_off(1);
                    // remove ]
                    name.pop();
                    // new configuration, get the name
                    let new_conf = Configuration {
                        start_x: 0,
                        start_y: 0,
                        end_x: 0,
                        end_y: 0,
                        runs: vec![Run {
                            automap_copy: true,
                            index_rules: Default::default(),
                        }],
                    };
                    let cur_count = configs.len();
                    let full_name = format!("%{cur_count}%{name}");
                    configs.insert(full_name.clone(), new_conf);
                    cur_conf_name = Some(full_name);
                    cur_run_index = Some(0);
                } else if let Some(cur_conf) = line
                    .starts_with("NewRun")
                    .then_some(cur_conf.as_deref_mut())
                    .flatten()
                {
                    // add new run
                    cur_conf.runs.push(Run {
                        automap_copy: true,
                        index_rules: Default::default(),
                    });
                    cur_run_index = Some(cur_conf.runs.len() - 1);
                } else if let Some(cur_run) = line
                    .starts_with("Index")
                    .then(|| {
                        cur_conf
                            .as_deref_mut()
                            .zip(cur_run_index)
                            .and_then(|(c, i)| c.runs.get_mut(i))
                    })
                    .flatten()
                {
                    // new index

                    let (id, orientation1, orientation2, orientation3) =
                        scan_fmt_some!(&line, "Index {d} {} {} {}", i32, String, String, String);
                    let id = id.ok_or_else(|| anyhow!("line \"{line}\" failed to find id"))?;

                    let mut flags = TileFlags::empty();

                    if let Some(ori) = orientation1 {
                        flags = Self::check_index_flags(flags, &ori, false);
                    }

                    if let Some(ori) = orientation2 {
                        flags = Self::check_index_flags(flags, &ori, false);
                    }

                    if let Some(ori) = orientation3 {
                        flags = Self::check_index_flags(flags, &ori, false);
                    }

                    // add the index rule object and make it current
                    cur_run.index_rules.push(CIndexRule {
                        id,
                        rules: Default::default(),
                        flags,
                        random_probability: 1.0,
                        modulo_rules: Default::default(),
                        default_rule: true,
                        skip_empty: false,
                        skip_full: false,
                    });
                    cur_index_index = Some(cur_run.index_rules.len() - 1);
                } else if let Some(cur_index) = line
                    .starts_with("Pos")
                    .then(|| {
                        cur_conf
                            .as_deref_mut()
                            .zip(cur_run_index)
                            .and_then(|(c, i)| c.runs.get_mut(i).zip(cur_index_index))
                            .and_then(|(r, i)| r.index_rules.get_mut(i))
                    })
                    .flatten()
                {
                    let mut index_type = IndexTy::NoRule;
                    let mut new_index_list: Vec<IndexInfo> = Default::default();

                    let (x, y, index_ty_str, rest) =
                        scan_fmt_some!(&line, "Pos {d} {d} {}{/.*/}", i32, i32, String, String);
                    let x = x.ok_or_else(|| anyhow!("line \"{line}\" failed to find x"))?;
                    let y = y.ok_or_else(|| anyhow!("line \"{line}\" failed to find y"))?;
                    let index_ty_str = index_ty_str
                        .ok_or_else(|| anyhow!("line \"{line}\" failed to find index type"))?;

                    if index_ty_str == "EMPTY" {
                        index_type = IndexTy::Index;
                        let new_index_info = IndexInfo {
                            id: 0,
                            flags: TileFlags::empty(),
                            test_flags: false,
                        };
                        new_index_list.push(new_index_info);
                    } else if index_ty_str == "FULL" {
                        index_type = IndexTy::NotIndex;
                        let new_index_info = IndexInfo {
                            id: 0,
                            flags: TileFlags::empty(),
                            test_flags: false,
                        };
                        new_index_list.push(new_index_info);
                    } else if index_ty_str == "INDEX" || index_ty_str == "NOTINDEX" {
                        if index_ty_str == "INDEX" {
                            index_type = IndexTy::Index;
                        } else {
                            index_type = IndexTy::NotIndex;
                        }

                        let mut line = rest.unwrap_or_default();
                        loop {
                            let (id, orientation1, orientation2, orientation3, orientation4, rest) = scan_fmt_some!(
                                &line,
                                "{d} {} {} {} {}{/.*/}",
                                i32,
                                String,
                                String,
                                String,
                                String,
                                String
                            );
                            let id =
                                id.ok_or_else(|| anyhow!("line \"{line}\" failed to find id"))?;
                            line = rest.unwrap_or_default();

                            let mut new_index_info = IndexInfo {
                                id,
                                flags: TileFlags::empty(),
                                test_flags: false,
                            };

                            if orientation1.as_ref().is_some_and(|ori| ori == "OR") {
                                new_index_list.push(new_index_info);
                                line = format!(
                                    "{}{}{}{line}",
                                    orientation2.map(|o| format!("{o} ")).unwrap_or_default(),
                                    orientation3.map(|o| format!("{o} ")).unwrap_or_default(),
                                    orientation4.map(|o| format!("{o} ")).unwrap_or_default()
                                );
                                continue;
                            } else if let Some(ori) = orientation1 {
                                new_index_info.flags =
                                    Self::check_index_flags(new_index_info.flags, &ori, true);
                                new_index_info.test_flags =
                                    !(new_index_info.flags == TileFlags::empty() && ori != "NONE");
                            } else {
                                new_index_list.push(new_index_info);
                                break;
                            }

                            if orientation2.as_ref().is_some_and(|ori| ori == "OR") {
                                new_index_list.push(new_index_info);
                                line = format!(
                                    "{}{}{line}",
                                    orientation3.map(|o| format!("{o} ")).unwrap_or_default(),
                                    orientation4.map(|o| format!("{o} ")).unwrap_or_default()
                                );
                                continue;
                            } else if let Some(ori) = (new_index_info.flags != TileFlags::empty())
                                .then_some(orientation2)
                                .flatten()
                            {
                                new_index_info.flags =
                                    Self::check_index_flags(new_index_info.flags, &ori, false);
                            } else {
                                new_index_list.push(new_index_info);
                                break;
                            }

                            if orientation3.as_ref().is_some_and(|ori| ori == "OR") {
                                new_index_list.push(new_index_info);
                                line = format!(
                                    "{}{line}",
                                    orientation4.map(|o| format!("{o} ")).unwrap_or_default()
                                );
                                continue;
                            } else if let Some(ori) = (new_index_info.flags != TileFlags::empty())
                                .then_some(orientation3)
                                .flatten()
                            {
                                new_index_info.flags =
                                    Self::check_index_flags(new_index_info.flags, &ori, false);
                            } else {
                                new_index_list.push(new_index_info);
                                break;
                            }

                            if orientation4.as_ref().is_some_and(|ori| ori == "OR") {
                                new_index_list.push(new_index_info);
                                continue;
                            } else {
                                new_index_list.push(new_index_info);
                                break;
                            }
                        }
                    }

                    if index_type != IndexTy::NoRule {
                        cur_index.rules.push(PosRule {
                            x,
                            y,
                            index_ty: index_type,
                            indices: new_index_list.clone(),
                        });

                        if x == 0 && y == 0 {
                            for index in new_index_list {
                                if index.id == 0 && index_type == IndexTy::Index {
                                    // Skip full tiles if we have a rule "POS 0 0 INDEX 0"
                                    // because that forces the tile to be empty
                                    cur_index.skip_full = true;
                                } else if (index.id > 0 && index_type == IndexTy::Index)
                                    || (index.id == 0 && index_type == IndexTy::NotIndex)
                                {
                                    // Skip empty tiles if we have a rule "POS 0 0 INDEX i" where i > 0
                                    // or if we have a rule "POS 0 0 NOTINDEX 0"
                                    cur_index.skip_empty = true;
                                }
                            }
                        }

                        let cur_conf = cur_conf.unwrap();
                        cur_conf.start_x = cur_conf.start_x.min(x);
                        cur_conf.start_y = cur_conf.start_y.min(y);
                        cur_conf.end_x = cur_conf.end_x.min(x);
                        cur_conf.end_y = cur_conf.end_y.min(y);
                    }
                } else if let Some(cur_index) = line
                    .starts_with("Random")
                    .then(|| {
                        cur_conf
                            .as_deref_mut()
                            .zip(cur_run_index)
                            .and_then(|(c, i)| c.runs.get_mut(i).zip(cur_index_index))
                            .and_then(|(r, i)| r.index_rules.get_mut(i))
                    })
                    .flatten()
                {
                    let (value, specifier) = scan_fmt_some!(&line, "Random {}{/.*/}", String, char);
                    let value = value
                        .ok_or_else(|| anyhow!("line \"{line}\" failed to find value"))
                        .and_then(|v| v.parse::<f32>().map_err(|err| anyhow!(err)))?;
                    if specifier.is_some_and(|s| s == '%') {
                        cur_index.random_probability = value / 100.0;
                    } else {
                        cur_index.random_probability = 1.0 / value;
                    }
                } else if let Some(cur_index) = line
                    .starts_with("Modulo")
                    .then(|| {
                        cur_conf
                            .as_deref_mut()
                            .zip(cur_run_index)
                            .and_then(|(c, i)| c.runs.get_mut(i).zip(cur_index_index))
                            .and_then(|(r, i)| r.index_rules.get_mut(i))
                    })
                    .flatten()
                {
                    let (mut mod_x, mut mod_y, off_x, off_y) =
                        scan_fmt!(&line, "Modulo {d} {d} {d} {d}", i32, i32, i32, i32)
                            .map_err(|err| anyhow!("line \"{line}\" failed: {err}"))?;
                    if mod_x == 0 {
                        mod_x = 1;
                    }
                    if mod_y == 0 {
                        mod_y = 1;
                    }
                    cur_index.modulo_rules.push(CModuloRule {
                        mod_x,
                        mod_y,
                        offset_x: off_x,
                        offset_y: off_y,
                    });
                } else if let Some(cur_index) = line
                    .starts_with("NoDefaultRule")
                    .then(|| {
                        cur_conf
                            .as_deref_mut()
                            .zip(cur_run_index)
                            .and_then(|(c, i)| c.runs.get_mut(i).zip(cur_index_index))
                            .and_then(|(r, i)| r.index_rules.get_mut(i))
                    })
                    .flatten()
                {
                    cur_index.default_rule = false;
                } else if let Some(cur_run) = line
                    .starts_with("NoLayerCopy")
                    .then(|| {
                        cur_conf
                            .zip(cur_run_index)
                            .and_then(|(c, i)| c.runs.get_mut(i))
                    })
                    .flatten()
                {
                    cur_run.automap_copy = false;
                }
            }
        }

        // add default rule for Pos 0 0 if there is none
        for conf in configs.values_mut() {
            for run in &mut conf.runs {
                for index_rule in &mut run.index_rules {
                    let mut found = false;

                    // Search for the exact rule "POS 0 0 INDEX 0" which corresponds to the default rule
                    for rule in &index_rule.rules {
                        if rule.x == 0 && rule.y == 0 && rule.index_ty == IndexTy::Index {
                            for index in &rule.indices {
                                if index.id == 0 {
                                    found = true;
                                }
                            }
                            break;
                        }

                        if found {
                            break;
                        }
                    }

                    // If the default rule was not found, and we require it, then add it
                    if !found && index_rule.default_rule {
                        let new_index_list = vec![IndexInfo {
                            id: 0,
                            flags: Default::default(),
                            test_flags: false,
                        }];
                        index_rule.rules.push(PosRule {
                            x: 0,
                            y: 0,
                            index_ty: IndexTy::NotIndex,
                            indices: new_index_list,
                        });

                        index_rule.skip_empty = true;
                        index_rule.skip_full = false;
                    }

                    if index_rule.skip_empty && index_rule.skip_full {
                        index_rule.skip_empty = false;
                        index_rule.skip_full = false;
                    }
                }
            }
        }

        Ok(Self {
            file: file.to_vec(),
            configs,
        })
    }

    fn check_index_flags(mut flags: TileFlags, flag_name: &str, check_for_none: bool) -> TileFlags {
        if flag_name == "XFLIP" {
            flags |= TileFlags::XFLIP;
        } else if flag_name == "YFLIP" {
            flags |= TileFlags::YFLIP;
        } else if flag_name == "ROTATE" {
            flags |= TileFlags::ROTATE;
        } else if flag_name == "NONE" && check_for_none {
            flags = TileFlags::empty();
        }
        flags
    }
}

#[derive(Debug)]
pub struct LegacyRule {
    pub config: Configuration,
}

impl LegacyRule {
    fn neighbours_extra(&self) -> NonZeroU16 {
        4.try_into().unwrap()
    }
    fn expected_neighbours(&self) -> NonZeroU16 {
        let extra = self.neighbours_extra();
        let max_width = self
            .config
            .end_x
            .saturating_sub(self.config.start_x)
            .max(0)
            .clamp(0, (u16::MAX - extra.get()) as i32) as u16;
        let max_height = self
            .config
            .end_y
            .saturating_sub(self.config.start_y)
            .max(0)
            .clamp(0, (u16::MAX - extra.get()) as i32) as u16;
        let size = max_width.max(max_height).max(1);
        (size + self.neighbours_extra().get()).try_into().unwrap()
    }
}

impl AutoMapperInterface for LegacyRule {
    fn supported_modes(&self) -> Vec<AutoMapperModes> {
        vec![AutoMapperModes::DesignTileLayer {
            neighbouring_tiles: Some(self.expected_neighbours()),
        }]
    }

    fn run(
        &mut self,
        seed: u64,
        input: AutoMapperInputModes,
    ) -> Result<AutoMapperOutputModes, String> {
        //void CAutoMapper::Proceed(CLayerTiles *pLayer, CLayerTiles *pGameLayer, int ReferenceId, int ConfigId, int Seed, int SeedOffsetX, int SeedOffsetY)
        //if(!m_FileLoaded || pLayer.Readonly || ConfigId < 0 || ConfigId >= (int)m_vConfigs.size())
        //return;
        let AutoMapperInputModes::DesignTileLayer {
            mut tiles,
            width,
            height,
            off_x,
            off_y,
            full_width,
            full_height,
        } = input;
        let reference_layer_index = -1i32;

        let conf = &self.config;

        let ref_tile_index = [
            DdraceTileNum::Solid,
            DdraceTileNum::Death,
            DdraceTileNum::NoHook,
            DdraceTileNum::Freeze,
            DdraceTileNum::Unfreeze,
            DdraceTileNum::DFreeze,
            DdraceTileNum::DUnfreeze,
            DdraceTileNum::LFreeze,
            DdraceTileNum::LUnfreeze,
        ];

        let ref_auto_map_names = [
            "Game Layer",
            "Hookable",
            "Death",
            "Unhookable",
            "Freeze",
            "Unfreeze",
            "Deep Freeze",
            "Deep Unfreeze",
            "Live Freeze",
            "Live Unfreeze",
        ];

        assert!(
            ref_auto_map_names.len() == ref_tile_index.len() + 1,
            "g_apAutoMapReferenceNames and s_aTileIndex must include the same items"
        );

        let prev_tiles = tiles.clone();

        // TODO:
        let game_tiles = tiles.clone();
        let game_width = width;
        let game_height = height;

        let seed_offset_x = off_x as usize;
        let seed_offset_y = off_y as usize;

        // Determine copy range
        let extra = self.neighbours_extra();
        let x_skip = if off_x == 0 { 0 } else { extra.get() };
        let y_skip = if off_y == 0 { 0 } else { extra.get() };
        let width_skip = if full_width.get() == off_x.saturating_add(width.get()) {
            0
        } else {
            extra.get()
        };
        let height_skip = if full_height.get() == off_y.saturating_add(height.get()) {
            0
        } else {
            extra.get()
        };

        let end_y = height.get().saturating_sub(height_skip) as usize;
        let start_y = (y_skip as usize).min(end_y);
        let end_x = width.get().saturating_sub(width_skip) as usize;
        let start_x = (x_skip as usize).min(end_x);

        // for every run: copy tiles, automap, overwrite tiles
        for (h, run) in conf.runs.iter().enumerate() {
            let is_filterable = h == 0 && reference_layer_index >= 0;

            // don't make copy if it's requested
            let (tile_buffer, buffer_width) = if is_filterable {
                (&game_tiles, game_width)
            } else {
                (&tiles, width)
            };
            let read_layer = if run.automap_copy {
                let mut read_layer =
                    vec![Tile::default(); width.get() as usize * height.get() as usize];

                let loop_width = if is_filterable {
                    game_width.min(width)
                } else {
                    width
                };
                let loop_height = if is_filterable {
                    game_height.min(height)
                } else {
                    height
                };

                for y in 0..loop_height.get() as usize {
                    for x in 0..loop_width.get() as usize {
                        let read_tile = &tile_buffer[y * buffer_width.get() as usize + x];
                        let tile = &mut read_layer[y * width.get() as usize + x];
                        if h == 0
                            && reference_layer_index >= 1
                            && read_tile.index
                                != ref_tile_index[reference_layer_index as usize - 1] as u8
                        {
                            tile.index = 0;
                        } else {
                            tile.index = read_tile.index;
                        }
                        tile.flags = read_tile.flags;
                    }
                }
                Some(read_layer)
            } else {
                None
            };

            // auto map
            for y in 0..height.get() as usize {
                for x in 0..width.get() as usize {
                    for (i, index_rule) in run.index_rules.iter().enumerate() {
                        let tile_index = y * width.get() as usize + x;
                        let cur_tile = &mut tiles[tile_index];
                        let read_tile = if let Some(read_layer) = &read_layer {
                            read_layer[tile_index]
                        } else {
                            *cur_tile
                        };

                        if read_tile.index == 0 {
                            if cur_tile.index != 0 && is_filterable
                            // TODO: This is a lazy workaround
                            {
                                cur_tile.index = 0;
                                cur_tile.flags = index_rule.flags;
                                continue;
                            }

                            // skip empty tiles
                            if index_rule.skip_empty {
                                continue;
                            }
                        }
                        // skip full tiles
                        if index_rule.skip_full && read_tile.index != 0 {
                            continue;
                        }

                        let mut respected_rules = true;
                        let mut j = 0;
                        while j < index_rule.rules.len() && respected_rules {
                            let rule = &index_rule.rules[j];

                            let check_x = x as i32 + rule.x;
                            let check_y = y as i32 + rule.y;
                            let (check_index, check_flags) = if check_x >= 0
                                && check_x < width.get() as i32
                                && check_y >= 0
                                && check_y < height.get() as i32
                            {
                                let check_tile =
                                    check_y as usize * width.get() as usize + check_x as usize;
                                let read_tile = if let Some(read_layer) = &read_layer {
                                    read_layer[check_tile]
                                } else {
                                    let tile_buffer =
                                        if is_filterable { &game_tiles } else { &tiles };
                                    tile_buffer[check_tile]
                                };
                                (
                                    read_tile.index as i32,
                                    read_tile.flags
                                        & (TileFlags::ROTATE | TileFlags::XFLIP | TileFlags::YFLIP),
                                )
                            } else {
                                (-1, TileFlags::empty())
                            };

                            if rule.index_ty == IndexTy::Index {
                                respected_rules = false;
                                for index in &rule.indices {
                                    if check_index == index.id
                                        && (!index.test_flags || check_flags == index.flags)
                                    {
                                        respected_rules = true;
                                        break;
                                    }
                                }
                            } else if rule.index_ty == IndexTy::NotIndex {
                                for index in &rule.indices {
                                    if check_index == index.id
                                        && (!index.test_flags || check_flags == index.flags)
                                    {
                                        respected_rules = false;
                                        break;
                                    }
                                }
                            }

                            j += 1;
                        }

                        let passed_modulo_check = if index_rule.modulo_rules.is_empty() {
                            true
                        } else {
                            index_rule.modulo_rules.iter().any(|m| {
                                (x as i32 + seed_offset_x as i32 + m.offset_x) % m.mod_x == 0
                                    && (y as i32 + seed_offset_y as i32 + m.offset_y) % m.mod_y == 0
                            })
                        };

                        if respected_rules
                            && passed_modulo_check
                            && (index_rule.random_probability >= 1.0
                                || (hash_location(
                                    seed as u32,
                                    h as u32,
                                    i as u32,
                                    (x + seed_offset_x) as u32,
                                    (y + seed_offset_y) as u32,
                                ) as f32)
                                    < HASH_MAX as f32 * index_rule.random_probability)
                        {
                            let cur_tile = &mut tiles[y * width.get() as usize + x];
                            cur_tile.index = index_rule.id as u8;
                            cur_tile.flags = index_rule.flags;
                        }
                    }
                }
            }
        }

        // Copy back prev tiles for those range that should be skipped
        for y in 0..height.get() as usize {
            for x in 0..width.get() as usize {
                if x < start_x || x >= end_x || y < start_y || y >= end_y {
                    let tile_index = y * width.get() as usize + x;
                    tiles[tile_index] = prev_tiles[tile_index];
                }
            }
        }

        Ok(AutoMapperOutputModes::DesignTileLayer { tiles })
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use base_fs::filesys::FileSystem;
    use base_io_traits::fs_traits::{FileSystemEntryTy, FileSystemInterface};

    use crate::tools::tile_layer::legacy_rules::LegacyRulesLoading;

    fn create_fs() -> (FileSystem, tokio::runtime::Runtime) {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4) // should be at least 4
            .enable_all()
            .build()
            .unwrap();

        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../");
        std::env::set_current_dir(workspace_root).unwrap();
        (
            FileSystem::new(&rt, "ddnet-test", "ddnet-test", "ddnet-test", "ddnet-test").unwrap(),
            rt,
        )
    }

    #[test]
    fn rule() {
        let (fs, rt) = create_fs();

        let files = rt
            .block_on(fs.entries_in_dir("editor/rules".as_ref()))
            .unwrap();
        assert!(!files.is_empty());
        for (path, ty) in files {
            if matches!(ty, FileSystemEntryTy::Directory) {
                let final_path: &Path = "editor/rules".as_ref();
                let file = rt
                    .block_on(fs.read_file(final_path.join(path).join("base.rules").as_ref()))
                    .unwrap();
                LegacyRulesLoading::new(&file).unwrap();
            }
        }
    }
}
