use std::ops::{Index, IndexMut};

use map::map::groups::layers::tiles::{rotation_180, rotation_270, TileFlags, ROTATION_90};
use math::math::vector::{ubvec2, usvec2};

pub(super) type GraphicsTilePos = usvec2;
pub(super) type GraphicsTileTex = ubvec2;

#[repr(C)]
#[derive(Default)]
pub(super) struct GraphicTile {
    top_left: GraphicsTilePos,
    tex_coord_top_left: GraphicsTileTex,
    top_right: GraphicsTilePos,
    tex_coord_top_right: GraphicsTileTex,
    bottom_right: GraphicsTilePos,
    tex_coord_bottom_right: GraphicsTileTex,
    bottom_left: GraphicsTilePos,
    tex_coord_bottom_left: GraphicsTileTex,
}

impl Index<usize> for GraphicTile {
    type Output = GraphicsTilePos;

    fn index(&self, index: usize) -> &GraphicsTilePos {
        match index {
            0 => &self.top_left,
            1 => &self.top_right,
            2 => &self.bottom_right,
            3 => &self.bottom_left,
            _ => panic!("index out of bounds"),
        }
    }
}

impl IndexMut<usize> for GraphicTile {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.top_left,
            1 => &mut self.top_right,
            2 => &mut self.bottom_right,
            3 => &mut self.bottom_left,
            _ => panic!("index out of bounds"),
        }
    }
}

impl GraphicTile {
    pub(super) fn copy_into_slice(&self, dest: &mut [u8], textured: bool) -> usize {
        fn copy_pos_into_slice(pos: &GraphicsTilePos, dest: &mut [u8]) -> usize {
            let mut off: usize = 0;

            pos.x.to_ne_bytes().iter().for_each(|byte| {
                dest[off] = *byte;
                off += 1;
            });
            pos.y.to_ne_bytes().iter().for_each(|byte| {
                dest[off] = *byte;
                off += 1;
            });
            off
        }
        fn copy_tex_into_slice(tex: &GraphicsTileTex, dest: &mut [u8]) -> usize {
            let mut off: usize = 0;

            tex.x.to_ne_bytes().iter().for_each(|byte| {
                dest[off] = *byte;
                off += 1;
            });
            tex.y.to_ne_bytes().iter().for_each(|byte| {
                dest[off] = *byte;
                off += 1;
            });
            off
        }
        let mut off = 0;
        for index in 0..4 {
            off += copy_pos_into_slice(
                match index {
                    0 => &self.top_left,
                    1 => &self.top_right,
                    2 => &self.bottom_right,
                    3 => &self.bottom_left,
                    _ => panic!("out of bounds"),
                },
                &mut dest[off..],
            );
            if textured {
                off += copy_tex_into_slice(
                    match index {
                        0 => &self.tex_coord_top_left,
                        1 => &self.tex_coord_top_right,
                        2 => &self.tex_coord_bottom_right,
                        3 => &self.tex_coord_bottom_left,
                        _ => panic!("out of bounds"),
                    },
                    &mut dest[off..],
                );
            }
        }
        off
    }
}

fn fill_tmp_tile_speedup(
    tmp_tile: &mut GraphicTile,
    _flags: TileFlags,
    index: u8,
    x: i32,
    y: i32,
    angle_rotate: i16,
) {
    let angle = angle_rotate.unsigned_abs() % 360;
    fill_tmp_tile(
        tmp_tile,
        if angle >= 270 {
            rotation_270()
        } else if angle >= 180 {
            rotation_180()
        } else if angle >= 90 {
            ROTATION_90
        } else {
            TileFlags::empty()
        },
        if index == 0 {
            0
        } else {
            (angle % 90) as u8 + 1
        },
        x,
        y,
    );
}

pub fn tile_flags_to_uv(flags: TileFlags) -> (u8, u8, u8, u8, u8, u8, u8, u8) {
    let mut x0: u8 = 0;
    let mut y0: u8 = 0;
    let mut x1: u8 = x0 + 1;
    let mut y1: u8 = y0;
    let mut x2: u8 = x0 + 1;
    let mut y2: u8 = y0 + 1;
    let mut x3: u8 = x0;
    let mut y3: u8 = y0 + 1;

    if !(flags & TileFlags::XFLIP).is_empty() {
        x0 = x2;
        x1 = x3;
        x2 = x3;
        x3 = x0;
    }

    if !(flags & TileFlags::YFLIP).is_empty() {
        y0 = y3;
        y2 = y1;
        y3 = y1;
        y1 = y0;
    }

    if !(flags & TileFlags::ROTATE).is_empty() {
        let mut tmp = x0;
        x0 = x3;
        x3 = x2;
        x2 = x1;
        x1 = tmp;
        tmp = y0;
        y0 = y3;
        y3 = y2;
        y2 = y1;
        y1 = tmp;
    }

    (x0, y0, x1, y1, x2, y2, x3, y3)
}

fn fill_tmp_tile(tmp_tile: &mut GraphicTile, flags: TileFlags, index: u8, x: i32, y: i32) {
    // tile tex
    let (x0, y0, x1, y1, x2, y2, x3, y3) = tile_flags_to_uv(flags);

    tmp_tile.tex_coord_top_left.x |= x0;
    tmp_tile.tex_coord_top_left.x |= y0 << 1;
    tmp_tile.tex_coord_bottom_left.x |= x3;
    tmp_tile.tex_coord_bottom_left.x |= y3 << 1;
    tmp_tile.tex_coord_top_right.x |= x1;
    tmp_tile.tex_coord_top_right.x |= y1 << 1;
    tmp_tile.tex_coord_bottom_right.x |= x2;
    tmp_tile.tex_coord_bottom_right.x |= y2 << 1;

    tmp_tile.tex_coord_top_left.y = index;
    tmp_tile.tex_coord_bottom_left.y = index;
    tmp_tile.tex_coord_top_right.y = index;
    tmp_tile.tex_coord_bottom_right.y = index;

    // tile pos
    tmp_tile.top_left.x = x as u16;
    tmp_tile.top_left.y = y as u16;
    tmp_tile.bottom_left.x = x as u16;
    tmp_tile.bottom_left.y = (y + 1) as u16;
    tmp_tile.top_right.x = (x + 1) as u16;
    tmp_tile.top_right.y = y as u16;
    tmp_tile.bottom_right.x = (x + 1) as u16;
    tmp_tile.bottom_right.y = (y + 1) as u16;
}

pub(super) fn add_tile(
    tmp_tiles: &mut Vec<GraphicTile>,
    index: u8,
    flags: TileFlags,
    x: i32,
    y: i32,
    fill_speedup: bool,
    angle_rotate: i16,
    ignore_index_check: bool,
) -> bool {
    if index > 0 || ignore_index_check {
        let mut tile = GraphicTile::default();
        if fill_speedup {
            fill_tmp_tile_speedup(&mut tile, flags, index, x, y, angle_rotate);
        } else {
            fill_tmp_tile(&mut tile, flags, index, x, y);
        }
        tmp_tiles.push(tile);

        return true;
    }
    false
}
