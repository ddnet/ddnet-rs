use std::ops::RangeInclusive;

use hiarc::Hiarc;
use rand::SeedableRng;

use self::vector::{vec2, vec2_base};

pub mod vector;

pub const PI_F64: f64 = std::f64::consts::PI;
pub const PI: f32 = std::f32::consts::PI;

pub fn mix<T, TB>(a: &T, b: &T, amount: TB) -> T
where
    T: std::ops::Sub<T, Output = T>
        + std::ops::Add<T, Output = T>
        + std::ops::Mul<TB, Output = T>
        + Copy,
{
    *a + (*b - *a) * amount
}

pub fn blend<T, TB>(a: &T, b: &T, one: TB, amount: TB) -> T
where
    T: std::ops::Sub<T, Output = T>
        + std::ops::Add<T, Output = T>
        + std::ops::Mul<TB, Output = T>
        + Copy,
    TB: Copy + std::ops::Sub<TB, Output = TB>,
{
    *a * (one - amount) + (*b) * amount
}

pub fn lerp<T, TB>(a: &T, b: &T, amount: TB) -> T
where
    T: std::ops::Sub<T, Output = T>
        + std::ops::Add<T, Output = T>
        + std::ops::Mul<TB, Output = T>
        + Copy,
    TB: Copy + std::ops::Sub<TB, Output = TB> + num_traits::Float,
{
    blend::<T, TB>(a, b, TB::one(), amount)
}

const FXP_SCALE: i32 = 1 << 10;
pub fn fx2f(v: i32) -> f32 {
    (v as f32) / (FXP_SCALE as f32)
}
pub fn f2fx(v: f32) -> i32 {
    (v * FXP_SCALE as f32) as i32
}

pub fn dot<T>(a: &vec2_base<T>, b: &vec2_base<T>) -> T
where
    T: Copy + std::ops::Mul<T, Output = T> + std::ops::Add<T, Output = T>,
{
    a.x * b.x + a.y * b.y
}

pub fn length<T>(a: &vec2_base<T>) -> T
where
    T: Copy + std::ops::Mul<T, Output = T> + std::ops::Add<T, Output = T> + num_traits::Float,
{
    (dot(a, a)).sqrt()
}

pub fn normalize<T>(v: &vec2_base<T>) -> vec2_base<T>
where
    T: Default
        + Copy
        + std::ops::Mul<T, Output = T>
        + std::ops::Div<T, Output = T>
        + std::ops::Add<T, Output = T>
        + num_traits::Float,
{
    let divisor = length(v);
    if divisor == T::zero() {
        vec2_base::<T>::default()
    } else {
        let l = T::one() / divisor;
        vec2_base::<T>::new(v.x * l, v.y * l)
    }
}

pub fn normalize_pre_length<T>(v: &vec2_base<T>, len: T) -> vec2_base<T>
where
    T: Default
        + Copy
        + std::ops::Mul<T, Output = T>
        + std::ops::Add<T, Output = T>
        + num_traits::Float,
{
    if len == T::zero() {
        vec2_base::<T>::default()
    } else {
        vec2_base::<T>::new(v.x / len, v.y / len)
    }
}

pub fn distance(a: &vec2, b: &vec2) -> f32 {
    length(&(*a - *b))
}

pub fn distance_squared(a: &vec2, b: &vec2) -> f32 {
    let diff = *a - *b;
    dot(&diff, &diff)
}

pub fn closest_point_on_line(
    line_point_a: &vec2,
    line_point_b: &vec2,
    target_point: &vec2,
    out_pos: &mut vec2,
) -> bool {
    let seg_ab = *line_point_b - *line_point_a;
    let squared_magnitude_ab = dot(&seg_ab, &seg_ab);
    if squared_magnitude_ab > 0.0 {
        let ap = *target_point - *line_point_a;
        let ap_dot_ab = dot(&ap, &seg_ab);
        let t = ap_dot_ab / squared_magnitude_ab;
        *out_pos = *line_point_a + seg_ab * t.clamp(0.0, 1.0);
        return true;
    }
    false
}

#[inline(always)]
pub fn round_to_int(f: f32) -> i32 {
    f.round() as i32
}

#[inline(always)]
pub fn round_f64_to_int(f: f64) -> i32 {
    f.round() as i32
}

pub fn angle(a: &vec2) -> f32 {
    if a.x == 0.0 && a.y == 0.0 {
        return 0.0;
    } else if a.x == 0.0 {
        return if a.y < 0.0 { -PI / 2.0 } else { PI / 2.0 };
    }
    let mut result = (a.y / a.x).atan();
    if a.x < 0.0 {
        result += PI;
    }
    result
}

/// A rng generator that focuses on reproducibility rather
/// than security or anything else.
#[derive(Debug, Hiarc)]
pub struct Rng {
    #[hiarc_skip_unsafe]
    rng: rand_xoshiro::Xoshiro256PlusPlus,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: rand_xoshiro::Xoshiro256PlusPlus::seed_from_u64(seed),
        }
    }

    pub fn random_int_in(&mut self, range: RangeInclusive<u64>) -> u64 {
        rand::Rng::random_range(&mut self.rng, range)
    }

    pub fn random_int(&mut self) -> u64 {
        self.random_int_in(u64::MIN..=u64::MAX)
    }

    pub fn random_float_in(&mut self, range: RangeInclusive<f32>) -> f32 {
        rand::Rng::random_range(&mut self.rng, range)
    }

    /// random float in `[0..1]`
    pub fn random_float(&mut self) -> f32 {
        self.random_float_in(0.0..=1.0)
    }

    /// Get a random index into the given slice.
    ///
    /// # Panics
    ///
    /// Panics if the slice is empty.
    pub fn random_index<T>(&mut self, s: &[T]) -> usize {
        assert!(!s.is_empty(), "given slice was empty.");
        self.random_int_in(0..=s.len() as u64 - 1) as usize
    }
}

pub trait RngSlice<T> {
    /// Get a random entry of the given slice.
    ///
    /// # Panics
    ///
    /// Panics if the slice is empty.
    fn random_entry(&self, rng: &mut Rng) -> &T;
    fn random_val_entry(&self, rng_val: usize) -> &T;
}

impl<T> RngSlice<T> for [T] {
    fn random_entry(&self, rng: &mut Rng) -> &T {
        &self[rng.random_index(self)]
    }
    fn random_val_entry(&self, rng_val: usize) -> &T {
        &self[rng_val % self.len()]
    }
}
