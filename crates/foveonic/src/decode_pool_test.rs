//! Verifies FIFO planes and rows share worker bounds, join failures and preserve cancellation.

use super::*;
use std::{sync::mpsc, time::Duration};

type Latch = (Mutex<bool>, Condvar);

#[test]
fn borrowed_cpu_preparation_and_nested_planes_share_capacity_and_cancellation() {
    let pool = Arc::new(Pool::new(2).unwrap());
    let gate = Arc::new(background::Gate::default());
    let values = [3, 5, 7];
    background::interruptible(gate.clone(), || {
        background::decoder(
            Task {
                pool: pool.clone(),
                turn: Arc::new(Order::default()).issue(),
            },
            || {
                assert_eq!(
                    cpu(|| {
                        assert!(pool.0.current_thread_index().is_some());
                        assert!(background::context().is_some());
                        planes(|channel| {
                            assert!(pool.0.current_thread_index().is_some());
                            Ok(values[channel])
                        })
                    })?,
                    values
                );
                let _foreground = gate.foreground();
                let ran = std::sync::atomic::AtomicBool::new(false);
                assert!(cpu(|| {
                    ran.store(true, std::sync::atomic::Ordering::Relaxed);
                    Ok(())
                })
                .unwrap_err()
                .is::<background::Interrupted>());
                assert!(!ran.load(std::sync::atomic::Ordering::Relaxed));
                Ok(())
            },
        )
    })
    .unwrap();
}

