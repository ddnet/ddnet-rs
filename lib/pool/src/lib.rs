#![deny(warnings)]
#![deny(clippy::all)]
#![allow(clippy::needless_doctest_main)]
#![allow(clippy::new_ret_no_self)]

pub mod arc;
pub mod datatypes;
pub mod mixed_datatypes;
pub mod mixed_pool;
pub mod mt_datatypes;
pub mod mt_pool;
pub mod mt_recycle;
pub mod pool;
pub mod pool_clone;
pub mod rc;
pub mod recycle;
pub mod traits;

#[allow(clippy::needless_range_loop)]
#[cfg(test)]
mod tests {
    use std::ops::DerefMut;

    use crate::{
        arc::ArcPool, datatypes::PoolVec as SingleThreadedPoolVec,
        mt_datatypes::PoolVec as ThreadSafePoolVec, mt_pool::Pool as ThreadedSafePool,
        pool::Pool as SingleThreadedPool, rc::RcPool,
    };

    #[test]
    fn it_works() {
        let pool = SingleThreadedPool::<Vec<u8>>::with_capacity(10);
        let mut v = pool.new();
        v.push(1);
        // no items in yet
        assert_eq!(pool.items_in_pool(), 0);
        drop(v);
        assert_eq!(pool.items_in_pool(), 1);
    }

    #[test]
    fn it_works_builder() {
        let pool = SingleThreadedPool::<Vec<u8>>::builder()
            .with_capacity(10)
            .build();
        let mut v = pool.new();
        v.push(1);
        // no items in yet
        assert_eq!(pool.items_in_pool(), 0);
        drop(v);
        assert_eq!(pool.items_in_pool(), 1);
    }

    #[test]
    fn it_works_mt() {
        let pool = ThreadedSafePool::<Vec<u8>>::with_capacity(10);
        let mut v = pool.new();
        v.push(1);
        assert_eq!(pool.items_in_pool(), 0);
        drop(v);
        assert_eq!(pool.items_in_pool(), 1);
    }

    #[test]
    fn it_works_mt_builder() {
        let pool = ThreadedSafePool::<Vec<u8>>::builder()
            .with_capacity(10)
            .build();
        let mut v = pool.new();
        v.push(1);
        assert_eq!(pool.items_in_pool(), 0);
        drop(v);
        assert_eq!(pool.items_in_pool(), 1);
    }

    #[test]
    fn it_works_rc_ptr() {
        let pool = RcPool::<u8>::with_capacity(1);
        let v = pool.new_rc(1);
        let ptr_v: *const u8 = &(***v);
        let val_v: u8 = ***v;
        drop(v);
        let v = pool.new_rc(1);
        let ptr_v2: *const u8 = &(***v);
        let val_v2: u8 = ***v;
        assert_eq!(ptr_v, ptr_v2);
        assert_eq!(val_v, val_v2);
    }

    #[test]
    fn it_works_arc_ptr() {
        let pool = ArcPool::<u8>::with_capacity(1);
        let v = pool.new_arc(1);
        let ptr_v: *const u8 = &(***v);
        let val_v: u8 = ***v;
        drop(v);
        let v = pool.new_arc(1);
        let ptr_v2: *const u8 = &(***v);
        let val_v2: u8 = ***v;
        assert_eq!(ptr_v, ptr_v2);
        assert_eq!(val_v, val_v2);
    }

    #[test]
    fn it_works_rc_ptr_clone() {
        let pool = RcPool::<u8>::with_capacity(1);
        let v = pool.new_rc(1);
        let ptr_v: *const u8 = &(***v);
        let v_keep = v.clone();
        drop(v);
        let v = pool.new_rc(1);
        let ptr_v2: *const u8 = &(***v);
        assert_ne!(ptr_v, ptr_v2);
        drop(v_keep);
    }

    #[test]
    fn it_works_arc_ptr_clone() {
        let pool = ArcPool::<u8>::with_capacity(1);
        let v = pool.new_arc(1);
        let ptr_v: *const u8 = &(***v);
        let v_keep = v.clone();
        drop(v);
        let v = pool.new_arc(1);
        let ptr_v2: *const u8 = &(***v);
        assert_ne!(ptr_v, ptr_v2);
        drop(v_keep);
    }

