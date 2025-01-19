use std::time::Duration;

use hiarc::Hiarc;
use is_sorted::IsSorted;
use math::math::vector::{ffixed, fvec3, nffixed, nfvec4, vec1_base};
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

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
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
