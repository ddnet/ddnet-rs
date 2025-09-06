pub mod graphics {
    use std::{
        cell::RefCell,
        fmt::Debug,
        rc::Rc,
        sync::{Arc, Mutex},
    };

    use graphics_backend_traits::{
        frame_fetcher_plugin::{BackendFrameFetcher, BackendPresentedImageDataRgba},
        traits::GraphicsBackendInterface,
    };
    use graphics_base_traits::traits::GraphicsStreamedData;
    use hiarc::Hiarc;
    use image_utils::png::save_png_image;

    use crate::{
        graphics_mt::GraphicsMultiThreaded,
        handles::{
            backend::backend::GraphicsBackendHandle,
            buffer_object::buffer_object::GraphicsBufferObjectHandle,
            canvas::canvas::GraphicsCanvasHandle,
            quad_container::quad_container::GraphicsQuadContainerHandle,
            shader_storage::shader_storage::GraphicsShaderStorageHandle,
            stream::stream::GraphicsStreamHandle, texture::texture::GraphicsTextureHandle,
        },
    };

    use graphics_types::{
        commands::{AllCommands, CommandMultiSampling, CommandVsync, CommandsMisc},
        types::{VideoMode, WindowProps},
    };

    const _FAKE_MODES: [VideoMode; 68] = [
        VideoMode::new(8192, 4320, 8192, 4320, 0, 8, 8, 8, 0),
        VideoMode::new(7680, 4320, 7680, 4320, 0, 8, 8, 8, 0),
        VideoMode::new(5120, 2880, 5120, 2880, 0, 8, 8, 8, 0),
        VideoMode::new(4096, 2160, 4096, 2160, 0, 8, 8, 8, 0),
        VideoMode::new(3840, 2160, 3840, 2160, 0, 8, 8, 8, 0),
        VideoMode::new(2560, 1440, 2560, 1440, 0, 8, 8, 8, 0),
        VideoMode::new(2048, 1536, 2048, 1536, 0, 8, 8, 8, 0),
        VideoMode::new(1920, 2400, 1920, 2400, 0, 8, 8, 8, 0),
        VideoMode::new(1920, 1440, 1920, 1440, 0, 8, 8, 8, 0),
        VideoMode::new(1920, 1200, 1920, 1200, 0, 8, 8, 8, 0),
        VideoMode::new(1920, 1080, 1920, 1080, 0, 8, 8, 8, 0),
        VideoMode::new(1856, 1392, 1856, 1392, 0, 8, 8, 8, 0),
        VideoMode::new(1800, 1440, 1800, 1440, 0, 8, 8, 8, 0),
        VideoMode::new(1792, 1344, 1792, 1344, 0, 8, 8, 8, 0),
        VideoMode::new(1680, 1050, 1680, 1050, 0, 8, 8, 8, 0),
        VideoMode::new(1600, 1200, 1600, 1200, 0, 8, 8, 8, 0),
        VideoMode::new(1600, 1000, 1600, 1000, 0, 8, 8, 8, 0),
        VideoMode::new(1440, 1050, 1440, 1050, 0, 8, 8, 8, 0),
        VideoMode::new(1440, 900, 1440, 900, 0, 8, 8, 8, 0),
        VideoMode::new(1400, 1050, 1400, 1050, 0, 8, 8, 8, 0),
        VideoMode::new(1368, 768, 1368, 768, 0, 8, 8, 8, 0),
        VideoMode::new(1280, 1024, 1280, 1024, 0, 8, 8, 8, 0),
        VideoMode::new(1280, 960, 1280, 960, 0, 8, 8, 8, 0),
        VideoMode::new(1280, 800, 1280, 800, 0, 8, 8, 8, 0),
        VideoMode::new(1280, 768, 1280, 768, 0, 8, 8, 8, 0),
        VideoMode::new(1152, 864, 1152, 864, 0, 8, 8, 8, 0),
        VideoMode::new(1024, 768, 1024, 768, 0, 8, 8, 8, 0),
        VideoMode::new(1024, 600, 1024, 600, 0, 8, 8, 8, 0),
        VideoMode::new(800, 600, 800, 600, 0, 8, 8, 8, 0),
        VideoMode::new(768, 576, 768, 576, 0, 8, 8, 8, 0),
        VideoMode::new(720, 400, 720, 400, 0, 8, 8, 8, 0),
        VideoMode::new(640, 480, 640, 480, 0, 8, 8, 8, 0),
        VideoMode::new(400, 300, 400, 300, 0, 8, 8, 8, 0),
        VideoMode::new(320, 240, 320, 240, 0, 8, 8, 8, 0),
        VideoMode::new(8192, 4320, 8192, 4320, 0, 5, 6, 5, 0),
        VideoMode::new(7680, 4320, 7680, 4320, 0, 5, 6, 5, 0),
        VideoMode::new(5120, 2880, 5120, 2880, 0, 5, 6, 5, 0),
        VideoMode::new(4096, 2160, 4096, 2160, 0, 5, 6, 5, 0),
        VideoMode::new(3840, 2160, 3840, 2160, 0, 5, 6, 5, 0),
        VideoMode::new(2560, 1440, 2560, 1440, 0, 5, 6, 5, 0),
        VideoMode::new(2048, 1536, 2048, 1536, 0, 5, 6, 5, 0),
        VideoMode::new(1920, 2400, 1920, 2400, 0, 5, 6, 5, 0),
        VideoMode::new(1920, 1440, 1920, 1440, 0, 5, 6, 5, 0),
        VideoMode::new(1920, 1200, 1920, 1200, 0, 5, 6, 5, 0),
        VideoMode::new(1920, 1080, 1920, 1080, 0, 5, 6, 5, 0),
        VideoMode::new(1856, 1392, 1856, 1392, 0, 5, 6, 5, 0),
        VideoMode::new(1800, 1440, 1800, 1440, 0, 5, 6, 5, 0),
        VideoMode::new(1792, 1344, 1792, 1344, 0, 5, 6, 5, 0),
        VideoMode::new(1680, 1050, 1680, 1050, 0, 5, 6, 5, 0),
        VideoMode::new(1600, 1200, 1600, 1200, 0, 5, 6, 5, 0),
        VideoMode::new(1600, 1000, 1600, 1000, 0, 5, 6, 5, 0),
        VideoMode::new(1440, 1050, 1440, 1050, 0, 5, 6, 5, 0),
        VideoMode::new(1440, 900, 1440, 900, 0, 5, 6, 5, 0),
        VideoMode::new(1400, 1050, 1400, 1050, 0, 5, 6, 5, 0),
        VideoMode::new(1368, 768, 1368, 768, 0, 5, 6, 5, 0),
        VideoMode::new(1280, 1024, 1280, 1024, 0, 5, 6, 5, 0),
        VideoMode::new(1280, 960, 1280, 960, 0, 5, 6, 5, 0),
        VideoMode::new(1280, 800, 1280, 800, 0, 5, 6, 5, 0),
        VideoMode::new(1280, 768, 1280, 768, 0, 5, 6, 5, 0),
        VideoMode::new(1152, 864, 1152, 864, 0, 5, 6, 5, 0),
        VideoMode::new(1024, 768, 1024, 768, 0, 5, 6, 5, 0),
        VideoMode::new(1024, 600, 1024, 600, 0, 5, 6, 5, 0),
        VideoMode::new(800, 600, 800, 600, 0, 5, 6, 5, 0),
        VideoMode::new(768, 576, 768, 576, 0, 5, 6, 5, 0),
        VideoMode::new(720, 400, 720, 400, 0, 5, 6, 5, 0),
        VideoMode::new(640, 480, 640, 480, 0, 5, 6, 5, 0),
        VideoMode::new(400, 300, 400, 300, 0, 5, 6, 5, 0),
        VideoMode::new(320, 240, 320, 240, 0, 5, 6, 5, 0),
    ];

    // Callback when requested screenshot is finished
    pub trait ScreenshotCb: Debug + 'static {
        fn on_screenshot(&self, png: anyhow::Result<Vec<u8>>);
    }

    #[derive(Debug, Default)]
    struct ScreenshotFetcher {
        data: Mutex<Option<anyhow::Result<Vec<u8>>>>,
    }

    impl BackendFrameFetcher for ScreenshotFetcher {
        fn next_frame(&self, frame_data: BackendPresentedImageDataRgba) {
            let BackendPresentedImageDataRgba {
                width,
                height,
                mut dest_data_buffer,
                ..
            } = frame_data;

            // clear alpha values, not desired for screenshot
            for y in 0..height as usize {
                for x in 0..width as usize {
                    let img_off: usize = (y * width as usize * 4) + (x * 4);

                    dest_data_buffer[img_off + 3] = 255;
                }
            }

            *self.data.lock().unwrap() = Some(save_png_image(&dest_data_buffer, width, height));
        }

        fn current_fetch_index(
            &self,
        ) -> graphics_backend_traits::frame_fetcher_plugin::FetchCanvasIndex {
            graphics_backend_traits::frame_fetcher_plugin::FetchCanvasIndex::Onscreen
        }

        fn fetch_err(&self, err: graphics_backend_traits::frame_fetcher_plugin::FetchCanvasError) {
            *self.data.lock().unwrap() = Some(Err(err.into()));
        }
    }

    type ScreenshotHelper = Option<(Box<dyn ScreenshotCb>, Arc<ScreenshotFetcher>)>;

    #[derive(Debug, Hiarc, Clone)]
    pub struct Graphics {
        pub backend_handle: GraphicsBackendHandle,

        pub canvas_handle: GraphicsCanvasHandle,

        pub quad_container_handle: GraphicsQuadContainerHandle,

        pub buffer_object_handle: GraphicsBufferObjectHandle,

        pub shader_storage_handle: GraphicsShaderStorageHandle,

        pub stream_handle: GraphicsStreamHandle,

        pub texture_handle: GraphicsTextureHandle,

        #[hiarc_skip_unsafe]
        pending_screenshot: Rc<RefCell<ScreenshotHelper>>,
    }

    impl Graphics {
        pub fn new(
            backend: Rc<dyn GraphicsBackendInterface>,
            stream_data: GraphicsStreamedData,
            window_props: WindowProps,
        ) -> Graphics {
            let backend_handle = GraphicsBackendHandle::new(backend);
            let buffer_object_handle = GraphicsBufferObjectHandle::new(backend_handle.clone());
            let shader_storage_handle = GraphicsShaderStorageHandle::new(backend_handle.clone());
            Graphics {
                // handles
                canvas_handle: GraphicsCanvasHandle::new(backend_handle.clone(), window_props),

                quad_container_handle: GraphicsQuadContainerHandle::new(
                    backend_handle.clone(),
                    buffer_object_handle.clone(),
                ),
                buffer_object_handle,
                shader_storage_handle,
                stream_handle: GraphicsStreamHandle::new(stream_data, backend_handle.clone()),
                texture_handle: GraphicsTextureHandle::new(backend_handle.clone()),
                backend_handle,

                pending_screenshot: Default::default(),
            }
        }

        pub fn get_graphics_mt(&self) -> GraphicsMultiThreaded {
            GraphicsMultiThreaded::new(self.backend_handle.backend.get_backend_mt())
        }

        pub fn resized(&self, window_props: WindowProps) {
            self.canvas_handle.resized(window_props)
        }

        pub fn check_pending_screenshot(&self) {
            if self
                .pending_screenshot
                .borrow()
                .as_ref()
                .is_some_and(|(_, s)| s.data.lock().unwrap().is_some())
                && let Some((screenshot_db, fetcher)) = self.pending_screenshot.borrow_mut().take()
            {
                let Some(data) = fetcher.data.lock().unwrap().take() else {
                    panic!("Logic error, screenshot must exist at this point")
                };
                screenshot_db.on_screenshot(data);
                self.backend_handle
                    .backend
                    .detach_frame_fetcher("screenshot".to_string())
                    .unwrap();
            }
        }

        pub fn swap(&self) {
            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::Swap));
            self.backend_handle
                .run_backend_buffer(self.stream_handle.stream_data());

            self.check_pending_screenshot();
        }

        pub fn do_screenshot<F: ScreenshotCb>(&self, f: F) -> anyhow::Result<()> {
            let fetcher = Arc::new(ScreenshotFetcher::default());
            let fetcher_local = fetcher.clone();
            self.backend_handle
                .backend
                .attach_frame_fetcher("screenshot".to_string(), fetcher)?;
            *self.pending_screenshot.borrow_mut() = Some((Box::new(f), fetcher_local));
            Ok(())
        }

        pub fn vsync(&self, on: bool) {
            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::VSync(CommandVsync { on })));
        }

        pub fn multi_sampling(&self, sample_count: u32) {
            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::Multisampling(
                    CommandMultiSampling { sample_count },
                )));
        }
    }

    impl Drop for Graphics {
        fn drop(&mut self) {
            self.backend_handle
                .run_backend_buffer(self.stream_handle.stream_data());
        }
    }
}
