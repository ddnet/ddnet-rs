use hiarc::Hiarc;
use pool::traits::Recyclable;
use serde::{Serialize, de};
use std::{fmt::Display, ops::Deref};

use thiserror::Error;

use crate::reduced_ascii_str::{ReducedAsciiString, ReducedAsciiStringError};

#[derive(Error, Debug)]
pub enum NetworkStringError {
    #[error("The unicode char length exceeded the allowed maximum length")]
    InvalidLength,
}

/// A string that that checks the max __unicode__ (code points) length
/// of a string at deserialization & creation time
#[derive(Debug, Default, Hiarc, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct NetworkString<const MAX_LENGTH: usize>(String);

impl<const MAX_LENGTH: usize> Deref for NetworkString<MAX_LENGTH> {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const MAX_LENGTH: usize> Display for NetworkString<MAX_LENGTH> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<const MAX_LENGTH: usize> From<NetworkString<MAX_LENGTH>> for String {
    fn from(value: NetworkString<MAX_LENGTH>) -> Self {
        value.0
    }
}

impl<const MAX_LENGTH: usize> std::borrow::Borrow<str> for NetworkString<MAX_LENGTH> {
    fn borrow(&self) -> &str {
        self.0.as_str()
    }
}

impl<const MAX_LENGTH: usize> std::borrow::Borrow<String> for NetworkString<MAX_LENGTH> {
    fn borrow(&self) -> &String {
        &self.0
    }
}

impl<const MAX_LENGTH: usize> NetworkString<MAX_LENGTH> {
    pub fn new(s: impl Into<String>) -> Result<Self, NetworkStringError> {
        let s = s.into();
        if s.chars().count() > MAX_LENGTH {
            Err(NetworkStringError::InvalidLength)
        } else {
            Ok(Self(s))
        }
    }

    /// Removes all characters that are over the limmit
    pub fn new_lossy(s: impl Into<String>) -> Self {
        let s = s.into().chars().take(MAX_LENGTH).collect();
        Self(s)
    }

    pub fn try_set(&mut self, s: impl AsRef<str>) -> Result<(), NetworkStringError> {
        if s.as_ref().chars().count() > MAX_LENGTH {
            Err(NetworkStringError::InvalidLength)
        } else {
            self.0.clear();
            self.0.push_str(s.as_ref());
            Ok(())
        }
    }
}

impl<'de, const MAX_LENGTH: usize> de::Deserialize<'de> for NetworkString<MAX_LENGTH> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        <String as de::Deserialize>::deserialize(deserializer).and_then(|inner| {
            if inner.chars().count() > MAX_LENGTH {
                Err(de::Error::invalid_length(
                    inner.chars().count(),
                    &"a unicode char length lower than the maximum",
                ))
            } else {
                Ok(Self(inner))
            }
        })
    }
}

