pub mod canvas {
    use graphics_types::{
        commands::{
            AllCommands, CommandOffscreenCanvasCreate, CommandOffscreenCanvasDestroy,
            CommandOffscreenCanvasSkipFetchingOnce, CommandSwitchCanvasMode,
            CommandSwitchCanvasModeType, CommandUpdateViewport, CommandsMisc,
        },
        types::WindowProps,
    };
    use hiarc::{Hiarc, hiarc_safer_rc_refcell};

    use crate::handles::backend::backend::GraphicsBackendHandle;

    #[derive(Debug, Hiarc)]
    pub struct GraphicsCanvas {
        window_props: WindowProps,
    }

    #[derive(Debug, Hiarc)]
    pub struct GraphicsCanvasSetup {
        onscreen: GraphicsCanvas,
        offscreen: GraphicsCanvas,
    }

    #[derive(Debug, Hiarc)]
    pub enum GraphicsCanvasMode {
        Onscreen,
        Offscreen { offscreen_canvas: OffscreenCanvas },
    }

    #[derive(Debug, Hiarc, Clone, Copy)]
    pub struct GraphicsViewport {
        pub x: i32,
        pub y: i32,
        pub width: u32,
        pub height: u32,
    }

    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc)]
    pub struct GraphicsCanvasHandle {
        backend_handle: GraphicsBackendHandle,

        canvases: GraphicsCanvasSetup,
        cur_canvas_mode: GraphicsCanvasMode,

        offscreen_canvas_id_gen: u128,

        cur_dynamic_viewport: Option<GraphicsViewport>,
    }

    #[hiarc_safer_rc_refcell]
    impl GraphicsCanvasHandle {
        pub fn new(backend_handle: GraphicsBackendHandle, window_props: WindowProps) -> Self {
            Self {
                backend_handle,
                canvases: GraphicsCanvasSetup {
                    onscreen: GraphicsCanvas { window_props },
                    offscreen: GraphicsCanvas { window_props },
                },
                cur_canvas_mode: GraphicsCanvasMode::Onscreen,

                offscreen_canvas_id_gen: 0,

                cur_dynamic_viewport: None,
            }
        }

        pub fn resized(&mut self, window_props: WindowProps) {
            self.canvases.onscreen.window_props = window_props;
        }

        /// `pixels_per_point` is basically the DPI
        pub fn offscreen_canvas(
            &mut self,
            width: u32,
            height: u32,
            pixels_per_point: f64,
            has_multi_sampling: Option<u32>,
        ) -> OffscreenCanvas {
            let id = self.offscreen_canvas_id_gen;
            self.offscreen_canvas_id_gen += 1;
            OffscreenCanvas::new(
                id,
                self.backend_handle.clone(),
                width,
                height,
                pixels_per_point,
                has_multi_sampling,
            )
        }

        pub fn switch_canvas(&mut self, mode: GraphicsCanvasMode) {
            let switch_canvas = match &mode {
                GraphicsCanvasMode::Offscreen { offscreen_canvas } => {
                    let width = offscreen_canvas.width();
                    let height = offscreen_canvas.height();
                    let pixels_per_point = offscreen_canvas.pixels_per_point();
                    self.canvases.offscreen.window_props.window_width =
                        width as f64 / pixels_per_point;
                    self.canvases.offscreen.window_props.window_height =
                        height as f64 / pixels_per_point;
                    self.canvases.offscreen.window_props.canvas_width = width;
                    self.canvases.offscreen.window_props.canvas_height = height;
                    CommandSwitchCanvasModeType::Offscreen {
                        id: offscreen_canvas.get_index_unsafe(),
                    }
                }
                GraphicsCanvasMode::Onscreen => CommandSwitchCanvasModeType::Onscreen,
            };
            self.cur_canvas_mode = mode;
            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::SwitchCanvas(
                    CommandSwitchCanvasMode {
                        mode: switch_canvas,
                    },
                )));
        }

        /// update the viewport of the window's canvas where the origin is top left
        /// the dynamic viewport will affect calls to canvas_width-/height aswell
        /// as window_width-/height
        pub fn update_window_viewport(&mut self, x: i32, y: i32, width: u32, height: u32) {
            let cmd = CommandUpdateViewport {
                x,
                y,
                width,
                height,
            };
            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::UpdateViewport(cmd)));
            self.cur_dynamic_viewport = Some(GraphicsViewport {
                x,
                y,
                width,
                height,
            });
            let cur_canvas = &self.get_cur_canvas().window_props;
            if x == 0
                && y == 0
                && width == cur_canvas.canvas_width
                && height == cur_canvas.canvas_height
            {
                self.cur_dynamic_viewport = None;
            }
        }

        /// reset the viewport to the original window viewport
        pub fn reset_window_viewport(&mut self) {
            let window_props = self.window_props();
            self.update_window_viewport(0, 0, window_props.canvas_width, window_props.canvas_height)
        }

        fn get_cur_canvas(&self) -> &GraphicsCanvas {
            match self.cur_canvas_mode {
                GraphicsCanvasMode::Onscreen => &self.canvases.onscreen,
                GraphicsCanvasMode::Offscreen { .. } => &self.canvases.offscreen,
            }
        }

        /// get the current dynamic viewport, if any
        pub fn dynamic_viewport(&self) -> Option<GraphicsViewport> {
            self.cur_dynamic_viewport
        }

        /// the aspect of the window canvas, independent of the current viewport
        /// this function should generally __not__ be used over `canvas_aspect`,
        /// except you know what you are doing
        pub fn window_canvas_aspect(&self) -> f32 {
            let canvas = self.get_cur_canvas();
            canvas.window_props.canvas_width as f32 / canvas.window_props.canvas_height as f32
        }

        /// the width of the window canvas, independent of the current viewport
        /// this function should generally __not__ be used over `canvas_width`,
        /// except you know what you are doing
        pub fn window_canvas_width(&self) -> u32 {
            let canvas = self.get_cur_canvas();
            canvas.window_props.canvas_width
        }

        /// the height of the window canvas, independent of the current viewport
        /// this function should generally __not__ be used over `canvas_height`,
        /// except you know what you are doing
        pub fn window_canvas_height(&self) -> u32 {
            let canvas = self.get_cur_canvas();
            canvas.window_props.canvas_height
        }

        /// this is the aspect of the canvas you are currently able to draw on
        /// it respects the current mapped viewport
        /// generally you should use this function of `window_canvas_aspect` except
        /// you need to know the aspect of the _real_ canvas
        pub fn canvas_aspect(&self) -> f32 {
            self.cur_dynamic_viewport
                .as_ref()
                .map(|vp| (vp.width as f32 / vp.height as f32))
                .unwrap_or(self.window_canvas_aspect())
        }

        /// this is the width of the canvas you are currently able to draw on
        /// it respects the current mapped viewport
        /// generally you should use this function of `window_canvas_width` except
        /// you need to know the width of the _real_ canvas
        pub fn canvas_width(&self) -> u32 {
            self.cur_dynamic_viewport
                .as_ref()
                .map(|vp| vp.width)
                .unwrap_or(self.window_canvas_width())
        }

        /// this is the height of the canvas you are currently able to draw on
        /// it respects the current mapped viewport
        /// generally you should use this function of `window_canvas_height` except
        /// you need to know the height of the _real_ canvas
        pub fn canvas_height(&self) -> u32 {
            self.cur_dynamic_viewport
                .as_ref()
                .map(|vp| vp.height)
                .unwrap_or(self.window_canvas_height())
        }

        /// this function always respects the current viewport
        /// if you want to acess the real width use `window_props`
        pub fn window_width(&self) -> f32 {
            self.cur_dynamic_viewport
                .as_ref()
                .map(|vp| vp.width as f32 / self.pixels_per_point())
                .unwrap_or({
                    let canvas = self.get_cur_canvas();
                    canvas.window_props.window_width as f32 / self.pixels_per_point()
                })
        }

        /// this function always respects the current viewport
        /// if you want to acess the real height use `window_props`
        pub fn window_height(&self) -> f32 {
            self.cur_dynamic_viewport
                .as_ref()
                .map(|vp| vp.height as f32 / self.pixels_per_point())
                .unwrap_or({
                    let canvas = self.get_cur_canvas();
                    canvas.window_props.window_height as f32 / self.pixels_per_point()
                })
        }

        pub fn window_props(&self) -> WindowProps {
            let canvas = self.get_cur_canvas();
            canvas.window_props
        }

        pub fn pixels_per_point(&self) -> f32 {
            let canvas = self.get_cur_canvas();
            canvas.window_props.canvas_width as f32 / canvas.window_props.window_width as f32
        }
    }

    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc)]
    pub struct OffscreenCanvas {
        index: u128,
        backend_handle: GraphicsBackendHandle,

        width: u32,
        height: u32,
        pixels_per_point: f64,
    }

    #[hiarc_safer_rc_refcell]
    impl Drop for OffscreenCanvas {
        fn drop(&mut self) {
            let cmd = CommandOffscreenCanvasDestroy {
                offscreen_index: self.index,
            };
            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::OffscreenCanvasDestroy(cmd)));
        }
    }

    #[hiarc_safer_rc_refcell]
    impl OffscreenCanvas {
        pub fn new(
            index: u128,
            backend_handle: GraphicsBackendHandle,
            width: u32,
            height: u32,
            pixels_per_point: f64,
            has_multi_sampling: Option<u32>,
        ) -> Self {
            let cmd = CommandOffscreenCanvasCreate {
                offscreen_index: index,

                width,
                height,
                has_multi_sampling,
            };
            backend_handle.add_cmd(AllCommands::Misc(CommandsMisc::OffscreenCanvasCreate(cmd)));
            Self {
                index,
                backend_handle,

                width,
                height,
                pixels_per_point,
            }
        }

        /// Skip fetching until the next swap of the graphics
        /// chain was called -> disabling it for this frame.
        pub fn skip_fetching_once(&self) {
            let cmd = CommandOffscreenCanvasSkipFetchingOnce {
                offscreen_index: self.get_index_unsafe(),
            };

            self.backend_handle.add_cmd(AllCommands::Misc(
                CommandsMisc::OffscreenCanvasSkipFetchingOnce(cmd),
            ));
        }

        pub fn width(&self) -> u32 {
            self.width
        }
        pub fn height(&self) -> u32 {
            self.height
        }
        pub fn pixels_per_point(&self) -> f64 {
            self.pixels_per_point
        }

        pub fn get_index_unsafe(&self) -> u128 {
            self.index
        }
    }
}
