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
    decoder: Option<crate::decode_pool::Task>,
    #[cfg(target_os = "macos")]
    processing: bool,
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
            decoder: None,
            #[cfg(target_os = "macos")]
            processing: false,
        }),
        work,
    )
}

/// User-requested processing yields to interaction without maintenance-level throttling.
pub(crate) fn processing<T>(gate: Arc<Gate>, work: impl FnOnce() -> T) -> T {
    with_context(
        Some(Context {
            gate,
            interrupt_epoch: None,
            decoder: None,
            #[cfg(target_os = "macos")]
            processing: true,
        }),
        || {
            checkpoint();
            work()
        },
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
            decoder: None,
            #[cfg(target_os = "macos")]
            processing: false,
        }),
        work,
    )
}
pub(crate) fn with_context<T>(context: Option<Context>, work: impl FnOnce() -> T) -> T {
    struct Restore(Option<Context>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CONTEXT.with(|c| *c.borrow_mut() = self.0.take());
            decode_priority();
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
pub(crate) fn decoder_context() -> Option<crate::decode_pool::Task> {
    context().and_then(|c| c.decoder)
}
pub(crate) fn decoder<T>(
    task: crate::decode_pool::Task,
    work: impl FnOnce() -> crate::source::Result<T>,
) -> crate::source::Result<T> {
    let turn = task.turn.clone();
    let mut scheduling = context().ok_or("missing speculative context")?;
    scheduling.decoder = Some(task);
    with_context(Some(scheduling), || {
        turn.wait()?;
        work()
    })
}

/// Apply explicit QoS to active loads and every inherited decoder context.
pub(crate) fn decode_priority() {
    #[cfg(target_os = "macos")]
    set_qos(match context() {
        Some(context) if context.processing || context.decoder.is_some() => QOS_UTILITY,
        Some(_) => QOS_BACKGROUND,
        None => QOS_USER_INITIATED,
    });
}
#[cfg(target_os = "macos")]
const QOS_BACKGROUND: u32 = 0x09;
#[cfg(target_os = "macos")]
const QOS_UTILITY: u32 = 0x11;
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
        // Reused workers need no repeated scheduling calls.
        if PRIORITY_APPLIED.with(|applied| applied.replace(true)) {
            return;
        }
        unsafe extern "C" {
            fn getpriority(which: i32, who: u32) -> i32;
            fn setpriority(which: i32, who: u32, value: i32) -> i32;
        }
        unsafe {
            const PRIO_PROCESS: i32 = 0;
            const BACKGROUND_NICE: i32 = 10;
            // Pool threads may already inherit this value. Never add it twice.
            let inherited = getpriority(PRIO_PROCESS, 0);
            setpriority(PRIO_PROCESS, 0, inherited.max(BACKGROUND_NICE));
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
            assert_eq!(qos(), QOS_USER_INITIATED);
            processing(Arc::new(Gate::default()), || {
                assert_eq!(qos(), QOS_UTILITY);
                run(Some(Arc::new(Gate::default())), || {
                    assert_eq!(qos(), QOS_BACKGROUND)
                });
                assert_eq!(
                    qos(),
                    QOS_UTILITY,
                    "nested maintenance must restore batch priority"
                );
                let inherited = context();
                std::thread::spawn(move || {
                    with_context(inherited, || assert_eq!(qos(), QOS_UTILITY))
                })
                .join()
                .unwrap();
            });
            assert_eq!(qos(), QOS_USER_INITIATED);
            let pool = Arc::new(crate::decode_pool::Pool::new(3).unwrap());
            let order = Arc::new(crate::decode_pool::Order::default());
            interruptible(Arc::new(Gate::default()), || {
                decoder(
                    crate::decode_pool::Task {
                        pool,
                        turn: order.issue(),
                    },
                    || {
                        assert_eq!(qos(), QOS_UTILITY);
                        let task = decoder_context().unwrap();
                        task.pool.planes(&task.turn, |_| {
                            assert_eq!(qos(), QOS_UTILITY);
                            Ok(())
                        })?;
                        Ok(())
                    },
                )
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
        for requested in [false, true] {
            let gate = Arc::new(Gate::default());
            let queued = gate.foreground();
            let running = gate.foreground();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let work = || {
                    checkpoint();
                    tx.send(()).unwrap();
                };
                if requested {
                    processing(gate, work)
                } else {
                    run(Some(gate), work)
                }
            });
            assert!(rx.recv_timeout(Duration::from_millis(20)).is_err());
            drop(queued);
            assert!(rx.recv_timeout(Duration::from_millis(20)).is_err());
            drop(running);
            rx.recv_timeout(Duration::from_secs(2)).unwrap();
            checkpoint(); // Ordinary decode callers never acquire the background gate.
        }
    }
}
