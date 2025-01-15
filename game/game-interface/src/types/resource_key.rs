use std::{borrow::Borrow, str::FromStr};

use hiarc::Hiarc;
use pool::traits::Recyclable;
use serde::{Deserialize, Serialize};

use base::{
    network_string::{NetworkAsciiStringError, NetworkReducedAsciiString},
    reduced_ascii_str::{ReducedAsciiString, ReducedAsciiStringError},
};

/// This type is used to allow [`NetworkResourceKey`]s and [`ResourceKey`]s to
/// borrow from the same underlying type.
#[derive(
    Debug, Hiarc, Default, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize,
)]
pub struct ResourceKeyBase {
    pub name: ReducedAsciiString,
    pub hash: Option<base::hash::Hash>,
}

/// The resource key is the general way to describe
/// names of resources like skins or weapons etc.
///
/// It must contain a string name
/// (reduced to a limited character set [ReducedAsciiString])
/// and an optional hash.
/// If the hash is not used then the client
/// automatically decides which resource to load
/// If the hash exists, it __only__ loads/shows the
/// if a resource with that hash exists (or can
/// be downloaded).
pub type ResourceKey = ResourceKeyBase;

impl Recyclable for ResourceKey {
    fn new() -> Self {
        Self::default()
    }

    fn reset(&mut self) {
        self.name.clear();
        self.hash = None;
    }
}

impl ResourceKey {
    pub fn from_str_lossy(s: &str) -> Self {
        ResourceKey {
            name: ReducedAsciiString::from_str_lossy(s),
            hash: None,
        }
    }

    pub fn eq_str(&self, s: &ascii::AsciiStr) -> bool {
        self.name.as_str() == s.as_str() && self.hash.is_none()
    }

    pub fn clone_from_network<const MAX_LENGTH: usize>(
        &mut self,
        n: &NetworkResourceKey<MAX_LENGTH>,
    ) {
        self.hash = n.key.hash;
        self.name.clone_from(&n.key.name);
    }
}

impl From<ReducedAsciiString> for ResourceKey {
    fn from(value: ReducedAsciiString) -> Self {
        Self {
            hash: None,
            name: value,
        }
    }
}

impl<const MAX_LENGTH: usize> From<NetworkResourceKey<MAX_LENGTH>> for ResourceKey {
    fn from(value: NetworkResourceKey<MAX_LENGTH>) -> Self {
        Self {
            hash: value.key.hash,
            name: value.key.name,
        }
    }
}

impl TryFrom<&str> for ResourceKey {
    type Error = ReducedAsciiStringError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(Self {
            name: ReducedAsciiString::new(
                ascii::AsciiString::from_str(value)
                    .map_err(ReducedAsciiStringError::ConversionFromStringFailed)?,
            )?,

            ..Default::default()
        })
    }
}

/// A resource key that that checks the max length & only gives a reduced
/// ascii character set of a string at deserialization & creation time.
/// See [`NetworkReducedAsciiString`].
#[derive(
    Debug, Default, Hiarc, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct NetworkResourceKey<const MAX_LENGTH: usize> {
    key: ResourceKeyBase,
}

impl<const MAX_LENGTH: usize> NetworkResourceKey<MAX_LENGTH> {
    pub fn new(
        s: impl TryInto<ReducedAsciiString, Error = ReducedAsciiStringError>,
    ) -> Result<Self, NetworkAsciiStringError> {
        let s: ReducedAsciiString = s
            .try_into()
            .map_err(NetworkAsciiStringError::ReducedAsciiStrErr)?;
        Self::try_from(s)
    }

    pub fn from_str_lossy(s: &str) -> Self {
        NetworkResourceKey {
            key: ResourceKeyBase {
                name: NetworkReducedAsciiString::<MAX_LENGTH>::from_str_lossy(s).into(),
                hash: None,
            },
        }
    }
}

impl<const MAX_LENGTH: usize> TryFrom<&ascii::AsciiStr> for NetworkResourceKey<MAX_LENGTH> {
    type Error = NetworkAsciiStringError;
    fn try_from(value: &ascii::AsciiStr) -> Result<Self, Self::Error> {
        Self::new(value.as_str())
    }
}

impl<const MAX_LENGTH: usize> TryFrom<&str> for NetworkResourceKey<MAX_LENGTH> {
    type Error = NetworkAsciiStringError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<const MAX_LENGTH: usize> TryFrom<ReducedAsciiString> for NetworkResourceKey<MAX_LENGTH> {
    type Error = NetworkAsciiStringError;

    fn try_from(value: ReducedAsciiString) -> Result<Self, Self::Error> {
        Self::try_from(ResourceKeyBase {
            name: value,
            hash: None,
        })
    }
}

impl<const MAX_LENGTH: usize> TryFrom<ResourceKeyBase> for NetworkResourceKey<MAX_LENGTH> {
    type Error = NetworkAsciiStringError;

    fn try_from(value: ResourceKeyBase) -> Result<Self, Self::Error> {
        Ok(Self {
            key: ResourceKeyBase {
                name: NetworkReducedAsciiString::<MAX_LENGTH>::try_from(value.name)?.into(),
                hash: value.hash,
            },
        })
    }
}

impl<const MAX_LENGTH: usize> Borrow<ResourceKeyBase> for NetworkResourceKey<MAX_LENGTH> {
    fn borrow(&self) -> &ResourceKeyBase {
        &self.key
    }
}

impl<const MAX_LENGTH: usize> Recyclable for NetworkResourceKey<MAX_LENGTH> {
    fn new() -> Self {
        Self::default()
    }

    fn reset(&mut self) {
        self.key.reset();
    }
}

pub type PoolNetworkResourceKey<const MAX_LENGTH: usize> =
    pool::recycle::Recycle<NetworkResourceKey<MAX_LENGTH>>;
pub type MtPoolNetworkResourceKey<const MAX_LENGTH: usize> =
    pool::mt_recycle::Recycle<NetworkResourceKey<MAX_LENGTH>>;
pub type NetworkResourceKeyPool<const MAX_LENGTH: usize> =
    pool::pool::Pool<NetworkResourceKey<MAX_LENGTH>>;
pub type MtNetworkResourceKeyPool<const MAX_LENGTH: usize> =
    pool::mt_pool::Pool<NetworkResourceKey<MAX_LENGTH>>;
