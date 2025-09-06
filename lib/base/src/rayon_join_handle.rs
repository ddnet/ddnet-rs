use std::sync::{Arc, atomic::AtomicBool};

use anyhow::anyhow;
use hiarc::Hiarc;

/// a struct that helps using rayon threadpool's spawn similar to standard's thread::spawn's join handle
/// the main difference is that you have to move this struct into the spawn closure
#[derive(Debug, Hiarc)]
struct RayonJoinHandleImpl<T> {
    is_finished: AtomicBool,
    result: parking_lot::Mutex<Option<anyhow::Result<T>>>,
    nty: parking_lot::Condvar,
}

impl<T> Default for RayonJoinHandleImpl<T> {
    fn default() -> Self {
        Self {
            is_finished: Default::default(),
            result: Default::default(),
            nty: Default::default(),
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct RayonJoinHandle<T>(Arc<RayonJoinHandleImpl<T>>);

impl<T: Send + 'static> RayonJoinHandle<T> {
    /// returns `true` if the task is finished, `false` otherwise
    pub fn is_finished(&self) -> bool {
        self.0
            .is_finished
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// returns the result. If [`RayonJoinHandle::is_finished`] returned `true`
    /// the current thread does not block, otherwise it does
    pub fn join(self) -> anyhow::Result<T> {
        let mut g = self.0.result.lock();
        self.0.nty.wait_while(&mut g, |_| !self.is_finished());

        let res = g
            .take()
            .ok_or_else(|| anyhow!("value was not written to"))?;
        drop(g);
        res
    }

    /// this will spawn the task on the thread
    pub fn run<F>(tp: &rayon::ThreadPool, f: F) -> Self
    where
        F: Send + FnOnce() -> anyhow::Result<T> + 'static,
    {
        let res = Self(Default::default());
        let res_thread = res.0.clone();
        tp.spawn(move || {
            let val = f();
            let mut g = res_thread.result.lock();
            res_thread
                .is_finished
                .store(true, std::sync::atomic::Ordering::Relaxed);
            res_thread.nty.notify_all();
            *g = Some(val);
        });
        res
    }
}
