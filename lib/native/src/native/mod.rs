use std::sync::Arc;

use app::NativeApp;
use base::system::SystemTime;
pub use native_display::NativeDisplayBackend;
use winit::monitor::MonitorHandle;

use crate::input::InputEventHandler;

use self::winit_wrapper::WinitWrapper;

pub mod app;
mod winit_wrapper;

pub use winit::dpi::PhysicalSize;
pub use winit::event::DeviceId;
pub use winit::event::MouseButton;
pub use winit::event::MouseScrollDelta;
pub use winit::window::Window;
pub use winit::{
    event::WindowEvent,
    keyboard::{KeyCode, PhysicalKey},
};

pub trait NativeImpl {
    /// If `true`: confines the mouse to the window rect (if supported).
    /// If `false`: the mouse can be moved out of the current window.
    ///
    /// If the operation fails, it queues the opeartion to a later cycle.
    ///
    /// This function caches the confine mode and can safely be called
    /// every frame.
    ///
    /// # Important
    ///
    /// If unsupported, then the mouse will not be confined!
    fn confine_mouse(&mut self, confined: bool);
    /// If `true`: hides the cursor and locks the absolute mouse to the current position.
    /// If `false`: shows the cursor, the absolute mouse can be moved freely
    /// (as free as specified in [`NativeImpl::confine_mouse`]).
    ///
    /// This function caches the cursor state and can safely be called
    /// every frame.
    ///
    /// # Important
    ///
    /// If unsupported, then the absolute mouse might still move around, but is _tried_
    /// to be confined to the current window and later teleported back to the locked
    /// position!
    fn relative_mouse(&mut self, relative: bool);
    /// Change the window config.
    /// Automatically only applies _actual_ changes.
    fn set_window_config(&mut self, wnd: NativeWindowOptions) -> anyhow::Result<()>;
    fn borrow_window(&self) -> &Window;
    fn monitors(&self) -> Vec<MonitorHandle>;
    fn window_options(&self) -> NativeWindowOptions;
    fn quit(&self);
    fn start_arguments(&self) -> &Vec<String>;
}

pub trait FromNativeImpl: InputEventHandler {
    fn run(&mut self, native: &mut dyn NativeImpl);
    /// New width and height in pixels!
    fn resized(&mut self, native: &mut dyn NativeImpl, new_width: u32, new_height: u32);
    /// The window options changed, usually the implementor does not need to do anything.
    /// But if it wants to serialize the current options it can do so.
    fn window_options_changed(&mut self, wnd: NativeWindowOptions);
    fn destroy(self);

    fn window_created_ntfy(&mut self, native: &mut dyn NativeImpl) -> anyhow::Result<()>;
    fn window_destroyed_ntfy(&mut self, native: &mut dyn NativeImpl) -> anyhow::Result<()>;
}

pub trait FromNativeLoadingImpl<L>
where
    Self: Sized,
{
    fn load_with_display_handle(
        loading: &mut L,
        display_handle: NativeDisplayBackend,
    ) -> anyhow::Result<()>;
    fn new(loading: L, native: &mut dyn NativeImpl) -> anyhow::Result<Self>;
}

#[derive(Debug)]
pub struct NativeWindowMonitorDetails {
    pub name: String,
    pub size: PhysicalSize<u32>,
}

#[derive(Debug)]
pub struct Pixels<T> {
    pub width: T,
    pub height: T,
}

pub type PhysicalPixels = Pixels<u32>;
pub type LogicalPixels = Pixels<f64>;

#[derive(Debug)]
pub enum WindowMode {
    Fullscreen {
        resolution: Option<PhysicalPixels>,
        /// If creating a fullscreen window fails, falls back to this
        /// windowed size instead.
        fallback_window: LogicalPixels,
    },
    Windowed(LogicalPixels),
}

impl WindowMode {
    pub fn is_fullscreen(&self) -> bool {
        matches!(self, Self::Fullscreen { .. })
    }
    pub fn is_windowed(&self) -> bool {
        matches!(self, Self::Windowed(_))
    }
}

#[derive(Debug)]
pub struct NativeWindowOptions {
    pub mode: WindowMode,
    /// if fullscreen is `false` & maximized is `true` & decorated is `false`
    /// => borderless fullscreen
    pub decorated: bool,
    pub maximized: bool,
    pub refresh_rate_milli_hertz: u32,
    pub monitor: Option<NativeWindowMonitorDetails>,
}

#[derive(Debug)]
pub struct NativeCreateOptions<'a> {
    pub do_bench: bool,
    pub dbg_input: bool,
    pub title: String,
    pub sys: &'a Arc<SystemTime>,
    pub start_arguments: Vec<String>,
    pub window: NativeWindowOptions,
}

pub struct Native {}

impl Native {
    pub fn run_loop<F, L>(
        native_user_loading: L,
        app: NativeApp,
        native_options: NativeCreateOptions,
    ) -> anyhow::Result<()>
    where
        F: FromNativeImpl + FromNativeLoadingImpl<L> + 'static,
    {
        WinitWrapper::run::<F, L>(native_options, app, native_user_loading)
    }
}
