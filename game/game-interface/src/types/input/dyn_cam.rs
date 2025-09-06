use hiarc::Hiarc;
use math::math::{
    dot,
    vector::{dvec2, ffixed, vec2_base},
};
use serde::{Serialize, de, ser};

/// An offset to a camera caused by a dynamic camera, which some guarantees:
/// - the length of offset (x, y) is in range [-10.0 - 10.0]
/// - the coordinates x and y are also both in range [-10.0 - 10.0]
#[derive(Debug, Hiarc, Default, Copy, Clone, PartialEq)]
pub struct CharacterInputDynCamOffset {
    x: ffixed,
    y: ffixed,
}

fn check_range<T: PartialOrd + From<i8>>(v: T) -> bool {
    v >= T::from(-10i8) && v <= T::from(10i8)
}

fn length(x: ffixed, y: ffixed) -> ffixed {
    dot(&vec2_base::new(x, y), &vec2_base::new(x, y)).sqrt()
}

impl<'de> de::Deserialize<'de> for CharacterInputDynCamOffset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        <(ffixed, ffixed) as de::Deserialize>::deserialize(deserializer).and_then(|(x, y)| {
            let len = length(x, y);
            if !check_range(len) || !check_range(x) || !check_range(y) {
                if !check_range(len) {
                    Err(de::Error::invalid_value(
                        de::Unexpected::Float(len.to_num()),
                        &"the length of the offset must be in range [-10.0 - 10.0]",
                    ))
                } else {
                    Err(de::Error::invalid_value(
                        de::Unexpected::Float(if !check_range(x) {
                            x.to_num()
                        } else {
                            y.to_num()
                        }),
                        &"both coordinates x & y must be in range [-10.0 - 10.0]",
                    ))
                }
            } else {
                Ok(Self { x, y })
            }
        })
    }
}

impl Serialize for CharacterInputDynCamOffset {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        <(ffixed, ffixed) as ser::Serialize>::serialize(&(self.x, self.y), serializer)
    }
}

impl CharacterInputDynCamOffset {
    pub fn to_vec2(&self) -> dvec2 {
        dvec2::new(self.x.to_num(), self.y.to_num())
    }
    pub fn from_vec2(mut v: dvec2) -> Self {
        if !v.x.is_finite() || !v.y.is_finite() {
            // reset broken coordinate
            v = Default::default();
        }

        let len = math::math::length(&v);
        if !check_range(len) || !check_range(v.x) || !check_range(v.y) {
            v = Default::default();
        }

        Self {
            x: ffixed::from_num(v.x),
            y: ffixed::from_num(v.y),
        }
    }
}
