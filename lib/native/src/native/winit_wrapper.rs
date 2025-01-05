use std::cell::Cell;

use anyhow::anyhow;
use base::{benchmark::Benchmark, linked_hash_map_view::FxLinkedHashSet};
use native_display::{get_native_display_backend, NativeDisplayBackend};
use raw_window_handle::HasDisplayHandle;
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize, Size},
    error::ExternalError,
    event_loop::EventLoop,
    monitor::{MonitorHandle, VideoModeHandle},
    window::{CursorGrabMode, Fullscreen, Window, WindowAttributes},
};

use crate::native::app::{MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH};

use super::{
    app::NativeApp, FromNativeImpl, FromNativeLoadingImpl, NativeCreateOptions, NativeImpl,
    NativeWindowMonitorDetails, NativeWindowOptions,
};

struct WindowMouse {
    last_user_mouse_mode_request: CursorGrabMode,
    last_user_mouse_cursor_mode: bool,
    last_mouse_mode: CursorGrabMode,
    last_mouse_cursor_mode: bool,
    last_mouse_cursor_pos: (f64, f64),
    last_mouse_cursor_locked: bool,

    cursor_main_pos: (f64, f64),

    dbg_mode: bool,
}

impl WindowMouse {
    fn toggle_cursor_internal(&mut self, show: bool, window: &Window) -> bool {
        if self.last_mouse_cursor_mode != show && !self.dbg_mode {
            self.last_mouse_cursor_mode = show;
            window.set_cursor_visible(show);
            if !show {
                self.last_mouse_cursor_locked =
                    self.mouse_grab_internal(CursorGrabMode::Locked, window, None);

                self.last_mouse_cursor_pos = self.cursor_main_pos
            } else if self.last_mouse_cursor_locked {
                let _ = self.try_set_mouse_grab(window);
            } else if let Err(err) = window.set_cursor_position(PhysicalPosition::new(
                self.last_mouse_cursor_pos.0,
                self.last_mouse_cursor_pos.1,
            )) {
                log::info!("Failed to set cursor position: {err}");
            }

            true
        } else {
            true
        }
    }
    fn mouse_grab_internal(
        &mut self,
        mode: CursorGrabMode,
        window: &Window,
        internal_events: Option<&mut FxLinkedHashSet<InternalEvent>>,
    ) -> bool {
        if self.last_mouse_mode != mode && !self.dbg_mode {
            match window.set_cursor_grab(mode) {
                Ok(_) => {
                    self.last_mouse_mode = mode;
                    true
                }
                Err(err) => {
                    if !matches!(err, ExternalError::NotSupported(_)) {
                        if let Some(internal_events) = internal_events {
                            internal_events.insert(InternalEvent::MouseGrabWrong);
                        }
                    }
                    false
                }
            }
        } else {
            true
        }
    }

    fn try_set_mouse_grab(&mut self, window: &Window) -> anyhow::Result<()> {
        if self.last_mouse_mode != self.last_user_mouse_mode_request {
            if self.mouse_grab_internal(self.last_user_mouse_mode_request, window, None) {
                Ok(())
            } else {
                Err(anyhow!("mouse grab failed immediatelly."))
            }
        } else {
            Ok(())
        }
    }
}

pub(crate) struct WinitWindowWrapper {
    window: Window,

    mouse: WindowMouse,
    internal_events: FxLinkedHashSet<InternalEvent>,

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

