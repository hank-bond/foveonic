//! Pauses optional decode work at bounded checkpoints while foreground requests run.

use std::{
    cell::{Cell, RefCell},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
};

pub(crate) const IO_CHUNK_BYTES: usize = 1024 * 1024;

#[derive(Default)]
pub(crate) struct Gate {
    active: AtomicUsize,
    lock: Mutex<()>,
    wake: Condvar,
}
/// Holds foreground priority through queued and active work, including supersession.
pub struct Activity(Arc<Gate>);
impl Gate {
    pub fn foreground(self: &Arc<Self>) -> Activity {
        self.active.fetch_add(1, Ordering::AcqRel);
        Activity(self.clone())
    }
    pub fn idle(&self) -> bool {
        self.active.load(Ordering::Acquire) == 0
    }
    fn wait(&self) {
        if self.idle() {
            return;
        }
        let mut guard = self.lock.lock().unwrap();
        while !self.idle() {
            guard = self.wake.wait(guard).unwrap();
        }
    }
}
impl Drop for Activity {
    fn drop(&mut self) {
        let _guard = self.0.lock.lock().unwrap();
        if self.0.active.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.0.wake.notify_all();
        }
    }
}
thread_local! {
    static CONTEXT: RefCell<Option<Arc<Gate>>> = const { RefCell::new(None) };
    static PRIORITY_APPLIED: Cell<bool> = const { Cell::new(false) };
}
pub(crate) fn context() -> Option<Arc<Gate>> {
    CONTEXT.with(|c| c.borrow().clone())
}
pub(crate) fn run<T>(gate: Option<Arc<Gate>>, work: impl FnOnce() -> T) -> T {
    struct Restore(Option<Arc<Gate>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CONTEXT.with(|c| *c.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(CONTEXT.with(|c| c.replace(gate)));
    work()
}
pub(crate) fn checkpoint() {
    CONTEXT.with(|c| {
        if let Some(gate) = c.borrow().as_ref() {
            gate.wait();
        }
    });
}
/// Apply background CPU and I/O scheduling to the cache worker and inherited decode threads.
pub(crate) fn lower_priority() {
    // Cache reads and preview jobs reuse their threads. Linux nice() is additive,
    // so applying it for each photograph would keep lowering the same thread.
    if PRIORITY_APPLIED.with(|applied| applied.replace(true)) {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn pthread_set_qos_class_self_np(class: u32, relative_priority: i32) -> i32;
        }
        // Newly created plane workers inherit the worker's QoS.
        const QOS_CLASS_BACKGROUND: u32 = 0x09;
        unsafe {
            pthread_set_qos_class_self_np(QOS_CLASS_BACKGROUND, 0);
        }
    }
    #[cfg(target_os = "linux")]
    {
        unsafe extern "C" {
            fn nice(increment: i32) -> i32;
        }
        // On Linux niceness is per thread and inherited by its children.
        unsafe {
            const BACKGROUND_NICE_INCREMENT: i32 = 10;
            nice(BACKGROUND_NICE_INCREMENT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};
    #[test]
    fn background_waits_for_all_queued_and_running_foreground_work() {
        let gate = Arc::new(Gate::default());
        let queued = gate.foreground();
        let running = gate.foreground();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            run(Some(gate), || {
                checkpoint();
                tx.send(()).unwrap();
            })
        });
        assert!(rx.recv_timeout(Duration::from_millis(20)).is_err());
        drop(queued);
        assert!(rx.recv_timeout(Duration::from_millis(20)).is_err());
        drop(running);
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        checkpoint(); // Ordinary decode callers never acquire the background gate.
    }
}
