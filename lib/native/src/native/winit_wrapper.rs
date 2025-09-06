use std::{cell::Cell, collections::VecDeque};

use anyhow::anyhow;
use base::benchmark::Benchmark;
use native_display::{NativeDisplayBackend, get_native_display_backend};
use raw_window_handle::HasDisplayHandle;
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize, Size},
    error::ExternalError,
    event_loop::EventLoop,
    monitor::{MonitorHandle, VideoModeHandle},
    window::{CursorGrabMode, Fullscreen, Window, WindowAttributes},
};

use crate::native::app::{
    ApplicationHandlerType, MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, NativeEventLoop,
};

use super::{
    FromNativeImpl, FromNativeLoadingImpl, NativeCreateOptions, NativeImpl,
    NativeWindowMonitorDetails, NativeWindowOptions, WindowMode, app::NativeApp,
};

#[derive(Debug)]
enum GrabModeResultInternal {
    /// Applied or ignored because same mode
    Applied,
    NotSupported,
    ShouldQueue(GrabMode),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum GrabModeResult {
    /// Applied or ignored because same mode
    Applied,
    NotSupported,
    Queued,
}

#[derive(Debug, Clone, Copy)]
struct GrabMode {
    mode: CursorGrabMode,
    /// fallback mode if the grab mode is not supported.
    fallback: Option<CursorGrabMode>,
}

struct WindowMouse {
    cur_grab_mode: CursorGrabMode,
    user_requested_is_confined: bool,
    cur_relative_cursor: bool,
    last_mouse_cursor_pos: (f64, f64),

    internal_events: VecDeque<InternalEvent>,

    cursor_main_pos: (f64, f64),

    dbg_mode: bool,
}

impl WindowMouse {
    fn toggle_relative_cursor_internal(&mut self, relative: bool, window: &Window) -> bool {
        if self.cur_relative_cursor != relative && !self.dbg_mode {
            self.cur_relative_cursor = relative;
            window.set_cursor_visible(!relative);
            if relative {
                self.mouse_grab_internal(
                    GrabMode {
                        mode: CursorGrabMode::Locked,
                        fallback: Some(CursorGrabMode::Confined),
                    },
                    window,
                );

                self.last_mouse_cursor_pos = self.cursor_main_pos;
            } else {
                let is_locked = self.cur_grab_mode == CursorGrabMode::Locked;
                let _ = self.mouse_grab_internal(
                    if self.user_requested_is_confined {
                        GrabMode {
                            mode: CursorGrabMode::Confined,
                            fallback: Some(CursorGrabMode::None),
                        }
                    } else {
                        GrabMode {
                            mode: CursorGrabMode::None,
                            fallback: None,
                        }
                    },
                    window,
                );
                if !is_locked
                    && let Err(err) = window.set_cursor_position(PhysicalPosition::new(
                        self.last_mouse_cursor_pos.0,
                        self.last_mouse_cursor_pos.1,
                    ))
                {
                    log::info!("Failed to set cursor position: {err}");
                }
            }

            true
        } else {
            true
        }
    }
    fn mouse_grab_apply_internal(
        &mut self,
        mode: GrabMode,
        window: &Window,
    ) -> GrabModeResultInternal {
        if self.cur_grab_mode != mode.mode && !self.dbg_mode {
            match window.set_cursor_grab(mode.mode) {
                Ok(_) => {
                    self.cur_grab_mode = mode.mode;
                    GrabModeResultInternal::Applied
                }
                Err(err) => {
                    if !matches!(err, ExternalError::NotSupported(_)) {
                        GrabModeResultInternal::ShouldQueue(mode)
                    } else if let Some(mode) = mode.fallback {
                        self.mouse_grab_apply_internal(
                            GrabMode {
                                mode,
                                fallback: None,
                            },
                            window,
                        )
                    } else {
                        GrabModeResultInternal::NotSupported
                    }
                }
            }
        } else {
            GrabModeResultInternal::Applied
        }
    }
    fn mouse_grab_internal(&mut self, mode: GrabMode, window: &Window) -> GrabModeResult {
        if !self.internal_events.is_empty() && !self.dbg_mode {
            self.internal_events
                .push_back(InternalEvent::MouseGrabWrong(mode));
            GrabModeResult::Queued
        } else {
            match self.mouse_grab_apply_internal(mode, window) {
                GrabModeResultInternal::Applied => GrabModeResult::Applied,
                GrabModeResultInternal::NotSupported => GrabModeResult::NotSupported,
                GrabModeResultInternal::ShouldQueue(mode) => {
                    self.internal_events
                        .push_back(InternalEvent::MouseGrabWrong(mode));
                    GrabModeResult::Queued
                }
            }
        }
    }
}

pub(crate) struct WinitWindowWrapper {
    window: Window,

