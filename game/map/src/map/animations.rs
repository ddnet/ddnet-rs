use std::{fmt::Debug, ops::IndexMut, time::Duration};

pub use time::Duration as TimeDuration;

use fixed::traits::{FromFixed, ToFixed};
use hiarc::Hiarc;
use is_sorted::IsSorted;
use math::math::{
    mix,
    vector::{ffixed, fvec3, lffixed, nffixed, nfvec4, vec1_base, vec2_base},
    PI,
};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use serde_with::serde_as;

#[derive(Debug, Hiarc, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnimBezierPoint {
    pub x: Duration,
    pub y: ffixed,
}

#[derive(Debug, Hiarc, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnimBezier {
    /// Outgoing bezier (visually right from the env point)
    pub out_tangent: AnimBezierPoint,
    /// Incoming bezier for the next point
    /// (visually left from the __next__ env point).
    ///
    /// The incoming duration is the absolute time difference
    /// to the env point's time. When it is used it usually
    /// must be made negative.
    pub in_tangent: AnimBezierPoint,
}

#[serde_as]
#[derive(Debug, Hiarc, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnimBeziers<const COUNT: usize> {
    #[serde_as(as = "[_; COUNT]")]
    pub value: [AnimBezier; COUNT],
}

#[repr(u8)]
#[derive(Debug, Hiarc, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AnimPointCurveType<const COUNT: usize> {
    Step = 0,
    Linear,
    Slow,
    Fast,
    Smooth,
    Bezier(AnimBeziers<COUNT>),
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub struct AnimPoint<T, const CHANNELS: usize> {
    pub time: Duration,
    pub curve_type: AnimPointCurveType<CHANNELS>,
    pub value: T,
}

impl<T, const CHANNELS: usize> PartialEq for AnimPoint<T, CHANNELS> {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time
    }
}

impl<T, const CHANNELS: usize> PartialOrd for AnimPoint<T, CHANNELS> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.time.partial_cmp(&other.time)
    }
}

