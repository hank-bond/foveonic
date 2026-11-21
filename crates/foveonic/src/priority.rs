//! Orders all work in one set of tiers and lets lower tiers yield to higher-tier demand across threads.

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

/// Every tier yields to all tiers above it. This is the only priority order in the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    /// Images shown in the editor's photo viewports.
    Viewport,
    /// Navigator images and scopes.
    Auxiliary,
    /// Browser thumbnails needed to use the app.
    Thumbnail,
    /// User-requested file output.
    Export,
    /// Run-ahead work nobody has asked for yet.
    Speculative,
}
const TIERS: usize = Priority::Speculative as usize + 1;

impl Priority {
    fn index(self) -> usize {
        self as usize
    }
}

/// Shares demand for each tier across every thread that does prioritized work.
#[derive(Default)]
pub struct Gate {
    demand: [AtomicUsize; TIERS],
    speculative_epoch: AtomicU64,
    attempt_epochs: [AtomicU64; TIERS],
    lock: Mutex<()>,
    wake: Condvar,
}

/// Holds demand for one tier through queued and active work, including supersession.
pub struct Demand {
    gate: Arc<Gate>,
    priority: Priority,
}

impl Gate {
    /// Lower tiers pause at their next checkpoint and current lower-tier attempts are invalidated.
    pub fn demand(self: &Arc<Self>, priority: Priority) -> Demand {
        let _guard = self.lock.lock().unwrap();
        self.demand[priority.index()].fetch_add(1, Ordering::AcqRel);
        for epoch in &self.attempt_epochs[priority.index() + 1..] {
            epoch.fetch_add(1, Ordering::AcqRel);
        }
        if priority == Priority::Viewport {
            self.speculative_epoch.fetch_add(1, Ordering::AcqRel);
        }
        self.wake.notify_all();
        Demand {
            gate: self.clone(),
            priority,
        }
    }
    /// No higher tier has demand.
    pub fn ready(&self, priority: Priority) -> bool {
        self.demand[..priority.index()]
            .iter()
            .all(|count| count.load(Ordering::Acquire) == 0)
    }
    pub fn interrupt_speculative(&self) {
        let _guard = self.lock.lock().unwrap();
        self.speculative_epoch.fetch_add(1, Ordering::AcqRel);
        self.attempt_epochs[Priority::Speculative.index()].fetch_add(1, Ordering::AcqRel);
        self.wake.notify_all();
    }
    /// Viewport work runs without a context: it never pauses, and the GPU and
    /// scheduler treat contextless work as interactive.
    pub(crate) fn context(self: &Arc<Self>, priority: Priority) -> Option<Context> {
        (priority != Priority::Viewport).then(|| Context {
            gate: self.clone(),
            interrupt_epoch: None,
            attempt_epoch: None,
            decoder: None,
            decoding: false,
            priority,
        })
    }
    /// Run `work` now if no higher tier has demand. Its cancel probe turns true when
    /// higher-tier demand arrives, even demand that has already finished.
    pub fn attempt<T>(
        self: &Arc<Self>,
        priority: Priority,
        work: impl FnOnce(&dyn Fn() -> bool) -> T,
    ) -> Option<T> {
        if priority == Priority::Viewport {
            return Some(work(&|| false));
        }
        let epoch = self.attempt_epochs[priority.index()].load(Ordering::Acquire);
        if !self.ready(priority) {
            return None;
        }
        let mut attempt = self.context(priority);
        if let Some(context) = &mut attempt {
            context.attempt_epoch = Some((priority, epoch));
        }
        with_context(attempt, || {
            let scheduling = context().expect("priority attempt context");
            let cancel = || scheduling.interrupted();
            (!cancel()).then(|| work(&cancel))
        })
    }
    /// Wait until no higher tier has demand, then run `work`. The caller holds its own demand.
    pub fn when_ready<T>(
        self: &Arc<Self>,
        priority: Priority,
        cancel: &dyn Fn() -> bool,
        work: impl FnOnce() -> T,
    ) -> Option<T> {
        const RECHECK: std::time::Duration = std::time::Duration::from_millis(10);
        loop {
            if cancel() {
                return None;
            }
            let guard = self.lock.lock().unwrap();
            if self.ready(priority) {
                drop(guard);
                break;
            }
            drop(self.wake.wait_timeout(guard, RECHECK).unwrap());
        }
        if cancel() {
            return None;
        }
        Some(self.run(priority, work))
    }
    /// Run pausable work at a tier. It waits at entry and at checkpoints while a
    /// higher tier has demand. The caller holds its own demand when lower tiers
    /// should wait for it.
    pub fn run<T>(self: &Arc<Self>, priority: Priority, work: impl FnOnce() -> T) -> T {
        with_context(self.context(priority), || {
            checkpoint();
            work()
        })
    }
    /// Run speculative work that viewport or auxiliary demand cancels at its next
    /// checkpoint, even if that demand has already finished. Thumbnail and export
    /// demand only pause it. Decoder children inherit the cancellation. Unlike
    /// `attempt(Speculative)`, demand from thumbnails or exports does not cancel it.
    pub fn speculate<T>(self: &Arc<Self>, work: impl FnOnce(&dyn Fn() -> bool) -> T) -> Option<T> {
        if !self.ready(Priority::Speculative) {
            return None;
        }
        interruptible(self.clone(), || {
            let scheduling = context().expect("speculative context");
            let cancel = || scheduling.interrupted();
            (!cancel()).then(|| work(&cancel))
        })
    }
    fn wait(&self, context: &Context) {
        if self.ready(context.priority) || context.interrupted() {
            return;
        }
        let _scope = crate::diagnostics::scope_with(
            crate::diagnostics::Category::Cpu,
            "priority.wait",
            |fields| {
                fields.u64("priority", context.priority as u64);
            },
        );
        if rayon::current_thread_index().is_some() {
            // A paused CPU task must not occupy the last worker needed by the
            // higher-tier task that will release its gate. Help without holding
            // the gate lock, and sleep briefly only when no pool work is ready.
            const IDLE_SLEEP: std::time::Duration = std::time::Duration::from_millis(1);
            while !self.ready(context.priority) && !context.interrupted() {
                if rayon::yield_now() == Some(rayon::Yield::Executed) {
                    continue;
                }
                let guard = self.lock.lock().unwrap();
                if !self.ready(context.priority) && !context.interrupted() {
                    drop(self.wake.wait_timeout(guard, IDLE_SLEEP).unwrap());
                }
            }
            return;
        }
        let mut guard = self.lock.lock().unwrap();
        while !self.ready(context.priority) && !context.interrupted() {
            guard = self.wake.wait(guard).unwrap();
        }
    }
}
impl Drop for Demand {
    fn drop(&mut self) {
        let _guard = self.gate.lock.lock().unwrap();
        self.gate.demand[self.priority.index()].fetch_sub(1, Ordering::AcqRel);
        self.gate.wake.notify_all();
    }
}
#[derive(Clone)]
pub(crate) struct Context {
    gate: Arc<Gate>,
    interrupt_epoch: Option<u64>,
    attempt_epoch: Option<(Priority, u64)>,
    decoder: Option<crate::decode_pool::Task>,
    /// Decoding children of export and speculative work run at utility QoS.
    decoding: bool,
    priority: Priority,
}
impl Context {
    pub(crate) fn interrupted(&self) -> bool {
        self.interrupt_epoch.is_some_and(|epoch| {
            !self.gate.ready(Priority::Thumbnail)
                || self.gate.speculative_epoch.load(Ordering::Acquire) != epoch
        }) || self.attempt_epoch.is_some_and(|(priority, epoch)| {
            !self.gate.ready(priority)
                || self.gate.attempt_epochs[priority.index()].load(Ordering::Acquire) != epoch
        })
    }
}

