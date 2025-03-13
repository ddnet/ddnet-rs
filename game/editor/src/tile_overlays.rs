use std::{path::Path, rc::Rc, sync::Arc};

use anyhow::anyhow;
use base_io::io::Io;
use graphics::{
    graphics::graphics::Graphics, graphics_mt::GraphicsMultiThreaded,
    handles::texture::texture::TextureContainer2dArray,
};
use graphics_types::{
    commands::TexFlags,
    types::{GraphicsBackendMemory, GraphicsMemoryAllocationType},
};
use hiarc::Hiarc;
use image_utils::{png::load_png_image_as_rgba, utils::texture_2d_to_3d};

#[derive(Debug)]
struct TileLayerTexturesDdnetLoading {
    index: GraphicsBackendMemory,
    flag: GraphicsBackendMemory,
}

#[derive(Debug, Hiarc)]
pub struct TileLayerOverlaysDdnet {
    pub index: TextureContainer2dArray,
    pub flag: TextureContainer2dArray,
}

impl TileLayerOverlaysDdnet {
    pub fn new(
        io: &Io,
        thread_pool: &Arc<rayon::ThreadPool>,
        graphics: &Graphics,
    ) -> anyhow::Result<Rc<Self>> {
        let fs = io.fs.clone();
        let tp = thread_pool.clone();
        let graphics_mt = graphics.get_graphics_mt();
        let loading = io
            .rt
            .spawn(async move {
                let editor_base: &Path = "editor/tile_layers".as_ref();

                fn load(
                    file: Vec<u8>,
                    thread_pool: &rayon::ThreadPool,
                    graphics_mt: &GraphicsMultiThreaded,
                ) -> anyhow::Result<GraphicsBackendMemory> {
                    let mut img = Vec::new();
                    let img = load_png_image_as_rgba(&file, |w, h, _| {
                        img = vec![0; w * h * 4];
                        &mut img
                    })?;

                    anyhow::ensure!(img.width % 16 == 0, "width not divislbe by 16");
                    anyhow::ensure!(img.height % 16 == 0, "width not divislbe by 16");
                    anyhow::ensure!(img.height > 0 && img.width > 0, "width or height 0");

                    let mut dst: Vec<u8> = vec![0; img.width as usize * img.height as usize * 4];
                    let mut dst_w = 0;
                    let mut dst_h = 0;

                    if !texture_2d_to_3d(
                        thread_pool,
                        img.data,
                        img.width as usize,
                        img.height as usize,
                        4,
                        16,
                        16,
                        &mut dst,
                        &mut dst_w,
                        &mut dst_h,
                    ) {
                        return Err(anyhow!(
                            "Failed to read editor physics layer, \
                            not convertable to 2d array texture."
                        ));
                    }

                    let width = dst_w.try_into()?;
                    let height = dst_h.try_into()?;

                    let mut mem =
                        graphics_mt.mem_alloc(GraphicsMemoryAllocationType::TextureRgbaU82dArray {
                            width,
                            height,
                            depth: 256.try_into().unwrap(),
                            flags: TexFlags::empty(),
                        });

                    mem.as_mut_slice().copy_from_slice(&dst);

                    let _ = graphics_mt.try_flush_mem(&mut mem, true);

                    Ok(mem)
                }

                Ok(TileLayerTexturesDdnetLoading {
                    index: load(
                        fs.read_file(&editor_base.join("index.png")).await?,
                        &tp,
                        &graphics_mt,
                    )?,
                    flag: load(
                        fs.read_file(&editor_base.join("flag.png")).await?,
                        &tp,
                        &graphics_mt,
                    )?,
                })
            })
            .get_storage()?;

        Ok(Rc::new(Self {
            index: graphics
                .texture_handle
                .load_texture_2d_array_rgba_u8(loading.index, "index")?,
            flag: graphics
                .texture_handle
                .load_texture_2d_array_rgba_u8(loading.flag, "flag")?,
        }))
    }
}
