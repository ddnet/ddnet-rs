use map::map::groups::layers::tiles::{rotation_180, rotation_270, TileFlags, ROTATION_90};
use math::math::vector::ubvec2;

type GraphicsTileTex = ubvec2;

#[repr(C)]
#[derive(Default)]
pub(super) struct GraphicTile {
    // use u32 directly and do bit shifting on the cpu
    pos_x_and_tex_coord: u32,
}

impl GraphicTile {
    pub(super) fn copy_into_slice(&self, dest: &mut [u8]) -> usize {
        fn copy_pos_into_slice(pos: &u32, dest: &mut [u8]) -> usize {
            let mut off: usize = 0;

            pos.to_ne_bytes().iter().for_each(|byte| {
                dest[off] = *byte;
                off += 1;
            });
            off
        }
        let mut off = 0;
        off += copy_pos_into_slice(&self.pos_x_and_tex_coord, &mut dest[off..]);
        off
    }
}

fn fill_tmp_tile_speedup(
    tmp_tile: &mut GraphicTile,
    _flags: TileFlags,
    index: u8,
    x: i32,
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

fn fill_tmp_tile(tmp_tile: &mut GraphicTile, flags: TileFlags, index: u8, x: i32) {
    // tile tex
    let (x0, y0, x1, y1, x2, y2, x3, y3) = tile_flags_to_uv(flags);

    let mut tex_coord = GraphicsTileTex::default();
    tex_coord.x |= x0;
    tex_coord.x |= y0 << 1;
    tex_coord.x |= x1 << 2;
    tex_coord.x |= y1 << 3;
    tex_coord.x |= x2 << 4;
    tex_coord.x |= y2 << 5;
    tex_coord.x |= x3 << 6;
    tex_coord.x |= y3 << 7;

    tex_coord.y = index;

    // tile pos
    let pos = x as u16;

    tmp_tile.pos_x_and_tex_coord =
        (pos as u32) | (((tex_coord.x as u32) | ((tex_coord.y as u32) << 8)) << 16);
}

pub(super) fn add_tile(
    tmp_tiles: &mut Vec<GraphicTile>,
    index: u8,
    flags: TileFlags,
    x: i32,
    fill_speedup: bool,
    angle_rotate: i16,
    ignore_index_check: bool,
) -> bool {
    if index > 0 || ignore_index_check {
        let mut tile = GraphicTile::default();
        if fill_speedup {
            fill_tmp_tile_speedup(&mut tile, flags, index, x, angle_rotate);
        } else {
            fill_tmp_tile(&mut tile, flags, index, x);
        }
        tmp_tiles.push(tile);

        return true;
    }
    false
}
