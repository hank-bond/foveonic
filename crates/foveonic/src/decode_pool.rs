//! Shares a bounded FIFO sensor-layer pool across ordered speculative photo loads.

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
        let mut state = self.order.state.lock().unwrap();
        loop {
            background::checkpoint_or_cancel()?;
            if self.ticket == state.next {
                return Ok(());
            }
            state = self.order.wake.wait_timeout(state, TURN_POLL).unwrap().0;
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
                    *result = Some(background::with_context(scheduling, || {
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
    fn one<T: Send>(&self, turn: &Turn, decode: impl FnOnce() -> Result<T> + Send) -> Result<T> {
        let mut result = None;
        self.0.in_place_scope_fifo(|scope| {
            let scheduling = background::context();
            let result = &mut result;
            scope.spawn_fifo(move |_| {
                *result = Some(background::with_context(scheduling, || {
                    background::checkpoint_or_cancel()?;
                    decode()
                }));
            });
            turn.release();
        });
        result.ok_or("missing sensor decode result")?
    }
}

/// Classic's interleaved sensor stream is one task in the same bounded pool.
pub(crate) fn one<T: Send>(decode: impl FnOnce() -> Result<T> + Send) -> Result<T> {
    if let Some(task) = background::decoder_context() {
        task.pool.one(&task.turn, decode)
    } else {
        decode()
    }
}

#[cfg(test)]
#[path = "decode_pool_test.rs"]
mod tests;
