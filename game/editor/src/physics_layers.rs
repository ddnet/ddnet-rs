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
struct PhysicsLayerTexturesDdnetLoading {
    game: GraphicsBackendMemory,
    front: GraphicsBackendMemory,
    tele: GraphicsBackendMemory,
    speedup: GraphicsBackendMemory,
    switch: GraphicsBackendMemory,
    tune: GraphicsBackendMemory,
}

#[derive(Debug, Hiarc)]
pub struct PhysicsLayerOverlaysDdnet {
    pub game: TextureContainer2dArray,
    pub front: TextureContainer2dArray,
    pub tele: TextureContainer2dArray,
    pub speedup: TextureContainer2dArray,
    pub switch: TextureContainer2dArray,
    pub tune: TextureContainer2dArray,
}

impl PhysicsLayerOverlaysDdnet {
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
                let editor_base: &Path = "editor/physics_layers/ddnet".as_ref();

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

                Ok(PhysicsLayerTexturesDdnetLoading {
                    game: load(
                        fs.read_file(&editor_base.join("game.png")).await?,
                        &tp,
                        &graphics_mt,
                    )?,
                    front: load(
                        fs.read_file(&editor_base.join("front.png")).await?,
                        &tp,
                        &graphics_mt,
                    )?,
                    tele: load(
                        fs.read_file(&editor_base.join("tele.png")).await?,
                        &tp,
                        &graphics_mt,
                    )?,
                    speedup: load(
                        fs.read_file(&editor_base.join("speedup.png")).await?,
                        &tp,
                        &graphics_mt,
                    )?,
                    switch: load(
                        fs.read_file(&editor_base.join("switch.png")).await?,
                        &tp,
                        &graphics_mt,
                    )?,
                    tune: load(
                        fs.read_file(&editor_base.join("tune.png")).await?,
                        &tp,
                        &graphics_mt,
                    )?,
                })
            })
            .get_storage()?;

        Ok(Rc::new(Self {
            game: graphics
                .texture_handle
                .load_texture_2d_array_rgba_u8(loading.game, "game")?,
            front: graphics
                .texture_handle
                .load_texture_2d_array_rgba_u8(loading.front, "front")?,
            tele: graphics
                .texture_handle
                .load_texture_2d_array_rgba_u8(loading.tele, "tele")?,
            speedup: graphics
                .texture_handle
                .load_texture_2d_array_rgba_u8(loading.speedup, "speedup")?,
            switch: graphics
                .texture_handle
                .load_texture_2d_array_rgba_u8(loading.switch, "switch")?,
            tune: graphics
                .texture_handle
                .load_texture_2d_array_rgba_u8(loading.tune, "tune")?,
        }))
    }
}