#[derive(Debug)]
pub(crate) struct Interrupted;
impl std::fmt::Display for Interrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("lower-priority work interrupted")
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

/// Viewport demand invalidates the whole speculative attempt, even if it finishes
/// before the next checkpoint. Decoder children inherit the same epoch.
pub(crate) fn interruptible<T>(gate: Arc<Gate>, work: impl FnOnce() -> T) -> T {
    let epoch = gate.speculative_epoch.load(Ordering::Acquire);
    with_context(
        Some(Context {
            gate,
            interrupt_epoch: Some(epoch),
            attempt_epoch: None,
            decoder: None,
            decoding: false,
            priority: Priority::Speculative,
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
    if let Some(context) = context() {
        context.gate.wait(&context);
    }
}

/// Pausable jobs retain progress. Viewport or auxiliary demand still cancels speculative
/// work immediately, including workers waiting behind thumbnail demand.
pub(crate) fn checkpoint_or_cancel() -> crate::source::Result<()> {
    if let Some(context) = context() {
        context.gate.wait(&context);
        if context.interrupted() {
            return Err(Box::new(Interrupted) as Box<dyn std::error::Error + Send + Sync>);
        }
    }
    Ok(())
}

/// Decoder children keep the tier of the work that requested them.
pub(crate) fn decoding<T>(inherited: Option<Context>, work: impl FnOnce() -> T) -> T {
    let context = inherited.map(|context| Context {
        decoding: true,
        ..context
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
            if context.decoder.is_some()
                || context.priority == Priority::Export
                || (context.decoding && context.priority == Priority::Speculative) =>
        {
            QOS_UTILITY
        }
        Some(context) if matches!(context.priority, Priority::Auxiliary) => QOS_USER_INITIATED,
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
    #[test]
    fn execution_tiers_cancel_on_transient_higher_demand_but_not_their_own() {
        let gate = Arc::new(Gate::default());
        let _auxiliary = gate.demand(Priority::Auxiliary);
        gate.attempt(Priority::Auxiliary, |cancel| {
            let same_tier = gate.demand(Priority::Auxiliary);
            let lower_tier = gate.demand(Priority::Thumbnail);
            assert!(!cancel());
            drop((same_tier, lower_tier));
            let viewport = gate.demand(Priority::Viewport);
            drop(viewport);
            assert!(cancel());
            assert!(checkpoint_or_cancel().is_err());
        })
        .unwrap();
        drop(_auxiliary);
        let _thumbnails = gate.demand(Priority::Thumbnail);
        gate.attempt(Priority::Thumbnail, |cancel| {
            assert!(!cancel());
            let auxiliary = gate.demand(Priority::Auxiliary);
            drop(auxiliary);
            assert!(cancel());
            decoding(context(), || {
                assert!(
                    checkpoint_or_cancel().is_err(),
                    "decoded children retain the attempt's priority"
                );
            });
        })
        .unwrap();
    }

    #[test]
    fn viewport_attempt_runs_as_interactive_work_without_a_context() {
        let gate = Arc::new(Gate::default());
        let _auxiliary = gate.demand(Priority::Auxiliary);
        assert_eq!(
            gate.attempt(Priority::Viewport, |cancel| !cancel()
                && context().is_none()),
            Some(true)
        );
    }

    #[test]
    fn denied_execution_never_runs_or_waits_for_its_own_demand() {
        let gate = Arc::new(Gate::default());
        let viewport = gate.demand(Priority::Viewport);
        assert!(gate
            .attempt(Priority::Auxiliary, |_| panic!("blocked auxiliary ran"))
            .is_none());
        assert!(gate
            .attempt(Priority::Thumbnail, |_| panic!("blocked thumbnail ran"))
            .is_none());
        drop(viewport);
        let auxiliary = gate.demand(Priority::Auxiliary);
        assert!(gate
            .attempt(Priority::Auxiliary, |cancel| assert!(!cancel()))
            .is_some());
        assert!(!gate.ready(Priority::Thumbnail));
        assert!(gate
            .attempt(Priority::Thumbnail, |_| panic!(
                "thumbnail overtook auxiliary"
            ))
            .is_none());
        drop(auxiliary);
        let _thumbnail = gate.demand(Priority::Thumbnail);
        assert!(gate.ready(Priority::Thumbnail));
        assert!(gate
            .attempt(Priority::Thumbnail, |cancel| assert!(!cancel()))
            .is_some());
    }

    #[test]
    fn auxiliary_runs_below_viewport_above_thumbnails_without_waiting_on_itself() {
        let gate = Arc::new(Gate::default());
        let viewport = gate.demand(Priority::Viewport);
        let demand = gate.demand(Priority::Auxiliary);
        assert!(!gate.ready(Priority::Auxiliary));
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker_gate = gate.clone();
        let worker = std::thread::spawn(move || {
            worker_gate
                .when_ready(Priority::Auxiliary, &|| false, || {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                })
                .unwrap();
            drop(demand);
        });
        assert!(started_rx.recv_timeout(Duration::from_millis(20)).is_err());
        drop(viewport);
        assert!(gate.ready(Priority::Auxiliary));
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(!gate.ready(Priority::Thumbnail));
        assert!(!gate.ready(Priority::Speculative));
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        assert!(gate.ready(Priority::Thumbnail));
        assert!(gate.ready(Priority::Speculative));
    }

    #[test]
    fn cancelled_auxiliary_wait_retires_while_viewport_is_still_active() {
        let gate = Arc::new(Gate::default());
        let viewport = gate.demand(Priority::Viewport);
        let demand = gate.demand(Priority::Auxiliary);
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let worker_gate = gate.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let (checked_tx, checked_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let checked = std::cell::Cell::new(false);
            assert!(worker_gate
                .when_ready(
                    Priority::Auxiliary,
                    &|| {
                        let cancelled = worker_cancelled.load(Ordering::Acquire);
                        if !checked.replace(true) {
                            checked_tx.send(()).unwrap();
                        }
                        cancelled
                    },
                    || panic!("cancelled job ran")
                )
                .is_none());
            drop(demand);
            done_tx.send(()).unwrap();
        });
        checked_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        cancelled.store(true, Ordering::Release);
        done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.join().unwrap();
        drop(viewport);
        assert!(gate.ready(Priority::Speculative));
    }

    #[test]
    fn speculative_attempt_rejects_and_remembers_every_higher_tier() {
        let gate = Arc::new(Gate::default());
        for priority in [
            Priority::Viewport,
            Priority::Auxiliary,
            Priority::Thumbnail,
            Priority::Export,
        ] {
            let demand = gate.demand(priority);
            assert!(gate
                .attempt(Priority::Speculative, |_| panic!(
                    "busy gate admitted speculative work"
                ))
                .is_none());
            drop(demand);
            assert_eq!(
                gate.attempt(Priority::Speculative, |cancel| {
                    assert!(!cancel());
                    drop(gate.demand(priority));
                    assert!(
                        cancel(),
                        "completed higher-priority work must invalidate the attempt"
                    );
                    assert!(checkpoint_or_cancel().unwrap_err().is::<Interrupted>());
                }),
                Some(())
            );
            assert!(context().is_none());
        }
        assert_eq!(
            gate.attempt(Priority::Speculative, |cancel| !cancel()),
            Some(true)
        );
    }

    #[test]
    fn thumbnails_pause_and_auxiliary_cancels_speculative_decoding() {
        let gate = Arc::new(Gate::default());
        interruptible(gate.clone(), || {
            let thumbnails = gate.demand(Priority::Thumbnail);
            assert!(!gate.ready(Priority::Speculative));
            assert!(!context().unwrap().interrupted());
            drop(thumbnails);
            let auxiliary = gate.demand(Priority::Auxiliary);
            assert!(decoding(context(), checkpoint_or_cancel).is_err());
            drop(auxiliary);
            checkpoint_or_cancel().unwrap();
        });
    }

    #[test]
    fn ui_decoders_pass_their_own_demand_and_cancel_for_higher_tiers() {
        for priority in [Priority::Auxiliary, Priority::Thumbnail] {
            let gate = Arc::new(Gate::default());
            let own = gate.demand(priority);
            let (started_tx, started_rx) = mpsc::channel();
            let (resume_tx, resume_rx) = mpsc::channel();
            let worker_gate = gate.clone();
            let worker = std::thread::spawn(move || {
                worker_gate
                    .attempt(priority, |_| {
                        decoding(context(), || {
                            checkpoint_or_cancel().unwrap();
                            started_tx.send(()).unwrap();
                            resume_rx.recv().unwrap();
                            assert!(checkpoint_or_cancel().is_err());
                        });
                    })
                    .unwrap();
            });
            let admitted = started_rx.recv_timeout(Duration::from_secs(2));
            let higher = match priority {
                Priority::Auxiliary => gate.demand(Priority::Viewport),
                Priority::Thumbnail => gate.demand(Priority::Auxiliary),
                _ => unreachable!(),
            };
            // Cancellation remembers even a higher-tier task that already ended.
            drop(higher);
            drop(own);
            resume_tx.send(()).unwrap();
            worker.join().unwrap();
            admitted.expect("UI decode waited behind its own demand");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn contextless_decode_and_inherited_contexts_set_native_qos() {
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
            crate::decode_pool::planes(|_| {
                assert_eq!(qos(), QOS_USER_INITIATED);
                Ok(())
            })
            .unwrap();
            let mut active_rows = [0; 12];
            crate::decode_pool::rows(&mut active_rows, 1, |_, _| {
                assert_eq!(qos(), QOS_USER_INITIATED);
                Ok(())
            })
            .unwrap();
            Arc::new(Gate::default()).run(Priority::Speculative, || {
                assert_eq!(qos(), QOS_BACKGROUND);
                let inherited = context();
                std::thread::spawn(move || {
                    with_context(inherited, || assert_eq!(qos(), QOS_BACKGROUND))
                })
                .join()
                .unwrap();
            });
            assert_eq!(qos(), QOS_USER_INITIATED);
            Arc::new(Gate::default()).run(Priority::Thumbnail, || {
                assert_eq!(qos(), QOS_UTILITY);
            });
            assert_eq!(qos(), QOS_USER_INITIATED);
            Arc::new(Gate::default()).run(Priority::Export, || {
                assert_eq!(qos(), QOS_UTILITY);
                crate::decode_pool::planes(|_| {
                    assert_eq!(qos(), QOS_UTILITY);
                    Ok(())
                })
                .unwrap();
                crate::decode_pool::rows(&mut active_rows, 1, |_, _| {
                    assert_eq!(qos(), QOS_UTILITY);
                    Ok(())
                })
                .unwrap();
                Arc::new(Gate::default())
                    .run(Priority::Speculative, || assert_eq!(qos(), QOS_BACKGROUND));
                assert_eq!(
                    qos(),
                    QOS_UTILITY,
                    "nested speculative work must restore export priority"
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
                        task.pool.planes(Some(&task.turn), |_| {
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
    fn viewport_precedes_thumbnails_then_exports_then_speculative_work() {
        let gate = Arc::new(Gate::default());
        let viewport = gate.demand(Priority::Viewport);
        let thumbnails = gate.demand(Priority::Thumbnail);
        let queued_batch = gate.demand(Priority::Export);
        let (thumb_tx, thumb_rx) = mpsc::channel();
        let (batch_tx, batch_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (background_tx, background_rx) = mpsc::channel();
        let small_gate = gate.clone();
        let small = std::thread::spawn(move || {
            small_gate.run(Priority::Thumbnail, || thumb_tx.send(()).unwrap())
        });
        let batch_gate = gate.clone();
        let batch = std::thread::spawn(move || {
            let _demand = batch_gate.demand(Priority::Export);
            batch_gate.run(Priority::Export, || {
                batch_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
        });
        let background = std::thread::spawn(move || {
            gate.run(Priority::Speculative, || {
                checkpoint();
                background_tx.send(()).unwrap();
            })
        });
        assert!(thumb_rx.recv_timeout(Duration::from_millis(20)).is_err());
        drop(viewport);
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
    fn an_active_thumbnail_yields_when_viewport_demand_arrives() {
        let gate = Arc::new(Gate::default());
        let (ready_tx, ready_rx) = mpsc::channel();
        let (advance_tx, advance_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let thumbnail_gate = gate.clone();
        let worker = std::thread::spawn(move || {
            thumbnail_gate.run(Priority::Thumbnail, || {
                ready_tx.send(()).unwrap();
                advance_rx.recv().unwrap();
                checkpoint_or_cancel().unwrap();
                done_tx.send(()).unwrap();
            })
        });
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let viewport = gate.demand(Priority::Viewport);
        advance_tx.send(()).unwrap();
        assert!(done_rx.recv_timeout(Duration::from_millis(20)).is_err());
        drop(viewport);
        done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn interrupted_speculative_work_unwinds_inherited_workers_before_viewport_finishes() {
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
        let viewport = gate.demand(Priority::Viewport);
        advance_tx.send(()).unwrap();
        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        thread.join().unwrap();
        // Cancellation completes while this guard still holds viewport demand.
        assert!(!gate.ready(Priority::Speculative));
        drop(viewport);
        interruptible(gate.clone(), checkpoint_or_cancel).unwrap();
        interruptible(gate.clone(), || {
            drop(gate.demand(Priority::Viewport));
            assert!(checkpoint_or_cancel().unwrap_err().is::<Interrupted>());
        });
    }

    #[test]
    fn speculative_work_waits_for_all_queued_and_running_viewport_work() {
        for requested in [false, true] {
            let gate = Arc::new(Gate::default());
            let queued = gate.demand(Priority::Viewport);
            let running = gate.demand(Priority::Viewport);
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let work = || {
                    checkpoint();
                    tx.send(()).unwrap();
                };
                if requested {
                    gate.run(Priority::Export, work)
                } else {
                    gate.run(Priority::Speculative, work)
                }
            });
            assert!(rx.recv_timeout(Duration::from_millis(20)).is_err());
            drop(queued);
            assert!(rx.recv_timeout(Duration::from_millis(20)).is_err());
            drop(running);
            rx.recv_timeout(Duration::from_secs(2)).unwrap();
            checkpoint(); // Contextless decode callers never wait on the gate.
        }
    }
}
