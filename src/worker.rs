//! A worker owns its result channel, so results cannot leak into a later job.
//! Keep canceled workers alive until `is_finished` before dropping them on an
//! event-loop thread. Drop joins for deterministic final shutdown.
use std::{sync::mpsc, thread};

pub struct Worker<T> {
    result: mpsc::Receiver<T>,
    thread: Option<thread::JoinHandle<()>>,
}

impl<T: Send + 'static> Worker<T> {
    pub fn spawn(run: impl FnOnce() -> T + Send + 'static) -> Self {
        let (tx, result) = mpsc::sync_channel(1);
        let thread = Some(thread::spawn(move || {
            let _ = tx.send(run());
        }));
        Self { result, thread }
    }
}

impl<T> Worker<T> {
    pub fn try_recv(&self) -> Option<T> {
        self.result.try_recv().ok()
    }

    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }

    pub fn join(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl<T> Drop for Worker<T> {
    fn drop(&mut self) {
        self.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delayed_result_cannot_complete_a_replacement_worker() {
        let (release, gate) = mpsc::channel();
        let old = Worker::spawn(move || {
            gate.recv().unwrap();
            "old"
        });
        let mut new = Worker::spawn(|| "new");
        new.join();
        assert_eq!(new.try_recv(), Some("new"));
        assert!(!old.is_finished());
        release.send(()).unwrap();
        drop(old);
        assert_eq!(new.try_recv(), None);
    }
}