    #[test]
    fn it_works_capacity() {
        let pool = SingleThreadedPool::<Vec<u8>>::with_sized(10, || Vec::with_capacity(10));
        let v = pool.new();
        assert_eq!(pool.items_in_pool(), 9);
        assert_eq!(v.capacity(), 10);
    }

    #[test]
    fn it_works_capacity_mt() {
        let pool = ThreadedSafePool::<Vec<u8>>::with_sized(10, || Vec::with_capacity(10));
        let v = pool.new();
        assert_eq!(pool.items_in_pool(), 9);
        assert_eq!(v.capacity(), 10);
    }

    #[test]
    fn it_works_max_items() {
        let pool = SingleThreadedPool::<Vec<u8>>::builder()
            .with_capacity(10)
            .with_limit(10.try_into().unwrap())
            .build();
        let mut items = Vec::new();
        for _ in 0..20 {
            items.push(pool.new());
        }
        drop(items);
        assert_eq!(pool.items_in_pool(), 10);
    }

    #[test]
    fn it_works_max_items_mt() {
        let pool = ThreadedSafePool::<Vec<u8>>::builder()
            .with_capacity(10)
            .with_limit(10.try_into().unwrap())
            .build();
        let mut items = Vec::new();
        for _ in 0..20 {
            items.push(pool.new());
        }
        drop(items);
        assert_eq!(pool.items_in_pool(), 10);
    }

    fn bench_pool(size: usize) {
        let mut pooled_vec: Vec<SingleThreadedPoolVec<u8>> = Default::default();
        let mut pooled_vec_mt: Vec<ThreadSafePoolVec<u8>> = Default::default();
        let mut vec: Vec<Vec<u8>> = Default::default();
        let mut copy_vec: Vec<u8> = Default::default();
        for i in 0..size {
            copy_vec.push((i % 255) as u8);
        }
        pooled_vec.reserve(size);
        pooled_vec_mt.reserve(size);
        vec.reserve(size);
        let bench = std::time::Instant::now();

        let pool = SingleThreadedPool::<Vec<u8>>::with_sized(size, || Vec::with_capacity(size));
        for _ in 0..size {
            pooled_vec.push(pool.new());
        }
        for i in 0..size {
            pooled_vec[i].deref_mut().clone_from(&copy_vec);
        }

        println!("time st-pool round 1: {}s", bench.elapsed().as_secs_f64());

        let bench = std::time::Instant::now();

        let pool_mt = ThreadedSafePool::<Vec<u8>>::with_sized(size, || Vec::with_capacity(size));
        for _ in 0..size {
            pooled_vec_mt.push(pool_mt.new());
        }
        for i in 0..size {
            pooled_vec_mt[i].deref_mut().clone_from(&copy_vec);
        }

        println!("time mt-pool round 1: {}s", bench.elapsed().as_secs_f64());

        let bench = std::time::Instant::now();

        for _ in 0..size {
            vec.push(Vec::new());
        }
        for i in 0..size {
            vec[i].clone_from(&copy_vec);
        }

        println!("time no-pool round 1: {}s", bench.elapsed().as_secs_f64());

        // #### round 2 #####
        let bench = std::time::Instant::now();
        pooled_vec.clear();
        for _ in 0..size {
            pooled_vec.push(pool.new());
        }
        for i in 0..size {
            pooled_vec[i].deref_mut().clone_from(&copy_vec);
        }

        println!("time st-pool round 2: {}s", bench.elapsed().as_secs_f64());

        let bench = std::time::Instant::now();
        pooled_vec_mt.clear();
        for _ in 0..size {
            pooled_vec_mt.push(pool_mt.new());
        }
        for i in 0..size {
            pooled_vec_mt[i].deref_mut().clone_from(&copy_vec);
        }

        println!("time mt-pool round 2: {}s", bench.elapsed().as_secs_f64());

        let bench = std::time::Instant::now();
        vec.clear();
        for _ in 0..size {
            vec.push(Vec::new());
        }
        for i in 0..size {
            vec[i].clone_from(&copy_vec);
        }

        println!("time no-pool round 2: {}s", bench.elapsed().as_secs_f64());
    }

    #[test]
    fn it_works_bench() {
        println!("size: 4000");
        bench_pool(4000);
        println!("size: 8000");
        bench_pool(8000);
        println!("size: 16000");
        bench_pool(16000);
    }
}
