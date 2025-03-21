use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    mem::ManuallyDrop,
};

use hashlink::{LinkedHashMap, LinkedHashSet};

use crate::{
    mt_pool::Pool,
    mt_recycle::Recycle,
    traits::{Recyclable, UnclearedVec},
};

pub type PoolLinkedHashMap<K, V> = Recycle<LinkedHashMap<K, V>>;
pub type PoolLinkedHashSet<K> = Recycle<LinkedHashSet<K>>;
pub type PoolFxLinkedHashMap<K, V> = Recycle<LinkedHashMap<K, V, rustc_hash::FxBuildHasher>>;
pub type PoolFxLinkedHashSet<K> = Recycle<LinkedHashSet<K, rustc_hash::FxBuildHasher>>;
pub type PoolHashMap<K, V> = Recycle<HashMap<K, V>>;
pub type PoolHashSet<K> = Recycle<HashSet<K>>;

pub type PoolUnclearedVec<T> = Recycle<UnclearedVec<T>>;
pub type PoolVec<T> = Recycle<Vec<T>>;
pub type PoolVecDeque<T> = Recycle<VecDeque<T>>;
pub type PoolBTreeMap<K, V> = Recycle<BTreeMap<K, V>>;
pub type PoolBTreeSet<K> = Recycle<BTreeSet<K>>;

pub type PoolCow<'a, T> = Recycle<std::borrow::Cow<'a, T>>;
/// **Important**: Cause heap allocations when first added
pub type PoolBox<T> = Recycle<Box<T>>;

impl<'a, T> PoolCow<'a, [T]>
where
    [T]: ToOwned<Owned: Recyclable + Default + Send> + Sync,
{
    pub fn new_cow_without_pool(v: &'a [T]) -> Self {
        Self {
            pool: None,
            item: ManuallyDrop::new(std::borrow::Cow::Borrowed(v)),
        }
    }
}

impl<'a, T> From<&'a [T]> for PoolCow<'a, [T]>
where
    [T]: ToOwned<Owned: Recyclable + Default + Send> + Sync,
{
    fn from(value: &'a [T]) -> Self {
        Self::new_cow_without_pool(value)
    }
}

pub type PoolString = Recycle<String>;
impl PoolString {
    pub fn new_str_without_pool(string: &str) -> Self {
        Self {
            pool: None,
            item: ManuallyDrop::new(string.to_string()),
        }
    }
}

pub type StringPool = Pool<String>;
impl StringPool {
    pub fn new_str(&self, string: &str) -> PoolString {
        let mut s = self.new();
        s.push_str(string);
        s
    }
}