impl<const MAX_LENGTH: usize> TryFrom<String> for NetworkString<MAX_LENGTH> {
    type Error = NetworkStringError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<const MAX_LENGTH: usize> TryFrom<&str> for NetworkString<MAX_LENGTH> {
    type Error = NetworkStringError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<const MAX_LENGTH: usize> Recyclable for NetworkString<MAX_LENGTH> {
    fn new() -> Self {
        Self::default()
    }

    fn reset(&mut self) {
        self.0.clear();
    }
}

#[derive(Error, Debug)]
pub enum NetworkAsciiStringError {
    #[error("The ascii string length exceeded the allowed maximum length of {0}")]
    InvalidLength(usize),
    #[error("{0}")]
    ReducedAsciiStrErr(ReducedAsciiStringError),
}

/// A string that is purely ascii and additionally is limited to the following
/// char set (see also [`ReducedAsciiString`] for the base limitations):
/// - `MAX_LENGTH`
#[derive(Debug, Default, Hiarc, Clone, Hash, Serialize, PartialOrd, Ord, PartialEq, Eq)]
pub struct NetworkReducedAsciiString<const MAX_LENGTH: usize>(ReducedAsciiString);

impl<const MAX_LENGTH: usize> Deref for NetworkReducedAsciiString<MAX_LENGTH> {
    type Target = ReducedAsciiString;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const MAX_LENGTH: usize> NetworkReducedAsciiString<MAX_LENGTH> {
    pub fn is_valid(s: &ReducedAsciiString) -> Result<(), NetworkAsciiStringError> {
        if s.chars().count() > MAX_LENGTH {
            Err(NetworkAsciiStringError::InvalidLength(MAX_LENGTH))
        } else {
            ReducedAsciiString::is_valid(s).map_err(NetworkAsciiStringError::ReducedAsciiStrErr)?;
            Ok(())
        }
    }

    pub fn new(
        s: impl TryInto<ReducedAsciiString, Error = ReducedAsciiStringError>,
    ) -> Result<Self, NetworkAsciiStringError> {
        let s: ReducedAsciiString = s
            .try_into()
            .map_err(NetworkAsciiStringError::ReducedAsciiStrErr)?;
        Self::try_from(s)
    }

    pub fn from_str_lossy(s: &str) -> Self {
        let s: ascii::AsciiString = ReducedAsciiString::from_str_lossy(s)
            .chars()
            .take(MAX_LENGTH)
            .collect();

        Self::new(s.as_str()).unwrap()
    }
}

impl<'de, const MAX_LENGTH: usize> de::Deserialize<'de> for NetworkReducedAsciiString<MAX_LENGTH> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        <ReducedAsciiString as de::Deserialize>::deserialize(deserializer).and_then(|inner| {
            Self::is_valid(&inner)
                .map_err(|err| match err {
                    NetworkAsciiStringError::InvalidLength(len) => de::Error::invalid_length(
                        inner.chars().count(),
                        &format!("a char length lower than the maximum: {len}").as_str(),
                    ),
                    NetworkAsciiStringError::ReducedAsciiStrErr(ReducedAsciiStringError::InvalidCharacter(char)) => de::Error::invalid_value(
                        de::Unexpected::Char(char),
                        &"expected a pure ascii string with reduced character set ([A-Z,a-z,0-9] & \"_ \")",
                    ),
                    err => de::Error::custom(format!("{err}"))
                })
                .map(|_| Self(inner))
        })
    }
}

impl<const MAX_LENGTH: usize> TryFrom<&str> for NetworkReducedAsciiString<MAX_LENGTH> {
    type Error = NetworkAsciiStringError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<const MAX_LENGTH: usize> From<NetworkReducedAsciiString<MAX_LENGTH>> for ReducedAsciiString {
    fn from(value: NetworkReducedAsciiString<MAX_LENGTH>) -> Self {
        value.0
    }
}

impl<const MAX_LENGTH: usize> TryFrom<ReducedAsciiString>
    for NetworkReducedAsciiString<MAX_LENGTH>
{
    type Error = NetworkAsciiStringError;
    fn try_from(value: ReducedAsciiString) -> Result<Self, Self::Error> {
        Self::is_valid(&value)?;
        Ok(Self(value))
    }
}

impl<const MAX_LENGTH: usize> Recyclable for NetworkReducedAsciiString<MAX_LENGTH> {
    fn new() -> Self {
        Self::default()
    }

    fn reset(&mut self) {
        self.0.clear();
    }
}

pub type PoolNetworkString<const MAX_LENGTH: usize> =
    pool::recycle::Recycle<NetworkString<MAX_LENGTH>>;
pub type MtPoolNetworkString<const MAX_LENGTH: usize> =
    pool::mt_recycle::Recycle<NetworkString<MAX_LENGTH>>;
pub type NetworkStringPool<const MAX_LENGTH: usize> = pool::pool::Pool<NetworkString<MAX_LENGTH>>;
pub type MtNetworkStringPool<const MAX_LENGTH: usize> =
    pool::mt_pool::Pool<NetworkString<MAX_LENGTH>>;

pub type PoolNetworkReducedAsciiString<const MAX_LENGTH: usize> =
    pool::recycle::Recycle<NetworkReducedAsciiString<MAX_LENGTH>>;
pub type MtPoolNetworkReducedAsciiString<const MAX_LENGTH: usize> =
    pool::mt_recycle::Recycle<NetworkReducedAsciiString<MAX_LENGTH>>;
pub type NetworkReducedAsciiStringPool<const MAX_LENGTH: usize> =
    pool::pool::Pool<NetworkReducedAsciiString<MAX_LENGTH>>;
pub type MtNetworkReducedAsciiStringPool<const MAX_LENGTH: usize> =
    pool::mt_pool::Pool<NetworkReducedAsciiString<MAX_LENGTH>>;
