use std::borrow::Borrow;
use std::hash::Hash;

use hashlink::{LinkedHashMap, LinkedHashSet};

pub type FxLinkedHashMap<K, V> = LinkedHashMap<K, V, rustc_hash::FxBuildHasher>;
pub type FxLinkedHashSet<K> = LinkedHashSet<K, rustc_hash::FxBuildHasher>;

/// Create a view that only returns elements where
/// the filter function returns true
pub struct LinkedHashMapView<'a, K, V, S, F, FV>
where
    F: Fn(&K) -> bool,
    FV: Fn(&V) -> bool,
{
    hash_map: &'a LinkedHashMap<K, V, S>,
    key_filter_func: F,
    val_filter_func: FV,
}

impl<'a, K: Eq + Hash, V, S, F, FV> LinkedHashMapView<'a, K, V, S, F, FV>
where
    F: Fn(&K) -> bool,
    FV: Fn(&V) -> bool,
    S: std::hash::BuildHasher,
{
    pub const fn new(
        hash_map: &'a LinkedHashMap<K, V, S>,
        key_filter_func: F,
        val_filter_func: FV,
    ) -> Self {
        Self {
            hash_map,
            key_filter_func,
            val_filter_func,
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        if !(self.key_filter_func)(key.borrow()) {
            None
        } else {
            self.hash_map
                .get(key)
                .and_then(|v| (self.val_filter_func)(v).then_some(v))
        }
    }

    /// you know what you are doing
    pub fn into_inner(self) -> (&'a LinkedHashMap<K, V, S>, F, FV) {
        (self.hash_map, self.key_filter_func, self.val_filter_func)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.hash_map
            .iter()
            .filter(|(k, v)| (self.key_filter_func)(k) || (self.val_filter_func)(v))
    }

    #[allow(clippy::should_implement_trait)]
    pub fn into_iter(self) -> impl Iterator<Item = (&'a K, &'a V)> {
        self.hash_map
            .iter()
            .filter(move |(k, v)| (self.key_filter_func)(k) || (self.val_filter_func)(v))
    }
}

/// Create a mutable view that only returns elements where
/// the filter function returns true
pub struct LinkedHashMapViewMut<'a, K, V, S, F, FV>
where
    F: Fn(&K) -> bool,
    FV: Fn(&V) -> bool,
{
    hash_map: &'a mut LinkedHashMap<K, V, S>,
    key_filter_func: F,
    val_filter_func: FV,
}

impl<'a, K: Eq + Hash, V, S, F, FV> LinkedHashMapViewMut<'a, K, V, S, F, FV>
where
    F: Fn(&K) -> bool,
    FV: Fn(&V) -> bool,
    S: std::hash::BuildHasher,
{
    pub const fn new(
        hash_map: &'a mut LinkedHashMap<K, V, S>,
        key_filter_func: F,
        val_filter_func: FV,
    ) -> Self {
        Self {
            hash_map,
            key_filter_func,
            val_filter_func,
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        if !(self.key_filter_func)(key.borrow()) {
            None
        } else {
            self.hash_map
                .get(key)
                .and_then(|v| (self.val_filter_func)(v).then_some(v))
        }
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        if !(self.key_filter_func)(key.borrow()) {
            None
        } else {
            self.hash_map
                .get_mut(key)
                .and_then(|v| (self.val_filter_func)(v).then_some(v))
        }
    }

    /// you know what you are doing
    pub fn into_inner(self) -> (&'a mut LinkedHashMap<K, V, S>, F, FV) {
        (self.hash_map, self.key_filter_func, self.val_filter_func)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.hash_map
            .iter()
            .filter(|(k, v)| (self.key_filter_func)(k) || (self.val_filter_func)(v))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut V)> {
        self.hash_map
            .iter_mut()
            .filter(|(k, v)| (self.key_filter_func)(k) || (self.val_filter_func)(v))
    }

    #[allow(clippy::should_implement_trait)]
    pub fn into_iter(self) -> impl Iterator<Item = (&'a K, &'a mut V)> {
        self.hash_map
            .iter_mut()
            .filter(move |(k, v)| (self.key_filter_func)(k) || (self.val_filter_func)(v))
    }
}