        let video_mode = if let Some(monitor) = &monitor {
            monitor
                .video_modes()
                .find(|video_mode| {
                    video_mode.refresh_rate_millihertz() == wnd.refresh_rate_milli_hertz
                        && video_mode.size().width == wnd.width
                        && video_mode.size().height == wnd.height
                })
                .or_else(|| {
                    // try to find ignoring the refresh rate
                    monitor.video_modes().find(|video_mode| {
                        video_mode.size().width == wnd.width
                            && video_mode.size().height == wnd.height
                    })
                })
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
        if !wnd.fullscreen && wnd.maximized && !wnd.decorated {
            Some(winit::window::Fullscreen::Borderless(Some(monitor)))
        } else if wnd.fullscreen {
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
    fn mouse_grab(&mut self) {
        self.mouse.last_user_mouse_mode_request = CursorGrabMode::Confined;
        self.mouse.mouse_grab_internal(
            CursorGrabMode::Confined,
            &self.window,
            Some(&mut self.internal_events),
        );
    }
    fn toggle_cursor(&mut self, show: bool) {
        self.mouse.last_user_mouse_cursor_mode = show;
        self.mouse.toggle_cursor_internal(show, &self.window);
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
            let _ = self
                .window
                .request_inner_size(Size::Physical(winit::dpi::PhysicalSize {
                    width: wnd.width,
                    height: wnd.height,
                }));

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

        NativeWindowOptions {
            fullscreen: self
                .window
                .fullscreen()
                .is_some_and(|f| matches!(f, Fullscreen::Exclusive(_))),
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
            width: self.window.inner_size().width.max(MIN_WINDOW_WIDTH),
            height: self.window.inner_size().height.max(MIN_WINDOW_HEIGHT),
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

#[derive(Hash, PartialEq, Eq)]
enum InternalEvent {
    MouseGrabWrong,
}

pub(crate) struct WinitWrapper {}

impl WinitWrapper {
    pub fn create_event_loop<F, L>(
        native_options: &NativeCreateOptions,
        app: NativeApp,
        loading: &mut L,
    ) -> anyhow::Result<EventLoop<NativeApp>>
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

        impl<F, L> ApplicationHandler<NativeApp> for NativeUser<'_, F, L>
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
                                .with_inner_size(Size::Physical(winit::dpi::PhysicalSize {
                                    width: native_options.window.width,
                                    height: native_options.window.height,
                                }))
                                .with_maximized(native_options.window.maximized)
                                .with_decorations(native_options.window.decorated);
                        }
                        window_builder = window_builder.with_fullscreen(fullscreen_mode);

                        let window = event_loop.create_window(window_builder).unwrap();
                        benchmark.bench("initializing the window");
                        let mut window = WinitWindowWrapper {
                            window,
                            mouse: WindowMouse {
                                last_user_mouse_mode_request: CursorGrabMode::None,
                                last_user_mouse_cursor_mode: true,
                                last_mouse_mode: CursorGrabMode::None,
                                last_mouse_cursor_mode: true,
                                last_mouse_cursor_pos: Default::default(),
                                last_mouse_cursor_locked: false,
                                cursor_main_pos: Default::default(),

                                dbg_mode: native_options.dbg_input,
                            },
                            internal_events: Default::default(),
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
                {
                    if !native_user.raw_window_event(&window.window, &event) {
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
                            winit::event::WindowEvent::DroppedFile(_) => todo!(),
                            winit::event::WindowEvent::HoveredFile(_) => todo!(),
                            winit::event::WindowEvent::HoveredFileCancelled => todo!(),
                            winit::event::WindowEvent::Focused(has_focus) => {
                                if !has_focus {
                                    window.mouse.mouse_grab_internal(
                                        CursorGrabMode::None,
                                        &window.window,
                                        Some(&mut window.internal_events),
                                    );
                                } else {
                                    window.mouse.mouse_grab_internal(
                                        window.mouse.last_user_mouse_mode_request,
                                        &window.window,
                                        Some(&mut window.internal_events),
                                    );
                                }
                                native_user.window_options_changed(window.window_options());
                            } // TODO: also important for android
                            winit::event::WindowEvent::KeyboardInput {
                                device_id,
                                event,
                                is_synthetic: _,
                            } => {
                                if !event.repeat {
                                    match event.state {
                                        winit::event::ElementState::Pressed => native_user
                                            .key_down(
                                                &window.window,
                                                &device_id,
                                                event.physical_key,
                                            ),
                                        winit::event::ElementState::Released => native_user.key_up(
                                            &window.window,
                                            &device_id,
                                            event.physical_key,
                                        ),
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
                                native_user.mouse_move(
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
                            } => native_user.scroll(
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
                                winit::event::ElementState::Pressed => native_user.mouse_down(
                                    &window.window,
                                    &device_id,
                                    window.mouse.cursor_main_pos.0,
                                    window.mouse.cursor_main_pos.1,
                                    &button,
                                ),
                                winit::event::ElementState::Released => native_user.mouse_up(
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
                                native_user.mouse_down(
                                    &window.window,
                                    &touch.device_id,
                                    touch.location.x,
                                    touch.location.y,
                                    &winit::event::MouseButton::Left,
                                );
                                native_user.mouse_up(
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
                                window.internal_events.retain_with_order(|ev| match ev {
                                    InternalEvent::MouseGrabWrong => {
                                        window.mouse.try_set_mouse_grab(&window.window).is_err()
                                    }
                                });
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
                        } => native_user.mouse_move(
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
                if let Self::Some { window, .. } = self {
                    if window.destroy.get() {
                        event_loop.exit();
                    }
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
