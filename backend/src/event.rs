use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::Notify;

pub struct Event {
    notify: Notify,
    occured: AtomicBool,
}

impl Event {
    pub fn new() -> Arc<Self> {
        Arc::new(Event {
            notify: Notify::new(),
            occured: AtomicBool::new(false),
        })
    }

    pub async fn wait(self: Arc<Self>) {
        if self.occured.load(Ordering::Acquire) {
            return;
        }

        self.notify.notified().await;
    }

    pub fn has_occured(&self) -> bool {
        self.occured.load(Ordering::Acquire)
    }

    pub fn trigger(self: &Arc<Self>) {
        if !self.occured.swap(true, Ordering::Release) {
            self.notify.notify_waiters();
        }
    }
}
