use std::{
    mem::size_of,
    ops::{Add, AddAssign, Div, DivAssign, Index, IndexMut, Mul, MulAssign, Neg, Sub, SubAssign},
};

use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

/// This exists only for indexing
#[repr(C)]
#[derive(Debug, Hiarc, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub struct vec1_base<T> {
    pub x: T,
}

impl<T> vec1_base<T> {
    #[inline]
    pub fn new(x: T) -> vec1_base<T> {
        vec1_base::<T> { x }
    }
}

impl<T: Copy + Clone> Index<usize> for vec1_base<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.x,
            _ => panic!("out of bounds."),
        }
    }
}

impl<T: Copy + Clone> IndexMut<usize> for vec1_base<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.x,
            _ => panic!("out of bounds."),
        }
    }
}

#[repr(C)]
#[derive(Debug, Hiarc, Hash, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub struct vec2_base<T> {
    pub x: T,
    pub y: T,
}

impl<T> vec2_base<T> {
    #[inline]
    pub fn new(x: T, y: T) -> vec2_base<T> {
        vec2_base::<T> { x, y }
    }
}

impl<T: Copy + Clone> vec2_base<T>
where
    T: std::default::Default,
{
    pub fn r(&mut self) -> &mut T {
        &mut self.x
    }
    pub fn g(&mut self) -> &mut T {
        &mut self.y
    }

    pub fn u(&mut self) -> &mut T {
        &mut self.x
    }
    pub fn v(&mut self) -> &mut T {
        &mut self.y
    }
}

impl<T: Copy + Clone + Neg<Output = T>> Neg for vec2_base<T> {
    type Output = vec2_base<T>;

    fn neg(self) -> Self::Output {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }
}

impl<T: Copy + Clone + Mul<Output = T>> Mul<T> for vec2_base<T> {
    type Output = vec2_base<T>;

    fn mul(self, rhs: T) -> Self::Output {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl<T: Copy + Clone + Div<Output = T>> Div<T> for vec2_base<T> {
    type Output = vec2_base<T>;

    fn div(self, rhs: T) -> Self::Output {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl<T: Copy + Clone + DivAssign<T>> DivAssign<T> for vec2_base<T> {
    fn div_assign(&mut self, rhs: T) {
        self.x /= rhs;
        self.y /= rhs;
    }
}

impl<T: Copy + Clone + Add<Output = T>> Add<vec2_base<T>> for vec2_base<T> {
    type Output = vec2_base<T>;

    fn add(self, rhs: vec2_base<T>) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl<T: Copy + Clone + AddAssign<T>> AddAssign<vec2_base<T>> for vec2_base<T> {
    fn add_assign(&mut self, rhs: vec2_base<T>) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl<T: Copy + Clone + MulAssign<T>> MulAssign<T> for vec2_base<T> {
    fn mul_assign(&mut self, rhs: T) {
        self.x *= rhs;
        self.y *= rhs;
    }
}

impl<T: Copy + Clone + Sub<Output = T>> Sub<vec2_base<T>> for vec2_base<T> {
    type Output = vec2_base<T>;

    fn sub(self, rhs: vec2_base<T>) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl<T: Copy + Clone + SubAssign<T>> SubAssign<vec2_base<T>> for vec2_base<T> {
    fn sub_assign(&mut self, rhs: vec2_base<T>) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

impl<T: Copy + Clone> Index<usize> for vec2_base<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.x,
            1 => &self.y,
            _ => panic!("out of bounds."),
        }
    }
}

impl<T: Copy + Clone> IndexMut<usize> for vec2_base<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.x,
            1 => &mut self.y,
            _ => panic!("out of bounds."),
        }
    }
}

#[repr(C)]
#[derive(Debug, Hiarc, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub struct vec3_base<T: Copy + Clone> {
    pub x: T,
    pub y: T,
    pub z: T,
}

impl<T: Copy + Clone> vec3_base<T> {
    pub fn new(x: T, y: T, z: T) -> Self {
        Self { x, y, z }
    }

