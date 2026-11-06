//! Shares bounded FIFO sensor-plane and row work across ordered speculative photo loads.

use crate::{background, source::Result};
use std::{
    collections::BTreeSet,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

pub(crate) struct Pool(rayon::ThreadPool);
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
                .thread_name(|i| format!("cache-layer-{i}"))
                .start_handler(|_| background::lower_priority())
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
        turn: &Turn,
        decode: impl Fn(usize) -> Result<T> + Sync,
    ) -> Result<Vec<T>> {
        const SENSOR_PLANES: usize = 3;
        let mut results: [_; SENSOR_PLANES] = std::array::from_fn(|_| None);
        self.0.in_place_scope_fifo(|scope| {
            for (channel, result) in results.iter_mut().enumerate() {
                let scheduling = background::context();
                let decode = &decode;
                scope.spawn_fifo(move |_| {
                    *result = Some(background::decoding(scheduling, || {
                        background::checkpoint_or_cancel()?;
                        decode(channel)
                    }));
                });
            }
            // The next photo can now read and queue its layers while these run.
            turn.release();
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
        turn: &Turn,
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
                scope.spawn_fifo(move |_| {
                    *result = Some(background::decoding(scheduling, || {
                        background::checkpoint_or_cancel()?;
                        decode(chunk * chunk_rows, output)
                    }));
                });
            }
            turn.release();
        });
        // Scope completion joins every chunk. Row order, rather than completion
        // order, chooses the earliest damaged row when several chunks fail.
        results
            .into_iter()
            .try_for_each(|result| result.ok_or("missing sensor row result")?)
    }
}

/// Decode independent rows into disjoint output slices. Speculative photos share
/// their configured FIFO pool. Active loads use at most four inherited workers.
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
        task.pool.rows(&task.turn, output, row_size, &decode)
    } else {
        const FOREGROUND_WORKERS: usize = 4;
        let chunk_rows = (output.len() / row_size).div_ceil(FOREGROUND_WORKERS);
        std::thread::scope(|scope| {
            let workers: Vec<_> = output
                .chunks_mut(chunk_rows * row_size)
                .enumerate()
                .map(|(chunk, output)| {
                    let scheduling = background::context();
                    let decode = &decode;
                    scope.spawn(move || {
                        background::with_context(scheduling, || decode(chunk * chunk_rows, output))
                    })
                })
                .collect();
            let results: Vec<_> = workers
                .into_iter()
                .map(|worker| {
                    worker
                        .join()
                        .map_err(|_| "sensor row decoder worker panicked".into())
                        .and_then(|result| result)
                })
                .collect();
            results.into_iter().collect()
        })
    }
}

#[cfg(test)]
#[path = "decode_pool_test.rs"]
mod tests;
