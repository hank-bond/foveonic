//! Prioritizes selected photos and thumbnails over batches and interruptible speculative work.

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

// Only queued/running user work holds a demand counter. CPU work already
// decoding an in-memory speculative source may finish below interactive work.
#[derive(Clone, Copy)]
enum Priority {
    Foreground = 0,
    Thumbnail = 1,
    Processing = 2,
    Background = 3,
    Decode,
}
impl Priority {
    fn higher_levels(self) -> usize {
        match self {
            Self::Decode => Self::Thumbnail as usize,
            _ => self as usize,
        }
    }
}

#[derive(Default)]
pub(crate) struct Gate {
    demand: [AtomicUsize; Priority::Background as usize],
    epoch: AtomicU64,
    lock: Mutex<()>,
    wake: Condvar,
}
/// Holds priority through queued and active work, including supersession.
pub struct Activity {
    gate: Arc<Gate>,
    priority: Priority,
}
impl Gate {
    fn activity(self: &Arc<Self>, priority: Priority) -> Activity {
        let _guard = self.lock.lock().unwrap();
        self.demand[priority as usize].fetch_add(1, Ordering::AcqRel);
        if matches!(priority, Priority::Foreground) {
            self.epoch.fetch_add(1, Ordering::AcqRel);
        }
        self.wake.notify_all();
        Activity {
            gate: self.clone(),
            priority,
        }
    }
    pub fn foreground(self: &Arc<Self>) -> Activity {
        self.activity(Priority::Foreground)
    }
    pub fn thumbnails(self: &Arc<Self>) -> Activity {
        self.activity(Priority::Thumbnail)
    }
    fn available(&self, priority: Priority) -> bool {
        self.demand[..priority.higher_levels()]
            .iter()
            .all(|count| count.load(Ordering::Acquire) == 0)
    }
    pub fn idle(&self) -> bool {
        self.available(Priority::Background)
    }
    pub fn interrupt_prefetch(&self) {
        let _guard = self.lock.lock().unwrap();
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.wake.notify_all();
    }
    fn wait(&self, context: &Context) {
        if self.available(context.priority) || context.interrupted() {
            return;
        }
        let mut guard = self.lock.lock().unwrap();
        while !self.available(context.priority) && !context.interrupted() {
            guard = self.wake.wait(guard).unwrap();
        }
    }
}
impl Drop for Activity {
    fn drop(&mut self) {
        let _guard = self.gate.lock.lock().unwrap();
        self.gate.demand[self.priority as usize].fetch_sub(1, Ordering::AcqRel);
        self.gate.wake.notify_all();
    }
}
#[derive(Clone)]
pub(crate) struct Context {
    gate: Arc<Gate>,
    interrupt_epoch: Option<u64>,
    decoder: Option<crate::decode_pool::Task>,
    priority: Priority,
}
impl Context {
    fn interrupted(&self) -> bool {
        self.interrupt_epoch.is_some_and(|epoch| {
            !self.gate.available(Priority::Thumbnail)
                || self.gate.epoch.load(Ordering::Acquire) != epoch
        })
    }
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
            priority: Priority::Background,
        }),
        work,
    )
}

/// User-requested processing yields to interaction without maintenance-level throttling.
pub(crate) fn processing<T>(gate: Arc<Gate>, work: impl FnOnce() -> T) -> T {
    let _activity = gate.activity(Priority::Processing);
    with_context(
        Some(Context {
            gate,
            interrupt_epoch: None,
            decoder: None,
            priority: Priority::Processing,
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
            priority: Priority::Background,
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
            context.gate.wait(context);
        }
    });
}

/// Pausable jobs retain progress. A foreground request still cancels speculative
/// work immediately, including workers waiting behind thumbnail demand.
pub(crate) fn checkpoint_or_cancel() -> crate::source::Result<()> {
    CONTEXT.with(|c| {
        if let Some(context) = c.borrow().as_ref() {
            context.gate.wait(context);
            if context.interrupted() {
                return Err(Box::new(Interrupted) as Box<dyn std::error::Error + Send + Sync>);
            }
        }
        Ok(())
    })
}