    pub fn dot(a: &Self, b: &Self) -> T
    where
        T: Copy + std::ops::Mul<T, Output = T> + std::ops::Add<T, Output = T>,
    {
        a.x * b.x + a.y * b.y
    }

    pub fn length(self) -> T
    where
        T: Copy + std::ops::Mul<T, Output = T> + std::ops::Add<T, Output = T> + num_traits::Float,
    {
        Self::dot(&self, &self).sqrt()
    }

    pub fn normalize(self) -> Self
    where
        T: Default
            + Copy
            + std::ops::Mul<T, Output = T>
            + std::ops::Div<T, Output = T>
            + std::ops::Add<T, Output = T>
            + num_traits::Float,
    {
        let divisor = self.length();
        if divisor == T::zero() {
            Self::default()
        } else {
            let l = T::one() / divisor;
            Self {
                x: self.x * l,
                y: self.y * l,
                z: self.z * l,
            }
        }
    }
}

impl<T: Copy + Clone> Index<usize> for vec3_base<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.x,
            1 => &self.y,
            2 => &self.z,
            _ => panic!("out of bounds."),
        }
    }
}

impl<T: Copy + Clone> IndexMut<usize> for vec3_base<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.x,
            1 => &mut self.y,
            2 => &mut self.z,
            _ => panic!("out of bounds."),
        }
    }
}

#[repr(C)]
#[derive(Debug, Hiarc, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub struct vec4_base<T: Copy + Clone> {
    pub x: T,
    pub y: T,
    pub z: T,
    pub w: T,
}

impl<T: Copy + Clone> vec4_base<T>
where
    T: std::default::Default,
{
    pub fn new(x: T, y: T, z: T, w: T) -> Self {
        Self { x, y, z, w }
    }

    pub fn r(&self) -> T {
        self.x
    }
    pub fn g(&self) -> T {
        self.y
    }
    pub fn b(&self) -> T {
        self.z
    }
    pub fn a(&self) -> T {
        self.w
    }

    pub fn set_r(&mut self, val: T) {
        self.x = val;
    }
    pub fn set_g(&mut self, val: T) {
        self.y = val;
    }
    pub fn set_b(&mut self, val: T) {
        self.z = val;
    }
    pub fn set_a(&mut self, val: T) {
        self.w = val;
    }
}

impl<T: Copy + Clone + Mul<Output = T>> Mul<T> for vec4_base<T> {
    type Output = vec4_base<T>;

    fn mul(self, rhs: T) -> Self::Output {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
            w: self.w * rhs,
        }
    }
}

impl<T: Copy + Clone + Sub<Output = T>> Sub<vec4_base<T>> for vec4_base<T> {
    type Output = vec4_base<T>;

    fn sub(self, rhs: vec4_base<T>) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
            w: self.w - rhs.w,
        }
    }
}

impl<T: Copy + Clone + Add<Output = T>> Add<vec4_base<T>> for vec4_base<T> {
    type Output = vec4_base<T>;

    fn add(self, rhs: vec4_base<T>) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
            w: self.w + rhs.w,
        }
    }
}

impl<T: Copy + Clone> Index<usize> for vec4_base<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.x,
            1 => &self.y,
            2 => &self.z,
            3 => &self.w,
            _ => panic!("out of bounds."),
        }
    }
}

impl<T: Copy + Clone> IndexMut<usize> for vec4_base<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.x,
            1 => &mut self.y,
            2 => &mut self.z,
            3 => &mut self.w,
            _ => panic!("out of bounds."),
        }
    }
}

#[allow(non_camel_case_types)]
pub type vec2 = vec2_base<f32>;
#[allow(non_camel_case_types)]
pub type dvec2 = vec2_base<f64>;
#[allow(non_camel_case_types)]
pub type vec3 = vec3_base<f32>;
#[allow(non_camel_case_types)]
pub type vec4 = vec4_base<f32>;
#[allow(non_camel_case_types)]
pub type ivec2 = vec2_base<i32>;
#[allow(non_camel_case_types)]
pub type ivec4 = vec4_base<i32>;
#[allow(non_camel_case_types)]
pub type uvec2 = vec2_base<u32>;
#[allow(non_camel_case_types)]
pub type ubvec2 = vec2_base<u8>;
#[allow(non_camel_case_types)]
pub type ubvec4 = vec4_base<u8>;
#[allow(non_camel_case_types)]
pub type usvec2 = vec2_base<u16>;

