use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use config::config::{ConfigEngine, ConfigMonitor};
use graphics::graphics::graphics::Graphics;
use native::{
    input::InputEventHandler,
    native::{FromNativeImpl, NativeImpl, NativeWindowOptions, WindowMode},
};

use crate::{backend::GraphicsBackend, window::BackendWindow};

/// A helper function for clients to notify the graphics about the resize.
/// And update the config values properly.
pub fn client_graphics_resized_update_config(
    graphics: &Graphics,
    graphics_backend: &GraphicsBackend,
    config: &mut ConfigEngine,
    native: &mut dyn NativeImpl,
    new_width: u32,
    new_height: u32,
) {
    let window_props = graphics_backend.resized(
        &graphics.backend_handle.backend_cmds,
        graphics.stream_handle.stream_data(),
        native,
        new_width,
        new_height,
    );
    graphics.resized(window_props);
    // update config variables
    let wnd = &mut config.wnd;
    let window = native.borrow_window();
    if wnd.fullscreen {
        wnd.fullscreen_width = new_width;
        wnd.fullscreen_height = new_height;
    } else {
        let scale_factor = window.scale_factor();
        wnd.window_width = new_width as f64 / scale_factor;
        wnd.window_height = new_height as f64 / scale_factor;
    }
    if let Some(monitor) = window.current_monitor() {
        wnd.refresh_rate_mhz = monitor
            .refresh_rate_millihertz()
            .unwrap_or(wnd.refresh_rate_mhz);
    }
}

/// A helper function for clients to update the config values properly,
/// after the window props changed.
pub fn client_window_props_changed_update_config(
    config: &mut ConfigEngine,
    wnd: NativeWindowOptions,
) {
    let config_wnd = &mut config.wnd;
    config_wnd.fullscreen = wnd.mode.is_fullscreen();
    config_wnd.decorated = wnd.decorated;
    config_wnd.maximized = wnd.maximized;
    match wnd.mode {
        WindowMode::Fullscreen {
            resolution,
            fallback_window,
        } => {
            if let Some(resolution) = resolution {
                config_wnd.fullscreen_width = resolution.width;
                config_wnd.fullscreen_height = resolution.height;
            }

            config_wnd.window_width = fallback_window.width;
            config_wnd.window_height = fallback_window.height;
        }
        WindowMode::Windowed(pixels) => {
            config_wnd.window_width = pixels.width;
            config_wnd.window_height = pixels.height;
        }
    }
    config_wnd.refresh_rate_mhz = wnd.refresh_rate_milli_hertz;
    config_wnd.monitor = wnd
        .monitor
        .map(|monitor| ConfigMonitor {
            name: monitor.name,
            width: monitor.size.width,
            height: monitor.size.height,
        })
        .unwrap_or_default();
}

pub fn client_graphics_window_created_ntfy(
    graphics_backend: &GraphicsBackend,
    native: &mut dyn NativeImpl,
    config: &ConfigEngine,
) -> anyhow::Result<()> {
    graphics_backend.window_created_ntfy(
        BackendWindow::Winit {
            window: native.borrow_window(),
        },
        &config.dbg,
    )
}

pub fn client_graphics_window_destroyed_ntfy(
    graphics_backend: &GraphicsBackend,
    _native: &mut dyn NativeImpl,
) -> anyhow::Result<()> {
    graphics_backend.window_destroyed_ntfy()
}

pub trait AppWithGraphics {
    fn get_graphics_data(&mut self) -> (&Graphics, &GraphicsBackend, &mut ConfigEngine);

    // Copied from `FromNativeImpl`
    fn run(&mut self, native: &mut dyn NativeImpl);
    /// New width and height in pixels!
    fn resized(&mut self, _native: &mut dyn NativeImpl, _new_width: u32, _new_height: u32) {}
    /// The window options changed, usually the implementor does not need to do anything.
    /// But if it wants to serialize the current options it can do so.
    fn window_options_changed(&mut self, _wnd: NativeWindowOptions) {}
    fn destroy(self);

    /// The app lost or gained focus.
    fn focus_changed(&mut self, _focused: bool) {}

    /// File was dropped into the app.
    fn file_dropped(&mut self, _file: PathBuf) {}

    /// None if hovered was ended.
    fn file_hovered(&mut self, _file: Option<PathBuf>) {}

    fn window_created_ntfy(&mut self, _native: &mut dyn NativeImpl) -> anyhow::Result<()> {
        Ok(())
    }
    fn window_destroyed_ntfy(&mut self, _native: &mut dyn NativeImpl) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct GraphicsApp<T>(T);

impl<T: InputEventHandler> AsMut<dyn InputEventHandler> for GraphicsApp<T> {
    fn as_mut(&mut self) -> &mut dyn InputEventHandler {
        &mut self.0
    }
}

impl<T> GraphicsApp<T> {
    pub fn new(inner: T) -> Self {
        Self(inner)
    }
}

impl<T> Deref for GraphicsApp<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for GraphicsApp<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: AppWithGraphics + InputEventHandler> FromNativeImpl for GraphicsApp<T> {
    fn run(&mut self, native: &mut dyn NativeImpl) {
        self.0.run(native)
    }
    fn resized(&mut self, native: &mut dyn NativeImpl, new_width: u32, new_height: u32) {
        let (graphics, graphics_backend, config) = self.0.get_graphics_data();
        client_graphics_resized_update_config(
            graphics,
            graphics_backend,
            config,
            native,
            new_width,
            new_height,
        );
        self.0.resized(native, new_width, new_height)
    }
    fn window_options_changed(&mut self, wnd: NativeWindowOptions) {
        let (_, _, config) = self.0.get_graphics_data();
        client_window_props_changed_update_config(config, wnd.clone());
        self.0.window_options_changed(wnd)
    }
    fn destroy(self) {
        self.0.destroy()
    }

    fn focus_changed(&mut self, focused: bool) {
        self.0.focus_changed(focused)
    }
    fn file_dropped(&mut self, file: PathBuf) {
        self.0.file_dropped(file)
    }
    fn file_hovered(&mut self, file: Option<PathBuf>) {
        self.0.file_hovered(file);
    }

    fn window_created_ntfy(&mut self, native: &mut dyn NativeImpl) -> anyhow::Result<()> {
        let (_, graphics_backend, config) = self.0.get_graphics_data();
        client_graphics_window_created_ntfy(graphics_backend, native, config)?;
        self.0.window_created_ntfy(native)
    }
    fn window_destroyed_ntfy(&mut self, native: &mut dyn NativeImpl) -> anyhow::Result<()> {
        let (_, graphics_backend, _) = self.0.get_graphics_data();
        client_graphics_window_destroyed_ntfy(graphics_backend, native)?;
        self.0.window_destroyed_ntfy(native)
    }
}