/// Small browser reads yield to the selected photo and precede optional work.
pub(crate) fn thumbnail<T>(gate: Arc<Gate>, work: impl FnOnce() -> T) -> T {
    with_context(
        Some(Context {
            gate,
            interrupt_epoch: None,
            decoder: None,
            priority: Priority::Thumbnail,
        }),
        || {
            checkpoint();
            work()
        },
    )
}

/// Compressed inputs are already in memory. Finish these bounded CPU tasks
/// without waiting for thumbnails, while retaining foreground cancellation.
pub(crate) fn decoding<T>(inherited: Option<Context>, work: impl FnOnce() -> T) -> T {
    let context = inherited.map(|mut context| {
        context.priority = Priority::Decode;
        context
    });
    with_context(context, work)
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
        Some(context)
            if context.decoder.is_some() || matches!(context.priority, Priority::Processing) =>
        {
            QOS_UTILITY
        }
        Some(context) if matches!(context.priority, Priority::Thumbnail) => QOS_UTILITY,
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
            thumbnail(Arc::new(Gate::default()), || {
                assert_eq!(qos(), QOS_UTILITY);
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
    fn selected_photo_precedes_thumbnails_then_batches_then_maintenance() {
        let gate = Arc::new(Gate::default());
        let foreground = gate.foreground();
        let thumbnails = gate.thumbnails();
        let queued_batch = gate.activity(Priority::Processing);
        let (thumb_tx, thumb_rx) = mpsc::channel();
        let (batch_tx, batch_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (background_tx, background_rx) = mpsc::channel();
        let small_gate = gate.clone();
        let small =
            std::thread::spawn(move || thumbnail(small_gate, || thumb_tx.send(()).unwrap()));
        let batch_gate = gate.clone();
        let batch = std::thread::spawn(move || {
            processing(batch_gate, || {
                batch_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
        });
        let background = std::thread::spawn(move || {
            run(Some(gate), || {
                checkpoint();
                background_tx.send(()).unwrap();
            })
        });
        assert!(thumb_rx.recv_timeout(Duration::from_millis(20)).is_err());
        drop(foreground);
        thumb_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(batch_rx.recv_timeout(Duration::from_millis(20)).is_err());
        assert!(background_rx.try_recv().is_err());
        drop(thumbnails);
        batch_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(queued_batch);
        assert!(background_rx
            .recv_timeout(Duration::from_millis(20))
            .is_err());
        release_tx.send(()).unwrap();
        background_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        small.join().unwrap();
        batch.join().unwrap();
        background.join().unwrap();
    }

    #[test]
    fn an_active_thumbnail_yields_when_a_selected_photo_arrives() {
        let gate = Arc::new(Gate::default());
        let (ready_tx, ready_rx) = mpsc::channel();
        let (advance_tx, advance_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let thumbnail_gate = gate.clone();
        let worker = std::thread::spawn(move || {
            thumbnail(thumbnail_gate, || {
                ready_tx.send(()).unwrap();
                advance_rx.recv().unwrap();
                checkpoint_or_cancel().unwrap();
                done_tx.send(()).unwrap();
            })
        });
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let foreground = gate.foreground();
        advance_tx.send(()).unwrap();
        assert!(done_rx.recv_timeout(Duration::from_millis(20)).is_err());
        drop(foreground);
        done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn speculative_cpu_work_finishes_during_thumbnail_demand_but_foreground_still_cancels() {
        let gate = Arc::new(Gate::default());
        let (ready_tx, ready_rx) = mpsc::channel();
        let (advance_tx, advance_rx) = mpsc::channel();
        let (cpu_tx, cpu_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let background_gate = gate.clone();
        let worker = std::thread::spawn(move || {
            interruptible(background_gate, || {
                ready_tx.send(()).unwrap();
                advance_rx.recv().unwrap();
                decoding(context(), checkpoint_or_cancel).unwrap();
                cpu_tx.send(()).unwrap();
                let error = checkpoint_or_cancel().unwrap_err();
                done_tx.send(error.is::<Interrupted>()).unwrap();
            })
        });
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let thumbnails = gate.thumbnails();
        advance_tx.send(()).unwrap();
        cpu_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(done_rx.recv_timeout(Duration::from_millis(20)).is_err());
        let foreground = gate.foreground();
        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        worker.join().unwrap();
        drop(foreground);
        drop(thumbnails);
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
