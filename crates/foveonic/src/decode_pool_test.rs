//! Verifies FIFO layers share arbitrary worker counts without photo barriers or lost cancellation.

use super::*;
use std::{sync::mpsc, time::Duration};

type Latch = (Mutex<bool>, Condvar);
struct ReleaseAll(Arc<Vec<Latch>>);
impl Drop for ReleaseAll {
    fn drop(&mut self) {
        for latch in self.0.iter() {
            *latch.0.lock().unwrap() = true;
            latch.1.notify_all();
        }
    }
}
#[test]
fn five_workers_start_rgb_rg_then_take_the_next_b_before_the_first_photo_finishes() {
    let pool = Arc::new(Pool::new(5).unwrap());
    let order = Arc::new(Order::default());
    let turns: Vec<_> = (0..3).map(|_| order.issue()).collect();
    let gates: Arc<Vec<Latch>> = Arc::new(
        (0..9)
            .map(|_| (Mutex::new(false), Condvar::new()))
            .collect(),
    );
    let (started, receive) = mpsc::channel();
    std::thread::scope(|scope| {
        let _release = ReleaseAll(gates.clone());
        let jobs: Vec<_> = turns
            .into_iter()
            .enumerate()
            .map(|(photo, turn)| {
                let pool = pool.clone();
                let gates = gates.clone();
                let started = started.clone();
                scope.spawn(move || {
                    background::interruptible(Arc::new(background::Gate::default()), || {
                        background::decoder(Task { pool, turn }, || {
                            let task = background::decoder_context().unwrap();
                            task.pool.planes(&task.turn, |layer| {
                                started
                                    .send((photo, layer, std::thread::current().id()))
                                    .unwrap();
                                let latch = &gates[photo * 3 + layer];
                                let mut released = latch.0.lock().unwrap();
                                while !*released {
                                    released = latch.1.wait(released).unwrap();
                                }
                                Ok(layer)
                            })
                        })
                    })
                    .unwrap()
                })
            })
            .collect();
        let first: Vec<_> = (0..5)
            .map(|_| receive.recv_timeout(Duration::from_secs(10)).unwrap())
            .collect();
        let mut layers: Vec<_> = first.iter().map(|p| (p.0, p.1)).collect();
        layers.sort();
        assert_eq!(layers, [(0, 0), (0, 1), (0, 2), (1, 0), (1, 1)]);
        assert_eq!(
            first
                .iter()
                .map(|p| p.2)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            5
        );
        // Release only R1. G1/B1 remain blocked, so no whole-photo barrier can pass.
        *gates[0].0.lock().unwrap() = true;
        gates[0].1.notify_one();
        let next = receive.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!((next.0, next.1), (1, 2));
        drop(_release);
        for job in jobs {
            assert_eq!(job.join().unwrap(), [0, 1, 2]);
        }
    });
}

#[test]
fn cancellation_reaches_queued_layers_and_unblocks_later_photo_admission() {
    let pool = Arc::new(Pool::new(1).unwrap());
    let order = Arc::new(Order::default());
    let first = order.issue();
    let second = order.issue();
    let gate = Arc::new(background::Gate::default());
    background::interruptible(gate.clone(), || {
        let _foreground = gate.foreground();
        let error = background::decoder(
            Task {
                pool: pool.clone(),
                turn: first,
            },
            || Ok(()),
        )
        .unwrap_err();
        assert!(error.is::<background::Interrupted>());
    });
    background::interruptible(gate.clone(), || {
        background::decoder(Task { pool, turn: second }, || {
            let task = background::decoder_context().unwrap();
            let _foreground = gate.foreground();
            assert!(task
                .pool
                .planes(&task.turn, |_| Ok(()))
                .unwrap_err()
                .is::<background::Interrupted>());
            Ok(())
        })
        .unwrap();
    });
}

#[test]
fn replacing_prefetch_cancels_active_and_queued_layers_before_the_next_photo() {
    let pool = Arc::new(Pool::new(1).unwrap());
    let order = Arc::new(Order::default());
    let first = order.issue();
    let second = order.issue();
    let gate = Arc::new(background::Gate::default());
    let latches: Arc<Vec<Latch>> = Arc::new(
        (0..3)
            .map(|_| (Mutex::new(false), Condvar::new()))
            .collect(),
    );
    let (started, receive) = mpsc::channel();
    std::thread::scope(|scope| {
        let release = ReleaseAll(latches.clone());
        let old_pool = pool.clone();
        let old_gate = gate.clone();
        let job = scope.spawn(move || {
            background::interruptible(old_gate, || {
                background::decoder(
                    Task {
                        pool: old_pool,
                        turn: first,
                    },
                    || {
                        let task = background::decoder_context().unwrap();
                        task.pool.planes(&task.turn, |layer| {
                            started.send(layer).unwrap();
                            let mut released = latches[layer].0.lock().unwrap();
                            while !*released {
                                released = latches[layer].1.wait(released).unwrap();
                            }
                            background::checkpoint_or_cancel()?;
                            Ok(layer)
                        })
                    },
                )
            })
        });
        receive.recv_timeout(Duration::from_secs(10)).unwrap();
        // A selection/window change invalidates the epoch without acquiring foreground work.
        gate.interrupt_prefetch();
        drop(release);
        assert!(job
            .join()
            .unwrap()
            .unwrap_err()
            .is::<background::Interrupted>());
        assert!(
            receive.try_recv().is_err(),
            "obsolete queued layers started decoding"
        );
    });
    let next = background::interruptible(gate, || {
        background::decoder(Task { pool, turn: second }, || {
            let task = background::decoder_context().unwrap();
            task.pool.planes(&task.turn, Ok)
        })
    })
    .unwrap();
    assert_eq!(next, [0, 1, 2]);
}
