#![deny(warnings)]
#![deny(clippy::all)]

use std::{
    num::{NonZeroI64, NonZeroU16, NonZeroU32, NonZeroU64, NonZeroUsize},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize},
        Arc,
    },
};

use thiserror::Error;

#[cfg(feature = "derive")]
pub use hiarc_macro::*;

const fn max(a: u64, b: u64) -> u64 {
    [a, b][(a < b) as usize]
}

/// # Safety
///
/// Not memory unsafe, which the keyword was originally designed for,
/// but logic unsafe to implement it your own,
/// since it breaks the whole hiarc concept.
pub unsafe trait HiarcTrait {
    const HI_VAL: u64;
}

unsafe impl HiarcTrait for String {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for bool {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for u8 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for i8 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for u16 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for i16 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for u32 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for i32 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for u64 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for i64 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for u128 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for i128 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for usize {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for isize {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for f32 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for f64 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for char {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for AtomicBool {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for AtomicU64 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for AtomicUsize {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for NonZeroU16 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for NonZeroU32 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for NonZeroU64 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for NonZeroI64 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for NonZeroUsize {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for std::time::Duration {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for std::time::Instant {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for std::time::SystemTime {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for std::path::Path {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for &std::path::Path {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for std::path::PathBuf {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for std::net::SocketAddr {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for std::net::SocketAddrV4 {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for std::net::SocketAddrV6 {
    const HI_VAL: u64 = 0;
}

unsafe impl<'a, T: HiarcTrait> HiarcTrait for std::borrow::Cow<'a, T>
where
    T: ?Sized + 'a + ToOwned,
{
    const HI_VAL: u64 = T::HI_VAL;
}

#[cfg(feature = "enable_parking_lot")]
unsafe impl<T: HiarcTrait> HiarcTrait for parking_lot::Mutex<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

#[cfg(feature = "enable_parking_lot")]
unsafe impl<T: HiarcTrait> HiarcTrait for parking_lot::RwLock<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

#[cfg(feature = "enable_parking_lot")]
unsafe impl HiarcTrait for parking_lot::Condvar {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_spin")]
unsafe impl<T: HiarcTrait> HiarcTrait for spin::Mutex<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

#[cfg(feature = "enable_spin")]
unsafe impl<T: HiarcTrait> HiarcTrait for spin::RwLock<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

#[cfg(feature = "enable_tokio")]
unsafe impl<T: HiarcTrait> HiarcTrait for tokio::sync::Mutex<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

#[cfg(feature = "enable_tokio")]
unsafe impl<T: HiarcTrait> HiarcTrait for tokio::sync::RwLock<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

#[cfg(feature = "enable_tokio")]
unsafe impl HiarcTrait for tokio::sync::Semaphore {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_tokio")]
unsafe impl HiarcTrait for tokio::sync::Notify {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_tokio")]
unsafe impl<T: HiarcTrait> HiarcTrait for tokio::sync::oneshot::Receiver<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

#[cfg(feature = "enable_tokio")]
unsafe impl<T: HiarcTrait> HiarcTrait for tokio::sync::oneshot::Sender<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

#[cfg(feature = "enable_anyhow")]
unsafe impl HiarcTrait for anyhow::Error {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_chrono")]
unsafe impl<T: HiarcTrait + chrono::TimeZone> HiarcTrait for chrono::DateTime<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

#[cfg(feature = "enable_chrono")]
unsafe impl HiarcTrait for chrono::Utc {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_egui")]
unsafe impl HiarcTrait for egui::Context {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_egui")]
unsafe impl HiarcTrait for egui::TextureId {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_egui")]
unsafe impl HiarcTrait for egui::Mesh {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_egui")]
unsafe impl HiarcTrait for egui::PointerState {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_egui")]
unsafe impl HiarcTrait for egui::Rect {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_egui")]
unsafe impl HiarcTrait for egui::Pos2 {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_egui")]
unsafe impl HiarcTrait for egui::Vec2 {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_rayon")]
unsafe impl HiarcTrait for rayon::ThreadPool {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_either")]
unsafe impl<A: HiarcTrait, B: HiarcTrait> HiarcTrait for either::Either<A, B> {
    const HI_VAL: u64 = max(A::HI_VAL, B::HI_VAL);
}

#[cfg(feature = "enable_time")]
unsafe impl HiarcTrait for time::Duration {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_arc_swap")]
unsafe impl<T: HiarcTrait> HiarcTrait for arc_swap::ArcSwap<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

#[cfg(feature = "enable_crossbeam")]
unsafe impl<T: HiarcTrait> HiarcTrait for crossbeam::channel::Receiver<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

#[cfg(feature = "enable_crossbeam")]
unsafe impl<T: HiarcTrait> HiarcTrait for crossbeam::channel::Sender<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait, E: HiarcTrait> HiarcTrait for Result<T, E> {
    const HI_VAL: u64 = max(T::HI_VAL, E::HI_VAL);
}

unsafe impl<T: HiarcTrait> HiarcTrait for std::sync::Mutex<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for std::sync::RwLock<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for std::sync::mpsc::Receiver<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for std::sync::mpsc::Sender<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for std::sync::mpsc::SyncSender<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for Vec<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for std::collections::VecDeque<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<K: HiarcTrait, V: HiarcTrait> HiarcTrait for std::collections::HashMap<K, V> {
    const HI_VAL: u64 = max(V::HI_VAL, K::HI_VAL);
}

unsafe impl<K: HiarcTrait> HiarcTrait for std::collections::HashSet<K> {
    const HI_VAL: u64 = K::HI_VAL;
}

unsafe impl<K: HiarcTrait, V: HiarcTrait> HiarcTrait for std::collections::BTreeMap<K, V> {
    const HI_VAL: u64 = max(V::HI_VAL, K::HI_VAL);
}

unsafe impl<K: HiarcTrait> HiarcTrait for std::collections::BTreeSet<K> {
    const HI_VAL: u64 = K::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for std::thread::JoinHandle<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for std::mem::ManuallyDrop<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for std::ops::Range<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for std::ops::RangeInclusive<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

#[cfg(feature = "enable_hashlink")]
unsafe impl<K: HiarcTrait, V: HiarcTrait> HiarcTrait for hashlink::LinkedHashMap<K, V> {
    const HI_VAL: u64 = max(V::HI_VAL, K::HI_VAL);
}

#[cfg(feature = "enable_hashlink")]
unsafe impl<K: HiarcTrait> HiarcTrait for hashlink::LinkedHashSet<K> {
    const HI_VAL: u64 = K::HI_VAL;
}

#[cfg(all(feature = "enable_hashlink", feature = "enable_rustc_hash"))]
unsafe impl<K: HiarcTrait, V: HiarcTrait> HiarcTrait
    for hashlink::LinkedHashMap<K, V, rustc_hash::FxBuildHasher>
{
    const HI_VAL: u64 = max(V::HI_VAL, K::HI_VAL);
}

#[cfg(all(feature = "enable_hashlink", feature = "enable_rustc_hash"))]
unsafe impl<K: HiarcTrait> HiarcTrait for hashlink::LinkedHashSet<K, rustc_hash::FxBuildHasher> {
    const HI_VAL: u64 = K::HI_VAL;
}

#[cfg(feature = "enable_rustc_hash")]
unsafe impl<K: HiarcTrait, V: HiarcTrait> HiarcTrait for rustc_hash::FxHashMap<K, V> {
    const HI_VAL: u64 = max(V::HI_VAL, K::HI_VAL);
}

#[cfg(feature = "enable_rustc_hash")]
unsafe impl<K: HiarcTrait> HiarcTrait for rustc_hash::FxHashSet<K> {
    const HI_VAL: u64 = K::HI_VAL;
}

#[cfg(feature = "enable_fixed")]
unsafe impl<U> HiarcTrait for fixed::FixedI64<U> {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_fixed")]
unsafe impl<U> HiarcTrait for fixed::FixedU64<U> {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl<U: HiarcTrait + kira::manager::backend::Backend> HiarcTrait
    for kira::manager::AudioManager<U>
{
    const HI_VAL: u64 = U::HI_VAL;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::manager::backend::DefaultBackend {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::manager::backend::mock::MockBackend {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::track::TrackHandle {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl<B: HiarcTrait + kira::manager::backend::Backend> HiarcTrait
    for kira::manager::AudioManagerSettings<B>
{
    const HI_VAL: u64 = B::HI_VAL;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::clock::ClockHandle {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::clock::ClockSpeed {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::sound::static_sound::StaticSoundData {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::sound::static_sound::StaticSoundHandle {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::sound::static_sound::StaticSoundSettings {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl<E: Send> HiarcTrait for kira::sound::streaming::StreamingSoundData<E> {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl<E> HiarcTrait for kira::sound::streaming::StreamingSoundHandle<E> {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::sound::streaming::StreamingSoundSettings {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::spatial::emitter::EmitterHandle {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::spatial::emitter::EmitterSettings {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::spatial::listener::ListenerHandle {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::spatial::listener::ListenerSettings {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::spatial::scene::SpatialSceneHandle {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::spatial::scene::SpatialSceneSettings {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::OutputDestination {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_kira")]
unsafe impl HiarcTrait for kira::clock::ClockTime {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_mint")]
unsafe impl<T: HiarcTrait> HiarcTrait for mint::Vector3<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

#[cfg(feature = "enable_ash")]
unsafe impl HiarcTrait for ash::vk::SurfaceKHR {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_ash")]
unsafe impl HiarcTrait for ash::vk::Image {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_ash")]
unsafe impl HiarcTrait for ash::vk::SurfaceCapabilitiesKHR {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_ash")]
unsafe impl HiarcTrait for ash::khr::swapchain::Device {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_tinyset")]
unsafe impl<T: HiarcTrait + tinyset::Fits64> HiarcTrait for tinyset::Set64<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

#[cfg(feature = "enable_url")]
unsafe impl HiarcTrait for url::Url {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_ascii")]
unsafe impl HiarcTrait for ascii::AsciiStr {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_ascii")]
unsafe impl HiarcTrait for ascii::AsciiString {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_winit")]
unsafe impl HiarcTrait for winit::keyboard::KeyCode {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_winit")]
unsafe impl HiarcTrait for winit::keyboard::PhysicalKey {
    const HI_VAL: u64 = 0;
}

#[cfg(feature = "enable_winit")]
unsafe impl HiarcTrait for winit::event::MouseButton {
    const HI_VAL: u64 = 0;
}

unsafe impl<T: HiarcTrait> HiarcTrait for Option<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for std::marker::PhantomData<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for std::ptr::NonNull<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<A: HiarcTrait, B: HiarcTrait> HiarcTrait for (A, B) {
    const HI_VAL: u64 = max(B::HI_VAL, A::HI_VAL);
}

unsafe impl<A: HiarcTrait, B: HiarcTrait, C: HiarcTrait> HiarcTrait for (A, B, C) {
    const HI_VAL: u64 = max(B::HI_VAL, max(A::HI_VAL, C::HI_VAL));
}

unsafe impl<A: HiarcTrait, B: HiarcTrait, C: HiarcTrait, D: HiarcTrait> HiarcTrait
    for (A, B, C, D)
{
    const HI_VAL: u64 = max(B::HI_VAL, max(A::HI_VAL, max(D::HI_VAL, C::HI_VAL)));
}

unsafe impl<A: HiarcTrait, B: HiarcTrait, C: HiarcTrait, D: HiarcTrait, E: HiarcTrait> HiarcTrait
    for (A, B, C, D, E)
{
    const HI_VAL: u64 = max(
        B::HI_VAL,
        max(A::HI_VAL, max(D::HI_VAL, max(E::HI_VAL, C::HI_VAL))),
    );
}

unsafe impl<T: HiarcTrait> HiarcTrait for Rc<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for std::cell::RefCell<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for std::cell::Cell<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for Arc<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for Box<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for [T] {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for &[T] {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for &mut [T] {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for *mut T {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for &mut T {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl<T: HiarcTrait> HiarcTrait for &T {
    const HI_VAL: u64 = T::HI_VAL;
}

unsafe impl HiarcTrait for () {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for str {
    const HI_VAL: u64 = 0;
}

unsafe impl HiarcTrait for &str {
    const HI_VAL: u64 = 0;
}

unsafe impl<T: HiarcTrait, const N: usize> HiarcTrait for [T; N] {
    const HI_VAL: u64 = T::HI_VAL;
}

/// this struct has nothing to do with [`std::cell::RefCell`].
///
/// It is only useful if other Hi* components want to have a `RefCell`
/// with certain limitations. E.g. borrowing is always unsafe, so outer
/// unsafe implementations never try it or have to use `unsafe` keyword
#[derive(Debug, Default)]
pub struct HiUnsafeRefCell<T>(std::cell::RefCell<T>);

impl<T> HiUnsafeRefCell<T> {
    pub fn new(data: T) -> Self {
        Self(std::cell::RefCell::new(data))
    }

    /// # Safety
    ///
    /// Even tho this function is not unsafe in the sense of memory safety,
    /// this function is only intended to be used by macros that know what they are doing.
    /// In case you call it, you risk panics.
    pub unsafe fn hi_borrow_mut(&self) -> std::cell::RefMut<T> {
        self.0.borrow_mut()
    }

    /// # Safety
    ///
    /// Even tho this function is not unsafe in the sense of memory safety,
    /// this function is only intended to be used by macros that know what they are doing.
    /// In case you call, it you risk panics.
    pub unsafe fn hi_borrow(&self) -> std::cell::Ref<T> {
        self.0.borrow()
    }

    /// # Safety
    ///
    /// Even tho this function is not unsafe in the sense of memory safety,
    /// this function is only intended to be used by macros that know what they are doing.
    /// In case you call, it you risk panics.
    pub unsafe fn into_inner(self) -> T {
        std::cell::RefCell::into_inner(self.0)
    }
}

/// this struct has nothing to do with [`std::sync::Mutex`].
///
/// It is only useful if other Hi* components want to have a `Mutex`
/// with certain limitations. E.g. borrowing is always unsafe, so outer
/// unsafe implementations never try it or have to use `unsafe` keyword
#[derive(Debug, Default)]
pub struct HiUnsafeMutex<T>(std::sync::Mutex<T>);

impl<T> HiUnsafeMutex<T> {
    pub fn new(data: T) -> Self {
        Self(std::sync::Mutex::new(data))
    }

    /// # Safety
    ///
    /// Even tho this function is not unsafe in the sense of memory safety,
    /// this function is only intended to be used by macros that know what they are doing.
    /// In case you call it, you risk panics.
    pub unsafe fn hi_borrow_mut(&self) -> std::sync::MutexGuard<T> {
        self.0.lock().unwrap()
    }

    /// # Safety
    ///
    /// Even tho this function is not unsafe in the sense of memory safety,
    /// this function is only intended to be used by macros that know what they are doing.
    /// In case you call, it you risk panics.
    pub unsafe fn hi_borrow(&self) -> std::sync::MutexGuard<T> {
        self.0.lock().unwrap()
    }

    /// # Safety
    ///
    /// Even tho this function is not unsafe in the sense of memory safety,
    /// this function is only intended to be used by macros that know what they are doing.
    /// In case you call, it you risk panics.
    pub unsafe fn into_inner(self) -> T {
        std::sync::Mutex::into_inner(self.0).unwrap()
    }
}

#[derive(Error, Debug)]
pub enum HiUnsafeSyncSendCellCastError {
    #[error("failed to unwrap the value from the outer wrapper")]
    FailedToUnwrap,
}

/// This struct takes an instance of [`HiUnsafeRefCell`] () and _tries_ wrap it's inner value into a wrapper,
/// that is [`Sync`] + [`Send`].
///
/// It fails if the outer wrappers are not the only owners of the value (e.g. [`Rc`]).
#[derive(Debug, Default)]
pub struct HiUnsafeSyncSendCell<T>(T);

impl<T> HiUnsafeSyncSendCell<T>
where
    T: Send + Sync,
{
    pub fn from_rc(val: Rc<HiUnsafeRefCell<T>>) -> Result<Self, HiUnsafeSyncSendCellCastError> {
        Ok(Self(unsafe {
            Rc::try_unwrap(val)
                .map_err(|_| HiUnsafeSyncSendCellCastError::FailedToUnwrap)?
                .into_inner()
        }))
    }

    pub fn from_unsafe_cell(val: HiUnsafeRefCell<T>) -> Self {
        Self(unsafe { val.into_inner() })
    }

    pub fn into_rc_unsafe_cell(self) -> Rc<HiUnsafeRefCell<T>> {
        Rc::new(HiUnsafeRefCell::new(self.0))
    }
}

unsafe impl<T: HiarcTrait> HiarcTrait for HiUnsafeSyncSendCell<T> {
    const HI_VAL: u64 = T::HI_VAL;
}

/// P = parameters, R = result
/// # Safety
/// not memory unsafe, but logic unsafe
/// this is the object-safe base for [`HiFnOnce`]
pub unsafe trait HiFnOnceBase<P: ?Sized, R: ?Sized> {
    fn call_once(self, param: P) -> R;
}

/// P = parameters, R = result
/// # Safety
/// not memory unsafe, but logic unsafe
pub unsafe trait HiFnOnce<P: ?Sized, R: ?Sized>: HiFnOnceBase<P, R> + HiarcTrait {}

/// P = parameters, R = result
/// # Safety
/// not memory unsafe, but logic unsafe
/// this is the object-safe base for [`HiFnMut`]
pub unsafe trait HiFnMutBase<P: ?Sized, R: ?Sized>: HiFnOnceBase<P, R> {
    fn call_mut(&mut self, param: P) -> R;
}

/// P = parameters, R = result
/// # Safety
/// not memory unsafe, but logic unsafe
pub unsafe trait HiFnMut<P: ?Sized, R: ?Sized>:
    HiFnMutBase<P, R> + HiarcTrait + HiFnOnce<P, R>
{
}

/// P = parameters, R = result
/// # Safety
/// not memory unsafe, but logic unsafe
/// this is the object-safe base for [`HiFn`]
pub unsafe trait HiFnBase<P: ?Sized, R: ?Sized>: HiFnMutBase<P, R> {
    fn call_ref(&self, param: P) -> R;
}

/// P = parameters, R = result
/// # Safety
/// not memory unsafe, but logic unsafe
pub unsafe trait HiFn<P: ?Sized, R: ?Sized>:
    HiFnBase<P, R> + HiarcTrait + HiFnMut<P, R>
{
}
