use hiarc::Hiarc;
use math::math::vector::{dvec2, uffixed};
use serde::{Serialize, de, ser};

/// the character cursor has few guarantees:
/// - width and height are in range [1.0 - u16::MAX * 2.0]
#[derive(Debug, Hiarc, Copy, Clone, PartialEq)]
pub struct CharacterInputViewport {
    width: uffixed,
    height: uffixed,
}

fn check_range<T: PartialOrd + From<u8> + From<u32>>(v: T) -> bool {
    v >= T::from(1u8) && v <= T::from(u16::MAX as u32 * 2)
}

impl<'de> de::Deserialize<'de> for CharacterInputViewport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        <(uffixed, uffixed) as de::Deserialize>::deserialize(deserializer).and_then(
            |(width, height)| {
                if !check_range(width) || !check_range(height) {
                    Err(de::Error::invalid_value(
                        de::Unexpected::Float(
                            if !check_range(width) { width } else { height }.to_num(),
                        ),
                        &"the value of either width or height \
                        must be in range [1.0 - u16::MAX * 2.0]",
                    ))
                } else {
                    Ok(Self { width, height })
                }
            },
        )
    }
}

impl Serialize for CharacterInputViewport {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        <(uffixed, uffixed) as ser::Serialize>::serialize(&(self.width, self.height), serializer)
    }
}

impl Default for CharacterInputViewport {
    fn default() -> Self {
        Self {
            width: uffixed::from_num(48.0),
            height: uffixed::from_num(48.0),
        }
    }
}

impl CharacterInputViewport {
    pub fn to_vec2(&self) -> dvec2 {
        dvec2::new(self.width.to_num(), self.height.to_num())
    }
    pub fn from_vec2(vp: &dvec2) -> Self {
        let mut vp = *vp;
        if !vp.x.is_finite() || !vp.y.is_finite() {
            // reset broken coordinate
            vp = dvec2::new(48.0, 48.0);
        } else if !check_range(vp.x) || !check_range(vp.y) {
            vp = dvec2::new(48.0, 48.0);
        }

        Self {
            width: uffixed::from_num(vp.x),
            height: uffixed::from_num(vp.y),
        }
    }
}