    mouse: WindowMouse,

    destroy: Cell<bool>,
    start_arguments: Vec<String>,

    suspended: bool,

    use_non_exclusive_fullscreen: bool,
}

impl WinitWindowWrapper {
    fn find_monitor_and_video_mode(
        available_monitors: impl Fn() -> Box<dyn Iterator<Item = MonitorHandle>>,
        primary_monitor: Option<MonitorHandle>,
        wnd: &NativeWindowOptions,
    ) -> anyhow::Result<(MonitorHandle, Option<VideoModeHandle>)> {
        let monitor = available_monitors().find(|monitor| {
            monitor
                .name()
                .as_ref()
                .map(|name| (name.as_str(), monitor.size()))
                == wnd
                    .monitor
                    .as_ref()
                    .map(|monitor| (monitor.name.as_str(), monitor.size))
        });

        let video_mode = if let (Some(monitor), WindowMode::Fullscreen { resolution, .. }) =
            (&monitor, &wnd.mode)
        {
            resolution
                .as_ref()
                .and_then(|resolution| {
                    monitor
                        .video_modes()
                        .find(|video_mode| {
                            video_mode.refresh_rate_millihertz() == wnd.refresh_rate_milli_hertz
                                && video_mode.size().width == resolution.width
                                && video_mode.size().height == resolution.height
                        })
                        .or_else(|| {
                            // try to find ignoring the refresh rate
                            monitor.video_modes().find(|video_mode| {
                                video_mode.size().width == resolution.width
                                    && video_mode.size().height == resolution.height
                            })
                        })
                })
                .or(monitor.video_modes().next())
        } else {
            None
        };

        let Some(monitor) = monitor
            .or(primary_monitor)
            .or_else(|| available_monitors().next())
        else {
            return Err(anyhow!("no monitor found."));
        };
        Ok((monitor, video_mode))
    }