// these types must stay stable
/// unsigned fixed-point number
#[allow(non_camel_case_types)]
pub type uffixed = fixed::FixedU64<fixed::types::extra::U32>;
#[allow(non_camel_case_types)]
pub type ufvec2 = vec2_base<uffixed>;
#[allow(non_camel_case_types)]
pub type ufvec3 = vec3_base<uffixed>;
#[allow(non_camel_case_types)]
pub type ufvec4 = vec4_base<uffixed>;
/// signed fixed-point number
#[allow(non_camel_case_types)]
pub type ffixed = fixed::FixedI64<fixed::types::extra::U32>;
#[allow(non_camel_case_types)]
pub type fvec2 = vec2_base<ffixed>;
#[allow(non_camel_case_types)]
pub type fvec3 = vec3_base<ffixed>;
#[allow(non_camel_case_types)]
pub type fvec4 = vec4_base<ffixed>;

/// unsigned fixed-point number
#[allow(non_camel_case_types)]
pub type luffixed = fixed::FixedU128<fixed::types::extra::U64>;
#[allow(non_camel_case_types)]
pub type lufvec2 = vec2_base<luffixed>;
#[allow(non_camel_case_types)]
pub type lufvec3 = vec3_base<luffixed>;
#[allow(non_camel_case_types)]
pub type lufvec4 = vec4_base<luffixed>;
/// signed fixed-point number
#[allow(non_camel_case_types)]
pub type lffixed = fixed::FixedI128<fixed::types::extra::U64>;
#[allow(non_camel_case_types)]
pub type lfvec2 = vec2_base<lffixed>;
#[allow(non_camel_case_types)]
pub type lfvec3 = vec3_base<lffixed>;
#[allow(non_camel_case_types)]
pub type lfvec4 = vec4_base<lffixed>;

/// normalized, [0-1] range, fixed-point number
#[allow(non_camel_case_types)]
pub type nffixed = fixed::FixedU64<fixed::types::extra::U63>;
#[allow(non_camel_case_types)]
pub type nfvec2 = vec2_base<nffixed>;
#[allow(non_camel_case_types)]
pub type nfvec3 = vec3_base<nffixed>;
#[allow(non_camel_case_types)]
pub type nfvec4 = vec4_base<nffixed>;

pub fn read_i32_le(data: &[u8]) -> i32 {
    i32::from_le_bytes([data[0], data[1], data[2], data[3]])
}

impl ivec4 {
    pub fn read_from_slice(data: &[u8]) -> Self {
        let (x, rest) = data.split_at(size_of::<i32>());
        let x = read_i32_le(x);

        let (y, rest) = rest.split_at(size_of::<i32>());
        let y = read_i32_le(y);

        let (z, rest) = rest.split_at(size_of::<i32>());
        let z = read_i32_le(z);

        let (w, _rest) = rest.split_at(size_of::<i32>());
        let w = read_i32_le(w);

        Self { x, y, z, w }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.x.to_le_bytes());
        w.extend(self.y.to_le_bytes());
        w.extend(self.z.to_le_bytes());
        w.extend(self.w.to_le_bytes());
    }
}

#[cfg(test)]
mod test {
    use crate::math::vector::fvec2;

    use super::ffixed;

    #[test]
    fn fixed_test() {
        let a = ffixed::from(-128);
        let b = fvec2::new((-128i32).into(), 128i32.into());

        assert!(a.to_num::<i32>() == -128);
        assert!(b.x.to_num::<i32>() == -128);
        assert!(b.y.to_num::<i32>() == 128);

        let zero_to_one = <fixed::FixedU32<fixed::types::extra::U31>>::from_num(0);
        assert!(zero_to_one.to_num::<i32>() == 0);
        let zero_to_one = <fixed::FixedU32<fixed::types::extra::U31>>::from_num(1);
        assert!(zero_to_one.to_num::<i32>() == 1);
    }
}
