//! Runs independent processing CPU work on one host-sized pool with decode-specific FIFO admission.

use crate::{background, source::Result};
use std::{
    collections::BTreeSet,
    sync::{Arc, Condvar, LazyLock, Mutex},
    time::Duration,
};

pub(crate) struct Pool(rayon::ThreadPool);
static HOST: LazyLock<std::result::Result<Arc<Pool>, String>> = LazyLock::new(|| {
    Pool::new(available_workers())
        .map(Arc::new)
        .map_err(|error| error.to_string())
});

pub(crate) fn available_workers() -> usize {
    std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
}

/// Foreground and speculative CPU tasks share this persistent pool. It is
/// independent of the UI's global Rayon pool and creates no workers per photo.
pub(crate) fn shared() -> Result<Arc<Pool>> {
    HOST.as_ref()
        .map(Arc::clone)
        .map_err(|error| error.clone().into())
}

/// Run borrowed CPU preparation within the same capacity as sensor decoding.
/// Nested plane/row work uses this pool too; file I/O stays on its caller.
pub(crate) fn cpu<T: Send>(work: impl FnOnce() -> Result<T> + Send) -> Result<T> {
    let diagnostic = crate::diagnostics::current_context();
    let _admission = crate::diagnostics::scope(crate::diagnostics::Category::Cpu, "cpu.install");
    let scheduling = background::context();
    let pool = background::decoder_context()
        .map(|task| task.pool)
        .map_or_else(shared, Ok)?;
    pool.0.install(|| {
        let _context = diagnostic.map(crate::diagnostics::enter);
        let _work = crate::diagnostics::scope(crate::diagnostics::Category::Cpu, "cpu.execute");
        background::with_context(scheduling, || {
            background::checkpoint_or_cancel()?;
            work()
        })
    })
}
#[derive(Clone)]
pub(crate) struct Task {
    pub pool: Arc<Pool>,
    pub turn: Arc<Turn>,
}
#[derive(Default)]
pub(crate) struct Order {
    state: Mutex<OrderState>,
    wake: Condvar,
}
#[derive(Default)]
struct OrderState {
    issued: usize,
    next: usize,
    released: BTreeSet<usize>,
}
pub(crate) struct Turn {
    order: Arc<Order>,
    ticket: usize,
}
impl Order {
    pub fn issue(self: &Arc<Self>) -> Arc<Turn> {
        let mut state = self.state.lock().unwrap();
        let ticket = state.issued;
        state.issued += 1;
        Arc::new(Turn {
            order: self.clone(),
            ticket,
        })
    }
}
impl Turn {
    pub fn wait(&self) -> Result<()> {
        let _scope = crate::diagnostics::scope(crate::diagnostics::Category::Cpu, "cpu.fifo_wait");
        const TURN_POLL: Duration = Duration::from_millis(5);
        loop {
            // Thumbnail demand can pause admission. Leave the order lock free
            // so already-admitted photos can release their turns while paused.
            background::checkpoint_or_cancel()?;
            let state = self.order.state.lock().unwrap();
            if self.ticket == state.next {
                return Ok(());
            }
            drop(self.order.wake.wait_timeout(state, TURN_POLL).unwrap());
        }
    }
    pub fn release(&self) {
        let mut state = self.order.state.lock().unwrap();
        if self.ticket < state.next {
            return;
        }
        state.released.insert(self.ticket);
        loop {
            let next = state.next;
            if !state.released.remove(&next) {
                break;
            }
            state.next += 1;
        }
        self.order.wake.notify_all();
    }
}
impl Drop for Turn {
    fn drop(&mut self) {
        self.release();
    }
}
impl Pool {
    pub fn new(threads: usize) -> Result<Self> {
        Ok(Self(
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .thread_name(|i| format!("processing-cpu-{i}"))
                .build()?,
        ))
    }
    pub fn threads(&self) -> usize {
        self.0.current_num_threads()
    }
    /// Borrow the compressed planes safely. Every photo contributes R/G/B to
    /// the same FIFO workers, without copying compressed data or creating child threads.
    pub fn planes<T: Send>(
        &self,
        turn: Option<&Turn>,
        decode: impl Fn(usize) -> Result<T> + Sync,
    ) -> Result<Vec<T>> {
        const SENSOR_PLANES: usize = 3;
        let mut results: [_; SENSOR_PLANES] = std::array::from_fn(|_| None);
        self.0.in_place_scope_fifo(|scope| {
            for (channel, result) in results.iter_mut().enumerate() {
                let scheduling = background::context();
                let diagnostic = crate::diagnostics::current_context();
                let decode = &decode;
                scope.spawn_fifo(move |_| {
                    let _context = diagnostic.map(crate::diagnostics::enter);
                    let _scope = crate::diagnostics::scope_with(
                        crate::diagnostics::Category::Cpu,
                        "decode.plane",
                        |fields| {
                            fields.u64("plane", channel as u64);
                        },
                    );
                    *result = Some(background::decoding(scheduling, || {
                        background::checkpoint_or_cancel()?;
                        decode(channel)
                    }));
                });
            }
            // The next photo can now read and queue its layers while these run.
            if let Some(turn) = turn {
                turn.release();
            }
        });
        results
            .into_iter()
            .map(|result| {
                result
                    .ok_or_else(|| "missing sensor layer result".into())
                    .and_then(|r| r)
            })
            .collect()
    }
    fn rows<T: Send>(
        &self,
        turn: Option<&Turn>,
        output: &mut [T],
        row_size: usize,
        decode: &(impl Fn(usize, &mut [T]) -> Result<()> + Sync),
    ) -> Result<()> {
        let chunk_rows = (output.len() / row_size).div_ceil(self.threads());
        let chunk_size = chunk_rows * row_size;
        let mut results: Vec<_> = output.chunks(chunk_size).map(|_| None).collect();
        self.0.in_place_scope_fifo(|scope| {
            for (chunk, (output, result)) in
                output.chunks_mut(chunk_size).zip(&mut results).enumerate()
            {
                let scheduling = background::context();
                let diagnostic = crate::diagnostics::current_context();
                scope.spawn_fifo(move |_| {
                    let _context = diagnostic.map(crate::diagnostics::enter);
                    let _scope = crate::diagnostics::scope_with(
                        crate::diagnostics::Category::Cpu,
                        "decode.rows",
                        |fields| {
                            fields.u64("first_row", (chunk * chunk_rows) as u64);
                            fields.u64("rows", (output.len() / row_size) as u64);
                        },
                    );
                    *result = Some(background::decoding(scheduling, || {
                        background::checkpoint_or_cancel()?;
                        decode(chunk * chunk_rows, output)
                    }));
                });
            }
            if let Some(turn) = turn {
                turn.release();
            }
        });
        // Scope completion joins every chunk. Row order, rather than completion
        // order, chooses the earliest damaged row when several chunks fail.
        results
            .into_iter()
            .try_for_each(|result| result.ok_or("missing sensor row result")?)
    }
}

/// Decode independent planes using the common processing CPU budget. Speculative
/// photo admission adds its FIFO turn; foreground decoding has no such turn.
pub(crate) fn planes<T: Send>(decode: impl Fn(usize) -> Result<T> + Sync) -> Result<Vec<T>> {
    if let Some(task) = background::decoder_context() {
        task.pool.planes(Some(&task.turn), decode)
    } else {
        shared()?.planes(None, decode)
    }
}

/// Decode independent rows into disjoint output slices on the same processing
/// pool as sensor planes. Every task inherits its caller's priority and cancellation.
pub(crate) fn rows<T: Send>(
    output: &mut [T],
    row_size: usize,
    decode: impl Fn(usize, &mut [T]) -> Result<()> + Sync,
) -> Result<()> {
    if output.is_empty() {
        return Ok(());
    }
    assert!(row_size != 0 && output.len().is_multiple_of(row_size));
    if let Some(task) = background::decoder_context() {
        task.pool.rows(Some(&task.turn), output, row_size, &decode)
    } else {
        shared()?.rows(None, output, row_size, &decode)
    }
}

#[cfg(test)]
#[path = "decode_pool_test.rs"]
mod tests;