    fn fullscreen_mode(
        monitor: MonitorHandle,
        video_mode: Option<VideoModeHandle>,
        wnd: &NativeWindowOptions,
        use_non_exclusive_fullscreen: bool,
    ) -> Option<Fullscreen> {
        if wnd.mode.is_windowed() && wnd.maximized && !wnd.decorated {
            Some(winit::window::Fullscreen::Borderless(Some(monitor)))
        } else if wnd.mode.is_fullscreen() {
            if let Some(video_mode) = video_mode.or_else(|| {
                monitor.video_modes().max_by(|v1, v2| {
                    let size1 = v1.size();
                    let size2 = v2.size();
                    let mut cmp = size1.width.cmp(&size2.width);
                    if matches!(cmp, std::cmp::Ordering::Equal) {
                        cmp = size1.height.cmp(&size2.height);
                        if matches!(cmp, std::cmp::Ordering::Equal) {
                            cmp = v1
                                .refresh_rate_millihertz()
                                .cmp(&v2.refresh_rate_millihertz());
                        };
                    }
                    cmp
                })
            }) {
                Some(if use_non_exclusive_fullscreen {
                    winit::window::Fullscreen::Borderless(Some(monitor))
                } else {
                    winit::window::Fullscreen::Exclusive(video_mode)
                })
            } else {
                Some(winit::window::Fullscreen::Borderless(Some(monitor)))
            }
        } else {
            None
        }
    }
}

impl NativeImpl for WinitWindowWrapper {
    fn confine_mouse(&mut self, confined: bool) {
        if !self.mouse.cur_relative_cursor && self.mouse.user_requested_is_confined != confined {
            if confined {
                self.mouse.mouse_grab_internal(
                    GrabMode {
                        mode: CursorGrabMode::Confined,
                        fallback: Some(CursorGrabMode::None),
                    },
                    &self.window,
                );
            } else if !confined {
                self.mouse.mouse_grab_internal(
                    GrabMode {
                        mode: CursorGrabMode::None,
                        fallback: None,
                    },
                    &self.window,
                );
            }
        }
        self.mouse.user_requested_is_confined = confined;
    }
    fn relative_mouse(&mut self, relative: bool) {
        self.mouse
            .toggle_relative_cursor_internal(relative, &self.window);
    }
    fn set_window_config(&mut self, wnd: NativeWindowOptions) -> anyhow::Result<()> {
        let (monitor, video_mode) = WinitWindowWrapper::find_monitor_and_video_mode(
            || Box::new(self.window.available_monitors()),
            self.window.primary_monitor(),
            &wnd,
        )?;
        let fullscreen_mode =
            Self::fullscreen_mode(monitor, video_mode, &wnd, self.use_non_exclusive_fullscreen);
        if fullscreen_mode.is_none() {
            let _ = self.window.request_inner_size(match &wnd.mode {
                WindowMode::Fullscreen {
                    fallback_window: size,
                    ..
                }
                | WindowMode::Windowed(size) => Size::Logical(winit::dpi::LogicalSize {
                    width: size.width,
                    height: size.height,
                }),
            });

            self.window.set_maximized(wnd.maximized);
            self.window.set_decorations(wnd.decorated);
        }
        self.window.set_fullscreen(fullscreen_mode);

        Ok(())
    }
    fn borrow_window(&self) -> &Window {
        &self.window
    }
    fn monitors(&self) -> Vec<MonitorHandle> {
        self.window.available_monitors().collect()
    }
    fn window_options(&self) -> NativeWindowOptions {
        let (refresh_rate_milli_hertz, monitor_name) = self
            .window
            .current_monitor()
            .map(|monitor| {
                (
                    monitor.refresh_rate_millihertz().unwrap_or_default(),
                    monitor.name().map(|name| {
                        let size = monitor.size();
                        NativeWindowMonitorDetails { name, size }
                    }),
                )
            })
            .unwrap_or_default();

        let pixels = super::Pixels {
            width: self.window.inner_size().width.max(MIN_WINDOW_WIDTH),
            height: self.window.inner_size().height.max(MIN_WINDOW_HEIGHT),
        };
        let scale_factor = self.window.scale_factor();
        let logical_pixels = super::Pixels {
            width: pixels.width as f64 / scale_factor,
            height: pixels.height as f64 / scale_factor,
        };
        NativeWindowOptions {
            mode: if self
                .window
                .fullscreen()
                .is_some_and(|f| matches!(f, Fullscreen::Exclusive(_)))
            {
                WindowMode::Fullscreen {
                    resolution: Some(pixels),
                    fallback_window: logical_pixels,
                }
            } else {
                WindowMode::Windowed(logical_pixels)
            },
            decorated: self.window.is_decorated()
                && !self
                    .window
                    .fullscreen()
                    .is_some_and(|f| matches!(f, Fullscreen::Borderless(_))),
            maximized: self.window.is_maximized()
                || self
                    .window
                    .fullscreen()
                    .is_some_and(|f| matches!(f, Fullscreen::Borderless(_))),
            refresh_rate_milli_hertz,
            monitor: monitor_name,
        }
    }
    fn quit(&self) {
        self.destroy.set(true);
    }
    fn start_arguments(&self) -> &Vec<String> {
        &self.start_arguments
    }
}

#[derive(Debug)]
enum InternalEvent {
    MouseGrabWrong(GrabMode),
}

pub(crate) struct WinitWrapper {}

impl WinitWrapper {
    pub fn create_event_loop<F, L>(
        native_options: &NativeCreateOptions,
        app: NativeApp,
        loading: &mut L,
    ) -> anyhow::Result<NativeEventLoop>
    where
        L: Sized,
        F: FromNativeLoadingImpl<L>,
    {
        let benchmark = Benchmark::new(native_options.do_bench);
        let init_event_loop = || {
            #[cfg_attr(not(target_os = "android"), allow(clippy::let_unit_value))]
            let _ = app;
            #[cfg(not(target_os = "android"))]
            let event_loop = EventLoop::new()?;
            #[cfg(target_os = "android")]
            use winit::platform::android::EventLoopBuilderExtAndroid;
            #[cfg(target_os = "android")]
            let event_loop = EventLoop::with_user_event().with_android_app(app).build()?;
            benchmark.bench("initializing the event loop");
            anyhow::Ok(event_loop)
        };
        let event_loop = match get_native_display_backend() {
            Ok(backend_display) => {
                F::load_with_display_handle(loading, backend_display)?;
                init_event_loop()?
            }
            Err(err) => {
                log::error!("{err}");
                let event_loop = init_event_loop()?;

                F::load_with_display_handle(
                    loading,
                    NativeDisplayBackend::Unknown(
                        event_loop
                            .display_handle()
                            .map_err(|err| {
                                anyhow!("failed to get display handle for load operation: {err}")
                            })?
                            .as_raw(),
                    ),
                )?;

                event_loop
            }
        };
        benchmark.bench("user loading with display handle");

        Ok(event_loop)
    }