#[test]
fn paused_cpu_batch_yields_one_worker_to_foreground_and_resumes() {
    let pool = Arc::new(Pool::new(1).unwrap());
    let gate = Arc::new(background::Gate::default());
    let (started, start) = mpsc::channel();
    let (proceed, resume) = mpsc::channel();
    let (foreground_done, done) = mpsc::channel();
    let (batch_resumed, resumed) = mpsc::channel();
    std::thread::scope(|scope| {
        let batch = {
            let pool = pool.clone();
            let gate = gate.clone();
            scope.spawn(move || {
                background::processing(gate, || {
                    background::decoder(
                        Task {
                            pool,
                            turn: Arc::new(Order::default()).issue(),
                        },
                        || {
                            cpu(move || {
                                started.send(()).unwrap();
                                resume.recv().unwrap();
                                background::checkpoint_or_cancel()?;
                                batch_resumed.send(()).unwrap();
                                Ok(19)
                            })
                        },
                    )
                })
            })
        };
        start.recv_timeout(Duration::from_secs(5)).unwrap();
        let foreground = gate.foreground();
        pool.0.spawn(move || {
            background::with_context(None, || foreground_done.send(()).unwrap());
        });
        proceed.send(()).unwrap();
        let completed = done.recv_timeout(Duration::from_secs(5));
        let resumed_early = resumed.try_recv().is_ok();
        // Release even on failure so the old blocking implementation cannot hang joins.
        drop(foreground);
        assert_eq!(batch.join().unwrap().unwrap(), 19);
        assert!(
            !resumed_early,
            "the batch must keep its progress paused until foreground releases the gate"
        );
        completed.expect("foreground work could not run while a one-worker batch was paused");
    });
}

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
fn foreground_and_speculative_planes_and_rows_use_the_persistent_host_pool() {
    let pool = shared().unwrap();
    assert!(Arc::ptr_eq(&pool, &shared().unwrap()));
    assert_eq!(pool.threads(), available_workers());
    let caller = std::thread::current().id();
    assert_eq!(
        planes(|channel| {
            assert!(pool.0.current_thread_index().is_some());
            assert_ne!(std::thread::current().id(), caller);
            assert!(background::context().is_none());
            Ok(channel + 1)
        })
        .unwrap(),
        [1, 2, 3]
    );
    let mut output = vec![0; pool.threads() * 4];
    rows(&mut output, 1, |first, chunk| {
        assert!(pool.0.current_thread_index().is_some());
        assert_ne!(std::thread::current().id(), caller);
        assert!(background::context().is_none());
        for (offset, value) in chunk.iter_mut().enumerate() {
            *value = first + offset;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(output, (0..output.len()).collect::<Vec<_>>());
    background::interruptible(Arc::new(background::Gate::default()), || {
        background::decoder(
            Task {
                pool: pool.clone(),
                turn: Arc::new(Order::default()).issue(),
            },
            || {
                planes(|channel| {
                    assert!(pool.0.current_thread_index().is_some());
                    assert!(Arc::ptr_eq(
                        &pool,
                        &background::decoder_context().unwrap().pool
                    ));
                    Ok(channel)
                })?;
                rows(&mut output, 1, |_, chunk| {
                    assert!(pool.0.current_thread_index().is_some());
                    assert!(Arc::ptr_eq(
                        &pool,
                        &background::decoder_context().unwrap().pool
                    ));
                    chunk.fill(7);
                    Ok(())
                })
            },
        )
    })
    .unwrap();
    assert!(output.iter().all(|&value| value == 7));
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
                            task.pool.planes(Some(&task.turn), |layer| {
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
                .planes(Some(&task.turn), |_| Ok(()))
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
                        task.pool.planes(Some(&task.turn), |layer| {
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
            task.pool.planes(Some(&task.turn), Ok)
        })
    })
    .unwrap();
    assert_eq!(next, [0, 1, 2]);
}

#[test]
fn row_photos_share_only_configured_workers_and_release_admission_after_queueing() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    for worker_count in [1, 2, 4] {
        let pool = Arc::new(Pool::new(worker_count).unwrap());
        let order = Arc::new(Order::default());
        let turns = [order.issue(), order.issue()];
        let gates: Arc<Vec<Latch>> = Arc::new(
            (0..worker_count * 2)
                .map(|_| (Mutex::new(false), Condvar::new()))
                .collect(),
        );
        let active = AtomicUsize::new(0);
        let maximum = AtomicUsize::new(0);
        let (started, receive) = mpsc::channel();
        std::thread::scope(|scope| {
            let release = ReleaseAll(gates.clone());
            let jobs: Vec<_> = turns
                .into_iter()
                .enumerate()
                .map(|(photo, turn)| {
                    let pool = pool.clone();
                    let gates = gates.clone();
                    let started = started.clone();
                    let active = &active;
                    let maximum = &maximum;
                    scope.spawn(move || {
                        let mut output = vec![0; worker_count * 4];
                        background::interruptible(Arc::new(background::Gate::default()), || {
                            background::decoder(Task { pool, turn }, || {
                                rows(&mut output, 1, |first, output| {
                                    let running = active.fetch_add(1, Ordering::SeqCst) + 1;
                                    maximum.fetch_max(running, Ordering::SeqCst);
                                    let thread = std::thread::current();
                                    started
                                        .send((
                                            photo,
                                            first,
                                            thread.id(),
                                            thread.name().unwrap().to_owned(),
                                        ))
                                        .unwrap();
                                    let latch = &gates[photo * worker_count + first / 4];
                                    let mut released = latch.0.lock().unwrap();
                                    while !*released {
                                        released = latch.1.wait(released).unwrap();
                                    }
                                    output.fill(photo + 1);
                                    active.fetch_sub(1, Ordering::SeqCst);
                                    Ok(())
                                })
                            })
                        })
                        .unwrap();
                        output
                    })
                })
                .collect();
            let first: Vec<_> = (0..worker_count)
                .map(|_| receive.recv_timeout(Duration::from_secs(10)).unwrap())
                .collect();
            assert!(first
                .iter()
                .all(|event| event.0 == 0 && event.3.starts_with("processing-cpu-")));
            assert_eq!(
                first
                    .iter()
                    .map(|event| event.2)
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                worker_count
            );
            // The second photo is admitted even though the first has not finished.
            *gates[0].0.lock().unwrap() = true;
            gates[0].1.notify_one();
            let next = receive.recv_timeout(Duration::from_secs(10)).unwrap();
            assert_eq!((next.0, next.1), (1, 0));
            drop(release);
            for (photo, job) in jobs.into_iter().enumerate() {
                assert_eq!(job.join().unwrap(), vec![photo + 1; worker_count * 4]);
            }
        });
        assert!(maximum.load(Ordering::SeqCst) <= worker_count);
    }
}

#[test]
fn row_cancellation_reaches_active_and_queued_chunks_and_releases_next_turn() {
    let pool = Arc::new(Pool::new(2).unwrap());
    let order = Arc::new(Order::default());
    let first = order.issue();
    let second = order.issue();
    let gate = Arc::new(background::Gate::default());
    let (started, receive) = mpsc::channel();
    let latches: Arc<Vec<Latch>> = Arc::new(
        (0..2)
            .map(|_| (Mutex::new(false), Condvar::new()))
            .collect(),
    );
    std::thread::scope(|scope| {
        let release = ReleaseAll(latches.clone());
        let (blocked, block_receive) = mpsc::channel();
        let blocker = latches.clone();
        pool.0.spawn_fifo(move || {
            blocked.send(()).unwrap();
            let mut released = blocker[1].0.lock().unwrap();
            while !*released {
                released = blocker[1].1.wait(released).unwrap();
            }
        });
        block_receive.recv_timeout(Duration::from_secs(10)).unwrap();
        let pool = pool.clone();
        let inherited = gate.clone();
        let job = scope.spawn(move || {
            background::interruptible(inherited, || {
                background::decoder(Task { pool, turn: first }, || {
                    let mut output = [0; 8];
                    rows(&mut output, 1, |_, _| {
                        started.send(()).unwrap();
                        let mut released = latches[0].0.lock().unwrap();
                        while !*released {
                            released = latches[0].1.wait(released).unwrap();
                        }
                        background::checkpoint_or_cancel()
                    })
                })
            })
        });
        receive.recv_timeout(Duration::from_secs(10)).unwrap();
        gate.interrupt_prefetch();
        drop(release);
        assert!(job
            .join()
            .unwrap()
            .unwrap_err()
            .is::<background::Interrupted>());
        assert!(
            receive.try_recv().is_err(),
            "cancelled queued row chunk entered the decoder"
        );
    });
    background::interruptible(gate, || {
        background::decoder(Task { pool, turn: second }, || {
            let mut output = [0; 8];
            rows(&mut output, 1, |_, output| {
                output.fill(7);
                Ok(())
            })?;
            assert_eq!(output, [7; 8]);
            Ok(())
        })
    })
    .unwrap();
}

#[test]
fn row_failures_join_every_chunk_and_choose_row_order_for_foreground_and_pool() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    for pooled in [false, true] {
        let completed = AtomicUsize::new(0);
        let mut output = [0; 16];
        let mut work = || {
            rows(&mut output, 1, |first, _| {
                completed.fetch_add(1, Ordering::SeqCst);
                Err(if first == 0 {
                    "earliest row failed"
                } else {
                    "later row failed"
                }
                .into())
            })
        };
        let error = if pooled {
            let pool = Arc::new(Pool::new(4).unwrap());
            let order = Arc::new(Order::default());
            background::interruptible(Arc::new(background::Gate::default()), || {
                background::decoder(
                    Task {
                        pool,
                        turn: order.issue(),
                    },
                    work,
                )
            })
            .unwrap_err()
        } else {
            work().unwrap_err()
        };
        assert_eq!(error.to_string(), "earliest row failed");
        let workers = if pooled {
            4
        } else {
            shared().unwrap().threads()
        };
        let rows_per_chunk = 16_usize.div_ceil(workers);
        assert_eq!(
            completed.load(Ordering::SeqCst),
            16_usize.div_ceil(rows_per_chunk)
        );
    }
    let gate = Arc::new(background::Gate::default());
    background::interruptible(gate.clone(), || {
        gate.interrupt_prefetch();
        let mut output = [0; 16];
        let error = rows(&mut output, 1, |_, _| background::checkpoint_or_cancel()).unwrap_err();
        assert!(error.is::<background::Interrupted>());
    });
}

#[test]
fn cached_pool_failure_preserves_first_report_and_typed_cause() {
    use std::error::Error;
    let original: Box<dyn Error + Send + Sync> = Box::new(std::io::Error::from_raw_os_error(13));
    let text = original.to_string();
    let reported = crate::support::reported_error(
        "decode_pool.create",
        original,
        crate::support::OperationContext::default(),
    );
    let first = reported
        .downcast_ref::<crate::support::ReportedError>()
        .unwrap()
        .report
        .clone();
    let shared: Arc<dyn Error + Send + Sync> = Arc::from(reported);
    let a = SharedPoolError(shared.clone());
    let b = SharedPoolError(shared);
    assert_eq!(a.to_string(), text);
    let repeated = crate::support::report_error("later", &b, Default::default());
    assert!(Arc::ptr_eq(&first, &repeated));
    let cause = a.source().unwrap().source().unwrap();
    assert_eq!(
        cause
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .raw_os_error(),
        Some(13)
    );
}
