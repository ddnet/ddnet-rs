use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    ops::{Deref, DerefMut},
};

use hashlink::{LinkedHashMap, LinkedHashSet};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

pub trait Recyclable {
    fn new() -> Self;
    fn reset(&mut self);
    fn should_put_to_pool(&self) -> bool {
        true
    }
}

/// Specifically for pooled vectors this vector simply skips the
/// [`Recyclable::reset`] call, the user has to be careful.
/// Useful if performance matters.
#[cfg_attr(feature = "enable_hiarc", derive(hiarc::Hiarc))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnclearedVec<T>(Vec<T>);
impl<T> Default for UnclearedVec<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}
impl<T> Recyclable for UnclearedVec<T> {
    fn new() -> Self {
        Self::default()
    }

    fn reset(&mut self) {}
}
impl<T> Deref for UnclearedVec<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T> DerefMut for UnclearedVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// # vec
impl<T> Recyclable for Vec<T> {
    fn new() -> Self {
        Vec::<T>::default()
    }

    fn reset(&mut self) {
        self.clear();
    }
}

// # vec deque
impl<T> Recyclable for VecDeque<T> {
    fn new() -> Self {
        VecDeque::<T>::default()
    }

    fn reset(&mut self) {
        self.clear();
    }
}

// # btree map
impl<K, V> Recyclable for BTreeMap<K, V> {
    fn new() -> Self {
        BTreeMap::<K, V>::default()
    }

    fn reset(&mut self) {
        self.clear();
    }
}

// # btree set
impl<K> Recyclable for BTreeSet<K> {
    fn new() -> Self {
        BTreeSet::<K>::default()
    }

    fn reset(&mut self) {
        self.clear();
    }
}

// # linked hash map
impl<K, V> Recyclable for LinkedHashMap<K, V> {
    fn new() -> Self {
        Default::default()
    }

    fn reset(&mut self) {
        self.clear()
    }
}

// # linked hash set
impl<K> Recyclable for LinkedHashSet<K> {
    fn new() -> Self {
        Default::default()
    }

    fn reset(&mut self) {
        self.clear()
    }
}

// # fx linked hash map
impl<K, V> Recyclable for LinkedHashMap<K, V, rustc_hash::FxBuildHasher> {
    fn new() -> Self {
        Default::default()
    }

    fn reset(&mut self) {
        self.clear()
    }
}

// # fx linked hash set
impl<K> Recyclable for LinkedHashSet<K, rustc_hash::FxBuildHasher> {
    fn new() -> Self {
        Default::default()
    }

    fn reset(&mut self) {
        self.clear()
    }
}

// # fx hash map
impl<K, V> Recyclable for FxHashMap<K, V> {
    fn new() -> Self {
        Default::default()
    }

    fn reset(&mut self) {
        self.clear()
    }
}

// # fx hash set
impl<K> Recyclable for FxHashSet<K> {
    fn new() -> Self {
        Default::default()
    }

    fn reset(&mut self) {
        self.clear()
    }
}

// # hash map
impl<K, V> Recyclable for HashMap<K, V> {
    fn new() -> Self {
        Default::default()
    }

    fn reset(&mut self) {
        self.clear()
    }
}

// # hash set
impl<K> Recyclable for HashSet<K> {
    fn new() -> Self {
        Default::default()
    }

    fn reset(&mut self) {
        self.clear()
    }
}

impl Recyclable for String {
    fn new() -> Self {
        String::default()
    }

    fn reset(&mut self) {
        self.clear()
    }
}

// # cow
impl<'a, T> Recyclable for std::borrow::Cow<'a, [T]>
where
    [T]: ToOwned<Owned: Default + Recyclable>,
{
    fn new() -> Self {
        std::borrow::Cow::<'a, [T]>::default()
    }

    fn reset(&mut self) {
        self.to_mut().reset();
    }
}

// # Box, note that they create heap allocations
impl<T: Recyclable> Recyclable for Box<T> {
    fn new() -> Self {
        Box::new(T::new())
    }

    fn reset(&mut self) {
        self.as_mut().reset();
    }
}
