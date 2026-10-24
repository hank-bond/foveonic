//! Gives foreground work priority over interruptible prefetch and pausable background jobs.

#[cfg(target_os = "linux")]
use std::cell::Cell;

use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
};

pub(crate) const IO_CHUNK_BYTES: usize = 1024 * 1024;

#[derive(Default)]
pub(crate) struct Gate {
    active: AtomicUsize,
    epoch: AtomicU64,
    lock: Mutex<()>,
    wake: Condvar,
}
/// Holds foreground priority through queued and active work, including supersession.
pub struct Activity(Arc<Gate>);
impl Gate {
    pub fn foreground(self: &Arc<Self>) -> Activity {
        self.active.fetch_add(1, Ordering::AcqRel);
        self.epoch.fetch_add(1, Ordering::AcqRel);
        Activity(self.clone())
    }
    pub fn idle(&self) -> bool {
        self.active.load(Ordering::Acquire) == 0
    }
    pub fn interrupt_prefetch(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
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
#[derive(Clone)]
pub(crate) struct Context {
    gate: Arc<Gate>,
    interrupt_epoch: Option<u64>,
}

#[derive(Debug)]
pub(crate) struct Interrupted;
impl std::fmt::Display for Interrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("background prefetch interrupted")
    }
}
impl std::error::Error for Interrupted {}

thread_local! {
    static CONTEXT: RefCell<Option<Context>> = const { RefCell::new(None) };
    #[cfg(target_os = "linux")]
    static PRIORITY_APPLIED: Cell<bool> = const { Cell::new(false) };
}
pub(crate) fn context() -> Option<Context> {
    CONTEXT.with(|c| c.borrow().clone())
}
pub(crate) fn run<T>(gate: Option<Arc<Gate>>, work: impl FnOnce() -> T) -> T {
    with_context(
        gate.map(|gate| Context {
            gate,
            interrupt_epoch: None,
        }),
        work,
    )
}

/// A foreground request invalidates this whole speculative attempt, even if it
/// finishes before the next checkpoint. Decoder children inherit the same epoch.
pub(crate) fn interruptible<T>(gate: Arc<Gate>, work: impl FnOnce() -> T) -> T {
    let epoch = gate.epoch.load(Ordering::Acquire);
    with_context(
        Some(Context {
            gate,
            interrupt_epoch: Some(epoch),
        }),
        work,
    )
}
pub(crate) fn with_context<T>(context: Option<Context>, work: impl FnOnce() -> T) -> T {
    struct Restore(Option<Context>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CONTEXT.with(|c| *c.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(CONTEXT.with(|c| c.replace(context)));
    decode_priority();
    work()
}
pub(crate) fn checkpoint() {
    CONTEXT.with(|c| {
        if let Some(context) = c.borrow().as_ref() {
            context.gate.wait();
        }
    });
}

/// Pausable jobs retain their progress. Speculative prefetch unwinds instead,
/// releasing compressed data and partial planes instead of retaining them while idle.
pub(crate) fn checkpoint_or_cancel() -> crate::source::Result<()> {
    CONTEXT.with(|c| {
        if let Some(context) = c.borrow().as_ref() {
            if let Some(epoch) = context.interrupt_epoch {
                if !context.gate.idle() || context.gate.epoch.load(Ordering::Acquire) != epoch {
                    return Err(Box::new(Interrupted) as Box<dyn std::error::Error + Send + Sync>);
                }
            } else {
                context.gate.wait();
            }
        }
        Ok(())
    })
}
/// Speculative photos use one CPU worker each, keeping a photo pool within its budget.
pub(crate) fn speculative() -> bool {
    CONTEXT.with(|c| {
        c.borrow()
            .as_ref()
            .is_some_and(|c| c.interrupt_epoch.is_some())
    })
}

/// Apply explicit QoS to active loads and every inherited decoder context.
pub(crate) fn decode_priority() {
    #[cfg(target_os = "macos")]
    set_qos(if context().is_some() {
        QOS_BACKGROUND
    } else {
        QOS_USER_INITIATED
    });
}
#[cfg(target_os = "macos")]
const QOS_BACKGROUND: u32 = 0x09;
#[cfg(target_os = "macos")]
const QOS_USER_INITIATED: u32 = 0x19;
#[cfg(target_os = "macos")]
fn set_qos(class: u32) {
    unsafe extern "C" {
        fn pthread_set_qos_class_self_np(class: u32, relative_priority: i32) -> i32;
    }
    unsafe {
        pthread_set_qos_class_self_np(class, 0);
    }
}

/// Lower dedicated background threads. Linux children inherit their parent's niceness.
pub(crate) fn lower_priority() {
    #[cfg(target_os = "macos")]
    set_qos(QOS_BACKGROUND);
    #[cfg(target_os = "linux")]
    {
        // nice() is additive. Reused workers must apply the adjustment only once.
        if PRIORITY_APPLIED.with(|applied| applied.replace(true)) {
            return;
        }
        unsafe extern "C" {
            fn nice(increment: i32) -> i32;
        }
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
    #[cfg(target_os = "macos")]
    #[test]
    fn active_decode_and_inherited_background_context_set_native_qos() {
        unsafe extern "C" {
            fn pthread_self() -> *mut std::ffi::c_void;
            fn pthread_get_qos_class_np(
                thread: *mut std::ffi::c_void,
                class: *mut u32,
                priority: *mut i32,
            ) -> i32;
        }
        fn qos() -> u32 {
            let mut class = 0;
            let mut priority = 0;
            unsafe {
                assert_eq!(
                    pthread_get_qos_class_np(pthread_self(), &mut class, &mut priority),
                    0
                );
            }
            class
        }
        std::thread::spawn(|| {
            decode_priority();
            assert_eq!(qos(), QOS_USER_INITIATED);
            run(Some(Arc::new(Gate::default())), || {
                assert_eq!(qos(), QOS_BACKGROUND);
                let inherited = context();
                std::thread::spawn(move || {
                    with_context(inherited, || assert_eq!(qos(), QOS_BACKGROUND))
                })
                .join()
                .unwrap();
            });
            decode_priority();
            assert_eq!(qos(), QOS_USER_INITIATED);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn interrupted_prefetch_unwinds_inherited_workers_before_foreground_finishes() {
        let gate = Arc::new(Gate::default());
        let (ready_tx, ready_rx) = mpsc::channel();
        let (advance_tx, advance_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let background = gate.clone();
        let thread = std::thread::spawn(move || {
            interruptible(background, || {
                let inherited = context();
                std::thread::scope(|scope| {
                    scope.spawn(move || {
                        with_context(inherited, || {
                            checkpoint_or_cancel().unwrap();
                            ready_tx.send(()).unwrap();
                            advance_rx.recv().unwrap();
                            let error = checkpoint_or_cancel().unwrap_err();
                            done_tx.send(error.is::<Interrupted>()).unwrap();
                        })
                    });
                });
            });
            assert!(context().is_none());
        });
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let foreground = gate.foreground();
        advance_tx.send(()).unwrap();
        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        thread.join().unwrap();
        // Cancellation completes while this guard still owns foreground priority.
        assert!(!gate.idle());
        drop(foreground);
        interruptible(gate.clone(), checkpoint_or_cancel).unwrap();
        interruptible(gate.clone(), || {
            drop(gate.foreground());
            assert!(checkpoint_or_cancel().unwrap_err().is::<Interrupted>());
        });
    }

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