impl<F, T, const CHANNELS: usize> AnimPoint<T, CHANNELS>
where
    T: Debug + Copy + Default + IndexMut<usize, Output = F>,
    F: Copy + FromFixed + ToFixed,
{
    #[allow(clippy::too_many_arguments)]
    fn solve_bezier<V>(
        x: V,
        p0: V,
        p1: V,
        p2: V,
        p3: V,
        sqrt: impl Fn(V) -> V,
        cbrt: impl Fn(V) -> V,
        acos: impl Fn(V) -> V,
        cos: impl Fn(V) -> V,
    ) -> V
    where
        V: std::ops::Sub<V, Output = V>
            + std::ops::Add<V, Output = V>
            + std::ops::Mul<V, Output = V>
            + std::ops::Div<V, Output = V>
            + std::ops::Neg<Output = V>
            + std::cmp::PartialEq<V>
            + std::cmp::PartialOrd<V>
            + From<u8>
            + From<f32>
            + Copy,
    {
        let x3 = -p0 + V::from(3) * p1 - V::from(3) * p2 + p3;
        let x2 = V::from(3) * p0 - V::from(6) * p1 + V::from(3) * p2;
        let x1 = -V::from(3) * p0 + V::from(3) * p1;
        let x0 = p0 - x;

        if x3 == V::from(0) && x2 == V::from(0) {
            // linear
            // a * t + b = 0
            let a = x1;
            let b = x0;

            if a == V::from(0) {
                return V::from(0);
            }
            -b / a
        } else if x3 == V::from(0) {
            // quadratic
            // t * t + b * t + c = 0
            let b = x1 / x2;
            let c = x0 / x2;

            if c == V::from(0) {
                return V::from(0);
            }

            let d = b * b - V::from(4) * c;
            let sqrt_d = sqrt(d);

            let t = (-b + sqrt_d) / V::from(2);

            if V::from(0) <= t && t <= V::from(1.0001) {
                return t;
            }
            (-b - sqrt_d) / V::from(2)
        } else {
            // cubic
            // t * t * t + a * t * t + b * t * t + c = 0
            let a = x2 / x3;
            let b = x1 / x3;
            let c = x0 / x3;

            // substitute t = y - a / 3
            let sub = a / V::from(3);

            // depressed form x^3 + px + q = 0
            // cardano's method
            let p = b / V::from(3) - a * a / V::from(9);
            let q = (V::from(2) * a * a * a / V::from(27) - a * b / V::from(3) + c) / V::from(2);

            let d = q * q + p * p * p;

            if d > V::from(0) {
                // only one 'real' solution
                let s = sqrt(d);
                return cbrt(s - q) - cbrt(s + q) - sub;
            } else if d == V::from(0) {
                // one single, one double solution or triple solution
                let s = cbrt(-q);
                let t = V::from(2) * s - sub;

                if V::from(0) <= t && t <= V::from(1.0001) {
                    return t;
                }
                -s - sub
            } else {
                // Casus irreducibilis ... ,_,
                let phi = acos(-q / sqrt(-(p * p * p))) / V::from(3);
                let s = V::from(2) * sqrt(-p);

                let t1 = s * cos(phi) - sub;

                if V::from(0) <= t1 && t1 <= V::from(1.0001) {
                    return t1;
                }

                let t2 = -s * cos(phi + V::from(PI) / V::from(3)) - sub;

                if V::from(0) <= t2 && t2 <= V::from(1.0001) {
                    return t2;
                }
                -s * cos(phi - V::from(PI) / V::from(3)) - sub
            }
        }
    }

    fn bezier<V, TB>(p0: &V, p1: &V, p2: &V, p3: &V, amount: TB) -> V
    where
        V: std::ops::Sub<V, Output = V>
            + std::ops::Add<V, Output = V>
            + std::ops::Mul<TB, Output = V>
            + Copy,
        TB: Copy,
    {
        // De-Casteljau Algorithm
        let c10 = mix(p0, p1, amount);
        let c11 = mix(p1, p2, amount);
        let c12 = mix(p2, p3, amount);

        let c20 = mix(&c10, &c11, amount);
        let c21 = mix(&c11, &c12, amount);

        // c30
        mix(&c20, &c21, amount)
    }

    pub fn eval_curve_for(
        point1: &Self,
        point2_time: &Duration,
        point2_value: &T,
        time: TimeDuration,
    ) -> T {
        let delta = (*point2_time - point1.time).clamp(Duration::from_nanos(100), Duration::MAX);
        let a: ffixed = (((lffixed::from_num(time.whole_nanoseconds()))
            - lffixed::from_num(point1.time.as_nanos()))
            / lffixed::from_num(delta.as_nanos()))
        .to_num();
        let a = match &point1.curve_type {
            AnimPointCurveType::Step => 0i32.into(),
            AnimPointCurveType::Linear => {
                // linear
                a
            }
            AnimPointCurveType::Slow => a * a * a,
            AnimPointCurveType::Fast => {
                let a = ffixed::from_num(1) - a;
                ffixed::from_num(1) - a * a * a
            }
            AnimPointCurveType::Smooth => {
                // second hermite basis
                ffixed::from_num(-2) * a * a * a + ffixed::from_num(3) * a * a
            }
            AnimPointCurveType::Bezier(beziers) => {
                let mut res = T::default();
                for c in 0..CHANNELS {
                    // monotonic 2d cubic bezier curve
                    let p0 = vec2_base::new(
                        ffixed::from_num(point1.time.as_secs_f64() * 1000.0),
                        point1.value[c].to_fixed(),
                    );
                    let p3 = vec2_base::new(
                        ffixed::from_num(point2_time.as_secs_f64() * 1000.0),
                        point2_value[c].to_fixed(),
                    );

                    let out_tang = vec2_base::new(
                        ffixed::from_num(beziers.value[c].out_tangent.x.as_secs_f64() * 1000.0),
                        beziers.value[c].out_tangent.y,
                    );
                    let in_tang = vec2_base::new(
                        ffixed::from_num(-beziers.value[c].in_tangent.x.as_secs_f64() * 1000.0),
                        beziers.value[c].in_tangent.y,
                    );

                    let mut p1 = p0 + out_tang;
                    let mut p2 = p3 + in_tang;

                    // validate bezier curve
                    p1.x = p1.x.clamp(p0.x, p3.x);
                    p2.x = p2.x.clamp(p0.x, p3.x);

                    // solve x(a) = time for a
                    let a = ffixed::from_num(
                        Self::solve_bezier(
                            time.as_seconds_f64() * 1000.0,
                            p0.x.to_num(),
                            p1.x.to_num(),
                            p2.x.to_num(),
                            p3.x.to_num(),
                            f64::sqrt,
                            f64::cbrt,
                            f64::acos,
                            f64::cos,
                        )
                        .clamp(0.0, 1.0),
                    );

                    // value = y(t)
                    res[c] = F::from_fixed(Self::bezier(&p0.y, &p1.y, &p2.y, &p3.y, a));
                }
                return res;
            }
        };

        let mut res = T::default();
        for c in 0..CHANNELS {
            let v0: ffixed = point1.value[c].to_fixed();
            let v1: ffixed = point2_value[c].to_fixed();
            res[c] = F::from_fixed(v0 + (v1 - v0) * a);
        }
        res
    }

    pub fn eval_curve(point1: &Self, point2: &Self, time: TimeDuration) -> T {
        Self::eval_curve_for(point1, &point2.time, &point2.value, time)
    }
}

pub type AnimPointPos = AnimPoint<fvec3, 3>;
pub type AnimPointColor = AnimPoint<nfvec4, 4>;
pub type AnimPointSound = AnimPoint<vec1_base<nffixed>, 1>;

fn points_deser<'de, T, D>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + PartialOrd,
{
    let res = <Vec<T>>::deserialize(deserializer)?;
    IsSorted::is_sorted(&mut res.iter())
        .then_some(res)
        .ok_or_else(|| serde::de::Error::custom("vec not sorted"))
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnimBase<T: DeserializeOwned + PartialOrd> {
    #[serde(deserialize_with = "points_deser")]
    pub points: Vec<T>,

    pub synchronized: bool,

    /// optional name, mostly intersting for editor
    pub name: String,
}

pub type PosAnimation = AnimBase<AnimPointPos>;
pub type ColorAnimation = AnimBase<AnimPointColor>;
pub type SoundAnimation = AnimBase<AnimPointSound>;

#[derive(Debug, Hiarc, Default, Clone, Serialize, Deserialize)]
pub struct Animations {
    pub pos: Vec<PosAnimation>,
    pub color: Vec<ColorAnimation>,
    pub sound: Vec<SoundAnimation>,
}