    pub(crate) fn run<'a, F, L>(
        native_options: NativeCreateOptions<'a>,
        app: NativeApp,
        mut native_user_loading: L,
    ) -> anyhow::Result<()>
    where
        F: FromNativeImpl + FromNativeLoadingImpl<L> + 'static,
    {
        let event_loop =
            Self::create_event_loop::<F, L>(&native_options, app, &mut native_user_loading)?;

        enum NativeUser<'a, F, L> {
            Some {
                user: F,
                window: WinitWindowWrapper,
            },
            Wait {
                loading: L,
                native_options: NativeCreateOptions<'a>,
            },
            None,
        }

        impl<F, L> ApplicationHandler<ApplicationHandlerType> for NativeUser<'_, F, L>
        where
            F: FromNativeImpl + FromNativeLoadingImpl<L> + 'static,
        {
            fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
                event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
                let selfi = std::mem::replace(self, Self::None);
                *self = match selfi {
                    NativeUser::Some {
                        mut window,
                        mut user,
                    } => {
                        window.window = event_loop
                            .create_window(WindowAttributes::default())
                            .unwrap();

                        window.window.request_redraw();

                        let inner_size = window.borrow_window().inner_size().clamp(
                            PhysicalSize {
                                width: MIN_WINDOW_WIDTH,
                                height: MIN_WINDOW_HEIGHT,
                            },
                            PhysicalSize {
                                width: u32::MAX,
                                height: u32::MAX,
                            },
                        );
                        if let Err(err) = user.window_created_ntfy(&mut window) {
                            log::error!("{err}");
                        }
                        user.resized(&mut window, inner_size.width, inner_size.height);
                        user.window_options_changed(window.window_options());

                        window.suspended = false;
                        Self::Some { user, window }
                    }
                    NativeUser::Wait {
                        loading: native_user_loading,
                        native_options,
                    } => {
                        let benchmark = Benchmark::new(native_options.do_bench);
                        let (monitor, video_mode) =
                            WinitWindowWrapper::find_monitor_and_video_mode(
                                || Box::new(event_loop.available_monitors()),
                                event_loop.primary_monitor(),
                                &native_options.window,
                            )
                            .unwrap();

                        #[cfg(target_os = "linux")]
                        fn use_non_exclusive_fullscreen(
                            event_loop: &winit::event_loop::ActiveEventLoop,
                        ) -> bool {
                            // wayland is so cool
                            use winit::platform::wayland::ActiveEventLoopExtWayland;
                            event_loop.is_wayland()
                        }
                        // i love windows: https://github.com/rust-windowing/winit/issues/3124
                        #[cfg(target_os = "windows")]
                        fn use_non_exclusive_fullscreen(
                            _event_loop: &winit::event_loop::ActiveEventLoop,
                        ) -> bool {
                            true
                        }
                        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
                        fn use_non_exclusive_fullscreen(
                            _event_loop: &winit::event_loop::ActiveEventLoop,
                        ) -> bool {
                            false
                        }

                        let use_non_exclusive_fullscreen = use_non_exclusive_fullscreen(event_loop);

                        let fullscreen_mode = WinitWindowWrapper::fullscreen_mode(
                            monitor,
                            video_mode,
                            &native_options.window,
                            use_non_exclusive_fullscreen,
                        );

                        let mut window_builder = winit::window::WindowAttributes::default()
                            .with_title(native_options.title)
                            .with_resizable(true)
                            .with_active(true)
                            .with_min_inner_size(Size::Physical(winit::dpi::PhysicalSize {
                                width: MIN_WINDOW_WIDTH,
                                height: MIN_WINDOW_HEIGHT,
                            }))
                            .with_theme(Some(winit::window::Theme::Dark));
                        if fullscreen_mode.is_none() {
                            window_builder = window_builder
                                .with_inner_size(match &native_options.window.mode {
                                    WindowMode::Fullscreen {
                                        fallback_window: pixels,
                                        ..
                                    }
                                    | WindowMode::Windowed(pixels) => {
                                        Size::Logical(winit::dpi::LogicalSize {
                                            width: pixels.width,
                                            height: pixels.height,
                                        })
                                    }
                                })
                                .with_maximized(native_options.window.maximized)
                                .with_decorations(native_options.window.decorated);
                        }
                        window_builder = window_builder.with_fullscreen(fullscreen_mode);

                        let window = event_loop.create_window(window_builder).unwrap();
                        benchmark.bench("initializing the window");
                        let mut window = WinitWindowWrapper {
                            window,
                            mouse: WindowMouse {
                                cur_grab_mode: CursorGrabMode::None,
                                user_requested_is_confined: false,
                                cur_relative_cursor: false,
                                last_mouse_cursor_pos: Default::default(),
                                cursor_main_pos: Default::default(),

                                dbg_mode: native_options.dbg_input,
                                internal_events: Default::default(),
                            },
                            destroy: Default::default(),
                            start_arguments: native_options.start_arguments,
                            suspended: false,

                            use_non_exclusive_fullscreen,
                        };
                        window.window.request_redraw();
                        let user = F::new(native_user_loading, &mut window).unwrap();
                        Self::Some { user, window }
                    }
                    NativeUser::None => {
                        // about to exit, don't do anything
                        Self::None
                    }
                }
            }

