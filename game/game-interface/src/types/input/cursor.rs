use hiarc::Hiarc;
use math::math::vector::{dvec2, ffixed};
use serde::{de, ser, Serialize};

/// the character cursor has few guarantees:
/// - x and y are never both 0 at the same time
///   (they have a threshold so that normalizing always works)
/// - x and y are in range [-1000.0 - u16::MAX * 2.0]
#[derive(Debug, Hiarc, Copy, Clone, PartialEq)]
pub struct CharacterInputCursor {
    x: ffixed,
    y: ffixed,
}

fn check_range<T: PartialOrd + From<i32> + From<i16>>(v: T) -> bool {
    v >= T::from(-1000i16) && v <= T::from(u16::MAX as i32 * 2)
}

impl<'de> de::Deserialize<'de> for CharacterInputCursor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        <(ffixed, ffixed) as de::Deserialize>::deserialize(deserializer).and_then(|(x, y)| {
            if !check_range(x) || !check_range(y) {
                Err(de::Error::invalid_value(
                    de::Unexpected::Float(if !check_range(x) { x } else { y }.to_num()),
                    &"the value of either x or y must be in range [-1000.0 - u16::MAX * 2.0]",
                ))
            } else if x.abs() < Self::min_cursor_val() && y.abs() < Self::min_cursor_val() {
                Err(de::Error::invalid_value(
                    de::Unexpected::Float(
                        if x.abs() < Self::min_cursor_val() {
                            x
                        } else {
                            y
                        }
                        .to_num(),
                    ),
                    &"the value of either x or y must not be under the threshold of 0.0001",
                ))
            } else {
                Ok(Self { x, y })
            }
        })
    }
}

impl Serialize for CharacterInputCursor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        <(ffixed, ffixed) as ser::Serialize>::serialize(&(self.x, self.y), serializer)
    }
}

impl Default for CharacterInputCursor {
    fn default() -> Self {
        Self {
            x: Self::min_cursor_val(),
            y: Default::default(),
        }
    }
}

impl CharacterInputCursor {
    pub fn min_cursor_val() -> ffixed {
        ffixed::from_num(0.0001)
    }

    pub fn to_vec2(&self) -> dvec2 {
        dvec2::new(self.x.to_num(), self.y.to_num())
    }
    pub fn from_vec2(cursor: &dvec2) -> Self {
        // make sure 0,0 is prevented
        let mut cursor = *cursor;
        if !cursor.x.is_finite() || !cursor.y.is_finite() {
            // reset broken coordinate
            cursor = dvec2::new(1.0, 0.0);
        } else if cursor.x.abs() < Self::min_cursor_val() && cursor.y.abs() < Self::min_cursor_val()
        {
            cursor.x = Self::min_cursor_val().to_num();
        } else if !check_range(cursor.x) || !check_range(cursor.y) {
            cursor = dvec2::new(1.0, 0.0);
        }

        Self {
            x: ffixed::from_num(cursor.x),
            y: ffixed::from_num(cursor.y),
        }
    }
}
