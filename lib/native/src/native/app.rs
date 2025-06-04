// Operating systems that don't require app states get
// a small wrapper around nothing,
// that implements clone to match the native app
// behavior of other platforms.
#[cfg(not(target_os = "android"))]
#[derive(Debug, Default, Clone)]
pub struct NativeApp;
#[cfg(not(target_os = "android"))]
pub(crate) type ApplicationHandlerType = ();
#[cfg(not(target_os = "android"))]
pub(crate) type NativeEventLoop = winit::event_loop::EventLoop<()>;

#[cfg(target_os = "android")]
pub use winit::platform::android::activity::AndroidApp as NativeApp;
#[cfg(target_os = "android")]
pub(crate) type ApplicationHandlerType = NativeApp;
#[cfg(target_os = "android")]
pub(crate) type NativeEventLoop = winit::event_loop::EventLoop<NativeApp>;

pub const MIN_WINDOW_WIDTH: u32 = 50;
pub const MIN_WINDOW_HEIGHT: u32 = 50;
