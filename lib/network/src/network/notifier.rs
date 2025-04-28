use std::{sync::Arc, time::Duration};

use tokio::sync::Notify;

#[derive(Debug, Clone)]
pub struct NetworkEventNotifier {
    pub(crate) rt: tokio::runtime::Handle,
    pub(crate) notifiers: [Option<Arc<Notify>>; 2],
}

impl NetworkEventNotifier {
    pub async fn wait_for_event_async(&self, timeout: Option<Duration>) -> bool {
        let nty1 = self.notifiers.first().and_then(|n| (*n).clone());
        let has_nty1 = nty1.is_some();
        let task1 = async move {
            if let Some(nty) = nty1 {
                nty.notified().await;
            }
        };
        let nty2 = self.notifiers.get(1).and_then(|n| (*n).clone());
        let has_nty2 = nty2.is_some();
        let task2 = async move {
            if let Some(nty) = nty2 {
                nty.notified().await;
            }
        };
        match timeout {
            Some(timeout) => {
                let res = tokio::select! {
                    res = tokio::time::timeout(timeout, task1), if has_nty1 => res,
                    res = tokio::time::timeout(timeout, task2), if has_nty2 => res
                };
                res.is_ok()
            }
            None => {
                tokio::select! {
                    _ = task1, if has_nty1 => {},
                    _ = task2, if has_nty2 => {}
                }
                true
            }
        }
    }

    /// returns false if timeout was exceeded, others always returns true
    pub fn wait_for_event(&self, timeout: Option<Duration>) -> bool {
        self.rt.block_on(self.wait_for_event_async(timeout))
    }

    pub fn notify_one(&self) {
        if let Some(nty) = &self.notifiers[0] {
            nty.notify_one();
        }
        if let Some(nty) = &self.notifiers[1] {
            nty.notify_one();
        }
    }
}
