use std::{
    marker::PhantomData,
    mem::ManuallyDrop,
    num::NonZeroUsize,
    sync::{Arc, atomic::AtomicUsize},
};

use crate::{mt_recycle::Recycle, traits::Recyclable};

#[cfg_attr(feature = "enable_hiarc", derive(hiarc::Hiarc))]
#[derive(Debug)]
pub(crate) struct PoolInner<T: Recyclable + Send> {
    pool: parking_lot::Mutex<Vec<T>>,
    lock_free_counter: AtomicUsize,
    max_items: Option<NonZeroUsize>,
}

impl<T: Recyclable + Send> PoolInner<T> {
    pub(crate) fn take(&self) -> Vec<T> {
        let mut pool = self.pool.lock();
        let res = std::mem::take(&mut *pool);
        self.lock_free_counter
            .store(0, std::sync::atomic::Ordering::SeqCst);
        res
    }

    pub(crate) fn push(&self, item: T) {
        let mut pool = self.pool.lock();
        pool.push(item);
        if let Some(max_items) = self.max_items {
            pool.truncate(max_items.get());
        }
        self.lock_free_counter
            .store(pool.len(), std::sync::atomic::Ordering::SeqCst);
    }

    pub(crate) fn get(&self) -> T {
        let mut pool = self.pool.lock();
        if let Some(item) = pool.pop() {
            self.lock_free_counter
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            item
        } else {
            T::new()
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.lock_free_counter
            .load(std::sync::atomic::Ordering::SeqCst)
            == 0
    }
}

/// Allows for a controlled allocation of the pool
#[cfg_attr(feature = "enable_hiarc", derive(hiarc::Hiarc))]
#[derive(Debug)]
pub struct PoolBuilder<T: Recyclable + Send> {
    capacity: Option<usize>,
    max_items: Option<NonZeroUsize>,

    p: PhantomData<T>,
}

impl<T: Recyclable + Send> Default for PoolBuilder<T> {
    fn default() -> Self {
        Self {
            capacity: Default::default(),
            max_items: Default::default(),
            p: Default::default(),
        }
    }
}

impl<T: Recyclable + Send> PoolBuilder<T> {
    pub fn build(self) -> Pool<T> {
        let pool = if let Some(capacity) = self.capacity {
            Vec::with_capacity(capacity)
        } else {
            Vec::new()
        };

        Pool {
            pool: Arc::new(PoolInner {
                pool: parking_lot::Mutex::new(pool),
                lock_free_counter: AtomicUsize::new(0),
                max_items: self.max_items,
            }),
        }
    }

    pub fn build_sized<F>(self, new_size: usize, item_constructor: F) -> Pool<T>
    where
        F: FnMut() -> T,
    {
        let mut pool = if let Some(capacity) = self.capacity {
            Vec::with_capacity(capacity.max(new_size))
        } else {
            Vec::with_capacity(new_size)
        };
        pool.resize_with(new_size, item_constructor);
        Pool {
            pool: Arc::new(PoolInner {
                pool: parking_lot::Mutex::new(pool),
                lock_free_counter: AtomicUsize::new(new_size),
                max_items: self.max_items,
            }),
        }
    }

    /// If capacity is 0, the pool will not allocate memory for any elements, but will still create heap memory.
    pub fn with_capacity(self, capacity: usize) -> Self {
        Self {
            capacity: Some(capacity),
            ..self
        }
    }

    /// If the limit is reached, then recycled items will not be pushed into the pool again.
    pub fn with_limit(self, max_items: NonZeroUsize) -> Self {
        Self {
            max_items: Some(max_items),
            ..self
        }
    }
}

/// Thread-safe version of the pool.
#[cfg_attr(feature = "enable_hiarc", derive(hiarc::Hiarc))]
#[derive(Debug)]
pub struct Pool<T: Recyclable + Send> {
    pub(crate) pool: Arc<PoolInner<T>>,
}

impl<T: Recyclable + Send> Pool<T> {
    pub fn builder() -> PoolBuilder<T> {
        PoolBuilder::default()
    }

    /// If capacity is 0, the pool will not allocate memory for any elements, but will still create heap memory.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            pool: Arc::new(PoolInner {
                pool: parking_lot::Mutex::new(Vec::with_capacity(capacity)),
                lock_free_counter: AtomicUsize::new(0),
                max_items: None,
            }),
        }
    }

    pub fn with_sized<F>(new_size: usize, item_constructor: F) -> Self
    where
        F: FnMut() -> T,
    {
        let mut pool = Vec::with_capacity(new_size);
        pool.resize_with(new_size, item_constructor);
        Self {
            pool: Arc::new(PoolInner {
                pool: parking_lot::Mutex::new(pool),
                lock_free_counter: AtomicUsize::new(new_size),
                max_items: None,
            }),
        }
    }

    pub fn new(&self) -> Recycle<T> {
        if let Some(item) = self.pool.pool.lock().pop() {
            self.pool
                .lock_free_counter
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            Recycle {
                pool: Some(self.pool.clone()),
                item: ManuallyDrop::new(item),
            }
        } else {
            Recycle {
                pool: Some(self.pool.clone()),
                item: ManuallyDrop::new(T::new()),
            }
        }
    }

    pub fn items_in_pool(&self) -> usize {
        self.pool
            .lock_free_counter
            .load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl<T: Recyclable + Send> Clone for Pool<T> {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
        }
    }
}