            fn suspended(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
                event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
                if let Self::Some {
                    user: native_user,
                    window,
                } = self
                {
                    if let Err(err) = native_user.window_destroyed_ntfy(window) {
                        log::error!(target: "native", "{err}");
                        window.destroy.set(true);
                    }
                    window.suspended = true;
                }
            }

            fn window_event(
                &mut self,
                event_loop: &winit::event_loop::ActiveEventLoop,
                _window_id: winit::window::WindowId,
                event: winit::event::WindowEvent,
            ) {
                // https://github.com/rust-windowing/winit/issues/3092
                // -> https://github.com/emilk/egui/issues/5008
                #[cfg(target_os = "linux")]
                {
                    if matches!(event, winit::event::WindowEvent::Ime(_)) {
                        return;
                    }
                }

                if let Self::Some {
                    user: native_user,
                    window,
                } = self
                    && !native_user
                        .as_mut()
                        .raw_window_event(&window.window, &event)
                {
                    match event {
                        winit::event::WindowEvent::Resized(new_size) => {
                            native_user.resized(window, new_size.width, new_size.height);
                            native_user.window_options_changed(window.window_options());
                        }
                        winit::event::WindowEvent::Moved(_) => {
                            native_user.window_options_changed(window.window_options());
                        }
                        winit::event::WindowEvent::CloseRequested => {
                            event_loop.exit();
                        }
                        winit::event::WindowEvent::Destroyed => {} // TODO: important for android
                        winit::event::WindowEvent::DroppedFile(path) => {
                            native_user.file_dropped(path);
                            native_user.file_hovered(None);
                        }
                        winit::event::WindowEvent::HoveredFile(path) => {
                            native_user.file_hovered(Some(path));
                        }
                        winit::event::WindowEvent::HoveredFileCancelled => {
                            native_user.file_hovered(None);
                        }
                        winit::event::WindowEvent::Focused(has_focus) => {
                            if !has_focus {
                                window.mouse.mouse_grab_internal(
                                    GrabMode {
                                        mode: CursorGrabMode::None,
                                        fallback: None,
                                    },
                                    &window.window,
                                );
                            } else {
                                window.mouse.mouse_grab_internal(
                                    if window.mouse.cur_relative_cursor {
                                        GrabMode {
                                            mode: CursorGrabMode::Locked,
                                            fallback: Some(CursorGrabMode::Confined),
                                        }
                                    } else if window.mouse.user_requested_is_confined {
                                        GrabMode {
                                            mode: CursorGrabMode::Confined,
                                            fallback: Some(CursorGrabMode::None),
                                        }
                                    } else {
                                        GrabMode {
                                            mode: CursorGrabMode::None,
                                            fallback: None,
                                        }
                                    },
                                    &window.window,
                                );
                            }
                            native_user.window_options_changed(window.window_options());
                            native_user.focus_changed(has_focus);
                        } // TODO: also important for android
                        winit::event::WindowEvent::KeyboardInput {
                            device_id,
                            event,
                            is_synthetic: _,
                        } => {
                            if !event.repeat {
                                match event.state {
                                    winit::event::ElementState::Pressed => native_user
                                        .as_mut()
                                        .key_down(&window.window, &device_id, event.physical_key),
                                    winit::event::ElementState::Released => native_user
                                        .as_mut()
                                        .key_up(&window.window, &device_id, event.physical_key),
                                }
                            }
                        }
                        winit::event::WindowEvent::ModifiersChanged(_) => {}
                        winit::event::WindowEvent::Ime(_) => {}
                        winit::event::WindowEvent::CursorMoved {
                            device_id,
                            position,
                        } => {
                            window.mouse.cursor_main_pos = (position.x, position.y);
                            native_user.as_mut().mouse_move(
                                &window.window,
                                &device_id,
                                position.x,
                                position.y,
                                0.0,
                                0.0,
                            )
                        }
                        winit::event::WindowEvent::CursorEntered { device_id: _ } => {}
                        winit::event::WindowEvent::CursorLeft { device_id: _ } => {}
                        winit::event::WindowEvent::MouseWheel {
                            device_id,
                            delta,
                            phase: _,
                            ..
                        } => native_user.as_mut().scroll(
                            &window.window,
                            &device_id,
                            window.mouse.cursor_main_pos.0,
                            window.mouse.cursor_main_pos.1,
                            &delta,
                        ),
                        winit::event::WindowEvent::MouseInput {
                            device_id,
                            state,
                            button,
                        } => match state {
                            winit::event::ElementState::Pressed => native_user.as_mut().mouse_down(
                                &window.window,
                                &device_id,
                                window.mouse.cursor_main_pos.0,
                                window.mouse.cursor_main_pos.1,
                                &button,
                            ),
                            winit::event::ElementState::Released => native_user.as_mut().mouse_up(
                                &window.window,
                                &device_id,
                                window.mouse.cursor_main_pos.0,
                                window.mouse.cursor_main_pos.1,
                                &button,
                            ),
                        },
                        winit::event::WindowEvent::TouchpadPressure {
                            device_id: _,
                            pressure: _,
                            stage: _,
                        } => {}
                        winit::event::WindowEvent::AxisMotion {
                            device_id: _,
                            axis: _,
                            value: _,
                        } => {}
                        winit::event::WindowEvent::Touch(touch) => {
                            native_user.as_mut().mouse_down(
                                &window.window,
                                &touch.device_id,
                                touch.location.x,
                                touch.location.y,
                                &winit::event::MouseButton::Left,
                            );
                            native_user.as_mut().mouse_up(
                                &window.window,
                                &touch.device_id,
                                touch.location.x,
                                touch.location.y,
                                &winit::event::MouseButton::Left,
                            );
                        }
                        winit::event::WindowEvent::ScaleFactorChanged {
                            scale_factor: _,
                            inner_size_writer: _,
                        } => {
                            // TODO
                            let inner_size = window.borrow_window().inner_size().clamp(
                                PhysicalSize {
                                    width: MIN_WINDOW_WIDTH,
                                    height: MIN_WINDOW_HEIGHT,
                                },
                                PhysicalSize {
                                    width: u32::MAX,
                                    height: u32::MAX,
                                },
                            );
                            native_user.resized(window, inner_size.width, inner_size.height);
                            native_user.window_options_changed(window.window_options());
                        }
                        winit::event::WindowEvent::ThemeChanged(_) => {
                            // not really interesting
                        }
                        winit::event::WindowEvent::Occluded(_) => {}
                        winit::event::WindowEvent::ActivationTokenDone {
                            serial: _,
                            token: _,
                        } => {
                            // no idea what this is
                        }
                        winit::event::WindowEvent::RedrawRequested => {
                            native_user.run(window);

                            if !window.suspended {
                                window.window.request_redraw();
                            }

                            // check internal events
                            if let Some(ev) = window.mouse.internal_events.pop_front() {
                                match ev {
                                    InternalEvent::MouseGrabWrong(mode) => {
                                        match window
                                            .mouse
                                            .mouse_grab_apply_internal(mode, &window.window)
                                        {
                                            GrabModeResultInternal::Applied
                                            | GrabModeResultInternal::NotSupported => {
                                                // ignore -> drop event
                                            }
                                            GrabModeResultInternal::ShouldQueue(mode) => {
                                                window.mouse.internal_events.push_front(
                                                    InternalEvent::MouseGrabWrong(mode),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        winit::event::WindowEvent::PinchGesture { .. } => {
                            todo!("should be implemented for macos support")
                        }
                        winit::event::WindowEvent::PanGesture { .. } => {
                            todo!("should be implemented for macos support")
                        }
                        winit::event::WindowEvent::DoubleTapGesture { .. } => {
                            todo!("should be implemented for macos support")
                        }
                        winit::event::WindowEvent::RotationGesture { .. } => {
                            todo!("should be implemented for macos support")
                        }
                    }
                }
            }

            fn device_event(
                &mut self,
                _event_loop: &winit::event_loop::ActiveEventLoop,
                device_id: winit::event::DeviceId,
                event: winit::event::DeviceEvent,
            ) {
                if let Self::Some {
                    user: native_user,
                    window,
                } = self
                {
                    match event {
                        winit::event::DeviceEvent::Added => {
                            // TODO:
                        }
                        winit::event::DeviceEvent::Removed => {
                            // TODO:
                        }
                        winit::event::DeviceEvent::MouseMotion {
                            delta: (delta_x, delta_y),
                        } => native_user.as_mut().mouse_move(
                            &window.window,
                            &device_id,
                            window.mouse.cursor_main_pos.0,
                            window.mouse.cursor_main_pos.1,
                            delta_x,
                            delta_y,
                        ),
                        winit::event::DeviceEvent::MouseWheel { .. } => {
                            /* TODO: the other mouse wheel event sends the opposite native_user.scroll(
                                device_id,
                                window.mouse.cursor_main_pos.0,
                                window.mouse.cursor_main_pos.1,
                                delta,
                            );*/
                        }
                        winit::event::DeviceEvent::Motion { axis: _, value: _ } => {}
                        winit::event::DeviceEvent::Button { button: _, state } => match state {
                            winit::event::ElementState::Pressed => {}
                            winit::event::ElementState::Released => {}
                        },
                        winit::event::DeviceEvent::Key(key_input) => match key_input.state {
                            winit::event::ElementState::Pressed => {}
                            winit::event::ElementState::Released => {}
                        },
                    }
                }
            }

            fn exiting(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
                if let Self::Some { window, .. } = self {
                    window.destroy.set(true);
                }
            }

            fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {}

            fn new_events(
                &mut self,
                event_loop: &winit::event_loop::ActiveEventLoop,
                _cause: winit::event::StartCause,
            ) {
                if let Self::Some { window, .. } = self
                    && window.destroy.get()
                {
                    event_loop.exit();
                }
            }
        }

        let mut native_user: NativeUser<'a, F, L> = NativeUser::Wait {
            loading: native_user_loading,
            native_options,
        };

        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
        event_loop.run_app(&mut native_user)?;

        match std::mem::replace(&mut native_user, NativeUser::None) {
            NativeUser::Some { user, .. } => {
                user.destroy();
            }
            NativeUser::Wait { .. } | NativeUser::None => {
                // nothing to do
            }
        }
        Ok(())
    }
}
